//! A blocking client for the Feedly cloud API, modeled on NetNewsWire's FeedlyAPICaller.

use super::models::*;
use super::oauth::OAuthClient;
use crate::credentials::Credentials;
use anyhow::{Context, Result, anyhow, bail};
use reqwest::StatusCode;
use reqwest::blocking::{Client, RequestBuilder, Response};
use serde::Serialize;
use serde::de::DeserializeOwned;

pub const BASE_URL: &str = "https://cloud.feedly.com";

/// Feedly's /v3/markers limit per request.
pub const MARK_CHUNK_SIZE: usize = 300;
/// Feedly's /v3/entries/.mget limit per request.
pub const ENTRIES_CHUNK_SIZE: usize = 500;

#[derive(Debug, thiserror::Error)]
pub enum FeedlyError {
    #[error("Feedly rejected the sign-in. Please sign in again.")]
    Unauthorized,
    #[error("Feedly is rate limiting requests. Try again later.")]
    RateLimited,
    #[error("Feedly returned HTTP {status}: {body}")]
    Http { status: u16, body: String },
}

#[derive(Debug, Clone, Copy)]
pub enum MarkAction {
    Read,
    Unread,
    Saved,
    Unsaved,
}

impl MarkAction {
    fn action_value(self) -> &'static str {
        match self {
            MarkAction::Read => "markAsRead",
            MarkAction::Unread => "keepUnread",
            MarkAction::Saved => "markAsSaved",
            MarkAction::Unsaved => "markAsUnsaved",
        }
    }
}

pub struct FeedlyApi {
    http: Client,
    pub credentials: Credentials,
    oauth: Option<OAuthClient>,
    /// Set when a token refresh happened, so the caller can persist the new credentials.
    pub credentials_changed: bool,
}

pub fn http_client() -> Client {
    Client::builder()
        .user_agent(concat!("Pheme/", env!("CARGO_PKG_VERSION"), " (Linux)"))
        .timeout(std::time::Duration::from_secs(60))
        .build()
        .expect("HTTP client")
}

impl FeedlyApi {
    pub fn new(credentials: Credentials, oauth: Option<OAuthClient>) -> Self {
        Self { http: http_client(), credentials, oauth, credentials_changed: false }
    }

    fn url(path: &str) -> String {
        format!("{BASE_URL}{path}")
    }

    fn authorize(&self, rb: RequestBuilder) -> RequestBuilder {
        rb.header("Authorization", format!("OAuth {}", self.credentials.access_token))
            .header("Accept", "application/json")
    }

    /// Sends a request, refreshing the access token and retrying exactly once on a 401.
    fn send(&mut self, build: impl Fn(&Client) -> RequestBuilder) -> Result<Response> {
        let response = self.authorize(build(&self.http)).send()?;
        if response.status() != StatusCode::UNAUTHORIZED {
            return check(response);
        }
        if !self.try_refresh()? {
            return Err(FeedlyError::Unauthorized.into());
        }
        check(self.authorize(build(&self.http)).send()?)
    }

    fn get_json<T: DeserializeOwned>(&mut self, path: &str, query: &[(&str, String)]) -> Result<T> {
        let url = Self::url(path);
        let response = self.send(|c| c.get(&url).query(query))?;
        response.json().with_context(|| format!("decoding {path}"))
    }

    fn post_json<B: Serialize, T: DeserializeOwned>(&mut self, path: &str, body: &B) -> Result<T> {
        let url = Self::url(path);
        let response = self.send(|c| c.post(&url).json(body))?;
        response.json().with_context(|| format!("decoding {path}"))
    }

    fn try_refresh(&mut self) -> Result<bool> {
        let (Some(oauth), Some(refresh)) = (&self.oauth, self.credentials.refresh_token.clone()) else {
            return Ok(false);
        };
        match oauth.refresh(&self.http, &refresh) {
            Ok(token) => {
                self.credentials.access_token = token.access_token;
                if let Some(r) = token.refresh_token {
                    self.credentials.refresh_token = Some(r);
                }
                self.credentials_changed = true;
                Ok(true)
            }
            Err(e) => {
                log::warn!("Feedly token refresh failed: {e:#}");
                Ok(false)
            }
        }
    }

