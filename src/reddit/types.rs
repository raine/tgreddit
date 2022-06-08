use super::*;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use url::Url;

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
pub struct ListingResponse {
    pub data: ListingResponseData,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct ListingResponseData {
    pub children: Vec<ListingItem>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct ListingItem {
    pub data: Post,
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
    pub post_hint: String,
}

impl Post {
    pub(crate) fn format_permalink_url(&self) -> String {
        format_url(&self.permalink)
    }

    pub fn is_downloadable_video(&self) -> bool {
        let is_downloadable_3rd_party = || -> Result<bool> {
            let url = Url::parse(&self.url)?;
            let host = url.host_str().context("no host in url")?;
            let path = url.path();
            let is_imgur_gif = host == "i.imgur.com" && path.ends_with(".gifv");
            let is_gfycat_gif = host == "gfycat.com";
            Ok(is_imgur_gif || is_gfycat_gif)
        };

        self.is_video || is_downloadable_3rd_party().unwrap_or(false)
    }

    pub fn is_image(&self) -> bool {
        self.post_hint == "image"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_downloadable_video() {
        let imgur_gifv = Post {
            id: "v6nu75".into(),
            created: 1654581100.0,
            post_hint: "link".into(),
            subreddit: "absoluteunit".into(),
            title: "Tipping a cow to trim its hooves".into(),
            is_video: false,
            ups: 469,
            permalink: "/r/absoluteunit/comments/v6nu75/tipping_a_cow_to_trim_its_hooves/".into(),
            url: "https://i.imgur.com/Zt6f5mB.gifv".into(),
        };

        assert!(imgur_gifv.is_downloadable_video());
    }

    #[test]
    fn is_image() {
        let post = Post {
            id: "v7i7os".into(),
            created: 1654667500.0,
            post_hint: "image".into(),
            subreddit: "absoluteunit".into(),
            title: "gigantic driftwood that washed ashore in Washington".into(),
            is_video: false,
            ups: 438,
            permalink: "/r/absoluteunit/comments/v7i7os/gigantic_driftwood_that_washed_ashore_in/"
                .into(),
            url: "https://i.redd.it/9x22l6lp0c491.jpg".into(),
        };

        assert!(post.is_image());
    }
}
