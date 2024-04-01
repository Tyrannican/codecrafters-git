use anyhow::{Context, Result};

use std::path::Path;

use crate::object::GitObject;

pub(crate) fn invoke(path: impl AsRef<Path>) -> Result<()> {
    let tree = GitObject::create_tree(path).context("creating tree object")?;
    tree.write().context("writing tree object")?;

    println!("{}", tree.hash);

    Ok(())
}
