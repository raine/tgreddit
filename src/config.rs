use log::error;
use serde::{Deserialize, Deserializer};
use std::{collections::HashMap, env};

use crate::reddit::TopPostsTimePeriod;

const CONFIG_PATH_ENV: &str = "CONFIG_PATH";

#[derive(Deserialize, Debug)]
pub struct Config {
    pub telegram_bot_token: String,
    pub check_interval_secs: u64,
    pub skip_initial_send: bool,

    #[serde(deserialize_with = "deserialize_channel_config")]
    pub channels: ChannelsConfig,
}

#[derive(Deserialize, Debug)]
pub struct SubredditConfig {
    pub subreddit: String,
    pub limit: u32,
    pub time: TopPostsTimePeriod,
}

pub type ChannelsConfig = HashMap<i64, Vec<SubredditConfig>>;

fn deserialize_channel_config<'de, D>(deserializer: D) -> Result<ChannelsConfig, D::Error>
where
    D: Deserializer<'de>,
{
    let str_map: HashMap<&str, Vec<SubredditConfig>> = HashMap::deserialize(deserializer)?;
    Ok(str_map
        .into_iter()
        .map(|(key, val)| (key.parse::<i64>().unwrap(), val))
        .collect())
}

pub fn read_config() -> Config {
    env::var(&CONFIG_PATH_ENV)
        .map_err(|_| format!("{CONFIG_PATH_ENV} environment variable not set"))
        .and_then(|config_path| std::fs::read(config_path).map_err(|e| e.to_string()))
        .and_then(|bytes| toml::from_slice(&bytes).map_err(|e| e.to_string()))
        .unwrap_or_else(|err| {
            error!("failed to read config: {err}");
            std::process::exit(1);
        })
}
