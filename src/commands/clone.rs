use anyhow::{Context, Result};
use reqwest::Client;
use reqwest::StatusCode;
use std::collections::HashSet;
use std::fmt::Write;
use std::path::Path;
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::task::{spawn, JoinHandle};

use crate::pack::PackFile;

pub(crate) async fn invoke(url: String, dst: Option<String>) -> Result<()> {
    if let Some(dst) = dst {
        create_destination(dst)
            .await
            .context("creating clone destination")?;
    }

    let client = Client::new();
    let advertised = ref_discovery(&url, &client)
        .await
        .context("ref discovery")?;
    let packs = fetch_refs(&url, advertised).await?;
    for mut pack in packs {
        pack.parse()?;
        break;
    }

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
    let want: HashSet<String> = advertised.into_iter().collect();
    let packs = Arc::new(Mutex::new(Vec::new()));
    let mut handles = vec![];

    println!("Downloading packs...");
    for reference in want.into_iter() {
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
            packs.push(PackFile::new(&reference, packfile));

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

async fn ref_discovery(url: &str, client: &Client) -> Result<Vec<String>> {
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
    let Some((head, _)) = head[8..].split_once(' ') else {
        anyhow::bail!("this is not a reference");
    };

    discovered.push(head.to_owned());
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

    Ok(discovered)
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
