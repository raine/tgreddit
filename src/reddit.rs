use cached::proc_macro::cached;
use log::info;
use serde::{Deserialize, Serialize};
use std::fmt::Debug;
use url::Url;

static REDDIT_BASE_URL: &str = "https://www.reddit.com";

fn get_base_url() -> Url {
    Url::parse(REDDIT_BASE_URL).unwrap()
}

fn format_url(path: &str) -> String {
    format!("{REDDIT_BASE_URL}{path}")
}

pub fn format_subreddit_url(subreddit: &str) -> String {
    format_url(&format!("/r/{subreddit}"))
}

#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq, Hash, Eq)]
pub enum TopPostsTimePeriod {
    Hour,
    Day,
    Week,
    Month,
    Year,
    All,
}

#[derive(Serialize, Deserialize, Debug)]
struct ListingResponse {
    data: ListingResponseData,
}

#[derive(Serialize, Deserialize, Debug)]
struct ListingResponseData {
    children: Vec<ListingItem>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct ListingItem {
    data: Post,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Post {
    pub id: String,
    pub created: f32,
    pub subreddit: String,
    pub title: String,
    pub is_video: bool,
    pub ups: u32,
    pub permalink: String,
    pub url: String,
}

impl Post {
    pub(crate) fn format_permalink_url(&self) -> String {
        format_url(&self.permalink)
    }
}

#[cached(
    result = true,
    time = 60,
    key = "String",
    convert = r#"{ format!("{}:{}:{:?}", subreddit, limit, time) }"#
)]
pub fn get_subreddit_top_posts(
    subreddit: &str,
    limit: u32,
    time: TopPostsTimePeriod,
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
