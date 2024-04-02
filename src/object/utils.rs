use anyhow::{Context, Result};
use flate2::read::ZlibEncoder;
use sha1::{Digest, Sha1};

use std::cmp::Ordering;
use std::io::{Read, Write};
use std::os::unix::fs::MetadataExt;
use std::path::Path;

use crate::object::GitObject;

pub(crate) fn create_filepath(hash: &str) -> Result<String> {
    std::fs::create_dir_all(format!(".git/objects/{}", &hash[..2]))
        .context("creating dir for object")?;

    Ok(format!(".git/objects/{}/{}", &hash[..2], &hash[2..]))
}

pub(crate) fn compress(content: impl Read) -> Result<Vec<u8>> {
    let mut compressed = Vec::new();
    let mut compressor = ZlibEncoder::new(content, flate2::Compression::default());
    compressor
        .read_to_end(&mut compressed)
        .context("compressing data")?;

    Ok(compressed)
}

pub(crate) fn hash_content(content: impl AsRef<Vec<u8>>) -> Vec<u8> {
    let mut hasher = Sha1::new();
    hasher.update(content.as_ref());
    let raw = hasher.finalize();
    let raw = Vec::from_iter(raw.into_iter());

    raw
}

pub(crate) fn build_tree(path: impl AsRef<Path>) -> Result<Vec<u8>> {
    let mut contents = vec![];

    let mut dir = std::fs::read_dir(path.as_ref())
        .with_context(|| format!("reading directory {}", path.as_ref().display()))?;

    // Sorting taken from https://github.com/jonhoo/codecrafters-git-rust/blob/master/src/commands/write_tree.rs#L14
    // Cheers, Jon!
    let mut entries = Vec::new();
    while let Some(entry) = dir.next() {
        let entry = entry.context("bad entry")?;
        let name = entry.file_name();
        let meta = entry.metadata().context("getting entry metadata")?;
        entries.push((entry, name, meta));
    }

    entries.sort_unstable_by(|a, b| {
        let afn = &a.1;
        let afn = afn.as_encoded_bytes();
        let bfn = &b.1;
        let bfn = bfn.as_encoded_bytes();

        let common_len = std::cmp::min(afn.len(), bfn.len());

        match afn[..common_len].cmp(&bfn[..common_len]) {
            Ordering::Equal => {}
            o => return o,
        }

        if afn.len() == bfn.len() {
            return Ordering::Equal;
        }

        let c1 = if let Some(c) = afn.get(common_len).copied() {
            Some(c)
        } else if a.2.is_dir() {
            Some(b'/')
        } else {
            None
        };

        let c2 = if let Some(c) = bfn.get(common_len).copied() {
            Some(c)
        } else if b.2.is_dir() {
            Some(b'/')
        } else {
            None
        };

        c1.cmp(&c2)
    });

    for (entry, filename, metadata) in entries {
        let path = entry.path();
        let filename = filename
            .into_string()
            .expect("this should be a valid filename");

        if metadata.is_file() {
            let obj = GitObject::create_blob(&path).context("creating blob for tree")?;
            let f_mode = metadata.mode();
            let mode_string = format!("{f_mode:o}");
            let raw = hash_content(&obj.content);
            write!(contents, "{mode_string} {filename}\0").context("writing blob to buffer")?;
            contents.extend(raw);
        } else {
            if filename.contains(".git") {
                continue;
            }
            let subtree = build_tree(&path).context("recursive call to tree")?;
            let raw = hash_content(&subtree);
            write!(contents, "40000 {filename}\0").context("writing subtree metadata")?;
            contents.extend(raw);
        }
    }

    let mut tree = vec![];
    write!(tree, "tree {}\0", contents.len()).context("writing tree metadata")?;
    tree.extend(contents);

    Ok(tree)
}
