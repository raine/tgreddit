use crate::reddit::{PostType, TopPostsTimePeriod};
use std::path::PathBuf;

#[derive(Debug)]
pub struct Video {
    pub path: PathBuf,
    pub width: u16,
    pub height: u16,
}

#[derive(Debug, PartialEq, Eq)]
pub struct Subscription {
    pub id: i64,
    pub chat_id: i64,
    pub subreddit: String,
    pub limit: Option<u32>,
    pub time: Option<TopPostsTimePeriod>,
    pub filter: Option<PostType>,
    pub paused: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubscriptionArgs {
    pub subreddit: String,
    pub limit: Option<u32>,
    pub time: Option<TopPostsTimePeriod>,
    pub filter: Option<PostType>,
}
