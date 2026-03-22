use crate::state::AppState;
use crate::*;
use anyhow::{Context, Result};
use regex::Regex;
use std::sync::{Arc, LazyLock};
use teloxide::{
    dispatching::DefaultKey,
    types::{
        CallbackQuery, InlineKeyboardButton, InlineKeyboardMarkup, MaybeInaccessibleMessage,
        MessageId,
    },
    utils::command::{BotCommands, ParseError},
};

#[derive(BotCommands, Clone)]
#[command(
    rename_rule = "lowercase",
    description = "These commands are supported:"
)]
pub enum Command {
    #[command(description = "display this text")]
    Help,
    #[command(
        description = "subscribe to subreddit's top posts",
        parse_with = parse_subscribe_message
    )]
    Sub(SubscriptionArgs),
    #[command(description = "unsubscribe from subreddit's top posts")]
    Unsub(String),
    #[command(description = "list subreddit subscriptions")]
    ListSubs,
    #[command(description = "get top posts", parse_with = parse_subscribe_message)]
    Get(SubscriptionArgs),
}

pub struct MyBot {
    pub dispatcher: Dispatcher<Arc<Bot>, anyhow::Error, DefaultKey>,
}

impl MyBot {
    pub fn new(tg: Arc<Bot>, app: Arc<AppState>) -> Self {
        let handler = dptree::entry()
            .branch(
                Update::filter_message().branch(
                    dptree::filter(|msg: Message, app: Arc<AppState>| {
                        msg.from
                            .as_ref()
                            .map(|user| app.config.authorized_user_ids.contains(&user.id.0))
                            .unwrap_or_default()
                    })
                    .filter_command::<Command>()
                    .endpoint(handle_command),
                ),
            )
            .branch(
                Update::filter_callback_query().branch(
                    dptree::filter(|q: CallbackQuery, app: Arc<AppState>| {
                        app.config.authorized_user_ids.contains(&q.from.id.0)
                    })
                    .endpoint(handle_callback_query),
                ),
            );

        let dispatcher = Dispatcher::builder(tg.clone(), handler)
            .dependencies(dptree::deps![app])
            .default_handler(|upd| async move {
                warn!("unhandled update: {:?}", upd);
            })
            .error_handler(LoggingErrorHandler::with_custom_text(
                "an error has occurred in the dispatcher",
            ))
            .build();

        MyBot { dispatcher }
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
    tg: Arc<Bot>,
    command: Command,
    app: Arc<AppState>,
) -> Result<()> {
    let result = match command {
        Command::Help => handle_help(&message, &tg).await,
        Command::Sub(args) => handle_sub(&message, &tg, &app, args).await,
        Command::Unsub(subreddit) => handle_unsub(&message, &tg, &app, subreddit).await,
        Command::ListSubs => handle_list_subs(&message, &tg, &app).await,
        Command::Get(args) => handle_get(&message, &tg, &app, args).await,
    };

    if let Err(err) = result {
        error!("failed to handle message: {}", err);
        tg.send_message(message.chat.id, "Something went wrong")
            .await?;
    }

    Ok(())
}

async fn handle_browse_next(
    q: &CallbackQuery,
    tg: &Bot,
    app: &AppState,
    session_id: &str,
) -> Result<()> {
    // Remove keyboard from the clicked message
    if let Some(MaybeInaccessibleMessage::Regular(msg)) = &q.message {
        let _ = tg.edit_message_reply_markup(msg.chat.id, msg.id).await;
    }

    browse::cleanup_expired(&app.browse_sessions);

    let next = {
        let mut sessions = app.browse_sessions.lock().unwrap();
        match sessions.get_mut(session_id) {
            Some(session) if !session.is_expired() && session.has_next() => {
                session.current_index += 1;
                let post = session.current_post().clone();
                let chat_id = session.chat_id;
                let keyboard = browse::build_keyboard(session_id, session);
                Some((post, chat_id, keyboard))
            }
            _ => {
                sessions.remove(session_id);
                None
            }
        }
    };

    match next {
        Some((post, chat_id, keyboard)) => {
            handlers::send_post(app, chat_id, &post, Some(keyboard)).await?;
        }
        None => {
            if let Some(MaybeInaccessibleMessage::Regular(msg)) = &q.message {
                tg.send_message(msg.chat.id, "Session expired, run /get again")
                    .await?;
            }
        }
    }

    Ok(())
}

async fn handle_browse_stop(
    q: &CallbackQuery,
    tg: &Bot,
    app: &AppState,
    session_id: &str,
) -> Result<()> {
    {
        app.browse_sessions.lock().unwrap().remove(session_id);
    }

    // Remove keyboard from the clicked message
    if let Some(MaybeInaccessibleMessage::Regular(msg)) = &q.message {
        let _ = tg.edit_message_reply_markup(msg.chat.id, msg.id).await;
    }

    Ok(())
}

async fn handle_help(message: &Message, tg: &Bot) -> Result<()> {
    tg.send_message(message.chat.id, Command::descriptions().to_string())
        .await?;
    Ok(())
}

async fn handle_sub(
    message: &Message,
    tg: &Bot,
    app: &AppState,
    mut args: SubscriptionArgs,
) -> Result<()> {
    let chat_id = message.chat.id.0;
    if args.subreddit.is_empty() {
        tg.send_message(ChatId(chat_id), "Usage: /sub <subreddit>")
            .await?;
        return Ok(());
    }
    let subreddit_about = reddit::get_subreddit_about(&args.subreddit).await;
    match subreddit_about {
        Ok(data) => {
            args.subreddit = data.display_name;
            let sub_id = app.db().subscribe(chat_id, &args)?;
            info!("subscribed in chat id {chat_id} with {args:#?};");

            let wizard_mode = args.limit.is_none() && args.time.is_none() && args.filter.is_none();
            if wizard_mode {
                let sub = app
                    .db()
                    .get_subscription_by_id(sub_id)?
                    .context("subscription just created not found")?;
                let keyboard = build_subscription_edit_keyboard(&sub);
                tg.send_message(
                    ChatId(chat_id),
                    format!("Subscribed to r/{}", args.subreddit),
                )
                .reply_markup(keyboard)
                .await?;
            } else {
                tg.send_message(
                    ChatId(chat_id),
                    format!("Subscribed to r/{}", args.subreddit),
                )
                .await?;
            }
        }
        Err(reddit::SubredditAboutError::NoSuchSubreddit) => {
            tg.send_message(ChatId(chat_id), "No such subreddit")
                .await?;
        }
        Err(err) => {
            Err(err)?;
        }
    }
    Ok(())
}

async fn handle_unsub(
    message: &Message,
    tg: &Bot,
    app: &AppState,
    subreddit: String,
) -> Result<()> {
    let chat_id = message.chat.id.0;
    let subreddit = subreddit.replace("r/", "");
    let reply = match app.db().unsubscribe(chat_id, &subreddit) {
        Ok(sub) => format!("Unsubscribed from r/{sub}"),
        Err(_) => format!("Error: Not subscribed to r/{subreddit}"),
    };
    tg.send_message(ChatId(chat_id), reply).await?;
    Ok(())
}

async fn handle_list_subs(message: &Message, tg: &Bot, app: &AppState) -> Result<()> {
    let subs = app.db().get_subscriptions_for_chat(message.chat.id.0)?;
    if subs.is_empty() {
        tg.send_message(message.chat.id, "No subscriptions").await?;
    } else {
        let keyboard = build_subscription_list_keyboard(&subs);
        tg.send_message(message.chat.id, "Your subscriptions:")
            .reply_markup(keyboard)
            .await?;
    }
    Ok(())
}

async fn handle_get(
    message: &Message,
    tg: &Bot,
    app: &AppState,
    args: SubscriptionArgs,
) -> Result<()> {
    if args.subreddit.is_empty() {
        tg.send_message(message.chat.id, "Usage: /get <subreddit>")
            .await?;
        return Ok(());
    }
    let subreddit = &args.subreddit;
    let limit = args
        .limit
        .or(app.config.default_limit)
        .unwrap_or(config::DEFAULT_LIMIT);
    let time = args
        .time
        .or(app.config.default_time)
        .unwrap_or(config::DEFAULT_TIME_PERIOD);
    let filter = args.filter.or(app.config.default_filter);
    let chat_id = message.chat.id.0;

    let posts = reddit::get_subreddit_top_posts(&app.http, subreddit, limit, &time)
        .await
        .context("failed to get posts")?
        .into_iter()
        .filter(|p| {
            if filter.is_some() {
                filter.as_ref() == Some(&p.post_type)
            } else {
                true
            }
        })
        .collect::<Vec<_>>();

    debug!("got {} post(s) for subreddit /r/{}", posts.len(), subreddit);

    if posts.is_empty() {
        tg.send_message(message.chat.id, "No posts found").await?;
        return Ok(());
    }

    // Single post: send directly without browse UI
    if posts.len() == 1 {
        if let Err(e) = handlers::handle_new_post(app, chat_id, &posts[0]).await {
            error!("failed to handle new post: {e}");
        }
        return Ok(());
    }

    // Multiple posts: create browse session and send first post with navigation
    let session_id = browse::generate_session_id();
    let session = browse::BrowseSession::new(posts, chat_id);
    let post = session.current_post().clone();
    let keyboard = browse::build_keyboard(&session_id, &session);
    app.browse_sessions
        .lock()
        .unwrap()
        .insert(session_id, session);
    browse::cleanup_expired(&app.browse_sessions);

    if let Err(e) = handlers::send_post(app, chat_id, &post, Some(keyboard)).await {
        error!("failed to handle new post: {e}");
    }

    Ok(())
}

static SUBREDDIT_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^[^\s]+").unwrap());
static LIMIT_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\blimit=(\d+)\b").unwrap());
static TIME_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\btime=(\w+)\b").unwrap());
static FILTER_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\bfilter=(\w+)\b").unwrap());

fn parse_subscribe_message(input: String) -> Result<(SubscriptionArgs,), ParseError> {
    if input.trim().is_empty() {
        return Ok((SubscriptionArgs {
            subreddit: String::new(),
            limit: None,
            time: None,
            filter: None,
        },));
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

// --- Callback query handling ---

pub async fn handle_callback_query(
    q: CallbackQuery,
    tg: Arc<Bot>,
    app: Arc<AppState>,
) -> Result<()> {
    // Clone id so q remains fully usable for browse handlers that need &q
    tg.answer_callback_query(q.id.clone()).await?;

    let data = q.data.as_deref().unwrap_or("");

    // Browse session callbacks — handle before extracting msg/chat_id since
    // browse handlers access q.message via MaybeInaccessibleMessage matching
    if data == "noop" {
        return Ok(());
    }
    if let Some(session_id) = data.strip_prefix("gn:") {
        let result = handle_browse_next(&q, &tg, &app, session_id).await;
        if let Err(err) = result {
            error!("failed to handle browse next: {err}");
            if let Some(MaybeInaccessibleMessage::Regular(msg)) = &q.message {
                tg.send_message(msg.chat.id, "Something went wrong").await?;
            }
        }
        return Ok(());
    }
    if let Some(session_id) = data.strip_prefix("gs:") {
        if let Err(err) = handle_browse_stop(&q, &tg, &app, session_id).await {
            error!("failed to handle browse stop: {err}");
        }
        return Ok(());
    }

    // Subscription callbacks
    let msg = q.message.as_ref().context("no message in callback query")?;
    let chat_id = msg.chat().id;
    let msg_id = msg.id();

    let result = if data == "sub_list" {
        show_subscription_list(&tg, chat_id, msg_id, &app).await
    } else if let Some(id) = parse_cb_id(data, "sub_edit:") {
        handle_sub_edit(&tg, chat_id, msg_id, &app, id).await
    } else if let Some(id) = parse_cb_id(data, "sub_limit:") {
        handle_sub_cycle(&tg, chat_id, msg_id, &app, id, SubField::Limit).await
    } else if let Some(id) = parse_cb_id(data, "sub_time:") {
        handle_sub_cycle(&tg, chat_id, msg_id, &app, id, SubField::Time).await
    } else if let Some(id) = parse_cb_id(data, "sub_filter:") {
        handle_sub_cycle(&tg, chat_id, msg_id, &app, id, SubField::Filter).await
    } else if let Some(id) = parse_cb_id(data, "sub_pause:") {
        handle_sub_pause(&tg, chat_id, msg_id, &app, id).await
    } else if let Some(id) = parse_cb_id(data, "sub_del:") {
        handle_sub_del(&tg, chat_id, msg_id, &app, id).await
    } else {
        warn!("unknown callback data: {data}");
        Ok(())
    };

    if let Err(err) = result {
        error!("failed to handle callback query: {err}");
    }

    Ok(())
}

fn parse_cb_id(data: &str, prefix: &str) -> Option<i64> {
    data.strip_prefix(prefix).and_then(|s| s.parse().ok())
}

enum SubField {
    Limit,
    Time,
    Filter,
}

async fn show_subscription_list(
    tg: &Bot,
    chat_id: ChatId,
    msg_id: MessageId,
    app: &AppState,
) -> Result<()> {
    let subs = app.db().get_subscriptions_for_chat(chat_id.0)?;
    if subs.is_empty() {
        tg.edit_message_text(chat_id, msg_id, "No subscriptions")
            .reply_markup(InlineKeyboardMarkup {
                inline_keyboard: vec![],
            })
            .await?;
    } else {
        let keyboard = build_subscription_list_keyboard(&subs);
        tg.edit_message_text(chat_id, msg_id, "Your subscriptions:")
            .reply_markup(keyboard)
            .await?;
    }
    Ok(())
}

async fn handle_sub_edit(
    tg: &Bot,
    chat_id: ChatId,
    msg_id: MessageId,
    app: &AppState,
    id: i64,
) -> Result<()> {
    // Bind DB result before any .await to drop the MutexGuard
    let sub_opt = app.db().get_subscription_by_id(id)?;
    let sub = match sub_opt {
        Some(sub) if sub.chat_id == chat_id.0 => sub,
        _ => return show_subscription_list(tg, chat_id, msg_id, app).await,
    };
    let keyboard = build_subscription_edit_keyboard(&sub);
    tg.edit_message_text(chat_id, msg_id, format!("r/{}", sub.subreddit))
        .reply_markup(keyboard)
        .await?;
    Ok(())
}

async fn handle_sub_cycle(
    tg: &Bot,
    chat_id: ChatId,
    msg_id: MessageId,
    app: &AppState,
    id: i64,
    field: SubField,
) -> Result<()> {
    let sub_opt = app.db().get_subscription_by_id(id)?;
    let mut sub = match sub_opt {
        Some(sub) if sub.chat_id == chat_id.0 => sub,
        _ => return show_subscription_list(tg, chat_id, msg_id, app).await,
    };

    {
        let db = app.db();
        match field {
            SubField::Limit => {
                let default = app.config.default_limit.unwrap_or(config::DEFAULT_LIMIT);
                let new_limit = next_limit(sub.limit, default);
                db.set_subscription_limit(id, Some(new_limit))?;
                sub.limit = Some(new_limit);
            }
            SubField::Time => {
                let default = app
                    .config
                    .default_time
                    .unwrap_or(config::DEFAULT_TIME_PERIOD);
                let new_time = next_time(sub.time, default);
                db.set_subscription_time(id, Some(new_time))?;
                sub.time = Some(new_time);
            }
            SubField::Filter => {
                let new_filter = next_filter(sub.filter);
                db.set_subscription_filter(id, new_filter)?;
                sub.filter = new_filter;
            }
        }
    }

    let keyboard = build_subscription_edit_keyboard(&sub);
    tg.edit_message_reply_markup(chat_id, msg_id)
        .reply_markup(keyboard)
        .await?;
    Ok(())
}

async fn handle_sub_pause(
    tg: &Bot,
    chat_id: ChatId,
    msg_id: MessageId,
    app: &AppState,
    id: i64,
) -> Result<()> {
    let sub_opt = app.db().get_subscription_by_id(id)?;
    let mut sub = match sub_opt {
        Some(sub) if sub.chat_id == chat_id.0 => sub,
        _ => return show_subscription_list(tg, chat_id, msg_id, app).await,
    };
    let new_paused = app.db().toggle_subscription_pause(id)?;
    sub.paused = new_paused;
    let keyboard = build_subscription_edit_keyboard(&sub);
    tg.edit_message_reply_markup(chat_id, msg_id)
        .reply_markup(keyboard)
        .await?;
    Ok(())
}

async fn handle_sub_del(
    tg: &Bot,
    chat_id: ChatId,
    msg_id: MessageId,
    app: &AppState,
    id: i64,
) -> Result<()> {
    let sub_opt = app.db().get_subscription_by_id(id)?;
    if let Some(sub) = sub_opt
        && sub.chat_id == chat_id.0
    {
        app.db().unsubscribe_by_id(id)?;
    }
    show_subscription_list(tg, chat_id, msg_id, app).await
}

fn build_subscription_list_keyboard(subs: &[Subscription]) -> InlineKeyboardMarkup {
    let buttons: Vec<Vec<InlineKeyboardButton>> = subs
        .iter()
        .map(|sub| {
            let mut label = format!("r/{}", sub.subreddit);
            if sub.paused {
                label.push_str(" ⏸\u{fe0f}");
            }
            let mut params = vec![];
            if let Some(time) = sub.time {
                params.push(format!("{time}"));
            }
            if let Some(limit) = sub.limit {
                params.push(format!("{limit}"));
            }
            if !params.is_empty() {
                label.push_str(&format!(" ({})", params.join(", ")));
            }
            vec![InlineKeyboardButton::callback(
                label,
                format!("sub_edit:{}", sub.id),
            )]
        })
        .collect();
    InlineKeyboardMarkup::new(buttons)
}

fn build_subscription_edit_keyboard(sub: &Subscription) -> InlineKeyboardMarkup {
    let limit_label = match sub.limit {
        Some(l) => format!("Limit: {l} \u{1f504}"),
        None => "Limit: default \u{1f504}".to_string(),
    };
    let time_label = match sub.time {
        Some(t) => format!("Time: {t} \u{1f504}"),
        None => "Time: default \u{1f504}".to_string(),
    };
    let filter_label = match sub.filter {
        Some(f) => format!("Filter: {f} \u{1f504}"),
        None => "Filter: all \u{1f504}".to_string(),
    };
    let pause_label = if sub.paused {
        "\u{25b6}\u{fe0f} Resume"
    } else {
        "\u{23f8}\u{fe0f} Pause"
    };

    InlineKeyboardMarkup::new(vec![
        vec![InlineKeyboardButton::callback(
            limit_label,
            format!("sub_limit:{}", sub.id),
        )],
        vec![InlineKeyboardButton::callback(
            time_label,
            format!("sub_time:{}", sub.id),
        )],
        vec![InlineKeyboardButton::callback(
            filter_label,
            format!("sub_filter:{}", sub.id),
        )],
        vec![
            InlineKeyboardButton::callback(pause_label, format!("sub_pause:{}", sub.id)),
            InlineKeyboardButton::callback("\u{274c} Unsubscribe", format!("sub_del:{}", sub.id)),
        ],
        vec![InlineKeyboardButton::callback(
            "\u{2b05}\u{fe0f} Back",
            "sub_list".to_string(),
        )],
    ])
}

const LIMITS: &[u32] = &[1, 3, 5, 10, 25];

fn next_limit(current: Option<u32>, default: u32) -> u32 {
    let effective = current.unwrap_or(default);
    let idx = LIMITS.iter().position(|&l| l == effective);
    match idx {
        Some(i) => LIMITS[(i + 1) % LIMITS.len()],
        None => LIMITS[0],
    }
}

fn next_time(
    current: Option<TopPostsTimePeriod>,
    default: TopPostsTimePeriod,
) -> TopPostsTimePeriod {
    use TopPostsTimePeriod::*;
    const TIMES: &[TopPostsTimePeriod] = &[Hour, Day, Week, Month, Year, All];
    let effective = current.unwrap_or(default);
    let idx = TIMES.iter().position(|&t| t == effective);
    match idx {
        Some(i) => TIMES[(i + 1) % TIMES.len()],
        None => TIMES[0],
    }
}

fn next_filter(current: Option<PostType>) -> Option<PostType> {
    use PostType::*;
    match current {
        None => Some(Image),
        Some(Image) => Some(Video),
        Some(Video) => Some(Link),
        Some(Link) => Some(SelfText),
        Some(SelfText) => Some(Gallery),
        Some(Gallery) | Some(Unknown) => None,
    }
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
