use crate::{download::*, types::*};
use anyhow::{Context, Result};
use log::*;
use reddit::{PostType, TopPostsTimePeriod};
use signal_hook::{
    consts::signal::{SIGINT, SIGTERM},
    iterator::Signals,
};
use std::collections::HashMap;
use std::string::ToString;
use std::{
    borrow::Cow,
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};
use teloxide::types::InputFile;
use teloxide::{
    payloads::{SendMessageSetters, SendPhotoSetters, SendVideoSetters},
    types::InputMediaPhoto,
};
use teloxide::{prelude::*, types::InputMedia};
use tempdir::TempDir;
use tokio::sync::broadcast;

mod args;
mod bot;
mod config;
mod db;
mod download;
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
        let post = reddit::get_link(&post_id).await.unwrap();
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
                Signals::new([SIGINT, SIGTERM]).expect("unable to watch for signals");

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

async fn handle_new_video_post(
    config: &config::Config,
    tg: &Bot,
    chat_id: i64,
    post: &reddit::Post,
) -> Result<()> {
    // The temporary directory will be deleted when _tmp_dir is dropped
    let (video, _tmp_dir) = tokio::task::block_in_place(|| ytdlp::download(&post.url))?;
    info!("got a video: {video:?}");
    let caption = messages::format_media_caption_html(post, config.links_base_url.as_deref());
    tg.send_video(ChatId(chat_id), InputFile::file(&video.path))
        .parse_mode(teloxide::types::ParseMode::Html)
        .caption(&caption)
        .height(video.height.into())
        .width(video.width.into())
        .await?;
    info!(
        "video uploaded post_id={} chat_id={chat_id} video={video:?}",
        post.id
    );
    Ok(())
}

async fn handle_new_image_post(
    config: &config::Config,
    tg: &Bot,
    chat_id: i64,
    post: &reddit::Post,
) -> Result<()> {
    match download_url_to_tmp(&post.url).await {
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
    tg: &Bot,
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
    tg: &Bot,
    chat_id: i64,
    post: &reddit::Post,
) -> Result<()> {
    let message_html = messages::format_media_caption_html(post, config.links_base_url.as_deref());
    tg.send_message(ChatId(chat_id), message_html)
        .parse_mode(teloxide::types::ParseMode::Html)
        .disable_web_page_preview(true)
        .await?;
    info!("message sent post_id={} chat_id={chat_id}", post.id);
    Ok(())
}

async fn download_gallery(post: &reddit::Post) -> Result<HashMap<String, (PathBuf, TempDir)>> {
    let media_metadata_map = post
        .media_metadata
        .as_ref()
        .expect("expected media_metadata to exist in gallery post");

    let mut map: HashMap<String, (PathBuf, TempDir)> = HashMap::new();
    for (id, media_metadata) in media_metadata_map {
        let s = &media_metadata.s;
        let url = &s.url.replace("&amp;", "&");
        info!("got media id={id} x={} y={} url={}", &s.x, &s.y, url);
        map.insert(id.to_string(), download_url_to_tmp(url).await?);
    }

    Ok(map)
}

async fn handle_new_gallery_post(
    config: &config::Config,
    tg: &Bot,
    chat_id: i64,
    post: &reddit::Post,
) -> Result<()> {
    // post.gallery_data is an array that describes the order of photos in the gallery, while
    // post.media_metadata is a map that contains the URL for each photo
    let gallery_data_items = &post
        .gallery_data
        .as_ref()
        .expect("expected media_metadata to exist in gallery post")
        .items;
    let gallery_files_map = download_gallery(post).await?;
    let mut media_group = vec![];
    let mut first = true;

    for item in gallery_data_items {
        let file = gallery_files_map.get(&item.media_id);
        match file {
            Some((image_path, _tempdir)) => {
                let mut input_media_photo = InputMediaPhoto::new(InputFile::file(image_path));
                // The first InputMediaPhoto in the vector needs to contain the caption and parse_mode;
                if first {
                    let caption =
                        messages::format_media_caption_html(post, config.links_base_url.as_deref());
                    input_media_photo = input_media_photo
                        .caption(&caption)
                        .parse_mode(teloxide::types::ParseMode::Html);
                    first = false;
                }

                media_group.push(InputMedia::Photo(input_media_photo))
            }
            None => {
                error!("could not find downloaded image for gallery data item: {item:?}");
            }
        }
    }

    tg.send_media_group(ChatId(chat_id), media_group).await?;
    info!("gallery uploaded post_id={} chat_id={chat_id}", post.id);

    Ok(())
}

async fn handle_new_post(
    config: &config::Config,
    tg: &Bot,
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
        post = Cow::Owned(reddit::get_link(&post.id).await.unwrap());
    }

    match post.post_type {
        reddit::PostType::Image => handle_new_image_post(config, tg, chat_id, &post).await,
        reddit::PostType::Video => handle_new_video_post(config, tg, chat_id, &post).await,
        reddit::PostType::Link => handle_new_link_post(config, tg, chat_id, &post).await,
        reddit::PostType::SelfText => handle_new_self_post(config, tg, chat_id, &post).await,
        reddit::PostType::Gallery => handle_new_gallery_post(config, tg, chat_id, &post).await,
        // /r/bestof posts have no characteristics like post_hint that could be used to
        // determine them as a type of Link; as a workaround, post Unknown post types the same way
        // as a link
        reddit::PostType::Unknown => {
            warn!("unknown post type, post={post:?}");
            handle_new_link_post(config, tg, chat_id, &post).await
        }
    }
}

async fn check_post_newness(
    config: &config::Config,
    tg: &Bot,
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

async fn check_new_posts(config: &config::Config, tg: &Bot) -> Result<()> {
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
    tg: &Bot,
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
    info!(
        "checking subreddit /r/{subreddit} for new posts for user {chat_id}",
        subreddit = subreddit,
        chat_id = chat_id
    );

    match reddit::get_subreddit_top_posts(subreddit, limit, &time).await {
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
