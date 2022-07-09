use crate::types::*;
use anyhow::{Context, Result};
use log::*;
use reddit::PostType;
use reddit::TopPostsTimePeriod;
use signal_hook::{
    consts::signal::{SIGINT, SIGTERM},
    iterator::Signals,
};
use std::string::ToString;
use std::{
    borrow::Cow,
    fs::File,
    io,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};
use teloxide::payloads::{SendMessageSetters, SendPhotoSetters};
use teloxide::prelude::*;
use teloxide::types::InputFile;
use tempdir::TempDir;
use tokio::sync::broadcast;
use url::Url;

mod args;
mod bot;
mod config;
mod db;
mod messages;
mod reddit;
mod types;
mod ytdlp;

const PKG_NAME: &str = env!("CARGO_PKG_NAME");

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();
    let config = Arc::new(config::read_config());
    info!("starting with config: {config:#?}");
    let mut db = db::Database::open(&config)?;
    db.migrate()?;
    drop(db);

    let (shutdown_tx, mut shutdown_rx) = broadcast::channel::<()>(1);
    let shutdown = Arc::new(AtomicBool::new(false));
    let bot = bot::MyBot::new(config.clone()).await?;

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
            return handle_new_post(&config, &bot.tg, chat_id.parse().unwrap(), &post).await;
        }
        return Ok(());
    }

    let sub_check_loop_handle = {
        let shutdown = shutdown.clone();
        let tg = bot.tg.clone();
        tokio::task::spawn(async move {
            while !shutdown.load(Ordering::Acquire) {
                check_new_posts(&config, &tg).await.unwrap_or_else(|err| {
                    error!("failed to check for new posts: {err}");
                });

                tokio::select! {
                   _ = tokio::time::sleep(Duration::from_secs(config.check_interval_secs)) => {}
                   _ = shutdown_rx.recv() => {
                       break
                   }
                }
            }
        })
    };
    let (bot_handle, bot_shutdown_token) = bot.spawn();

    {
        let shutdown = shutdown.clone();
        std::thread::spawn(move || {
            let mut forward_signals =
                Signals::new(&[SIGINT, SIGTERM]).expect("unable to watch for signals");

            for signal in forward_signals.forever() {
                info!("got signal {signal}, shutting down...");
                shutdown.swap(true, Ordering::Relaxed);
                let _res = bot_shutdown_token.shutdown();
                let _res = shutdown_tx.send(()).unwrap_or_else(|_| {
                    // Makes the second Ctrl-C exit instantly
                    std::process::exit(0);
                });
            }
        });
    }

    if let Err(err) = tokio::try_join!(bot_handle, sub_check_loop_handle) {
        panic!("{err}")
    }

    Ok(())
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

async fn handle_new_video_post(
    config: &config::Config,
    tg: &AutoSend<Bot>,
    chat_id: i64,
    post: &reddit::Post,
) -> Result<()> {
    let video = tokio::task::block_in_place(|| ytdlp::download(&post.url))?;
    info!("got a video: {video:?}");
    let caption = messages::format_media_caption_html(post, config.links_base_url.as_deref());
    tg.send_video(ChatId(chat_id), InputFile::file(&video.path))
        .parse_mode(teloxide::types::ParseMode::Html)
        .caption(&caption)
        .await?;
    info!(
        "video uploaded post_id={} chat_id={chat_id} video={video:?}",
        post.id
    );
    Ok(())
}

async fn handle_new_image_post(
    config: &config::Config,
    tg: &AutoSend<Bot>,
    chat_id: i64,
    post: &reddit::Post,
) -> Result<()> {
    match download_url(&post.url) {
        Ok((path, _tmp_dir)) => {
            // path will be deleted when _tmp_dir when goes out of scope
            let caption =
                messages::format_media_caption_html(post, config.links_base_url.as_deref());
            tg.send_photo(ChatId(chat_id), InputFile::file(path))
                .parse_mode(teloxide::types::ParseMode::Html)
                .caption(&caption)
                .await?;
            info!("image uploaded post_id={} chat_id={chat_id}", post.id);
            Ok(())
        }
        Err(e) => {
            error!("failed to download image: {e}");
            Err(e)
        }
    }
}

