use log::error;
use secrecy::{ExposeSecret, Secret};
use serde::Deserialize;
use std::{env, path::PathBuf};

use crate::{
    reddit::{PostType, TopPostsTimePeriod},
    PKG_NAME,
};

const CONFIG_PATH_ENV: &str = "CONFIG_PATH";
pub const DEFAULT_LIMIT: u32 = 1;
pub const DEFAULT_TIME_PERIOD: TopPostsTimePeriod = TopPostsTimePeriod::Day;

#[derive(Debug, Deserialize)]
pub struct SecretString(Secret<String>);

impl SecretString {
    pub fn expose_secret(&self) -> &str {
        self.0.expose_secret()
    }
}

impl Default for SecretString {
    fn default() -> Self {
        SecretString(Secret::new("".to_string()))
    }
}

#[derive(Deserialize, Debug, Default)]
pub struct Config {
    pub authorized_user_ids: Vec<u64>,
    #[serde(default = "default_db_path")]
    pub db_path: PathBuf,
    pub telegram_bot_token: SecretString,
    pub check_interval_secs: u64,
    #[serde(default = "default_skip_initial_send")]
    pub skip_initial_send: bool,
    pub links_base_url: Option<String>,
    pub default_limit: Option<u32>,
    pub default_time: Option<TopPostsTimePeriod>,
    pub default_filter: Option<PostType>,
}

pub fn read_config() -> Config {
    env::var(CONFIG_PATH_ENV)
        .map_err(|_| format!("{CONFIG_PATH_ENV} environment variable not set"))
        .and_then(|config_path| std::fs::read(config_path).map_err(|e| e.to_string()))
        .and_then(|bytes| toml::from_slice(&bytes).map_err(|e| e.to_string()))
        .unwrap_or_else(|err| {
            error!("failed to read config: {err}");
            std::process::exit(1);
        })
}

fn default_db_path() -> PathBuf {
    let xdg_dirs = xdg::BaseDirectories::with_prefix(PKG_NAME).unwrap();
    xdg_dirs.place_state_file("data.db3").unwrap()
}

fn default_skip_initial_send() -> bool {
    true
}
