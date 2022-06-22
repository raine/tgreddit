use super::*;
use anyhow::{Context, Result};
use serde::{Deserialize, Deserializer};
use url::Url;

#[derive(Debug, Clone, PartialEq, Hash, Eq, Deserialize, Copy)]
#[serde(rename_all = "snake_case")]
pub enum PostType {
    Image,
    Video,
    Link,
    SelfText,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Hash, Eq, Deserialize, Copy)]
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
    pub crosspost_parent_list: Option<Vec<Post>>,
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
            pub crosspost_parent_list: Option<Vec<Post>>,
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

                // If the post is a crosspost with a video, it can be downloaded with post.url as
                // url as yt-dlp follows redirects
                let is_downloadable_crosspost = || -> bool {
                    self.crosspost_parent_list
                        .as_ref()
                        .map(|list| list.iter().any(|post| post.post_type == PostType::Video))
                        .unwrap_or(false)
                };

                self.is_video
                    || is_downloadable_crosspost()
                    || is_downloadable_3rd_party().unwrap_or(false)
            }
        }

        let helper = PostHelper::deserialize(deserializer)?;
        let post_hint = helper.post_hint.as_deref();
        let post_type = if helper.is_downloadable_video() {
            PostType::Video
        } else if post_hint == Some("image") {
            PostType::Image
        // post_hint => rich:video can be a link to a youtube video, which are not worthwhile to
        // download due to their length, though exceptions could be made for short (< 1min) videos
        } else if post_hint == Some("link") || post_hint == Some("rich:video") {
            PostType::Link
        } else if helper.is_self {
            PostType::SelfText
        } else {
            PostType::Unknown
        };

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
            crosspost_parent_list: helper.crosspost_parent_list,
            post_type,
        })
    }
}

impl Post {
    pub(crate) fn format_permalink_url(&self) -> String {
        format_url_from_path(&self.permalink)
    }

    pub(crate) fn format_old_permalink_url(&self) -> String {
        to_old_reddit_url(&format_url_from_path(&self.permalink))
    }
}
