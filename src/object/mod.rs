mod utils;

use anyhow::{Context, Result};
use flate2::read::ZlibDecoder;
use sha1::{Digest, Sha1};

use std::ffi::CStr;
use std::io::{BufRead, BufReader, Read, Write};
use std::os::unix::fs::MetadataExt;
use std::path::Path;

use utils::{build_tree, compress, create_filepath, hash_content};

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

    pub(crate) fn create_tree(path: impl AsRef<Path>) -> Result<Self> {
        let tree_content = build_tree(path).context("constructing tree object")?;
        let size = tree_content.len();
        let raw = hash_content(&tree_content);
        let hash = hex::encode(raw);

        Ok(Self {
            hash,
            content: tree_content,
            obj_type: GitObjectType::Tree,
            size,
        })
    }

    pub(crate) fn create_commit(
        tree_hash: String,
        parent: Option<String>,
        message: String,
    ) -> Result<Self> {
        use std::fmt::Write; // Prevents clash with io::Write i guess

        let mut content = String::new();
        writeln!(content, "tree {tree_hash}")?;

        if let Some(parent) = parent {
            writeln!(content, "parent {parent}")?;
        }

        // TODO: Deal with getting Author name and Email
        let author = "Big Cheese";
        let email = "cheddar@dairyfarm.com";
        let time =
            std::time::SystemTime::now().duration_since(std::time::SystemTime::UNIX_EPOCH)?;
        writeln!(
            content,
            "author {author} <{email}> {} +0000",
            time.as_secs()
        )?;
        writeln!(
            content,
            "committer {author} <{email}> {} +0000",
            time.as_secs()
        )?;
        writeln!(content, "")?;
        writeln!(content, "{message}")?;

        let size = content.len();

        let mut commit = String::new();
        write!(commit, "commit {size}\0{content}")?;

        let commit = commit.as_bytes().to_vec();
        let raw = hash_content(&commit);
        let hash = hex::encode(raw);

        Ok(Self {
            content: commit,
            hash,
            size,
            obj_type: GitObjectType::Commit,
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

    pub(crate) fn create_raw(data: &[u8], obj: GitObjectType) -> Result<Self> {
        let mut content = vec![];
        write!(content, "{} {}\0", obj, data.len())?;
        content.extend(data);
        let hash = hex::encode(hash_content(&content));

        Ok(Self {
            content,
            size: data.len(),
            hash,
            obj_type: obj,
        })
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

impl std::fmt::Display for GitObjectType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match *self {
            Self::Blob => write!(f, "blob"),
            Self::Tree => write!(f, "tree"),
            Self::Commit => write!(f, "commit"),
        }
    }
}
