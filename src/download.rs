use anyhow::{Context, Result};
use log::*;

use std::io::Write;
use std::{
    fs::File,
    path::{Path, PathBuf},
};
use tempdir::TempDir;
use url::Url;

/// Downloads url to a file and returns the path along with handle to temp dir in which the file is.
/// Whe the temp dir value is dropped, the contents in file system are deleted.
pub async fn download_url_to_tmp(url: &str) -> Result<(PathBuf, TempDir)> {
    info!("downloading {url}");
    let mut res = reqwest::get(url).await?;
    let tmp_dir = TempDir::new("tgreddit")?;
    let parsed_url = Url::parse(url)?;
    let tmp_filename = Path::new(parsed_url.path())
        .file_name()
        .context("could not get basename from url")?;
    let tmp_path = tmp_dir.path().join(tmp_filename);
    let mut file = File::create(&tmp_path)
        .map_err(|_| anyhow::anyhow!("failed to create file {:?}", tmp_path))?;

    while let Some(bytes) = res.chunk().await? {
        file.write(&bytes)
            .map_err(|_| anyhow::anyhow!("error writing to file {:?}", tmp_path))?;
    }

    info!("downloaded {url} to {}", tmp_path.to_string_lossy());
    Ok((tmp_path, tmp_dir))
}
