use anyhow::Result;
use std::time::Duration;

use frankenstein::Api;
use log::{error, info, warn};
use seen_posts_cache::SeenPostsCache;

mod config;
mod messages;
mod reddit;
mod seen_posts_cache;
mod telegram;
mod types;
mod ytdlp;

fn main() -> Result<()> {
    env_logger::init();
    let config = config::read_config();
    let mut seen_posts_cache = SeenPostsCache::new();
    let tg_api = Api::new(&config.telegram_bot_token);

    loop {
        for (chat_id, subreddits) in &config.channels {
            handle_channel_config(&config, &tg_api, &mut seen_posts_cache, chat_id, subreddits)
        }

        std::thread::sleep(Duration::from_secs(config.check_interval_secs))
    }
}

fn handle_new_video_post(tg_api: &Api, chat_id: i64, post: &reddit::Post) -> Result<()> {
    match ytdlp::download(&post.format_permalink_url()) {
        Ok(video) => {
            info!("got a video: {video:?}");
            let caption = messages::format_video_caption_html(post);
            telegram::upload_video(tg_api, chat_id, &video, &caption).map(|_| ())
        }
        Err(e) => {
            error!("failed to download video: {e}");
            Err(e)
        }
    }
}

fn handle_new_post(tg_api: &Api, chat_id: i64, post: &reddit::Post) -> Result<()> {
    if post.is_video {
        handle_new_video_post(tg_api, chat_id, post)
    } else {
        warn!("post is not a video, not doing anything");
        Ok(())
    }
}

fn handle_channel_config(
    config: &config::Config,
    tg_api: &Api,
    seen_posts_cache: &mut SeenPostsCache,
    chat_id: &i64,
    subreddits: &[String],
) {
    for subreddit in subreddits {
        match reddit::get_subreddit_top_posts(subreddit, 1, reddit::TopPostsTimePeriod::Day) {
            Ok(posts) => {
                info!("got {} post(s) for subreddit /r/{subreddit}", posts.len());
                for post in posts {
                    info!("got {post:?}");
                    if seen_posts_cache.is_seen_post(*chat_id, &post.id) {
                        info!("post already seen, skipping...");
                        continue;
                    }

                    // First run should not send anything to telegram but the post should be marked
                    // as seen, unless skip_initial_send is enabled
                    let should_skip_initial_send = seen_posts_cache
                        .is_uninitialized(*chat_id, subreddit)
                        && config.skip_initial_send;

                    if should_skip_initial_send {
                        seen_posts_cache.mark_as_seen(*chat_id, subreddit, &post.id);
                        continue;
                    }

                    match handle_new_post(tg_api, *chat_id, &post) {
                        Ok(_) => seen_posts_cache.mark_as_seen(*chat_id, subreddit, &post.id),
                        Err(e) => error!("failed to handle new post: {e}"),
                    }
                }
            }
            Err(e) => {
                error!("failed to get posts for {subreddit}: {e}")
            }
        }
    }
}
