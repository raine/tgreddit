use std::collections::HashMap;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use teloxide::types::{InlineKeyboardButton, InlineKeyboardMarkup};

use crate::reddit::Post;

const SESSION_TTL: Duration = Duration::from_secs(600);

static SESSION_COUNTER: AtomicU64 = AtomicU64::new(0);

pub struct BrowseSession {
    pub posts: Vec<Post>,
    pub current_index: usize,
    pub chat_id: i64,
    created_at: Instant,
}

pub type BrowseSessions = Mutex<HashMap<String, BrowseSession>>;

impl BrowseSession {
    pub fn new(posts: Vec<Post>, chat_id: i64) -> Self {
        Self {
            posts,
            current_index: 0,
            chat_id,
            created_at: Instant::now(),
        }
    }

    pub fn is_expired(&self) -> bool {
        self.created_at.elapsed() > SESSION_TTL
    }

    pub fn has_next(&self) -> bool {
        self.current_index + 1 < self.posts.len()
    }

    pub fn total(&self) -> usize {
        self.posts.len()
    }

    pub fn position(&self) -> usize {
        self.current_index + 1
    }

    pub fn current_post(&self) -> &Post {
        &self.posts[self.current_index]
    }
}

pub fn generate_session_id() -> String {
    let count = SESSION_COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("s{count}")
}

pub fn build_keyboard(session_id: &str, session: &BrowseSession) -> InlineKeyboardMarkup {
    let counter = format!("{}/{}", session.position(), session.total());
    let mut buttons = vec![InlineKeyboardButton::callback(counter, "noop")];
    if session.has_next() {
        buttons.push(InlineKeyboardButton::callback(
            "Next ▶️",
            format!("gn:{session_id}"),
        ));
    }
    buttons.push(InlineKeyboardButton::callback(
        "Stop ⏹️",
        format!("gs:{session_id}"),
    ));
    InlineKeyboardMarkup::new(vec![buttons])
}

pub fn cleanup_expired(sessions: &BrowseSessions) {
    let mut map = sessions.lock().expect("browse sessions mutex poisoned");
    map.retain(|_, session| !session.is_expired());
}
