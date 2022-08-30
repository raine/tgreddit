use anyhow::{Context, Result};
use log::*;
use std::{
    fs::File,
    io,
    path::{Path, PathBuf},
};
use tempdir::TempDir;
use url::Url;

/// Downloads url to a file and returns the path along with handle to temp dir in which the file is.
/// Whe the temp dir value is dropped, the contents in file system are deleted.
pub fn download_url_to_tmp(url: &str) -> Result<(PathBuf, TempDir)> {
    info!("downloading {url}");
    let req = ureq::get(url);
    let res = req.call()?;
    let tmp_dir = TempDir::new("tgreddit")?;
    let mut reader = res.into_reader();
    let parsed_url = Url::parse(url)?;
    let tmp_filename = Path::new(parsed_url.path())
        .file_name()
        .context("could not get basename from url")?;
    let tmp_path = tmp_dir.path().join(&tmp_filename);
    let mut file = File::create(&tmp_path).unwrap();
    io::copy(&mut reader, &mut file)?;
    info!("downloaded {url} to {}", tmp_path.to_string_lossy());
    Ok((tmp_path, tmp_dir))
}
