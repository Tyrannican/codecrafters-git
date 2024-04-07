use std::collections::HashSet;

use anyhow::{Context, Result};
use reqwest::blocking::{Client, Response};
use reqwest::StatusCode;

pub(crate) fn invoke(url: String, _dst: String) -> Result<()> {
    let client = Client::new();
    let refs = ref_discovery(&url, &client).context("ref discovery")?;
    println!("Refs: {refs:?}");
    negotiation(&url, &client, refs).context("negotiation part")?;

    // TODO: Parse refs into PKT types

    Ok(())
}

fn negotiation(url: &str, client: &Client, refs: Vec<String>) -> Result<()> {
    let url = format!("{url}/git-upload-pack");
    let mut data = Vec::new();
    data.push(0x0a);
    for r in refs.into_iter() {
        data.extend(format!("0032want {r}\n").as_bytes());
    }

    data.extend(b"0000");

    println!("{}", String::from_utf8(data.clone())?);

    let response = client
        .post(&url)
        .header("Content-Type", "application/x-git-upload-pack-request")
        .body(data)
        .send()
        .context("sending negotiation")?;

    println!("Response: {response:?}");

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

    let ref_string = validate_ref_discovery(response).context("ref discovery validation")?;
    let refs = build_ref_request_list(ref_string);

    Ok(refs)
}

fn validate_ref_discovery(response: Response) -> Result<String> {
    match response.status() {
        StatusCode::OK | StatusCode::NOT_MODIFIED => {}
        _ => anyhow::bail!(format!(
            "failed status code validation: {}",
            response.status()
        )),
    }

    let re = regex::Regex::new(r"^[0-9a-f]{4}#").context("creating validation regex")?;
    let text = response.text().context("converting response to text")?;
    if !re.is_match(&text) {
        anyhow::bail!("failed regex validation");
    }

    if !text.ends_with("0000") {
        anyhow::bail!("missing pkt-line marker 0000");
    }

    Ok(text)
}

fn build_ref_request_list(ref_string: String) -> Vec<String> {
    let refs = ref_string
        .split('\n')
        .filter_map(|mut r| {
            if r.starts_with("0000") {
                r = &r[4..];
            }

            let Some((sha, _)) = r.split_once(' ') else {
                return None;
            };

            if r.is_empty() {
                return None;
            }

            Some(sha.to_owned())
        })
        .collect::<Vec<String>>();

    let refs = &refs[1..refs.len() - 1];

    refs.to_vec()
}
