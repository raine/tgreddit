use log::info;
use std::collections::{HashMap, HashSet};

use lru::LruCache;

const REMEMBERED_POSTS_COUNT: usize = 10;

#[derive(Debug)]
pub struct SeenPostsCache {
    /// Used to keep track of which post ids a telegram channel has seen.
    channel_posts: HashMap<i64, LruCache<String, bool>>,

    /// Used to keep track of if a subreddit has been fetched once already. This is useful so that
    /// we can distinguish unseen post from the first post program sees, and not send all posts on
    /// program's first subreddit check.
    channel_subreddit_initialized: HashMap<i64, HashSet<String>>,
}

impl SeenPostsCache {
    pub(crate) fn new() -> SeenPostsCache {
        SeenPostsCache {
            channel_posts: HashMap::new(),
            channel_subreddit_initialized: HashMap::new(),
        }
    }

    pub(crate) fn is_seen_post(&self, chat_id: i64, post_id: &str) -> bool {
        match self.channel_posts.get(&chat_id) {
            Some(posts) => posts.contains(post_id),
            None => false,
        }
    }

    pub(crate) fn is_uninitialized(&self, chat_id: i64, subreddit: &str) -> bool {
        match self.channel_subreddit_initialized.get(&chat_id) {
            Some(subreddits) => !subreddits.contains(subreddit),
            None => true,
        }
    }

    pub(crate) fn mark_as_seen(&mut self, chat_id: i64, subreddit: &str, post_id: &str) {
        // Initialize empty set for channel's seen post ids
        if self.channel_posts.get(&chat_id).is_none() {
            self.channel_posts
                .insert(chat_id, LruCache::new(REMEMBERED_POSTS_COUNT));
        }

        if self.channel_subreddit_initialized.get(&chat_id).is_none() {
            self.channel_subreddit_initialized
                .insert(chat_id, HashSet::new());
        }

        if let Some(posts) = self.channel_posts.get_mut(&chat_id) {
            posts.put(post_id.to_owned(), true);
        }

        if let Some(subreddits) = self.channel_subreddit_initialized.get_mut(&chat_id) {
            subreddits.insert(subreddit.to_string());
        }

        info!("marked post id {post_id} as seen for chat {chat_id} and subreddit {subreddit}");
    }
}
