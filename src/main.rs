use crate::{state::AppState, types::*};
use anyhow::{Context, Result};
use reddit::{PostType, TopPostsTimePeriod};
use secrecy::ExposeSecret;
use signal_hook::{
    consts::signal::{SIGINT, SIGTERM},
    iterator::Signals,
};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::time::Duration;
use teloxide::prelude::*;
use teloxide::utils::command::BotCommands;
use tokio::sync::broadcast;
use tracing::*;

mod args;
mod bot;
mod config;
mod db;
mod download;
mod handlers;
mod messages;
mod reddit;
mod state;
mod types;
mod ytdlp;

const PKG_NAME: &str = env!("CARGO_PKG_NAME");

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();
    let config = Arc::new(config::read_config());
    info!("starting with config: {config:#?}");

    let http = reqwest::Client::builder()
        .user_agent(reddit::api::APP_USER_AGENT)
        .build()?;

    let mut db = db::Database::open(&config)?;
    db.migrate()?;

    let tg = Arc::new(Bot::new(config.telegram_bot_token.expose_secret()));
    tg.set_my_commands(bot::Command::bot_commands()).await?;

    let app = Arc::new(AppState::new(config.clone(), http, tg.clone(), db));

    let (shutdown_tx, mut shutdown_rx) = broadcast::channel::<()>(1);
    let shutdown = Arc::new(AtomicBool::new(false));
    let bot = bot::MyBot::new(tg, app.clone());

    // Any arguments are for things that help with debugging and development
    // Not optimized for usability.
    //
    // Usage: tgreddit --debug-post <linkid>                    => Fetch post and print deserialized post
    //        tgreddit --debug-post <linkid> --chat-id <chatid> => Also send to telegram
    let opts = args::parse_args();
    if let Some(post_id) = opts.opt_str("debug-post") {
        let post = reddit::get_link(&app.http, &post_id).await.unwrap();
        info!("{:#?}", post);
        if let Some(chat_id) = opts.opt_str("chat-id") {
            return handlers::handle_new_post(&app, chat_id.parse().unwrap(), &post).await;
        }
        return Ok(());
    }

    let (bot_handle, bot_shutdown_token) = bot.spawn();

    let sub_check_loop_handle = {
        let shutdown = shutdown.clone();
        let app = app.clone();
        tokio::task::spawn(async move {
            while !shutdown.load(Ordering::Acquire) {
                check_new_posts(&app).await.unwrap_or_else(|err| {
                    error!("failed to check for new posts: {err}");
                });

                tokio::select! {
                   _ = tokio::time::sleep(Duration::from_secs(app.config.check_interval_secs)) => {}
                   _ = shutdown_rx.recv() => {
                       break
                   }
                }
            }
        })
    };

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

async fn check_post_newness(
    app: &AppState,
    chat_id: i64,
    filter: Option<reddit::PostType>,
    post: &reddit::Post,
    only_mark_seen: bool,
) -> Result<()> {
    if filter.is_some() && filter.as_ref() != Some(&post.post_type) {
        debug!("filter set and post does not match filter, skipping");
        return Ok(());
    }

    if app.db().is_post_seen(chat_id, post)? {
        debug!("post already seen, skipping...");
        return Ok(());
    }

    if only_mark_seen {
        app.db()
            .mark_post_seen_if_new(chat_id, post)
            .context("failed to mark post seen")?;
        info!("marked post seen (initial): {}", post.id);
        return Ok(());
    }

    // Send first, then mark as seen. If send fails, the post will be retried
    // on the next poll cycle instead of being silently dropped.
    match handlers::handle_new_post(app, chat_id, post).await {
        Ok(()) => {
            app.db()
                .mark_post_seen_if_new(chat_id, post)
                .context("failed to mark post seen")?;
            info!("sent and marked post seen: {}", post.id);
        }
        Err(e) => {
            error!("failed to handle new post (will retry next cycle): {e}");
        }
    }

    Ok(())
}

async fn check_new_posts(app: &AppState) -> Result<()> {
    info!("checking subscriptions for new posts");
    let subs = app.db().get_all_subscriptions()?;
    for sub in subs {
        check_new_posts_for_subscription(app, &sub)
            .await
            .unwrap_or_else(|err| {
                error!("failed to check subscription for new posts: {err}");
            });
    }

    Ok(())
}

async fn check_new_posts_for_subscription(app: &AppState, sub: &Subscription) -> Result<()> {
    let subreddit = &sub.subreddit;
    let limit = sub
        .limit
        .or(app.config.default_limit)
        .unwrap_or(config::DEFAULT_LIMIT);
    let time = sub
        .time
        .or(app.config.default_time)
        .unwrap_or(config::DEFAULT_TIME_PERIOD);
    let filter = sub.filter.or(app.config.default_filter);
    let chat_id = sub.chat_id;
    info!(
        "checking subreddit /r/{subreddit} for new posts for user {chat_id}",
        subreddit = subreddit,
        chat_id = chat_id
    );

    match reddit::get_subreddit_top_posts(&app.http, subreddit, limit, &time).await {
        Ok(posts) => {
            debug!("got {} post(s) for subreddit /r/{}", posts.len(), subreddit);

            // First run should not send anything to telegram but the post should be marked
            // as seen, unless skip_initial_send is enabled
            let is_new_subreddit = !app
                .db()
                .existing_posts_for_subreddit(chat_id, subreddit)
                .context("failed to query if subreddit has existing posts")?;
            let only_mark_seen = is_new_subreddit && app.config.skip_initial_send;

            for post in posts {
                debug!("got {post:?}");
                check_post_newness(app, chat_id, filter, &post, only_mark_seen)
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
