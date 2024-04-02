use anyhow::{Context, Result};

use crate::object::GitObject;

pub(crate) fn invoke(tree_hash: String, parent: Option<String>, message: String) -> Result<()> {
    let commit =
        GitObject::create_commit(tree_hash, parent, message).context("creating commit object")?;

    commit.write().context("writing commit")?;
    println!("{}", commit.hash);
    Ok(())
}
