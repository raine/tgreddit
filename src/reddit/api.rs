use super::*;
use anyhow::{Context, Result};
use reqwest::StatusCode;
use thiserror::Error;
use tracing::{error, info};
use url::Url;

static REDDIT_BASE_URL: &str = "https://www.reddit.com";
pub static APP_USER_AGENT: &str = concat!(env!("CARGO_PKG_NAME"), "/", env!("CARGO_PKG_VERSION"));

fn get_base_url() -> Url {
    Url::parse(REDDIT_BASE_URL).unwrap()
}

pub fn format_url_from_path(path: &str, base_url: Option<&str>) -> String {
    let base_url = match base_url {
        Some(u) => u,
        None => REDDIT_BASE_URL,
    };
    format!("{base_url}{path}")
}

pub fn to_old_reddit_url(url: &str) -> String {
    // If this fails it's bug
    let mut url = Url::parse(url).unwrap();
    url.set_host(Some("old.reddit.com")).unwrap();
    url.to_string()
}

pub fn format_subreddit_url(subreddit: &str, base_url: Option<&str>) -> String {
    format_url_from_path(&format!("/r/{subreddit}"), base_url)
}

pub async fn get_subreddit_top_posts(
    client: &reqwest::Client,
    subreddit: &str,
    limit: u32,
    time: &TopPostsTimePeriod,
) -> Result<Vec<Post>> {
    info!("getting top posts for /r/{subreddit} limit={limit} time={time:?}");
    let url = get_base_url()
        .join(&format!("/r/{subreddit}/top.json"))
        .unwrap();
    let response = client
        .get(url)
        .query(&[
            ("limit", &limit.to_string()),
            ("t", &format!("{:?}", time).to_lowercase()),
        ])
        .send()
        .await?;
    let status = response.status();
    let body = response
        .text()
        .await
        .context("failed to read Reddit response body")?;

    ensure_successful_response(status, &body)?;
    let response: ListingResponse =
        serde_json::from_str(&body).context("failed to decode Reddit response body")?;
    let posts = response
        .data
        .children
        .into_iter()
        .map(|e| Post::from(e.data))
        .collect();
    Ok(posts)
}

#[derive(Error, Debug)]
enum RedditResponseError {
    #[error("Reddit request failed with HTTP {0}: blocked by network security")]
    NetworkSecurityBlock(StatusCode),
    #[error("Reddit request failed with HTTP {status}: {detail}")]
    Http { status: StatusCode, detail: String },
}

fn ensure_successful_response(status: StatusCode, body: &str) -> Result<()> {
    if status.is_success() {
        return Ok(());
    }

    if body.contains("blocked by network security") {
        return Err(RedditResponseError::NetworkSecurityBlock(status).into());
    }

    let detail = body
        .trim()
        .lines()
        .next()
        .unwrap_or("empty response")
        .chars()
        .take(200)
        .collect();
    Err(RedditResponseError::Http { status, detail }.into())
}

pub fn is_network_security_block(error: &anyhow::Error) -> bool {
    matches!(
        error.downcast_ref::<RedditResponseError>(),
        Some(RedditResponseError::NetworkSecurityBlock(_))
    )
}

pub async fn get_link(client: &reqwest::Client, link_id: &str) -> Result<Post> {
    info!("getting link id {link_id}");
    let url = get_base_url().join("/api/info.json")?;
    let res = client
        .get(url)
        .query(&[("id", &format!("t3_{link_id}"))])
        .send()
        .await
        .context("failed to send request")?;

    let status = res.status();
    let body = res.text().await.context("failed to read response body")?;

    if !status.is_success() {
        error!("request failed with status: {}", status);
        error!("response body: {}", body);
        anyhow::bail!("Request failed with status: {}", status);
    }

    match serde_json::from_str::<ListingResponse>(&body) {
        Ok(parsed) => parsed
            .data
            .children
            .into_iter()
            .map(|e| Post::from(e.data))
            .next()
            .context("no post in response")
            .map_err(|e| {
                error!("failed to get posts for {link_id}: {:?}", e);
                e
            }),
        Err(e) => {
            error!("error decoding response body: {}", e);
            error!("response body: {}", body);
            Err(anyhow::anyhow!("error decoding response body: {}", e))
        }
    }
}

#[allow(clippy::large_enum_variant)]
#[derive(Error, Debug)]
pub enum SubredditAboutError {
    #[error("no such subreddit")]
    NoSuchSubreddit,
    #[error(transparent)]
    Reqwest(#[from] reqwest::Error),
    #[error(transparent)]
    UrlParseError(#[from] url::ParseError),
    #[error(transparent)]
    IO(#[from] std::io::Error),
}

/// Uses a separate no-redirect client to detect non-existent subreddits via 302.
/// Called infrequently (only on /sub command), so building a client here is fine.
pub async fn get_subreddit_about(subreddit: &str) -> Result<SubredditAbout, SubredditAboutError> {
    info!("getting subreddit about for /r/{subreddit}");
    let client = reqwest::Client::builder()
        .user_agent(APP_USER_AGENT)
        .redirect(reqwest::redirect::Policy::none())
        .build()?;
    let url = get_base_url().join(&format!("/r/{subreddit}/about.json"))?;
    let res = client.get(url).send().await?;

    match res.status() {
        reqwest::StatusCode::FOUND => Err(SubredditAboutError::NoSuchSubreddit),
        _ => {
            let data = res.json::<SubredditAboutResponse>().await?.data;
            Ok(data)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reports_reddit_network_security_blocks() {
        let error = ensure_successful_response(
            StatusCode::FORBIDDEN,
            "<html>You've been blocked by network security.</html>",
        )
        .unwrap_err();

        assert_eq!(
            error.to_string(),
            "Reddit request failed with HTTP 403 Forbidden: blocked by network security"
        );
        assert!(is_network_security_block(&error));
    }

    #[test]
    fn distinguishes_other_http_errors() {
        let error =
            ensure_successful_response(StatusCode::TOO_MANY_REQUESTS, "slow down").unwrap_err();

        assert!(!is_network_security_block(&error));
    }

    #[test]
    fn accepts_successful_reddit_responses() {
        ensure_successful_response(StatusCode::OK, "{}").unwrap();
    }
}
