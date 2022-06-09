use log::info;
use std::collections::{HashMap, HashSet};

use lru::LruCache;

const REMEMBERED_POSTS_COUNT: usize = 10;

#[derive(Debug)]
pub struct SeenPostsCache {
    /// Used to keep track of which post ids a telegram chat has seen for each subreddit.
    chats_subreddits_posts: HashMap<i64, HashMap<String, LruCache<String, bool>>>,

    /// Used to keep track of if a subreddit has been fetched once already. This is useful so that
    /// we can distinguish unseen post from the first post program sees, and not send all posts on
    /// program's first subreddit check.
    chats_subreddits_initialized: HashMap<i64, HashSet<String>>,
}

impl SeenPostsCache {
    pub(crate) fn new() -> SeenPostsCache {
        SeenPostsCache {
            chats_subreddits_posts: HashMap::new(),
            chats_subreddits_initialized: HashMap::new(),
        }
    }

    pub(crate) fn is_seen_post(&self, chat_id: i64, subreddit: &str, post_id: &str) -> bool {
        self.chats_subreddits_posts
            .get(&chat_id)
            .map_or(false, |subreddits_posts| {
                subreddits_posts
                    .get(subreddit)
                    .map_or(false, |posts| posts.contains(post_id))
            })
    }

    pub(crate) fn is_uninitialized(&self, chat_id: i64, subreddit: &str) -> bool {
        match self.chats_subreddits_initialized.get(&chat_id) {
            Some(subreddits) => !subreddits.contains(subreddit),
            None => true,
        }
    }

    pub(crate) fn mark_seen(&mut self, chat_id: i64, subreddit: &str, post_id: &str) {
        let subreddits_posts = match self.chats_subreddits_posts.get_mut(&chat_id) {
            Some(subreddits_posts) => subreddits_posts,
            None => {
                self.chats_subreddits_posts.insert(chat_id, HashMap::new());
                self.chats_subreddits_posts.get_mut(&chat_id).unwrap()
            }
        };

        let posts = match subreddits_posts.get_mut(subreddit) {
            Some(posts) => posts,
            None => {
                subreddits_posts
                    .insert(subreddit.to_owned(), LruCache::new(REMEMBERED_POSTS_COUNT));
                subreddits_posts.get_mut(subreddit).unwrap()
            }
        };

        posts.put(post_id.to_owned(), true);

        if self.chats_subreddits_initialized.get(&chat_id).is_none() {
            self.chats_subreddits_initialized
                .insert(chat_id, HashSet::new());
        }

        if let Some(subreddits) = self.chats_subreddits_initialized.get_mut(&chat_id) {
            subreddits.insert(subreddit.to_string());
        }

        info!("marked post id {post_id} as seen for chat {chat_id} and subreddit {subreddit}");
    }
}
