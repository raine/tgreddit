use anyhow::{Context, Result};
use frankenstein::Api;
use log::*;
use seen_posts_cache::SeenPostsCache;
use signal_hook::{
    consts::signal::{SIGINT, SIGTERM},
    iterator::Signals,
};
use std::{
    fs::File,
    io,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc,
    },
    time::Duration,
};
use tempdir::TempDir;
use url::Url;

mod args;
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
    let tg_api = Api::new(&config.telegram_bot_token);
    info!("starting with config: {config:?}");

    // Any arguments are for things that help with debugging and development
    // Not optimized for usability.
    //
    // Usage: tgreddit --debug-post <linkid>                    => Fetch post and print deserialized post
    //        tgreddit --debug-post <linkid> --chat-id <chatid> => Also send to telegram
    let opts = args::parse_args();
    if let Some(post_id) = opts.opt_str("debug-post") {
        let post = reddit::get_link(&post_id).unwrap();
        info!("{:#?}", post);
        if let Some(chat_id) = opts.opt_str("chat-id") {
            return handle_new_post(&tg_api, chat_id.parse().unwrap(), &post);
        }
        return Ok(());
    }

    let mut seen_posts_cache = SeenPostsCache::new();

    let shutdown = Arc::new(AtomicBool::new(false));
    let (send, recv) = mpsc::channel();

    {
        let shutdown = shutdown.clone();
        std::thread::spawn(move || {
            let mut forward_signals =
                Signals::new(&[SIGINT, SIGTERM]).expect("Unable to watch for signals");

            for _signal in forward_signals.forever() {
                shutdown.swap(true, Ordering::Relaxed);
                send.send(()).unwrap();
            }
        });
    }

    while !shutdown.load(Ordering::Acquire) {
        for (chat_id, subreddits) in &config.channels {
            handle_channel_config(&config, &tg_api, &mut seen_posts_cache, chat_id, subreddits)
        }

        // Sleep that can be interrupted from the thread above
        let _r = recv.recv_timeout(Duration::from_secs(config.check_interval_secs));
    }

    Ok(())
}

fn handle_new_video_post(tg_api: &Api, chat_id: i64, post: &reddit::Post) -> Result<()> {
    match ytdlp::download(&post.format_permalink_url()) {
        Ok(video) => {
            info!("got a video: {video:?}");
            let caption = messages::format_media_caption_html(post);
            telegram::upload_video(tg_api, chat_id, &video, &caption).map(|_| ())?;
            info!(
                "video uploaded post_id={} chat_id={chat_id} video={video:?}",
                post.id
            );
            Ok(())
        }
        Err(e) => {
            error!("failed to download video: {e}");
            Err(e)
        }
    }
}

/// Downloads url to a file and returns the path along with handle to temp dir in which the file is.
/// Whe the temp dir value is dropped, the contents in file system are deleted.
fn download_url(url: &str) -> Result<(PathBuf, TempDir)> {
    info!("downloading {url}");
    let req = ureq::get(url);
    let res = req.call()?;
    let tmp_dir = TempDir::new("tgreddit")?;
    let mut reader = res.into_reader();
    let parsed_url = Url::parse(url)?;
    let tmp_filename = Path::new(parsed_url.path())
        .file_name()
        .context("could not get basename from url")?;
    let tmp_path = tmp_dir.path().join(&tmp_filename);
    let mut file = File::create(&tmp_path).unwrap();
    io::copy(&mut reader, &mut file)?;
    info!("downloaded {url} to {}", tmp_path.to_string_lossy());
    Ok((tmp_path, tmp_dir))
}

fn handle_new_image_post(tg_api: &Api, chat_id: i64, post: &reddit::Post) -> Result<()> {
    match download_url(&post.url) {
        Ok((path, _tmp_dir)) => {
            // path will be deleted when _tmp_dir when goes out of scope
            let caption = messages::format_media_caption_html(post);
            telegram::upload_image(tg_api, chat_id, path, &caption).map(|_| ())?;
            info!("image uploaded post_id={} chat_id={chat_id}", post.id);
            Ok(())
        }
        Err(e) => {
            error!("failed to download image: {e}");
            Err(e)
        }
    }
}

fn handle_new_link_post(tg_api: &Api, chat_id: i64, post: &reddit::Post) -> Result<()> {
    let message_html = messages::format_link_message_html(post);
    telegram::send_message(tg_api, chat_id, &message_html, false).map(|_| ())?;
    info!("message sent post_id={} chat_id={chat_id}", post.id);
    Ok(())
}

fn handle_new_self_post(tg_api: &Api, chat_id: i64, post: &reddit::Post) -> Result<()> {
    let message_html = messages::format_self_message_html(post);
    telegram::send_message(tg_api, chat_id, &message_html, true).map(|_| ())?;
    info!("message sent post_id={} chat_id={chat_id}", post.id);
    Ok(())
}

fn handle_new_post(tg_api: &Api, chat_id: i64, post: &reddit::Post) -> Result<()> {
    match &post.post_hint {
        None => {
            let post = reddit::get_link(&post.id).unwrap();
            match post.post_hint {
                Some(_) => handle_new_post(tg_api, chat_id, &post),
                None => {
                    warn!("post still missing post_hint even when queried directly, skipping");
                    Ok(())
                }
            }
        }
        Some(_) => {
            if post.is_downloadable_video() {
                handle_new_video_post(tg_api, chat_id, post)
            } else if post.is_image() {
                handle_new_image_post(tg_api, chat_id, post)
            } else if post.is_link() {
                handle_new_link_post(tg_api, chat_id, post)
            } else if post.is_self {
                handle_new_self_post(tg_api, chat_id, post)
            } else {
                warn!("don't know what to do with {post:?}");
                Ok(())
            }
        }
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
                debug!("got {} post(s) for subreddit /r/{subreddit}", posts.len());
                for post in posts {
                    debug!("got {post:?}");
                    if seen_posts_cache.is_seen_post(*chat_id, subreddit, &post.id) {
                        debug!("post already seen, skipping...");
                        continue;
                    }

                    // First run should not send anything to telegram but the post should be marked
                    // as seen, unless skip_initial_send is enabled
                    let should_skip_initial_send = seen_posts_cache
                        .is_uninitialized(*chat_id, subreddit)
                        && config.skip_initial_send;

                    if should_skip_initial_send {
                        seen_posts_cache.mark_seen(*chat_id, subreddit, &post.id);
                        continue;
                    }

                    match handle_new_post(tg_api, *chat_id, &post) {
                        Ok(_) => seen_posts_cache.mark_seen(*chat_id, subreddit, &post.id),
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
