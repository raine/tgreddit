use super::*;
use anyhow::{Context, Result};
use log::info;
use thiserror::Error;
use url::Url;

// NOTE: Using a blocking http client for historical reasons. Maybe use async one in future.

static REDDIT_BASE_URL: &str = "https://www.reddit.com";

fn get_base_url() -> Url {
    Url::parse(REDDIT_BASE_URL).unwrap()
}

pub fn format_url_from_path(path: &str) -> String {
    format!("{REDDIT_BASE_URL}{path}")
}

pub fn to_old_reddit_url(url: &str) -> String {
    // If this fails it's bug
    let mut url = Url::parse(url).unwrap();
    url.set_host(Some("old.reddit.com")).unwrap();
    url.to_string()
}

pub fn format_subreddit_url(subreddit: &str) -> String {
    format_url_from_path(&format!("/r/{subreddit}"))
}

pub fn get_subreddit_top_posts(
    subreddit: &str,
    limit: u32,
    time: &TopPostsTimePeriod,
) -> Result<Vec<Post>, ureq::Error> {
    info!("getting top posts for /r/{subreddit} limit={limit} time={time:?}");
    let mut url = get_base_url().join(&format!("/r/{subreddit}/top.json"))?;
    url.query_pairs_mut()
        .append_pair("limit", &limit.to_string())
        .append_pair("t", &format!("{:?}", time).to_lowercase());
    let req = ureq::get(&url.to_string());
    let res: ListingResponse = req.call()?.into_json()?;
    let posts = res.data.children.into_iter().map(|e| e.data).collect();
    Ok(posts)
}

pub fn get_link(link_id: &str) -> Result<Post> {
    info!("getting link id {link_id}");
    let mut url = get_base_url().join("/api/info.json")?;
    url.query_pairs_mut()
        .append_pair("id", &format!("t3_{link_id}"));
    let req = ureq::get(&url.to_string());
    let res: ListingResponse = req.call()?.into_json()?;
    res.data
        .children
        .into_iter()
        .map(|e| e.data)
        .next()
        .context("no post in response")
}

#[allow(clippy::large_enum_variant)]
#[derive(Error, Debug)]
pub enum SubredditAboutError {
    #[error("no such subreddit")]
    NoSuchSubreddit,
    #[error(transparent)]
    Ureq(#[from] ureq::Error),
    #[error(transparent)]
    UrlParseError(#[from] url::ParseError),
    #[error(transparent)]
    IO(#[from] std::io::Error),
}

pub fn get_subreddit_about(subreddit: &str) -> Result<SubredditAbout, SubredditAboutError> {
    info!("getting subreddit about for /r/{subreddit}");
    let agent = ureq::AgentBuilder::new().redirects(0).build();
    let url = get_base_url().join(&format!("/r/{subreddit}/about.json"))?;
    let req = agent.get(&url.to_string());
    let res = req.call()?;
    match res.status() {
        302 => Err(SubredditAboutError::NoSuchSubreddit),
        _ => {
            let data = res.into_json::<SubredditAboutResponse>()?.data;
            Ok(data)
        }
    }
}
