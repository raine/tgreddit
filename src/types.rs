use crate::reddit::{PostType, TopPostsTimePeriod};
use std::path::PathBuf;

#[derive(Debug)]
pub struct Video {
    pub path: PathBuf,
    pub width: u16,
    pub height: u16,
}

#[derive(Debug, PartialEq)]
pub struct Subscription {
    pub chat_id: i64,
    pub subreddit: String,
    pub limit: Option<u32>,
    pub time: Option<TopPostsTimePeriod>,
    pub filter: Option<PostType>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SubscriptionArgs {
    pub subreddit: String,
    pub limit: Option<u32>,
    pub time: Option<TopPostsTimePeriod>,
    pub filter: Option<PostType>,
}