    pub fn profile(&mut self) -> Result<Profile> {
        self.get_json("/v3/profile", &[])
    }

    pub fn subscriptions(&mut self) -> Result<Vec<Subscription>> {
        self.get_json("/v3/subscriptions", &[])
    }

    pub fn categories(&mut self) -> Result<Vec<Category>> {
        self.get_json("/v3/categories", &[("sort", "feedly".into())])
    }

    pub fn stream_contents(
        &mut self,
        stream_id: &str,
        continuation: Option<&str>,
        newer_than_ms: Option<i64>,
        count: usize,
    ) -> Result<Stream> {
        let mut q = vec![("streamId", stream_id.to_string()), ("count", count.to_string())];
        if let Some(c) = continuation {
            q.push(("continuation", c.to_string()));
        }
        if let Some(n) = newer_than_ms {
            q.push(("newerThan", n.to_string()));
        }
        self.get_json("/v3/streams/contents", &q)
    }

    pub fn stream_ids(
        &mut self,
        stream_id: &str,
        continuation: Option<&str>,
        newer_than_ms: Option<i64>,
        unread_only: bool,
    ) -> Result<StreamIds> {
        let mut q = vec![
            ("streamId", stream_id.to_string()),
            ("count", "10000".to_string()),
            ("unreadOnly", unread_only.to_string()),
        ];
        if let Some(c) = continuation {
            q.push(("continuation", c.to_string()));
        }
        if let Some(n) = newer_than_ms {
            q.push(("newerThan", n.to_string()));
        }
        self.get_json("/v3/streams/ids", &q)
    }

    /// Walks every page of a stream's IDs. Returns the IDs and whether the walk was cut short.
    pub fn all_stream_ids(
        &mut self,
        stream_id: &str,
        newer_than_ms: Option<i64>,
        unread_only: bool,
        max_pages: usize,
    ) -> Result<(Vec<String>, bool)> {
        let mut ids = Vec::new();
        let mut continuation: Option<String> = None;
        for _ in 0..max_pages {
            let page = self.stream_ids(stream_id, continuation.as_deref(), newer_than_ms, unread_only)?;
            ids.extend(page.ids);
            continuation = page.continuation.filter(|c| !c.is_empty());
            if continuation.is_none() {
                return Ok((ids, false));
            }
        }
        Ok((ids, true))
    }

    pub fn entries(&mut self, ids: &[String]) -> Result<Vec<Entry>> {
        self.post_json("/v3/entries/.mget", &ids)
    }

    pub fn mark(&mut self, ids: &[String], action: MarkAction) -> Result<()> {
        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct Body<'a> {
            r#type: &'static str,
            action: &'static str,
            entry_ids: &'a [String],
        }
        let url = Self::url("/v3/markers");
        let body = Body { r#type: "entries", action: action.action_value(), entry_ids: ids };
        // Only the status code matters; decoding the body could turn success into failure.
        self.send(|c| c.post(&url).json(&body))?;
        Ok(())
    }

    pub fn logout(&mut self) -> Result<()> {
        let url = Self::url("/v3/auth/logout");
        self.send(|c| c.post(&url))?;
        Ok(())
    }
}

fn check(response: Response) -> Result<Response> {
    let status = response.status();
    if status.is_success() {
        return Ok(response);
    }
    if status == StatusCode::UNAUTHORIZED {
        bail!(FeedlyError::Unauthorized);
    }
    if status == StatusCode::TOO_MANY_REQUESTS {
        bail!(FeedlyError::RateLimited);
    }
    let body = response.text().unwrap_or_default();
    let body: String = body.chars().take(300).collect();
    Err(anyhow!(FeedlyError::Http { status: status.as_u16(), body }))
}
