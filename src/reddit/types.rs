use super::*;
use anyhow::{Context, Result};
use serde::de;
use serde::{Deserialize, Deserializer};
use url::Url;

#[derive(Debug, Clone, PartialEq, Hash, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PostType {
    Image,
    Video,
    Link,
    SelfText,
}

#[derive(Debug, Clone, PartialEq, Hash, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TopPostsTimePeriod {
    Hour,
    Day,
    Week,
    Month,
    Year,
    All,
}

#[derive(Deserialize, Debug)]
pub struct ListingResponse {
    pub data: ListingResponseData,
}

#[derive(Deserialize, Debug)]
pub struct ListingResponseData {
    pub children: Vec<ListingItem>,
}

#[derive(Deserialize, Debug)]
pub struct ListingItem {
    pub data: Post,
}

#[derive(Debug, Clone)]
pub struct Post {
    pub id: String,
    pub created: f32,
    pub subreddit: String,
    pub title: String,
    pub is_video: bool,
    pub ups: u32,
    pub permalink: String,
    pub url: String,
    pub post_hint: Option<String>,
    pub is_self: bool,
    pub post_type: PostType,
}

impl<'de> Deserialize<'de> for Post {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        pub struct PostHelper {
            pub id: String,
            pub created: f32,
            pub subreddit: String,
            pub title: String,
            pub is_video: bool,
            pub ups: u32,
            pub permalink: String,
            pub url: String,
            pub post_hint: Option<String>,
            pub is_self: bool,
        }

        impl PostHelper {
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
        }

        let helper = PostHelper::deserialize(deserializer)?;
        let post_hint = helper.post_hint.as_deref();
        let post_type = if helper.is_downloadable_video() {
            Ok(PostType::Video)
        } else if post_hint == Some("image") {
            Ok(PostType::Image)
        } else if post_hint == Some("link") {
            Ok(PostType::Link)
        } else if helper.is_self {
            Ok(PostType::SelfText)
        } else {
            Err(de::Error::custom("unknown post type"))
        }?;

        Ok(Post {
            id: helper.id,
            created: helper.created,
            subreddit: helper.subreddit,
            title: helper.title,
            is_video: helper.is_video,
            ups: helper.ups,
            permalink: helper.permalink,
            url: helper.url,
            post_hint: helper.post_hint,
            is_self: helper.is_self,
            post_type,
        })
    }
}

impl Post {
    pub(crate) fn format_permalink_url(&self) -> String {
        format_url(&self.permalink)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn video() {
        let post = Post {
            id: "v6nu75".into(),
            created: 1654581100.0,
            post_hint: Some("link".into()),
            subreddit: "absoluteunit".into(),
            title: "Tipping a cow to trim its hooves".into(),
            is_self: false,
            is_video: false,
            ups: 469,
            permalink: "/r/absoluteunit/comments/v6nu75/tipping_a_cow_to_trim_its_hooves/".into(),
            url: "https://i.imgur.com/Zt6f5mB.gifv".into(),
            post_type: PostType::Video,
        };

        assert_eq!(post.post_type, PostType::Video);
    }

    #[test]
    fn is_image() {
        let post = Post {
            id: "v7i7os".into(),
            created: 1654667500.0,
            post_hint: Some("image".into()),
            subreddit: "absoluteunit".into(),
            is_self: false,
            title: "gigantic driftwood that washed ashore in Washington".into(),
            is_video: false,
            ups: 438,
            permalink: "/r/absoluteunit/comments/v7i7os/gigantic_driftwood_that_washed_ashore_in/"
                .into(),
            url: "https://i.redd.it/9x22l6lp0c491.jpg".into(),
            post_type: PostType::Image,
        };

        assert_eq!(post.post_type, PostType::Image);
    }
}
