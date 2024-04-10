use anyhow::{Context, Result};
use reqwest::Client;
use reqwest::StatusCode;
use std::collections::HashSet;
use std::fmt::Write;
use tokio::io::AsyncRead;
use tokio::io::BufReader;

pub(crate) async fn invoke(url: String, _dst: String) -> Result<()> {
    let client = Client::new();
    let advertised = ref_discovery(&url, &client)
        .await
        .context("ref discovery")?;
    negotiation(&url, advertised, &client).await?;

    Ok(())
}

fn parse_packfile(pack: &[u8]) -> Result<()> {
    let mut reader = BufReader::new(pack);
    let total_objects = validate_header(&mut reader).context("validating packfile header")?;

    Ok(())
}

fn validate_header(reader: &mut BufReader<&[u8]>) -> Result<u32> {
    // TODO: Tokio-ify this
    //
    let mut check = [0; 4];
    //reader
    //    .read_exact(&mut check)
    //    .context("reading the packfile signature")?;
    //anyhow::ensure!(&check == b"PACK");
    //reader
    //    .read_exact(&mut check)
    //    .context("reading the packfile version")?;
    //anyhow::ensure!(u32::from_be_bytes(check) == 2);
    //reader
    //    .read_exact(&mut check)
    //    .context("reading the total objects in the pack")?;
    let total_objects = u32::from_be_bytes(check);

    Ok(total_objects)
}

// This is clone so we have nothing so omitting the have part
async fn negotiation(url: &str, advertised: Vec<String>, client: &Client) -> Result<()> {
    let url = format!("{url}/git-upload-pack");
    let want: HashSet<String> = advertised.into_iter().collect();

    for reference in want.iter() {
        let mut data = String::new();
        let line = format!("want {}\n", reference);
        let size = (line.len() as u16 + 4).to_be_bytes();
        let line = format!("{}{}", hex::encode(size), line);
        write!(data, "{line}")?;
        write!(data, "0000")?;
        writeln!(data, "0009done")?;

        let body = data.as_bytes().to_owned();
        let packfile = client
            .post(&url)
            .header("Content-Type", "x-git-upload-pack-request")
            .body(body)
            .send()
            .await
            .context("sending git upload pack request")?
            .bytes()
            .await?;

        parse_packfile(&packfile[8..]).context("packfile parsing")?;
        break;
    }

    Ok(())
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
