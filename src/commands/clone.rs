use anyhow::{Context, Result};
use reqwest::blocking::Client;
use reqwest::StatusCode;
use std::collections::HashSet;

pub(crate) fn invoke(url: String, _dst: String) -> Result<()> {
    let client = Client::new();
    let advertised = ref_discovery(&url, &client).context("ref discovery")?;
    negotiation(&url, advertised, &client)?;

    Ok(())
}

fn negotiation(url: &str, advertised: Vec<String>, client: &Client) -> Result<()> {
    let mut common: HashSet<String> = HashSet::default();
    let want: HashSet<String> = advertised.into_iter().collect();

    let url = format!("{url}/git-upload-pack");
    let mut data = vec![];
    for r in want.iter() {
        let content = format!("{}want {}\n", hex::encode(50_u16.to_be_bytes()), r);
        data.extend(content.bytes());
    }
    data.extend(b"00000009done\n");

    println!("{}", String::from_utf8(data.clone())?);

    let response = client
        .post(url)
        .header("Content-Type", "x-git-upload-pack-request")
        .body(data)
        .send()
        .context("sending the git upload pack request")?
        .text()
        .context("converting to human readable")?;

    //println!("Response: {response:?}");

    Ok(())
}

fn ref_discovery(url: &str, client: &Client) -> Result<Vec<String>> {
    println!("Performing ref discovery for {url}");
    let url = format!("{url}/info/refs");
    let response = client
        .get(&url)
        .query(&[("service", "git-upload-pack")])
        .send()
        .context("initiating ref discovery")?;

    let status = response.status();
    let response = response
        .text()
        .context("converting ref discovery response to bytes")?;

    println!("Response: {response}");
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

// TODO: Maybe Byte parse or somethign
// Each ref is preceded by 4 bytes detailing the length
// e.g. 003e | <some_hash>
// That's why it never worked...
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
