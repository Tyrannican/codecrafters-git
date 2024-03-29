use anyhow::{Context, Result};
use flate2::read::ZlibDecoder;
use std::ffi::CStr;
use std::io::{BufRead, BufReader, Read};

#[allow(dead_code)]
#[derive(Debug, PartialEq, Eq, Copy, Clone)]
pub(crate) enum GitObject {
    Blob,
    Tree,
    Commit,
    Invalid,
}

impl From<&str> for GitObject {
    fn from(raw: &str) -> Self {
        match raw {
            "blob" => Self::Blob,
            "tree" => Self::Tree,
            "commit" => Self::Commit,
            _ => Self::Invalid,
        }
    }
}

pub(crate) fn parse_object(hash: &str) -> Result<(GitObject, Vec<u8>)> {
    let path = format!(".git/objects/{}/{}", &hash[..2], &hash[2..]);
    let f = std::fs::File::open(&path).with_context(|| format!("opening git object {}", path))?;

    let decoder = ZlibDecoder::new(f);
    let mut decoder = BufReader::new(decoder);
    let mut buf = Vec::new();

    decoder
        .read_until(0, &mut buf)
        .context("reading object header")?;

    let header = CStr::from_bytes_with_nul(&buf).context("converting header to string")?;
    let header = header.to_str().context("converting header to string")?;
    let Some((obj_type, size)) = header.split_once(' ') else {
        anyhow::bail!("no object type or size");
    };

    let obj = GitObject::from(obj_type);
    let size = size.parse::<usize>().context("converting size")?;

    let mut content = Vec::with_capacity(size);
    decoder
        .read_to_end(&mut content)
        .context("reading content slice")?;

    Ok((obj, content))
}
