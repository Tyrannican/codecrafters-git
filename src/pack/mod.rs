use std::io::Read;

use anyhow::{Context, Result};
use bytes::Buf;

use crate::object::{GitObject, GitObjectType};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum PackFileObjectType {
    Commit,
    Tree,
    Blob,
    Tag,
    OffsetDelta,
    ReferenceDelta,
}

impl From<u8> for PackFileObjectType {
    fn from(value: u8) -> Self {
        match value {
            1 => Self::Commit,
            2 => Self::Tree,
            3 => Self::Blob,
            4 => Self::Tag,
            6 => Self::OffsetDelta,
            7 => Self::ReferenceDelta,
            _ => panic!("invalid value for packfile object type: {value}"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum DeltaInstruction {
    Copy { offset: u64, size: u64 },
    Insert { data: bytes::Bytes },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum CopyField {
    Offset1,
    Offset2,
    Offset3,
    Offset4,
    Size1,
    Size2,
    Size3,
}

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub(crate) struct PackFileObject {
    pub(crate) object: PackFileObjectType,
    pub(crate) content: Vec<u8>,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct PackFile {
    pub(crate) data: bytes::Bytes,
    pub(crate) total_objects: u32,
}

impl PackFile {
    pub fn new(data: bytes::Bytes) -> Self {
        Self {
            data,
            total_objects: 0,
        }
    }

    pub fn parse(&mut self) -> Result<()> {
        self.validate_header().context("validating pack header")?;
        for it in 0..self.total_objects {
            println!("Iteration {it}");
            let mut header = Vec::with_capacity(4);
            loop {
                let next = self.data.get_u8();
                header.push(next);

                if next & 0b1000_0000 == 0 {
                    break;
                }
            }

            let obj_type = PackFileObjectType::from((header[0] & 0b0111_0000) >> 4);
            let mut object_size = (header[0] & 0b0000_1111) as u64;

            for (i, b) in header[1..].iter().enumerate() {
                object_size |= ((b & 0b0111_1111) as u64) << (7 * i + 4);
            }

            println!("Object size: {object_size}");
            println!("Object type: {obj_type:?}");

            // TODO: Fix some offset magic in deltas
            match obj_type {
                PackFileObjectType::OffsetDelta => unimplemented!("nyi"),
                PackFileObjectType::ReferenceDelta => {
                    let base_name = self.data.split_to(20);
                    let hash = hex::encode(base_name);

                    let mut data = Vec::new();
                    let mut decoder = flate2::read::ZlibDecoder::new(&self.data[..]);
                    decoder
                        .read_to_end(&mut data)
                        .context("decompressing ref delta")?;

                    anyhow::ensure!(decoder.total_out() == object_size);

                    self.data.advance(decoder.total_in() as usize);

                    let mut data = bytes::Bytes::from(data);
                    let instructions = parse_delta(&mut data)?;

                    let obj = GitObject::load(&hash)?;
                    let mut data = Vec::new();
                    obj.content.reader().read_to_end(&mut data)?;
                    let content = apply_deltas(&data, instructions)?;

                    let obj = GitObject::create_raw(&content, obj.obj_type)?;
                    // obj.write()?;
                }
                _ => {
                    let mut content = Vec::new();
                    let mut decoder = flate2::read::ZlibDecoder::new(&self.data[..]);
                    decoder
                        .read_to_end(&mut content)
                        .context("decompressing object")?;

                    anyhow::ensure!(decoder.total_out() == object_size);
                    self.data.advance(decoder.total_in() as usize);
                    match obj_type {
                        PackFileObjectType::Commit => {
                            let obj = GitObject::create_raw(&content, GitObjectType::Commit)?;
                            // obj.write()?
                        }
                        PackFileObjectType::Blob => {
                            let obj = GitObject::create_raw(&content, GitObjectType::Blob)?;
                            // obj.write()?
                        }
                        PackFileObjectType::Tree => {
                            let obj = GitObject::create_raw(&content, GitObjectType::Tree)?;
                            // obj.write()?
                        }
                        _ => unimplemented!(),
                    }
                }
            }
        }

        Ok(())
    }

    fn validate_header(&mut self) -> Result<()> {
        let sig = self.data.split_to(4);
        anyhow::ensure!(
            &sig.to_vec() == b"PACK",
            format!(
                "failed signature - got {} instead",
                String::from_utf8(sig.to_vec())?
            )
        );
        let version = self.data.split_to(4);
        anyhow::ensure!(u32::from_be_bytes([version[0], version[1], version[2], version[3]]) == 2);
        let total_objects = self.data.split_to(4);
        let total_objects = u32::from_be_bytes([
            total_objects[0],
            total_objects[1],
            total_objects[2],
            total_objects[3],
        ]);

        self.total_objects = total_objects;

        Ok(())
    }
}

fn parse_delta(data: &mut bytes::Bytes) -> Result<Vec<DeltaInstruction>> {
    let mut instructions = Vec::new();

    while !data.is_empty() {
        let initial = data.get_u8();
        anyhow::ensure!(initial != 0, "expansion value");

        if initial & 0b1000_0000 == 0 {
            let size = initial;
            let add = data.split_to(size as usize);

            instructions.push(DeltaInstruction::Insert { data: add });
        } else {
            let mut fields = vec![];
            if initial & 0b0000_0001 != 0 {
                fields.push(CopyField::Offset1);
            }
            if initial & 0b0000_0010 != 0 {
                fields.push(CopyField::Offset2);
            }
            if initial & 0b0000_0100 != 0 {
                fields.push(CopyField::Offset3);
            }
            if initial & 0b0000_1000 != 0 {
                fields.push(CopyField::Offset4);
            }
            if initial & 0b0001_0000 != 0 {
                fields.push(CopyField::Size1);
            }
            if initial & 0b0010_0000 != 0 {
                fields.push(CopyField::Size2);
            }
            if initial & 0b0100_0000 != 0 {
                fields.push(CopyField::Size3);
            }

            let mut offset = 0;
            let mut size = 0;

            for field in fields.iter() {
                let byte = data.get_u8();

                match field {
                    CopyField::Offset1 => offset |= byte as u64,
                    CopyField::Offset2 => offset |= (byte as u64) << 8,
                    CopyField::Offset3 => offset |= (byte as u64) << 16,
                    CopyField::Offset4 => offset |= (byte as u64) << 24,
                    CopyField::Size1 => size |= byte as u64,
                    CopyField::Size2 => size |= (byte as u64) << 8,
                    CopyField::Size3 => size |= (byte as u64) << 16,
                }
            }

            if size == 0 {
                size = 0x10000;
            }

            instructions.push(DeltaInstruction::Copy { offset, size })
        }
    }

    Ok(instructions)
}

fn apply_deltas(base: &[u8], instructions: Vec<DeltaInstruction>) -> Result<Vec<u8>> {
    let mut result = Vec::new();
    for instruction in instructions.into_iter() {
        match instruction {
            DeltaInstruction::Copy { offset, size } => {
                result.extend_from_slice(&base[offset as usize..offset as usize + size as usize])
            }
            DeltaInstruction::Insert { data } => {
                result.extend_from_slice(&data);
            }
        }
    }

    Ok(result)
}
