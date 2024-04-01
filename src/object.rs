use anyhow::{Context, Result};
use flate2::read::{ZlibDecoder, ZlibEncoder};
use sha1::{Digest, Sha1};

use std::ffi::CStr;
use std::io::{BufRead, BufReader, Read, Write};
use std::os::unix::fs::MetadataExt;
use std::path::Path;

#[allow(dead_code)]
#[derive(Debug)]
pub(crate) struct GitObject {
    pub(crate) content: Vec<u8>,
    pub(crate) obj_type: GitObjectType,
    pub(crate) hash: String,
    pub(crate) size: usize,
}

impl GitObject {
    pub(crate) fn load(hash: &str) -> Result<Self> {
        anyhow::ensure!(hash.len() == 40);
        let path = create_filepath(hash)?;
        let f =
            std::fs::File::open(&path).with_context(|| format!("opening git object {}", path))?;
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

        let obj = GitObjectType::from(obj_type);
        let size = size.parse::<usize>().context("converting size")?;

        let mut content = Vec::with_capacity(size);
        decoder
            .read_to_end(&mut content)
            .context("reading content slice")?;

        Ok(Self {
            hash: hash.to_string(),
            content,
            obj_type: obj,
            size,
        })
    }

    pub(crate) fn create_blob(file: impl AsRef<Path>) -> Result<Self> {
        let metadata = std::fs::metadata(&file)
            .with_context(|| format!("getting {} metadata", file.as_ref().display()))?;
        let size = metadata.size() as usize;
        let mut buffer = vec![];
        write!(buffer, "blob {size}\0").context("writing blob header")?;

        let mut f = std::fs::File::open(&file).context("opening file")?;
        std::io::copy(&mut f, &mut buffer).context("copying file contents over")?;

        let mut hasher = Sha1::new();
        hasher.update(&buffer);

        let raw_hash = hasher.finalize();
        let hash = hex::encode(raw_hash);

        Ok(Self {
            hash,
            content: buffer,
            obj_type: GitObjectType::Blob,
            size,
        })
    }

    pub(crate) fn write(&self) -> Result<()> {
        let compressed = compress(&self.content[..]).context("attempting to compress data")?;
        let path = create_filepath(&self.hash)?;
        let mut f = if std::path::PathBuf::from(&path).exists() {
            std::fs::File::open(&path).with_context(|| format!("opening {path} to write object"))?
        } else {
            std::fs::File::create(&path).context("creating file to write object")?
        };

        f.write_all(&compressed).context("writing git object")?;

        Ok(())
    }
}

#[derive(Default, Debug, PartialEq, Eq, Copy, Clone)]
pub(crate) enum GitObjectType {
    #[default]
    Blob,
    Tree,
    Commit,
}

impl From<&str> for GitObjectType {
    fn from(raw: &str) -> Self {
        match raw {
            "blob" => Self::Blob,
            "tree" => Self::Tree,
            "commit" => Self::Commit,
            _ => Self::default(),
        }
    }
}

impl Into<String> for GitObjectType {
    fn into(self) -> String {
        match self {
            Self::Blob => "blob".to_string(),
            Self::Tree => "tree".to_string(),
            Self::Commit => "commit".to_string(),
        }
    }
}

fn create_filepath(hash: &str) -> Result<String> {
    std::fs::create_dir_all(format!(".git/objects/{}", &hash[..2]))
        .context("creating dir for object")?;

    Ok(format!(".git/objects/{}/{}", &hash[..2], &hash[2..]))
}

fn compress(content: impl Read) -> Result<Vec<u8>> {
    let mut compressed = Vec::new();
    let mut compressor = ZlibEncoder::new(content, flate2::Compression::default());
    compressor
        .read_to_end(&mut compressed)
        .context("compressing data")?;

    Ok(compressed)
}
