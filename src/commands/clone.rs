use anyhow::{Context, Result};
use reqwest::blocking::{Client, Response};
use reqwest::StatusCode;

pub(crate) fn invoke(url: String, _dst: String) -> Result<()> {
    let client = Client::new();
    let _refs = ref_discovery(&url, &client).context("ref discovery")?;
    // negotiation(&url, &client, refs).context("negotiation part")?;

    // TODO: Parse refs into PKT types

    Ok(())
}

fn _negotiation(url: &str, client: &Client, refs: Vec<String>) -> Result<()> {
    let url = format!("{url}/git-upload-pack");
    let test = "0a53e9ddeaddad63ad106860237bbf53411d11a7";
    println!("Length: {}", test.len());
    let mut data = Vec::new();
    data.extend(b"0049want ");
    data.extend(refs[0].as_bytes());
    data.push(0x0a);
    data.extend(b"0000");

    let response = client
        .post(&url)
        .header("Content-Type", "application/x-git-upload-pack-request")
        .header("Content-Length", data.len())
        .body(data)
        .send()
        .context("git upload pack request")?;

    println!("response: {response:?}");

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

    let raw_refs = validate_ref_discovery(response).context("ref discovery validation")?;

    Ok(vec![])
}

// TODO: Maybe Byte parse or somethign
// Each ref is preceded by 4 bytes detailing the length
// e.g. 003e | <some_hash>
// That's why it never worked...
fn validate_ref_discovery(response: Response) -> Result<Vec<String>> {
    match response.status() {
        StatusCode::OK | StatusCode::NOT_MODIFIED => {}
        _ => anyhow::bail!(format!(
            "failed status code validation: {}",
            response.status()
        )),
    }

    let re = regex::Regex::new(r"^[0-9a-f]{4}# service=git-upload-pack")
        .context("creating validation regex")?;

    let text = response.text().context("converting response to text")?;
    if !re.is_match(&text) {
        anyhow::bail!("failed regex validation");
    }

    if !text.ends_with("0000") {
        anyhow::bail!("missing pkt-line marker 0000");
    }

    let refs = text
        .split('\n')
        .map(|r| r.to_owned())
        .collect::<Vec<String>>();

    Ok(refs)
}
