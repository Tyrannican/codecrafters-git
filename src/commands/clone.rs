use anyhow::{Context, Result};
use reqwest::blocking::{Client, Response};
use reqwest::StatusCode;

pub(crate) fn invoke(url: String, _dst: String) -> Result<()> {
    let client = Client::new();
    let _refs = ref_discovery(&url, &client).context("ref discovery")?;

    // TODO: Parse refs into PKT types

    Ok(())
}

fn ref_discovery(url: &str, client: &Client) -> Result<String> {
    println!("Performing ref discovery for {url}");
    let url = format!("{url}/info/refs");
    let response = client
        .get(&url)
        .query(&[("service", "git-upload-pack")])
        .send()
        .context("initiating ref discovery")?;

    let response = validate_ref_discovery(response).context("ref discovery validation")?;

    Ok(response)
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
