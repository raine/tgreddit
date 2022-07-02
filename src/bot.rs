use crate::*;
use anyhow::Result;
use lazy_static::lazy_static;
use regex::Regex;
use std::sync::Arc;
use teloxide::{
    dispatching::DefaultKey,
    utils::command::{BotCommands, ParseError},
};

#[derive(BotCommands, Clone)]
#[command(rename = "lowercase", description = "These commands are supported:")]
pub enum Command {
    #[command(description = "display this text")]
    Help,
    #[command(
        description = "subscribe to subreddit's top posts",
        parse_with = "parse_subscribe_message"
    )]
    Sub(SubscriptionArgs),
    #[command(description = "unsubscribe from subreddit's top posts")]
    Unsub(String),
}

pub struct MyBot {
    pub dispatcher: Dispatcher<Arc<AutoSend<Bot>>, anyhow::Error, DefaultKey>,
    pub tg: Arc<AutoSend<Bot>>,
}

impl MyBot {
    pub async fn new(config: Arc<config::Config>) -> Result<Self> {
        let tg = Arc::new(Bot::new(config.telegram_bot_token.expose_secret()).auto_send());
        tg.set_my_commands(Command::bot_commands()).await?;

        let handler = Update::filter_message().branch(
            dptree::filter(|msg: Message, config: Arc<config::Config>| {
                msg.from()
                    .map(|user| config.authorized_user_ids.contains(&user.id.0))
                    .unwrap_or_default()
            })
            .filter_command::<Command>()
            .endpoint(handle_command),
        );

        let dispatcher = Dispatcher::builder(tg.clone(), handler)
            .dependencies(dptree::deps![config.clone()])
            .default_handler(|upd| async move {
                warn!("unhandled update: {:?}", upd);
            })
            .error_handler(LoggingErrorHandler::with_custom_text(
                "an error has occurred in the dispatcher",
            ))
            .build();

        let my_bot = MyBot {
            dispatcher,
            tg: tg.clone(),
        };
        Ok(my_bot)
    }

    pub fn spawn(
        mut self,
    ) -> (
        tokio::task::JoinHandle<()>,
        teloxide::dispatching::ShutdownToken,
    ) {
        let shutdown_token = self.dispatcher.shutdown_token();
        (
            tokio::spawn(async move { self.dispatcher.dispatch().await }),
            shutdown_token,
        )
    }
}

pub async fn handle_command(
    message: Message,
    tg: Arc<AutoSend<Bot>>,
    command: Command,
    config: Arc<config::Config>,
    _me: teloxide::types::Me,
) -> Result<()> {
    match command {
        Command::Help => {
            tg.send_message(message.chat.id, Command::descriptions().to_string())
                .await?;
        }
        Command::Sub(mut args) => {
            let db = db::Database::open(&config)?;
            let chat_id = message.chat.id.0;
            let subreddit_about = reddit::get_subreddit_about(&args.subreddit);
            match subreddit_about {
                Ok(data) => {
                    args.subreddit = data.display_name;
                    db.subscribe(chat_id, &args)?;
                    info!("subscribed in chat id {chat_id} with {args:#?};");
                    tg.send_message(
                        ChatId(chat_id),
                        format!("Subscribed to r/{}", args.subreddit),
                    )
                    .await?;
                }
                Err(reddit::SubredditAboutError::NoSuchSubreddit) => {
                    tg.send_message(ChatId(chat_id), "No such subreddit")
                        .await?;
                }
                Err(_) => {
                    tg.send_message(ChatId(chat_id), "Something went wrong")
                        .await?;
                }
            }
        }
        Command::Unsub(subreddit) => {
            let db = db::Database::open(&config)?;
            let chat_id = message.chat.id.0;
            let subreddit = subreddit.replace("r/", "");
            let reply = match db.unsubscribe(chat_id, &subreddit) {
                Ok(sub) => format!("Unsubscribed from r/{sub}"),
                Err(_) => format!("Error: Not subscribed to r/{subreddit}"),
            };
            tg.send_message(ChatId(chat_id), reply).await?;
        }
    };

    Ok(())
}

fn parse_subscribe_message(input: String) -> Result<(SubscriptionArgs,), ParseError> {
    lazy_static! {
        static ref SUBREDDIT_RE: Regex = Regex::new(r"^[^\s]+").unwrap();
        static ref LIMIT_RE: Regex = Regex::new(r"\blimit=(\d+)\b").unwrap();
        static ref TIME_RE: Regex = Regex::new(r"\btime=(\w+)\b").unwrap();
        static ref FILTER_RE: Regex = Regex::new(r"\bfilter=(\w+)\b").unwrap();
    }

    let subreddit_match = SUBREDDIT_RE
        .find(&input)
        .ok_or_else(|| ParseError::Custom("No subreddit given".into()))?;
    let subreddit = subreddit_match
        .as_str()
        .to_string()
        .replace("/r/", "")
        .replace("r/", "");
    let rest = &input[(subreddit_match.end())..];

    let limit: Option<u32> = LIMIT_RE
        .captures(rest)
        .and_then(|caps| caps.get(1))
        .and_then(|m| m.as_str().parse().ok());

    let time = Ok(TIME_RE.captures(rest))
        .map(|o| o.and_then(|caps| caps.get(1)))
        .and_then(|o| match o {
            Some(m) => m
                .as_str()
                .parse::<TopPostsTimePeriod>()
                .map(Some)
                .map_err(|e| ParseError::IncorrectFormat(e.into())),
            None => Ok(None),
        })?;

    let filter = Ok(FILTER_RE.captures(rest))
        .map(|o| o.and_then(|caps| caps.get(1)))
        .and_then(|o| match o {
            Some(m) => m
                .as_str()
                .parse::<PostType>()
                .map(Some)
                .map_err(|e| ParseError::IncorrectFormat(e.into())),
            None => Ok(None),
        })?;

    let args = SubscriptionArgs {
        subreddit,
        limit,
        time,
        filter,
    };

    Ok((args,))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_subscribe_message_only_subreddit() {
        let args = parse_subscribe_message("AnimalsBeingJerks".to_string()).unwrap();
        assert_eq!(
            args.0,
            SubscriptionArgs {
                subreddit: "AnimalsBeingJerks".to_string(),
                limit: None,
                time: None,
                filter: None,
            },
        )
    }

    #[test]
    fn test_parse_subscribe_message_strips_prefix() {
        let args = parse_subscribe_message("r/AnimalsBeingJerks".to_string()).unwrap();
        assert_eq!(
            args.0,
            SubscriptionArgs {
                subreddit: "AnimalsBeingJerks".to_string(),
                limit: None,
                time: None,
                filter: None,
            },
        );

        let args = parse_subscribe_message("/r/AnimalsBeingJerks".to_string()).unwrap();
        assert_eq!(
            args.0,
            SubscriptionArgs {
                subreddit: "AnimalsBeingJerks".to_string(),
                limit: None,
                time: None,
                filter: None,
            },
        )
    }

    #[test]
    fn test_parse_subscribe_message() {
        let args =
            parse_subscribe_message("AnimalsBeingJerks limit=5 time=week filter=video".to_string())
                .unwrap();
        assert_eq!(
            args.0,
            SubscriptionArgs {
                subreddit: "AnimalsBeingJerks".to_string(),
                limit: Some(5),
                time: Some(TopPostsTimePeriod::Week),
                filter: Some(PostType::Video),
            },
        )
    }
}
