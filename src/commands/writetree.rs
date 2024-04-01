use anyhow::{Context, Result};
use sha1::{Digest, Sha1};

use std::{fs, io::Write, os::unix::fs::MetadataExt, path::Path};

use crate::object::GitObject;

// TODO: Recursive magic for hash
pub(crate) fn invoke(path: impl AsRef<Path>) -> Result<()> {
    let tree = create_tree(path).context("creating tree object")?;
    let mut hasher = Sha1::new();
    hasher.update(&tree);
    let raw = hasher.finalize();
    let hash = hex::encode(&raw);
    write_tree(tree, &hash).context("writing tree to disk")?;
    println!("{hash}");
    Ok(())
}

fn create_tree(path: impl AsRef<Path>) -> Result<Vec<u8>> {
    let mut contents = vec![];

    for entry in fs::read_dir(path.as_ref()).context("reading directory contents")? {
        let entry = entry.context("converting to entry")?.path();
        let filename = entry.file_name().expect("this should have a filename");
        let filename = filename.to_str().expect("valid");

        if entry.is_file() {
            let obj = GitObject::create_blob(&entry).context("creating blob for tree")?;
            let metadata = fs::metadata(&entry).context("getting file metadata")?;
            let f_mode = metadata.mode();
            let mode_string = format!("{f_mode:0>6o}");
            let mut hasher = Sha1::new();
            hasher.update(&obj.content);
            let raw = hasher.finalize();
            write!(contents, "{mode_string} {filename}\0").context("writing blob to buffer")?;
            contents.extend(raw);
        } else {
            let subtree = create_tree(&entry).context("recursive call to tree")?;
            let mut hasher = Sha1::new();
            hasher.update(&subtree);
            let raw = hasher.finalize();
            write!(contents, "040000 {filename}\0").context("writing subtree metadata")?;
            contents.extend(raw);
        }
    }

    let mut tree = vec![];
    write!(tree, "tree {}\0", contents.len()).context("writing tree metadata")?;
    tree.extend(contents);

    Ok(tree)
}

fn write_tree(tree: Vec<u8>, hash: &str) -> Result<()> {
    Ok(())
}
