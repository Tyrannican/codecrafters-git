use crate::object::GitObject;
use anyhow::{Context, Result};

pub(crate) fn invoke(file: &str, write: bool) -> Result<()> {
    //
    let object = GitObject::create_blob(&file).context("creating git object")?;
    if write {
        object.write().context("writing git object")?;
    }

    println!("{}", object.hash);
    Ok(())
}
