use std::borrow::Cow;
use std::collections::HashMap;
use std::path::PathBuf;

use anyhow::{Context, Result};
use teloxide::payloads::{SendMessageSetters, SendPhotoSetters, SendVideoSetters};
use teloxide::prelude::*;
use teloxide::types::{
    InlineKeyboardMarkup, InputFile, InputMedia, InputMediaPhoto, LinkPreviewOptions,
};
use tempfile::TempDir;
use tracing::*;

use crate::{download, messages, reddit, state::AppState, ytdlp};

pub async fn handle_new_post(app: &AppState, chat_id: i64, post: &reddit::Post) -> Result<()> {
    send_post(app, chat_id, post, None).await
}

pub async fn send_post(
    app: &AppState,
    chat_id: i64,
    post: &reddit::Post,
    keyboard: Option<InlineKeyboardMarkup>,
) -> Result<()> {
    info!("got new {post:#?}");
    let mut post = Cow::Borrowed(post);

    // Sometimes post_hint is not in top list response but exists when getting the link directly,
    // but not always
    // TODO: It appears that post with is_gallery=true will never have post_hint set
    if post.post_hint.is_none() {
        info!("post missing post_hint, getting link directly");
        post = Cow::Owned(reddit::get_link(&app.http, &post.id).await?);
    }

    match post.post_type {
        reddit::PostType::Image => handle_image(app, chat_id, &post, keyboard).await,
        reddit::PostType::Video => handle_video(app, chat_id, &post, keyboard).await,
        reddit::PostType::Link => handle_link(app, chat_id, &post, keyboard).await,
        reddit::PostType::SelfText => handle_self_post(app, chat_id, &post, keyboard).await,
        reddit::PostType::Gallery => handle_gallery(app, chat_id, &post, keyboard).await,
        // /r/bestof posts have no characteristics like post_hint that could be used to
        // determine them as a type of Link; as a workaround, post Unknown post types the same way
        // as a link
        reddit::PostType::Unknown => {
            warn!("unknown post type, post={post:?}");
            handle_link(app, chat_id, &post, keyboard).await
        }
    }
}

async fn handle_video(
    app: &AppState,
    chat_id: i64,
    post: &reddit::Post,
    keyboard: Option<InlineKeyboardMarkup>,
) -> Result<()> {
    // The temporary directory will be deleted when _tmp_dir is dropped
    let (video, _tmp_dir) = ytdlp::download(&post.url).await?;
    info!("got a video: {video:?}");
    let caption = messages::format_media_caption_html(post, app.config.links_base_url.as_deref());
    let mut req = app
        .tg
        .send_video(ChatId(chat_id), InputFile::file(&video.path))
        .parse_mode(teloxide::types::ParseMode::Html)
        .caption(&caption)
        .height(video.height.into())
        .width(video.width.into());
    if let Some(kb) = keyboard {
        req = req.reply_markup(kb);
    }
    req.await?;
    info!(
        "video uploaded post_id={} chat_id={chat_id} video={video:?}",
        post.id
    );
    Ok(())
}

async fn handle_image(
    app: &AppState,
    chat_id: i64,
    post: &reddit::Post,
    keyboard: Option<InlineKeyboardMarkup>,
) -> Result<()> {
    match download::download_url_to_tmp(&app.http, &post.url).await {
        Ok((path, _tmp_dir)) => {
            let caption =
                messages::format_media_caption_html(post, app.config.links_base_url.as_deref());
            let mut req = app
                .tg
                .send_photo(ChatId(chat_id), InputFile::file(path))
                .parse_mode(teloxide::types::ParseMode::Html)
                .caption(&caption);
            if let Some(kb) = keyboard {
                req = req.reply_markup(kb);
            }
            req.await?;
            info!("image uploaded post_id={} chat_id={chat_id}", post.id);
            Ok(())
        }
        Err(e) => {
            error!("failed to download image: {e}");
            Err(e)
        }
    }
}

async fn handle_link(
    app: &AppState,
    chat_id: i64,
    post: &reddit::Post,
    keyboard: Option<InlineKeyboardMarkup>,
) -> Result<()> {
    let message_html =
        messages::format_link_message_html(post, app.config.links_base_url.as_deref());
    let mut req = app
        .tg
        .send_message(ChatId(chat_id), message_html)
        .parse_mode(teloxide::types::ParseMode::Html);
    if let Some(kb) = keyboard {
        req = req.reply_markup(kb);
    }
    req.await?;
    info!("message sent post_id={} chat_id={chat_id}", post.id);
    Ok(())
}

async fn handle_self_post(
    app: &AppState,
    chat_id: i64,
    post: &reddit::Post,
    keyboard: Option<InlineKeyboardMarkup>,
) -> Result<()> {
    let message_html =
        messages::format_media_caption_html(post, app.config.links_base_url.as_deref());
    let mut req = app
        .tg
        .send_message(ChatId(chat_id), message_html)
        .parse_mode(teloxide::types::ParseMode::Html)
        .link_preview_options(LinkPreviewOptions {
            is_disabled: true,
            url: None,
            prefer_small_media: false,
            prefer_large_media: false,
            show_above_text: false,
        });
    if let Some(kb) = keyboard {
        req = req.reply_markup(kb);
    }
    req.await?;
    info!("message sent post_id={} chat_id={chat_id}", post.id);
    Ok(())
}

async fn handle_gallery(
    app: &AppState,
    chat_id: i64,
    post: &reddit::Post,
    keyboard: Option<InlineKeyboardMarkup>,
) -> Result<()> {
    let gallery_data_items = &post
        .gallery_data
        .as_ref()
        .context("expected gallery_data to exist in gallery post")?
        .items;
    let media_metadata_map = post
        .media_metadata
        .as_ref()
        .context("expected media_metadata to exist in gallery post")?;

    // Download all gallery images into a single temp directory
    let tmp_dir = TempDir::with_prefix("tgreddit-gallery")?;
    let mut downloaded: HashMap<String, PathBuf> = HashMap::new();
    for (id, media_metadata) in media_metadata_map {
        let s = &media_metadata.s;
        let url = &s.url.replace("&amp;", "&");
        info!("got media id={id} x={} y={} url={}", &s.x, &s.y, url);
        let path = download::download_url_to_dir(&app.http, url, tmp_dir.path()).await?;
        downloaded.insert(id.to_string(), path);
    }

    // Build media group in gallery_data order (which defines display order)
    let mut media_group = vec![];
    let mut first = true;
    for item in gallery_data_items {
        match downloaded.get(&item.media_id) {
            Some(image_path) => {
                let mut input_media_photo = InputMediaPhoto::new(InputFile::file(image_path));
                if first {
                    let caption = messages::format_media_caption_html(
                        post,
                        app.config.links_base_url.as_deref(),
                    );
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

    app.tg
        .send_media_group(ChatId(chat_id), media_group)
        .await?;
    info!("gallery uploaded post_id={} chat_id={chat_id}", post.id);

    // Media groups don't support inline keyboards, so send keyboard as a separate reply
    if let Some(kb) = keyboard {
        app.tg
            .send_message(ChatId(chat_id), "·")
            .reply_markup(kb)
            .await?;
    }

    Ok(())
}
