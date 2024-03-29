use std::io::Write;

use anyhow::{Context, Result};

use crate::object::GitObject;

pub(crate) fn read_object(object: GitObject, content: Vec<u8>) -> Result<()> {
    match object {
        GitObject::Blob => read_blob(content).context("reading blob"),
        GitObject::Tree => read_tree(content).context("reading tree"),
        GitObject::Commit => read_commit(content).context("reading commit"),
        _ => unreachable!("this shouldn't happen"),
    }
}
fn read_blob(content: Vec<u8>) -> Result<()> {
    let mut stdout = std::io::stdout();
    stdout
        .write_all(&content)
        .context("writing blob to stdout")?;

    Ok(())
}
fn read_tree(content: Vec<u8>) -> Result<()> {
    println!("Parsing tree with {} bytes", content.len());

    Ok(())
}
fn read_commit(content: Vec<u8>) -> Result<()> {
    println!("Parsing commit with {} bytes", content.len());
    Ok(())
}
