use anyhow::{Context, Result};
use reqwest::Client;
use reqwest::StatusCode;

use std::{
    ffi::CStr,
    fmt::Write,
    io::{BufRead, BufReader, Read, Write as FileWrite},
    path::{Path, PathBuf},
    sync::Arc,
};
use tokio::sync::Mutex;
use tokio::task::{spawn, JoinHandle};

use crate::object::GitObject;
use crate::object::GitObjectType;
use crate::pack::PackFile;

// TODO: Rebuild the repo from the ref objects
// TODO: Focus only on `main` for now
pub(crate) async fn invoke(url: String, dst: Option<String>) -> Result<()> {
    if let Some(dst) = dst {
        create_destination(dst)
            .await
            .context("creating clone destination")?;
    }

    let client = Client::new();
    let (head, advertised) = ref_discovery(&url, &client)
        .await
        .context("ref discovery")?;
    let packs = fetch_refs(&url, advertised).await?;
    for mut pack in packs {
        pack.parse()
            .with_context(|| format!("parsing pack {}", pack.id))?;
    }

    build_repository(head).context("rebuilding repo from HEAD")?;

    Ok(())
}

async fn create_destination(dst: impl AsRef<Path>) -> Result<()> {
    if dst.as_ref().exists() {
        tokio::fs::remove_dir_all(&dst).await?;
    }
    tokio::fs::create_dir_all(&dst).await?;
    std::env::set_current_dir(dst)?;
    crate::commands::init::invoke()?;

    Ok(())
}

// This is clone so we have nothing so omitting the have part
async fn fetch_refs(url: &str, advertised: Vec<String>) -> Result<Vec<PackFile>> {
    let url = format!("{url}/git-upload-pack");
    let packs = Arc::new(Mutex::new(Vec::new()));
    let mut handles = vec![];

    println!("Downloading packs...");
    for reference in advertised.into_iter() {
        let url = url.clone();
        let packs = Arc::clone(&packs);

        let hdl: JoinHandle<Result<()>> = spawn(async move {
            let client = Client::new();
            let mut data = String::new();
            let line = format!("want {}\n", reference);
            let size = (line.len() as u16 + 4).to_be_bytes();
            let line = format!("{}{}", hex::encode(size), line);
            write!(data, "{line}")?;
            write!(data, "0000")?;
            writeln!(data, "0009done")?;

            let body = data.as_bytes().to_owned();
            let mut packfile = client
                .post(url)
                .header("Content-Type", "x-git-upload-pack-request")
                .body(body)
                .send()
                .await
                .context("sending git upload pack request")?
                .bytes()
                .await?;

            let mut packs = packs.lock().await;
            let _ = packfile.split_to(8);
            let packfile = PackFile::new(&reference, packfile).context("building packfile")?;
            packs.push(packfile);

            Ok(())
        });

        handles.push(hdl);
    }

    for hdl in handles {
        let _ = hdl.await.context("awaiting packfile fetch task")?;
    }

    let packs = Arc::try_unwrap(packs)
        .expect("cannot extract out packs")
        .into_inner();

    Ok(packs)
}

async fn ref_discovery(url: &str, client: &Client) -> Result<(String, Vec<String>)> {
    println!("Performing ref discovery for {url}");
    let url = format!("{url}/info/refs");
    let response = client
        .get(&url)
        .query(&[("service", "git-upload-pack")])
        .send()
        .await
        .context("initiating ref discovery")?;

    let status = response.status();
    let response = response
        .text()
        .await
        .context("converting ref discovery response to bytes")?;

    let refs: Vec<_> = response.split('\n').map(|r| r.to_string()).collect();
    validate_ref_header(&refs[0], status).context("validating ref discovery header")?;

    // This is dirty
    let mut discovered = Vec::new();
    let refs = &refs[1..];
    let head = &refs[0];
    let Some((head, rest)) = head[8..].split_once(' ') else {
        anyhow::bail!("this is not a reference");
    };

    anyhow::ensure!(rest.contains("HEAD"));
    for reference in refs[1..].into_iter() {
        // Encountered magic num, we're done
        if reference == "0000" {
            break;
        }

        let Some((reference, _)) = reference[4..].split_once(' ') else {
            anyhow::bail!("this is not a reference");
        };

        discovered.push(reference.to_owned());
    }

    Ok((head.to_string(), discovered))
}

fn validate_ref_header(header: &str, status: StatusCode) -> Result<()> {
    match status {
        StatusCode::OK | StatusCode::NOT_MODIFIED => {}
        _ => anyhow::bail!(format!("failed status code validation: {}", status)),
    }

    let re = regex::Regex::new(r"^[0-9a-f]{4}# service=git-upload-pack")
        .context("creating validation regex")?;

    if !re.is_match(&header) {
        anyhow::bail!("failed regex validation");
    }

    Ok(())
}

fn build_repository(head: String) -> Result<()> {
    let head = GitObject::load(&head).context("opening HEAD")?;
    let mut reader = BufReader::new(&head.content[..]);
    let mut content = String::new();
    reader.read_to_string(&mut content)?;

    let Some((tree, _)) = content.split_once('\n') else {
        anyhow::bail!("this is not the correct format...");
    };

    let Some((_, tree_hash)) = tree.split_once(' ') else {
        anyhow::bail!("expected to find a hash");
    };

    let root = std::env::current_dir()?;
    build_tree(&tree_hash, &root)?;

    Ok(())
}

fn build_tree(hash: &str, root: &PathBuf) -> Result<()> {
    let tree = GitObject::load(&hash).context("loading tree")?;
    let mut buf = BufReader::new(&tree.content[..]);

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

        let Some((_, name)) = file_info.split_once(' ') else {
            anyhow::bail!(format!("missing file mode and context: {file_info}"));
        };

        let path = root.join(name);
        let obj = GitObject::load(&hash).context("loading object in tree")?;
        if obj.obj_type == GitObjectType::Blob {
            let mut f = std::fs::File::create(&path)?;
            f.write_all(&obj.content)?;
        } else if obj.obj_type == GitObjectType::Tree {
            std::fs::create_dir_all(&path)?;
            build_tree(&hash, &path)?;
        }

        name_buf.clear();
    }

    Ok(())
}
