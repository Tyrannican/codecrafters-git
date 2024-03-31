use crate::object::GitObject;
use anyhow::{Context, Result};

use std::io::Write;

pub(crate) fn invoke(hash: &str) -> Result<()> {
    let object = GitObject::load(&hash).context("loading git object from hash")?;
    let mut stdout = std::io::stdout();
    stdout
        .write_all(&object.content)
        .context("writing object content to stdout")?;
    Ok(())
}
