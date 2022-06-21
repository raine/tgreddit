use anyhow::{Context, Result};
use frankenstein::Api;
use log::*;
use signal_hook::{
    consts::signal::{SIGINT, SIGTERM},
    iterator::Signals,
};
use std::{
    borrow::Cow,
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
mod db;
mod messages;
mod reddit;
mod telegram;
mod types;
mod ytdlp;

const PKG_NAME: &str = env!("CARGO_PKG_NAME");

fn main() -> Result<()> {
    env_logger::init();
    let config = config::read_config();
    let db = db::Database::open(&config)?;
    let tg_api = Api::new(config.telegram_bot_token.expose_secret());
    info!("starting with config: {config:#?}");

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
            check_new_posts_for_channel(&config, &db, &tg_api, *chat_id, subreddits)
        }

        // Sleep that can be interrupted from the thread above
        let _r = recv.recv_timeout(Duration::from_secs(config.check_interval_secs));
    }

    Ok(())
}

fn handle_new_video_post(tg_api: &Api, chat_id: i64, post: &reddit::Post) -> Result<()> {
    match ytdlp::download(&post.url) {
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
    info!("got new {post:#?}");
    let mut post = Cow::Borrowed(post);

    // Sometimes post_hint is not in top list response but exists when getting the link directly,
    // but not always
    // TODO: It appears that post with is_gallery=true will never have post_hint set
    if post.post_hint.is_none() {
        info!("post missing post_hint, getting like directly");
        post = Cow::Owned(reddit::get_link(&post.id).unwrap());
    }

    match post.post_type {
        reddit::PostType::Image => handle_new_image_post(tg_api, chat_id, &post),
        reddit::PostType::Video => handle_new_video_post(tg_api, chat_id, &post),
        reddit::PostType::Link => handle_new_link_post(tg_api, chat_id, &post),
        reddit::PostType::SelfText => handle_new_self_post(tg_api, chat_id, &post),
        reddit::PostType::Unknown => {
            warn!("unknown post type, skipping");
            Ok(())
        }
    }
}

fn check_post_newness(
    config: &config::Config,
    db: &db::Database,
    tg_api: &Api,
    chat_id: i64,
    filter: Option<reddit::PostType>,
    post: &reddit::Post,
) {
    if filter.is_some() && filter.as_ref() != Some(&post.post_type) {
        debug!("filter set and post does not match filter, skipping");
        return;
    }

    if db
        .is_post_seen(chat_id, post)
        .expect("failed to query if post is seen")
    {
        debug!("post already seen, skipping...");
        return;
    }

    // First run should not send anything to telegram but the post should be marked
    // as seen, unless skip_initial_send is enabled
    let is_new_subreddit = !db
        .existing_posts_for_subreddit(chat_id, &post.subreddit)
        .expect("failed to query if subreddit has existing posts");
    let only_mark_seen = is_new_subreddit && config.skip_initial_send;
    if !only_mark_seen {
        if let Err(e) = handle_new_post(tg_api, chat_id, post) {
            error!("failed to handle new post: {e}");
        }
    }

    db.mark_post_seen(chat_id, post)
        .expect("failed to mark post seen");
}

fn check_new_posts_for_subreddit(
    config: &config::Config,
    db: &db::Database,
    tg_api: &Api,
    chat_id: i64,
    subreddit_config: &config::SubredditConfig,
) {
    let subreddit = &subreddit_config.subreddit;
    let limit = subreddit_config
        .limit
        .or(config.default_limit)
        .unwrap_or(config::DEFAULT_LIMIT);
    let time = subreddit_config
        .time
        .or(config.default_time)
        .unwrap_or(config::DEFAULT_TIME_PERIOD);
    let filter = subreddit_config.filter.or(config.default_filter);

    match reddit::get_subreddit_top_posts(subreddit, limit, &time) {
        Ok(posts) => {
            debug!("got {} post(s) for subreddit /r/{}", posts.len(), subreddit);
            for post in posts {
                debug!("got {post:?}");
                check_post_newness(config, db, tg_api, chat_id, filter, &post)
            }
        }
        Err(e) => {
            error!("failed to get posts for {}: {e}", subreddit)
        }
    }
}

fn check_new_posts_for_channel(
    config: &config::Config,
    db: &db::Database,
    tg_api: &Api,
    chat_id: i64,
    subreddit_configs: &[config::SubredditConfig],
) {
    for subreddit_config in subreddit_configs {
        check_new_posts_for_subreddit(config, db, tg_api, chat_id, subreddit_config)
    }
}
