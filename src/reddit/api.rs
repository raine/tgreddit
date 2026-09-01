use super::*;
use anyhow::{Context, Result};
use reqwest::{RequestBuilder, StatusCode};
use secrecy::{ExposeSecret, SecretString};
use serde::Deserialize;
use thiserror::Error;
use tokio::{sync::Mutex, time::Instant};
use tracing::{error, info};
use url::Url;

static REDDIT_BASE_URL: &str = "https://www.reddit.com";
static REDDIT_OAUTH_BASE_URL: &str = "https://oauth.reddit.com";
static REDDIT_ACCESS_TOKEN_URL: &str = "https://www.reddit.com/api/v1/access_token";
pub static APP_USER_AGENT: &str = concat!(env!("CARGO_PKG_NAME"), "/", env!("CARGO_PKG_VERSION"));

struct CachedAccessToken {
    value: SecretString,
    expires_at: Instant,
}

#[derive(Deserialize)]
struct AccessTokenResponse {
    access_token: String,
    expires_in: u64,
}

pub struct RedditClient {
    http: reqwest::Client,
    no_redirect_http: reqwest::Client,
    client_id: Option<String>,
    access_token: Mutex<Option<CachedAccessToken>>,
}

impl RedditClient {
    pub fn new(client_id: Option<String>, user_agent: &str) -> Result<Self> {
        let http = reqwest::Client::builder().user_agent(user_agent).build()?;
        let no_redirect_http = reqwest::Client::builder()
            .user_agent(user_agent)
            .redirect(reqwest::redirect::Policy::none())
            .build()?;
        Ok(Self {
            http,
            no_redirect_http,
            client_id,
            access_token: Mutex::new(None),
        })
    }

    async fn get(&self, path: &str) -> Result<RequestBuilder> {
        self.get_with_client(&self.http, path).await
    }

    async fn get_without_redirects(&self, path: &str) -> Result<RequestBuilder> {
        self.get_with_client(&self.no_redirect_http, path).await
    }

    async fn get_with_client(
        &self,
        client: &reqwest::Client,
        path: &str,
    ) -> Result<RequestBuilder> {
        let base_url = if self.client_id.is_some() {
            REDDIT_OAUTH_BASE_URL
        } else {
            REDDIT_BASE_URL
        };
        let url = Url::parse(base_url)?.join(path)?;
        let mut request = client.get(url);
        if self.client_id.is_some() {
            let access_token = self.access_token().await?;
            request = request.bearer_auth(access_token.expose_secret());
        }
        Ok(request)
    }

    async fn access_token(&self) -> Result<SecretString> {
        let client_id = self
            .client_id
            .as_deref()
            .context("Reddit OAuth client ID is missing")?;
        let mut cached = self.access_token.lock().await;
        if let Some(token) = cached.as_ref()
            && token.expires_at > Instant::now()
        {
            return Ok(token.value.clone());
        }

        info!("requesting Reddit OAuth access token");
        let response = self
            .http
            .post(REDDIT_ACCESS_TOKEN_URL)
            .basic_auth(client_id, Some(""))
            .form(&[
                (
                    "grant_type",
                    "https://oauth.reddit.com/grants/installed_client",
                ),
                ("device_id", "DO_NOT_TRACK_THIS_DEVICE"),
            ])
            .send()
            .await?;
        let status = response.status();
        let body = response
            .text()
            .await
            .context("failed to read Reddit OAuth response body")?;
        ensure_successful_response(status, &body)?;

        let response: AccessTokenResponse =
            serde_json::from_str(&body).context("failed to decode Reddit OAuth response body")?;
        let token = SecretString::from(response.access_token);
        let lifetime = std::time::Duration::from_secs(response.expires_in.saturating_sub(60));
        *cached = Some(CachedAccessToken {
            value: token.clone(),
            expires_at: Instant::now() + lifetime,
        });
        Ok(token)
    }
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
    client: &RedditClient,
    subreddit: &str,
    limit: u32,
    time: &TopPostsTimePeriod,
) -> Result<Vec<Post>> {
    info!("getting top posts for /r/{subreddit} limit={limit} time={time:?}");
    let response = client
        .get(&format!("/r/{subreddit}/top.json"))
        .await?
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

pub async fn get_link(client: &RedditClient, link_id: &str) -> Result<Post> {
    info!("getting link id {link_id}");
    let res = client
        .get("/api/info.json")
        .await?
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
    Api(#[from] anyhow::Error),
    #[error(transparent)]
    Reqwest(#[from] reqwest::Error),
}

pub async fn get_subreddit_about(
    client: &RedditClient,
    subreddit: &str,
) -> Result<SubredditAbout, SubredditAboutError> {
    info!("getting subreddit about for /r/{subreddit}");
    let res = client
        .get_without_redirects(&format!("/r/{subreddit}/about.json"))
        .await?
        .send()
        .await?;

    match res.status() {
        reqwest::StatusCode::FOUND | reqwest::StatusCode::NOT_FOUND => {
            Err(SubredditAboutError::NoSuchSubreddit)
        }
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