async fn handle_new_link_post(
    config: &config::Config,
    tg: &AutoSend<Bot>,
    chat_id: i64,
    post: &reddit::Post,
) -> Result<()> {
    let message_html = messages::format_link_message_html(post, config.links_base_url.as_deref());
    tg.send_message(ChatId(chat_id), message_html)
        .parse_mode(teloxide::types::ParseMode::Html)
        .disable_web_page_preview(false)
        .await?;
    info!("message sent post_id={} chat_id={chat_id}", post.id);
    Ok(())
}

async fn handle_new_self_post(
    config: &config::Config,
    tg: &AutoSend<Bot>,
    chat_id: i64,
    post: &reddit::Post,
) -> Result<()> {
    let message_html = messages::format_self_message_html(post, config.links_base_url.as_deref());
    tg.send_message(ChatId(chat_id), message_html)
        .parse_mode(teloxide::types::ParseMode::Html)
        .disable_web_page_preview(true)
        .await?;
    info!("message sent post_id={} chat_id={chat_id}", post.id);
    Ok(())
}

async fn handle_new_post(
    config: &config::Config,
    tg: &AutoSend<Bot>,
    chat_id: i64,
    post: &reddit::Post,
) -> Result<()> {
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
        reddit::PostType::Image => handle_new_image_post(config, tg, chat_id, &post).await,
        reddit::PostType::Video => handle_new_video_post(config, tg, chat_id, &post).await,
        reddit::PostType::Link => handle_new_link_post(config, tg, chat_id, &post).await,
        reddit::PostType::SelfText => handle_new_self_post(config, tg, chat_id, &post).await,
        reddit::PostType::Unknown => {
            warn!("unknown post type, skipping");
            Ok(())
        }
    }
}

async fn check_post_newness(
    config: &config::Config,
    tg: &AutoSend<Bot>,
    chat_id: i64,
    filter: Option<reddit::PostType>,
    post: &reddit::Post,
    only_mark_seen: bool,
) -> Result<()> {
    let db = db::Database::open(config)?;
    if filter.is_some() && filter.as_ref() != Some(&post.post_type) {
        debug!("filter set and post does not match filter, skipping");
        return Ok(());
    }

    if db
        .is_post_seen(chat_id, post)
        .expect("failed to query if post is seen")
    {
        debug!("post already seen, skipping...");
        return Ok(());
    }

    if !only_mark_seen {
        // Intentionally marking post as seen if handling it fails. It's preferable to not have it
        // fail continuously.
        if let Err(e) = handle_new_post(config, tg, chat_id, post).await {
            error!("failed to handle new post: {e}");
        }
    }

    db.mark_post_seen(chat_id, post)?;
    info!("marked post seen: {}", post.id);

    Ok(())
}

async fn check_new_posts(config: &config::Config, tg: &AutoSend<Bot>) -> Result<()> {
    info!("checking subscriptions for new posts");
    let db = db::Database::open(config)?;
    let subs = db.get_all_subscriptions()?;
    for sub in subs {
        check_new_posts_for_subscription(config, tg, &sub)
            .await
            .unwrap_or_else(|err| {
                error!("failed to check subscription for new posts: {err}");
            });
    }

    Ok(())
}

async fn check_new_posts_for_subscription(
    config: &config::Config,
    tg: &AutoSend<Bot>,
    sub: &Subscription,
) -> Result<()> {
    let db = db::Database::open(config)?;
    let subreddit = &sub.subreddit;
    let limit = sub
        .limit
        .or(config.default_limit)
        .unwrap_or(config::DEFAULT_LIMIT);
    let time = sub
        .time
        .or(config.default_time)
        .unwrap_or(config::DEFAULT_TIME_PERIOD);
    let filter = sub.filter.or(config.default_filter);
    let chat_id = sub.chat_id;

    match reddit::get_subreddit_top_posts(subreddit, limit, &time) {
        Ok(posts) => {
            debug!("got {} post(s) for subreddit /r/{}", posts.len(), subreddit);

            // First run should not send anything to telegram but the post should be marked
            // as seen, unless skip_initial_send is enabled
            let is_new_subreddit = !db
                .existing_posts_for_subreddit(chat_id, subreddit)
                .context("failed to query if subreddit has existing posts")?;
            let only_mark_seen = is_new_subreddit && config.skip_initial_send;

            for post in posts {
                debug!("got {post:?}");
                check_post_newness(config, tg, chat_id, filter, &post, only_mark_seen)
                    .await
                    .unwrap_or_else(|err| {
                        error!("failed to check post newness: {err}");
                    });
            }
        }
        Err(e) => {
            error!("failed to get posts for {}: {e}", subreddit)
        }
    };

    Ok(())
}
