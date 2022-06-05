use anyhow::Result;
use frankenstein::{
    Api, InputFile, Message, MethodResponse, ParseMode, SendVideoParams, TelegramApi,
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
