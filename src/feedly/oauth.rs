//! Feedly OAuth 2.0 authorization code flow, using the system browser and a loopback redirect.
//!
//! NetNewsWire uses ASWebAuthenticationSession with a custom URL scheme. On Linux the
//! sturdiest equivalent is opening the user's browser and listening on the loopback
//! address named by the registered redirect URI.
//! https://developer.feedly.com/v3/auth/

use super::api::BASE_URL;
use super::models::TokenResponse;
use anyhow::{Context, Result, anyhow, bail};
use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

const SCOPE: &str = "https://cloud.feedly.com/subscriptions";
const DEFAULT_REDIRECT_URI: &str = "http://localhost:8080/";
const SIGN_IN_TIMEOUT: Duration = Duration::from_secs(300);

/// A Feedly API client registration. Feedly issues these on request; they are not public.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OAuthClient {
    pub client_id: String,
    pub client_secret: String,
    #[serde(default = "default_redirect")]
    pub redirect_uri: String,
}

fn default_redirect() -> String {
    DEFAULT_REDIRECT_URI.into()
}

impl OAuthClient {
    /// Looks for a client registration in the environment, then `~/.config/pheme/feedly.json`,
    /// then values baked in at build time.
    pub fn load() -> Option<Self> {
        let env = |k: &str| std::env::var(k).ok().filter(|v| !v.is_empty());
        if let (Some(id), Some(secret)) = (env("PHEME_FEEDLY_CLIENT_ID"), env("PHEME_FEEDLY_CLIENT_SECRET")) {
            return Some(Self {
                client_id: id,
                client_secret: secret,
                redirect_uri: env("PHEME_FEEDLY_REDIRECT_URI").unwrap_or_else(default_redirect),
            });
        }
        if let Some(path) = crate::paths::config_dir().map(|d| d.join("feedly.json"))
            && let Ok(text) = std::fs::read_to_string(&path)
        {
            match serde_json::from_str(&text) {
                Ok(client) => return Some(client),
                Err(e) => log::warn!("Ignoring {}: {e}", path.display()),
            }
        }
        match (option_env!("PHEME_FEEDLY_CLIENT_ID"), option_env!("PHEME_FEEDLY_CLIENT_SECRET")) {
            (Some(id), Some(secret)) if !id.is_empty() => Some(Self {
                client_id: id.into(),
                client_secret: secret.into(),
                redirect_uri: option_env!("PHEME_FEEDLY_REDIRECT_URI").unwrap_or(DEFAULT_REDIRECT_URI).into(),
            }),
            _ => None,
        }
    }

    fn authorization_url(&self, state: &str) -> String {
        let mut url = url::Url::parse(&format!("{BASE_URL}/v3/auth/auth")).unwrap();
        url.query_pairs_mut()
            .append_pair("response_type", "code")
            .append_pair("client_id", &self.client_id)
            .append_pair("redirect_uri", &self.redirect_uri)
            .append_pair("scope", SCOPE)
            .append_pair("state", state);
        url.into()
    }

