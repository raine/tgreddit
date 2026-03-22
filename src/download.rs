use anyhow::{Context, Result};
use log::*;

use std::io::Write;
use std::{
    fs::File,
    path::{Path, PathBuf},
};
use tempfile::TempDir;
use url::Url;

/// Downloads url to a new temp directory. Returns the file path and the TempDir handle.
/// When the TempDir is dropped, the contents are deleted.
pub async fn download_url_to_tmp(
    client: &reqwest::Client,
    url: &str,
) -> Result<(PathBuf, TempDir)> {
    let tmp_dir = TempDir::with_prefix("tgreddit")?;
    let path = download_url_to_dir(client, url, tmp_dir.path()).await?;
    Ok((path, tmp_dir))
}

/// Downloads url to the given directory. Returns the file path.
pub async fn download_url_to_dir(
    client: &reqwest::Client,
    url: &str,
    dir: &Path,
) -> Result<PathBuf> {
    info!("downloading {url}");
    let mut res = client.get(url).send().await?;
    let parsed_url = Url::parse(url)?;
    let tmp_filename = Path::new(parsed_url.path())
        .file_name()
        .context("could not get basename from url")?;
    let file_path = dir.join(tmp_filename);
    let mut file = File::create(&file_path)
        .map_err(|_| anyhow::anyhow!("failed to create file {:?}", file_path))?;

    while let Some(bytes) = res.chunk().await? {
        file.write(&bytes)
            .map_err(|_| anyhow::anyhow!("error writing to file {:?}", file_path))?;
    }

    info!("downloaded {url} to {}", file_path.to_string_lossy());
    Ok(file_path)
}
