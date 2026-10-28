//! Feedly cloud API response types.
//! Field docs follow https://developer.feedly.com/v3/ and NetNewsWire's FeedlyModel.swift.

use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct Category {
    pub id: String,
    pub label: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Subscription {
    /// `feed/<url>`.
    pub id: String,
    pub title: Option<String>,
    pub website: Option<String>,
    #[serde(default)]
    pub categories: Vec<Category>,
    pub icon_url: Option<String>,
    pub visual_url: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Profile {
    pub id: String,
    pub email: Option<String>,
    pub full_name: Option<String>,
    pub given_name: Option<String>,
}

impl Profile {
    pub fn display_name(&self) -> String {
        self.full_name
            .clone()
            .or_else(|| self.given_name.clone())
            .or_else(|| self.email.clone())
            .unwrap_or_else(|| "Feedly".into())
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct Link {
    pub href: String,
    /// When `None`, it's probably a web page.
    #[serde(rename = "type")]
    pub mime_type: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Content {
    pub content: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Origin {
    pub stream_id: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Visual {
    /// Sometimes the literal `none`.
    pub url: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Tag {
    pub id: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Entry {
    pub id: String,
    pub title: Option<String>,
    pub content: Option<Content>,
    pub summary: Option<Content>,
    pub author: Option<String>,
    /// Milliseconds since the epoch, when Feedly's servers processed the article.
    pub crawled: i64,
    pub published: Option<i64>,
    pub origin: Option<Origin>,
    pub canonical: Option<Vec<Link>>,
    pub alternate: Option<Vec<Link>>,
    #[serde(default)]
    pub unread: bool,
    pub tags: Option<Vec<Tag>>,
    pub enclosure: Option<Vec<Link>>,
    pub visual: Option<Visual>,
}

const RTL_PREFIX: &str = "<div style=\"direction:rtl;text-align:right\">";
const RTL_SUFFIX: &str = "</div>";

/// Feedly wraps right-to-left titles in a div; strip it for plain-text display.
fn sanitize_rtl(s: Option<&str>) -> Option<String> {
    let s = s?;
    if s.starts_with(RTL_PREFIX) && s.ends_with(RTL_SUFFIX) && s.len() >= RTL_PREFIX.len() + RTL_SUFFIX.len() {
        return Some(s[RTL_PREFIX.len()..s.len() - RTL_SUFFIX.len()].to_string());
    }
    Some(s.to_string())
}

impl Entry {
    pub fn feed_id(&self) -> Option<&str> {
        self.origin.as_ref()?.stream_id.as_deref()
    }

    pub fn clean_title(&self) -> Option<String> {
        sanitize_rtl(self.title.as_deref())
    }

    pub fn content_html(&self) -> Option<String> {
        self.content
            .as_ref()
            .and_then(|c| c.content.clone())
            .or_else(|| self.summary.as_ref().and_then(|c| c.content.clone()))
    }

    pub fn summary_html(&self) -> Option<String> {
        sanitize_rtl(self.summary.as_ref().and_then(|c| c.content.as_deref()))
    }

    /// Canonical links first, then alternates; only web pages.
    pub fn external_url(&self) -> Option<String> {
        self.canonical
            .iter()
            .chain(self.alternate.iter())
            .flatten()
            .find(|l| l.mime_type.as_deref().is_none_or(|t| t == "text/html"))
            .map(|l| l.href.clone())
    }

    pub fn image_url(&self) -> Option<String> {
        let url = self.visual.as_ref()?.url.as_deref()?;
        (!url.is_empty() && url != "none").then(|| url.to_string())
    }

    /// The publisher's date unless it's in the future relative to the crawl.
    pub fn date_published(&self) -> i64 {
        match self.published {
            Some(p) if p <= self.crawled => p,
            _ => self.crawled,
        }
    }

    pub fn is_saved(&self) -> bool {
        self.tags
            .iter()
            .flatten()
            .any(|t| t.id.ends_with("/tag/global.saved"))
    }

    pub fn enclosures(&self) -> Vec<Link> {
        self.enclosure.clone().unwrap_or_default()
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct Stream {
    pub continuation: Option<String>,
    #[serde(default)]
    pub items: Vec<Entry>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct StreamIds {
    pub continuation: Option<String>,
    #[serde(default)]
    pub ids: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TokenResponse {
    /// The Feedly user ID.
    pub id: String,
    pub access_token: String,
    pub refresh_token: Option<String>,
}

/// Feed resource IDs are `feed/<url>`.
pub fn feed_url_from_id(id: &str) -> &str {
    id.strip_prefix("feed/").unwrap_or(id)
}

pub fn global_all(user_id: &str) -> String {
    format!("user/{user_id}/category/global.all")
}

pub fn global_saved(user_id: &str) -> String {
    format!("user/{user_id}/tag/global.saved")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rtl_titles_are_unwrapped() {
        let t = format!("{RTL_PREFIX}שלום{RTL_SUFFIX}");
        assert_eq!(sanitize_rtl(Some(&t)).as_deref(), Some("שלום"));
        assert_eq!(sanitize_rtl(Some("plain")).as_deref(), Some("plain"));
    }

    #[test]
    fn future_published_dates_fall_back_to_crawled() {
        let json = r#"{"id":"a","crawled":1000,"published":5000}"#;
        let e: Entry = serde_json::from_str(json).unwrap();
        assert_eq!(e.date_published(), 1000);
    }

    #[test]
    fn external_url_prefers_html_links() {
        let json = r#"{"id":"a","crawled":1,
            "canonical":[{"href":"https://x/feed.mp3","type":"audio/mpeg"}],
            "alternate":[{"href":"https://x/post","type":"text/html"}]}"#;
        let e: Entry = serde_json::from_str(json).unwrap();
        assert_eq!(e.external_url().as_deref(), Some("https://x/post"));
    }
}
