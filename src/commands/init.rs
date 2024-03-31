use anyhow::{Context, Result};

use std::fs;

pub(crate) fn invoke() -> Result<()> {
    fs::create_dir_all(".git/objects").context("creating the git objects directory")?;
    fs::create_dir_all(".git/refs").context("creating the git refs directory")?;
    fs::write(".git/HEAD", "ref: refs/heads/main\n").context("writing HEAD file")?;
    println!("Initialized git directory");

    Ok(())
}
