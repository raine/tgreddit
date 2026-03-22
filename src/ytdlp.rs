use anyhow::{Context, Result};
use std::{
    ffi::OsString,
    fs,
    path::Path,
    sync::LazyLock,
};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tracing::{error, info};

use crate::types::*;

use regex::Regex;
use tempfile::TempDir;

fn make_ytdlp_args(output: &Path, url: &str) -> Vec<OsString> {
    vec![
        "--paths".into(),
        output.into(),
        "--output".into(),
        // To get telegram show correct aspect ratio for video, we need the dimensions and simplest
        // way to make that happens is have yt-dlp write them in the filename.
        "video_%(width)sx%(height)s.%(ext)s".into(),
        url.into(),
    ]
}

/// Downloads given url with yt-dlp and returns path to video
pub async fn download(url: &str) -> Result<(Video, TempDir)> {
    let tmp_dir = TempDir::with_prefix("tgreddit")?;
    let tmp_path = tmp_dir.path().to_owned();
    let ytdlp_args = make_ytdlp_args(&tmp_path, url);

    info!("running yt-dlp with arguments {:?}", ytdlp_args);
    let mut child = Command::new("yt-dlp")
        .args(&ytdlp_args)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .context("failed to run yt-dlp")?;

    let stdout = child.stdout.take().context("failed to capture yt-dlp stdout")?;
    let stderr = child.stderr.take().context("failed to capture yt-dlp stderr")?;

    // Stream stdout and stderr concurrently
    let stdout_task = tokio::spawn(async move {
        let mut lines = BufReader::new(stdout).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            info!("{line}");
        }
    });

    let stderr_task = tokio::spawn(async move {
        let mut lines = BufReader::new(stderr).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            error!("yt-dlp stderr: {line}");
        }
    });

    let status = child.wait().await.context("failed to wait for yt-dlp")?;
    let _ = tokio::join!(stdout_task, stderr_task);

    if !status.success() {
        anyhow::bail!("yt-dlp exited with status: {}", status);
    }

    // yt-dlp is expected to write a single file, which is the video, to tmp_path
    let video_path = fs::read_dir(&tmp_path)
        .context("could not read files in temp dir")?
        .filter_map(|de| de.ok())
        .map(|de| de.path())
        .next()
        .context("no video file found in temp dir")?;

    let dimensions = parse_dimensions_from_path(&video_path)
        .context("video filename should have dimensions")?;

    let video = Video {
        path: video_path,
        width: dimensions.0,
        height: dimensions.1,
    };

    Ok((video, tmp_dir))
}

static DIMENSIONS_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"_(?P<width>\d+)x(?P<height>\d+)\.").unwrap());

fn parse_dimensions_from_path(path: &Path) -> Option<(u16, u16)> {
    let path_str = path.to_string_lossy();
    let caps = DIMENSIONS_RE.captures(&path_str)?;
    let width = caps.name("width")?.as_str().parse::<u16>().ok()?;
    let height = caps.name("height")?.as_str().parse::<u16>().ok()?;

    Some((width, height))
}

#[cfg(test)]
mod tests {
    use super::parse_dimensions_from_path;
    use std::path::Path;

    #[test]
    fn test_parse_dimensions_from_path() {
        assert_eq!(
            parse_dimensions_from_path(Path::new("/foo/bar/video_1920x1080.mp4")),
            Some((1920, 1080))
        );

        assert_eq!(
            parse_dimensions_from_path(Path::new("/foo/bar/video_asdfax1080.mp4")),
            None,
        );
    }
}
