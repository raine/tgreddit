use std::collections::HashMap;

use super::*;
use anyhow::{Context, Result};
use serde::Deserialize;
use strum::{Display, EnumString};
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
    pub data: RawPost,
}

#[derive(Deserialize, Debug, Clone)]
pub struct GalleryDataItem {
    pub media_id: String,
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
    pub s: Media,
}

/// Raw Reddit API response for a post. Deserialized directly from JSON.
#[derive(Deserialize, Debug, Clone)]
pub struct RawPost {
    pub id: String,
    pub subreddit: String,
    pub title: String,
    pub is_video: bool,
    pub permalink: String,
    pub url: String,
    pub post_hint: Option<String>,
    pub is_self: bool,
    pub is_gallery: Option<bool>,
    pub crosspost_parent_list: Option<Vec<RawPost>>,
    pub gallery_data: Option<GalleryData>,
    pub media_metadata: Option<HashMap<String, MediaMetadata>>,
}

/// Domain model for a Reddit post with classified type.
#[derive(Debug, Clone)]
pub struct Post {
    pub id: String,
    pub subreddit: String,
    pub title: String,
    pub permalink: String,
    pub url: String,
    pub post_hint: Option<String>,
    pub post_type: PostType,
    pub gallery_data: Option<GalleryData>,
    pub media_metadata: Option<HashMap<String, MediaMetadata>>,
}

/// Classify a raw Reddit post into a PostType based on its fields.
pub fn classify_post(raw: &RawPost) -> PostType {
    if is_downloadable_video(raw) {
        PostType::Video
    } else if raw.post_hint.as_deref() == Some("image") {
        PostType::Image
    // post_hint => rich:video can be a link to a youtube video, which are not worthwhile to
    // download due to their length, though exceptions could be made for short (< 1min) videos
    } else if raw.post_hint.as_deref() == Some("link")
        || raw.post_hint.as_deref() == Some("rich:video")
    {
        PostType::Link
    } else if raw.is_self {
        PostType::SelfText
    } else if raw.is_gallery.unwrap_or(false) {
        PostType::Gallery
    } else {
        PostType::Unknown
    }
}

fn is_downloadable_video(raw: &RawPost) -> bool {
    let is_downloadable_3rd_party = || -> Result<bool> {
        let url = Url::parse(&raw.url)?;
        let host = url.host_str().context("no host in url")?;
        let path = url.path();
        let is_imgur_gif = host == "i.imgur.com" && path.ends_with(".gifv");
        let is_gfycat_gif = host == "gfycat.com";
        Ok(is_imgur_gif || is_gfycat_gif)
    };

    // If the post is a crosspost with a video, it can be downloaded with post.url as
    // url as yt-dlp follows redirects
    let is_downloadable_crosspost = || -> bool {
        raw.crosspost_parent_list
            .as_ref()
            .map(|list| {
                list.iter()
                    .any(|parent| classify_post(parent) == PostType::Video)
            })
            .unwrap_or(false)
    };

    raw.is_video || is_downloadable_crosspost() || is_downloadable_3rd_party().unwrap_or(false)
}

impl From<RawPost> for Post {
    fn from(raw: RawPost) -> Self {
        let post_type = classify_post(&raw);
        Post {
            id: raw.id,
            subreddit: raw.subreddit,
            title: raw.title,
            permalink: raw.permalink,
            url: raw.url,
            post_hint: raw.post_hint,
            post_type,
            gallery_data: raw.gallery_data,
            media_metadata: raw.media_metadata,
        }
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_classify_video() {
        let raw = RawPost {
            id: "abc".into(),
            subreddit: "test".into(),
            title: "test".into(),
            is_video: true,
            permalink: "/r/test".into(),
            url: "https://v.redd.it/abc".into(),
            post_hint: Some("hosted:video".into()),
            is_self: false,
            is_gallery: None,
            crosspost_parent_list: None,
            gallery_data: None,
            media_metadata: None,
        };
        assert_eq!(classify_post(&raw), PostType::Video);
    }

    #[test]
    fn test_classify_image() {
        let raw = RawPost {
            id: "abc".into(),
            subreddit: "test".into(),
            title: "test".into(),
            is_video: false,
            permalink: "/r/test".into(),
            url: "https://i.redd.it/abc.jpg".into(),
            post_hint: Some("image".into()),
            is_self: false,
            is_gallery: None,
            crosspost_parent_list: None,
            gallery_data: None,
            media_metadata: None,
        };
        assert_eq!(classify_post(&raw), PostType::Image);
    }

    #[test]
    fn test_classify_gallery() {
        let raw = RawPost {
            id: "abc".into(),
            subreddit: "test".into(),
            title: "test".into(),
            is_video: false,
            permalink: "/r/test".into(),
            url: "https://reddit.com/gallery/abc".into(),
            post_hint: None,
            is_self: false,
            is_gallery: Some(true),
            crosspost_parent_list: None,
            gallery_data: None,
            media_metadata: None,
        };
        assert_eq!(classify_post(&raw), PostType::Gallery);
    }

    #[test]
    fn test_classify_self() {
        let raw = RawPost {
            id: "abc".into(),
            subreddit: "test".into(),
            title: "test".into(),
            is_video: false,
            permalink: "/r/test".into(),
            url: "https://reddit.com/r/test/abc".into(),
            post_hint: None,
            is_self: true,
            is_gallery: None,
            crosspost_parent_list: None,
            gallery_data: None,
            media_metadata: None,
        };
        assert_eq!(classify_post(&raw), PostType::SelfText);
    }

    #[test]
    fn test_classify_unknown_falls_back() {
        let raw = RawPost {
            id: "abc".into(),
            subreddit: "test".into(),
            title: "test".into(),
            is_video: false,
            permalink: "/r/test".into(),
            url: "https://example.com".into(),
            post_hint: None,
            is_self: false,
            is_gallery: None,
            crosspost_parent_list: None,
            gallery_data: None,
            media_metadata: None,
        };
        assert_eq!(classify_post(&raw), PostType::Unknown);
    }

    #[test]
    fn test_classify_imgur_gifv_as_video() {
        let raw = RawPost {
            id: "abc".into(),
            subreddit: "test".into(),
            title: "test".into(),
            is_video: false,
            permalink: "/r/test".into(),
            url: "https://i.imgur.com/abc.gifv".into(),
            post_hint: Some("link".into()),
            is_self: false,
            is_gallery: None,
            crosspost_parent_list: None,
            gallery_data: None,
            media_metadata: None,
        };
        assert_eq!(classify_post(&raw), PostType::Video);
    }
}
