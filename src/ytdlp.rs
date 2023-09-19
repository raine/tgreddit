use anyhow::Result;
use duct::cmd;
use lazy_static::lazy_static;
use log::{error, info};
use std::{
    ffi::OsString,
    fs,
    io::{BufRead, BufReader},
    path::Path,
};

use crate::types::*;

use regex::Regex;
use tempdir::TempDir;

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
pub fn download(url: &str) -> Result<(Video, TempDir)> {
    let tmp_dir = TempDir::new("tgreddit")?;
    let tmp_path = tmp_dir.path();
    let ytdlp_args = make_ytdlp_args(tmp_dir.path(), url);

    info!("running yt-dlp with arguments {:?}", ytdlp_args);
    let duct_exp = cmd("yt-dlp", ytdlp_args).stderr_to_stdout();
    let reader = match duct_exp.reader() {
        Ok(child) => child,
        Err(err) => {
            error!("failed to run yt-dlp:\n{}", err);
            return Err(anyhow::anyhow!(err));
        }
    };

    let lines = BufReader::new(reader).lines();
    for line_result in lines {
        match line_result {
            Ok(line) => info!("{line}"),
            Err(_) => panic!("failed to read line"),
        }
    }

    // yt-dlp is expected to write a single file, which is the video, to tmp_path
    let video_path = fs::read_dir(tmp_path)
        .expect("could not read files in temp dir")
        .map(|de| de.unwrap().path())
        .next()
        .expect("video file in temp dir");

    let dimensions =
        parse_dimensions_from_path(&video_path).expect("video filename should have dimensions");

    let video = Video {
        path: video_path,
        width: dimensions.0,
        height: dimensions.1,
    };

    Ok((video, tmp_dir))
}

fn parse_dimensions_from_path(path: &Path) -> Option<(u16, u16)> {
    lazy_static! {
        static ref RE: Regex = Regex::new(r"_(?P<width>\d+)x(?P<height>\d+)\.").unwrap();
    }

    let path_str = path.to_string_lossy();
    let caps = RE.captures(&path_str)?;
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
