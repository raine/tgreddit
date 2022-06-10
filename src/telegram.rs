use std::path::PathBuf;

use anyhow::Result;
use frankenstein::{
    Api, InputFile, Message, MethodResponse, ParseMode, SendMessageParams, SendPhotoParams,
    SendVideoParams, TelegramApi,
};

use crate::types::*;

pub fn upload_video(
    tg_api: &Api,
    chat_id: i64,
    video: &Video,
    caption: &str,
) -> Result<MethodResponse<Message>> {
    let send_video_params = SendVideoParams::builder()
        .chat_id(chat_id)
        .video(InputFile {
            path: video.path.to_owned(),
        })
        .width(video.width)
        .height(video.height)
        .parse_mode(ParseMode::Html)
        .caption(caption)
        .supports_streaming(true)
        .build();

    tg_api
        .send_video(&send_video_params)
        .map_err(anyhow::Error::from)
}

pub fn upload_image(
    tg_api: &Api,
    chat_id: i64,
    photo: PathBuf,
    caption: &str,
) -> Result<MethodResponse<Message>> {
    let send_photo_params = SendPhotoParams::builder()
        .chat_id(chat_id)
        .photo(InputFile { path: photo })
        .parse_mode(ParseMode::Html)
        .caption(caption)
        .build();

    tg_api
        .send_photo(&send_photo_params)
        .map_err(anyhow::Error::from)
}

pub fn send_message(
    tg_api: &Api,
    chat_id: i64,
    message_html: &str,
    disable_web_page_preview: bool,
) -> Result<MethodResponse<Message>> {
    let send_message_params = SendMessageParams::builder()
        .chat_id(chat_id)
        .text(message_html)
        .parse_mode(ParseMode::Html)
        .disable_web_page_preview(disable_web_page_preview)
        .build();

    tg_api
        .send_message(&send_message_params)
        .map_err(anyhow::Error::from)
}
