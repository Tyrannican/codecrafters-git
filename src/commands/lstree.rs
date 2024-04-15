use std::ffi::CStr;
use std::io::{BufRead, Read};

use crate::object::GitObject;
use anyhow::{Context, Result};

pub(crate) fn invoke(tree_hash: &str, name_only: bool) -> Result<()> {
    let obj = GitObject::load(&tree_hash).context("loading tree")?;
    let mut buf = std::io::BufReader::new(&obj.content[..]);

    loop {
        let mut name_buf = Vec::new();
        let mut hash_buf: [u8; 20] = [0; 20];

        buf.read_until(0, &mut name_buf)
            .context("reading tree file content")?;
        if name_buf.is_empty() {
            break;
        }
        buf.read_exact(&mut hash_buf)
            .context("reading tree file hash")?;

        let hash = hex::encode(hash_buf);

        let file_info =
            CStr::from_bytes_until_nul(&name_buf).context("converting file info to c_str")?;
        let file_info = file_info.to_str().context("converting c_str to str")?;

        let Some((mode, name)) = file_info.split_once(' ') else {
            anyhow::bail!(format!("missing file mode and context: {file_info}"));
        };

        if name_only {
            println!("{name}");
        } else {
            let obj = GitObject::load(&hash).context("loading object in tree")?;
            let obj_type: String = obj.obj_type.into();
            println!("{mode:0>6} {obj_type} {hash}\t{name}");
        }
        name_buf.clear();
    }

    Ok(())
}