    /// Runs the whole sign-in: opens the browser, waits for the redirect, exchanges the code.
    pub fn sign_in(&self, http: &Client, cancel: Arc<AtomicBool>) -> Result<TokenResponse> {
        let redirect = url::Url::parse(&self.redirect_uri).context("invalid redirect URI")?;
        if redirect.scheme() != "http" {
            bail!("The redirect URI must be an http://localhost address.");
        }
        let port = redirect.port_or_known_default().unwrap_or(80);
        let server = tiny_http::Server::http(("127.0.0.1", port))
            .map_err(|e| anyhow!("Could not listen on localhost:{port} for the Feedly redirect: {e}"))?;

        let state = random_state();
        open_in_browser(&self.authorization_url(&state))?;

        let started = Instant::now();
        let code = loop {
            if cancel.load(Ordering::Relaxed) {
                bail!("Sign-in was cancelled.");
            }
            if started.elapsed() > SIGN_IN_TIMEOUT {
                bail!("Timed out waiting for Feedly to respond.");
            }
            let Some(request) = server.recv_timeout(Duration::from_millis(250))? else {
                continue;
            };
            let full = format!("http://localhost{}", request.url());
            let parsed = url::Url::parse(&full)?;
            let get = |k: &str| parsed.query_pairs().find(|(n, _)| n == k).map(|(_, v)| v.into_owned());

            // Browsers also ask for /favicon.ico and the like.
            if get("code").is_none() && get("error").is_none() {
                let _ = request.respond(tiny_http::Response::empty(404));
                continue;
            }
            if let Some(error) = get("error") {
                respond_page(request, "Sign-in was not completed. You can close this window.");
                if error == "access_denied" {
                    bail!("Access to your Feedly account was denied.");
                }
                bail!("Feedly reported an error: {}", get("error_description").unwrap_or(error));
            }
            if get("state").as_deref() != Some(state.as_str()) {
                respond_page(request, "Something went wrong. Please try again from Pheme.");
                bail!("The authorization response didn’t match the authorization request.");
            }
            respond_page(request, "You’re signed in to Feedly. You can close this window and return to Pheme.");
            break get("code").unwrap();
        };

        self.exchange(http, &code, &state)
    }

    fn exchange(&self, http: &Client, code: &str, state: &str) -> Result<TokenResponse> {
        #[derive(Serialize)]
        struct Body<'a> {
            grant_type: &'static str,
            code: &'a str,
            redirect_uri: &'a str,
            state: &'a str,
            client_id: &'a str,
            client_secret: &'a str,
            scope: &'static str,
        }
        let body = Body {
            grant_type: "authorization_code",
            code,
            redirect_uri: &self.redirect_uri,
            state,
            client_id: &self.client_id,
            client_secret: &self.client_secret,
            scope: SCOPE,
        };
        post_token(http, &body)
    }

    pub fn refresh(&self, http: &Client, refresh_token: &str) -> Result<TokenResponse> {
        #[derive(Serialize)]
        struct Body<'a> {
            grant_type: &'static str,
            refresh_token: &'a str,
            client_id: &'a str,
            client_secret: &'a str,
        }
        let body = Body {
            grant_type: "refresh_token",
            refresh_token,
            client_id: &self.client_id,
            client_secret: &self.client_secret,
        };
        post_token(http, &body)
    }
}

/// Deliberately not routed through the API's 401-retry path: a 401 here means the
/// refresh token itself is bad, and retrying would recurse.
fn post_token<B: Serialize>(http: &Client, body: &B) -> Result<TokenResponse> {
    let response = http
        .post(format!("{BASE_URL}/v3/auth/token"))
        .header("Accept", "application/json")
        .json(body)
        .send()?;
    let status = response.status();
    if !status.is_success() {
        let text = response.text().unwrap_or_default();
        bail!("Feedly token request failed ({status}): {}", text.chars().take(300).collect::<String>());
    }
    Ok(response.json()?)
}

fn random_state() -> String {
    use rand::RngExt;
    let mut rng = rand::rng();
    (0..32).map(|_| format!("{:x}", rng.random_range(0..16u8))).collect()
}

pub fn open_in_browser(url: &str) -> Result<()> {
    std::process::Command::new("xdg-open")
        .arg(url)
        .spawn()
        .context("Could not open your web browser (xdg-open failed)")?;
    Ok(())
}

fn respond_page(request: tiny_http::Request, message: &str) {
    let html = format!(
        "<!doctype html><meta charset=utf-8><title>Pheme</title>\
         <body style=\"font-family:system-ui,sans-serif;display:grid;place-items:center;height:90vh;color:#333\">\
         <div style=\"text-align:center\"><h2>Pheme</h2><p>{message}</p></div>"
    );
    let header = tiny_http::Header::from_bytes(&b"Content-Type"[..], &b"text/html; charset=utf-8"[..]).unwrap();
    let _ = request.respond(tiny_http::Response::from_string(html).with_header(header));
}
