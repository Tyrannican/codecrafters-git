use anyhow::{Context, Result};

pub(crate) fn invoke(url: String, dst: String) -> Result<()> {
    println!("URL: {url} Destination: {dst}");
    Ok(())
}
