use std::io::{Cursor, Read};

use anyhow::{Context, Result};
use bytes::{Buf, Bytes};
use flate2::read::ZlibDecoder;
use sha1::{Digest, Sha1};

use crate::object::{GitObject, GitObjectType};

#[derive(Debug, Clone, PartialEq, Eq)]
enum DeltaInstruction {
    Data { data: Bytes },
    Copy { offset: usize, size: usize },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum PackFileObject {
    Commit,
    Tree,
    Blob,
    Tag,
    OffsetDelta,
    RefDelta,
}

impl From<u8> for PackFileObject {
    fn from(value: u8) -> Self {
        match value {
            1 => Self::Commit,
            2 => Self::Tree,
            3 => Self::Blob,
            4 => Self::Tag,
            6 => Self::OffsetDelta,
            7 => Self::RefDelta,
            _ => panic!("invalid value for packfile object type {value}"),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct PackHeader {
    signature: String,
    version: u8,
    objects: u32,
}

#[derive(Debug, Clone)]
pub(crate) struct PackFile {
    pub(crate) id: String,
    pub(crate) header: PackHeader,
    pub(crate) content: Cursor<Bytes>,
}

impl PackFile {
    pub(crate) fn new(id: &str, mut content: Bytes) -> Result<Self> {
        let raw_content = content.split_to(content.len() - 20);
        let checksum = hex::encode(&content);
        Self::verify_checksum(&raw_content, &checksum).context("verify pack checksum")?;

        Ok(Self {
            id: id.to_string(),
            header: PackHeader::default(),
            content: Cursor::new(raw_content),
        })
    }

    pub(crate) fn parse(&mut self) -> Result<()> {
        self.validate_pack_header()
            .context("packfile header validation")?;

        let mut parsed_objects = 0;
        while self.content.has_remaining() {
            let (object_type, size) = self.get_object_type_and_size();

            match object_type {
                PackFileObject::RefDelta => {
                    self.ref_delta(size).context("parsing ref delta object")?
                }
                PackFileObject::OffsetDelta => {
                    self.ofs_delta().context("parsing ofs delta object")?
                }
                _ => self
                    .object(size, object_type)
                    .context("parsing normal object")?,
            }
            parsed_objects += 1;
        }

        anyhow::ensure!(parsed_objects == self.header.objects);

        Ok(())
    }

    fn validate_pack_header(&mut self) -> Result<()> {
        let sig = self.content.get_u32().to_be_bytes();
        anyhow::ensure!(&sig == b"PACK", "not a valid packfile signature");

        let version = self.content.get_u32();
        anyhow::ensure!(version == 2, "invalid version for packfile header");

        let objects = self.content.get_u32();

        self.header.signature = "PACK".to_string();
        self.header.version = version as u8;
        self.header.objects = objects;

        Ok(())
    }

    fn get_object_type_and_size(&mut self) -> (PackFileObject, usize) {
        let lead = self.content.get_u8();
        let o_type = PackFileObject::from((lead & 0b0111_0000) >> 4);
        let mut size = (lead & 0b0000_1111) as usize;

        if (lead >> 7) & 1 == 0 {
            return (o_type, size);
        }

        let mut ofs = 0;
        loop {
            let byte = self.content.get_u8();
            size |= ((byte & 0b0111_1111) as usize) << (7 * ofs + 4);
            ofs += 1;

            if (byte >> 7) & 1 == 0 {
                return (o_type, size);
            }
        }
    }

    fn ref_delta(&mut self, expected_size: usize) -> Result<()> {
        let mut buf = Vec::with_capacity(20);
        for _ in 0..20 {
            buf.push(self.content.get_u8());
        }

        let base_name = hex::encode(&buf);

        let mut delta = Vec::new();
        let mut decoder = ZlibDecoder::new(self.content.clone());
        decoder
            .read_to_end(&mut delta)
            .context("decompressing ref delta data")?;

        anyhow::ensure!(decoder.total_out() == expected_size as u64);
        self.content.advance(decoder.total_in() as usize);

        let mut delta = Bytes::from(delta);
        let _base_size = delta_size(&mut delta);
        let _reconstructed_size = delta_size(&mut delta);
        let instructions = delta_instructions(delta);

        let base_obj = GitObject::load(&base_name)?;
        let raw_data = base_obj.content;
        let result = resolve_deltas(&raw_data, instructions);
        let new_obj = GitObject::create_raw(&result, base_obj.obj_type)
            .context("creating resolved ref delta object")?;

        new_obj
            .write()
            .context("writing resolved ref delta object")?;

        Ok(())
    }

    fn ofs_delta(&mut self) -> Result<()> {
        let ofs = self.get_ofs_delta_offset()?;
        let current_position = self.content.position();
        self.content.set_position(current_position - ofs);

        let mut content = Vec::new();
        let mut decoder = ZlibDecoder::new(self.content.clone());
        decoder
            .read_to_end(&mut content)
            .context("decompressing offset delta stream")?;

        self.content.set_position(current_position);

        // TODO: Determine correct object type

        Ok(())
    }

    fn object(&mut self, size: usize, object_type: PackFileObject) -> Result<()> {
        let mut content = Vec::with_capacity(size as usize);
        let mut decoder = ZlibDecoder::new(self.content.clone());
        decoder
            .read_to_end(&mut content)
            .context("decompressing object")?;

        anyhow::ensure!(decoder.total_out() == size as u64);
        self.content.advance(decoder.total_in() as usize);

        let object = match object_type {
            PackFileObject::Blob => GitObject::create_raw(&content, GitObjectType::Blob)?,
            PackFileObject::Commit => GitObject::create_raw(&content, GitObjectType::Commit)?,
            PackFileObject::Tree => GitObject::create_raw(&content, GitObjectType::Tree)?,
            PackFileObject::Tag => GitObject::create_raw(&content, GitObjectType::Tag)?,
            _ => unimplemented!("unknown type {object_type:?}"),
        };

        object
            .write()
            .with_context(|| format!("writing object {} from pack", object.hash))?;

        Ok(())
    }

    fn get_ofs_delta_offset(&mut self) -> Result<u64> {
        let mut ofs = 0;
        loop {
            let byte = self.content.get_u8();
            ofs |= (ofs << 7) | byte as u64;
            if (byte >> 7) & 1 == 0 {
                return Ok(ofs);
            }

            ofs += 1;
        }
    }

    fn verify_checksum(content: &Bytes, checksum: &str) -> Result<()> {
        let mut encoder = Sha1::new();
        encoder.update(&content);
        let check = encoder.finalize();
        let check = hex::encode(check);
        anyhow::ensure!(&check == checksum);

        Ok(())
    }
}

fn delta_size(delta: &mut Bytes) -> u64 {
    let mut size = 0;
    let mut idx = 0;

    loop {
        let byte = delta.get_u8();
        size |= ((byte & 0b0111_1111) as u64) << (7 * idx);

        if (byte >> 7) & 1 == 0 {
            break;
        }

        idx += 1;
    }

    size
}

fn delta_instructions(mut delta: Bytes) -> Vec<DeltaInstruction> {
    let mut instructions = Vec::new();

    // TODO: Cleanup
    while !delta.is_empty() {
        let lead = delta.get_u8();

        if (lead >> 7) & 1 == 0 {
            // Data instruction
            instructions.push(DeltaInstruction::Data {
                data: delta.split_to(lead as usize),
            });
        } else {
            // Copy instruction
            let mut offset: usize = 0;
            let mut size: usize = 0;

            if lead & 1 == 1 {
                let byte = delta.get_u8();
                offset |= byte as usize;
            }

            if (lead >> 1) & 1 == 1 {
                let byte = delta.get_u8();
                offset |= (byte as usize) << 8;
            }

            if (lead >> 2) & 1 == 1 {
                let byte = delta.get_u8();
                offset |= (byte as usize) << 16;
            }

            if (lead >> 3) & 1 == 1 {
                let byte = delta.get_u8();
                offset |= (byte as usize) << 24;
            }

            if (lead >> 4) & 1 == 1 {
                let byte = delta.get_u8();
                size |= byte as usize;
            }

            if (lead >> 5) & 1 == 1 {
                let byte = delta.get_u8();
                size |= (byte as usize) << 8;
            }

            if (lead >> 6) & 1 == 1 {
                let byte = delta.get_u8();
                size |= (byte as usize) << 16;
            }

            instructions.push(DeltaInstruction::Copy { offset, size });
        }
    }

    instructions
}

fn resolve_deltas(obj: &[u8], instructions: Vec<DeltaInstruction>) -> Vec<u8> {
    let mut result = Vec::new();

    for instr in instructions {
        match instr {
            DeltaInstruction::Data { data } => result.extend_from_slice(&data),
            DeltaInstruction::Copy { offset, size } => {
                result.extend_from_slice(&obj[offset..offset + size])
            }
        }
    }

    result
}
