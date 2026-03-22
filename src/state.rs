use std::sync::{Arc, Mutex, MutexGuard};

use teloxide::prelude::*;

use crate::config::Config;
use crate::db::Database;

pub struct AppState {
    pub config: Arc<Config>,
    pub http: reqwest::Client,
    pub tg: Arc<Bot>,
    db: Mutex<Database>,
}

impl AppState {
    pub fn new(config: Arc<Config>, http: reqwest::Client, tg: Arc<Bot>, db: Database) -> Self {
        Self {
            config,
            http,
            tg,
            db: Mutex::new(db),
        }
    }

    pub fn db(&self) -> MutexGuard<'_, Database> {
        self.db.lock().expect("database mutex poisoned")
    }
}
