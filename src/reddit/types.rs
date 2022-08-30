use std::collections::HashMap;

use super::*;
use anyhow::{Context, Result};
use serde::{Deserialize, Deserializer};
use strum_macros::{Display, EnumString};
use url::Url;

#[derive(Display, Debug, Clone, PartialEq, Hash, Eq, Deserialize, Copy, EnumString)]
#[serde(rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
pub enum PostType {
    Image,
    Video,
    Link,
    SelfText,
    Gallery,
    Unknown,
}

#[derive(Display, Debug, Clone, PartialEq, Hash, Eq, Deserialize, Copy, EnumString)]
#[serde(rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
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

#[derive(Deserialize, Debug, Clone)]
pub struct GalleryDataItem {
    pub caption: Option<String>,
    pub media_id: String,
    pub id: u32,
}

#[derive(Deserialize, Debug, Clone)]
pub struct GalleryData {
    pub items: Vec<GalleryDataItem>,
}

#[derive(Deserialize, Debug, Clone)]
pub struct Media {
    pub x: u16,
    pub y: u16,
    #[serde(rename = "u")]
    pub url: String,
}

#[derive(Deserialize, Debug, Clone)]
pub struct MediaMetadata {
    pub status: String,
    pub e: String,
    #[serde(rename = "m")]
    pub mime: String,
    pub s: Media,
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
    pub is_gallery: Option<bool>,
    pub post_type: PostType,
    pub crosspost_parent_list: Option<Vec<Post>>,
    pub gallery_data: Option<GalleryData>,
    pub media_metadata: Option<HashMap<String, MediaMetadata>>,
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
            pub is_gallery: Option<bool>,
            pub crosspost_parent_list: Option<Vec<Post>>,
            pub gallery_data: Option<GalleryData>,
            pub media_metadata: Option<HashMap<String, MediaMetadata>>,
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
        } else if helper.is_gallery.unwrap_or(false) {
            PostType::Gallery
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
            is_gallery: helper.is_gallery,
            post_type,
            gallery_data: helper.gallery_data,
            media_metadata: helper.media_metadata,
        })
    }
}

impl Post {
    pub(crate) fn format_permalink_url(&self, base_url: Option<&str>) -> String {
        format_url_from_path(&self.permalink, base_url)
    }

    pub(crate) fn format_old_permalink_url(&self) -> String {
        to_old_reddit_url(&format_url_from_path(&self.permalink, None))
    }
}

#[derive(Deserialize, Debug)]
pub struct SubredditAboutResponse {
    pub data: SubredditAbout,
}

#[derive(Deserialize, Debug)]
pub struct SubredditAbout {
    pub display_name: String,
    pub display_name_prefixed: String,
}
