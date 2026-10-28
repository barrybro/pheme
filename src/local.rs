//! The "On My Computer" account: feeds Pheme downloads and parses itself, like
//! NetNewsWire's local account. Read and starred state stays on this computer.

use crate::feedly::api::http_client;
use crate::store::{self, LocalArticle, LocalFeedState, local_feed_id};
use crate::text::decode_entities;
use anyhow::{Context, Result, anyhow, bail};
use reqwest::StatusCode;
use reqwest::blocking::Client;
use std::sync::Mutex;

/// Starter feeds, as NetNewsWire's local account has: (title, feed URL, web site).
/// Each is offered once; adding one here gives it to existing users on their next launch.
const DEFAULT_FEEDS: &[(&str, &str, &str)] = &[
    ("Daring Fireball", "https://daringfireball.net/feeds/main", "https://daringfireball.net/"),
    ("Six Colors", "https://feedpress.me/sixcolors?type=xml", "https://sixcolors.com/"),
    ("MacRumors", "https://feeds.macrumors.com/MacRumors-All", "https://www.macrumors.com/"),
];

/// The default feed URLs already offered, as a JSON list. Older versions stored "1",
/// meaning only Daring Fireball had been offered.
const DEFAULTS_OFFERED_KEY: &str = "local.default_feeds_added";

/// Subscribes to default feeds not offered before. Their articles arrive with the next
/// refresh; unsubscribing from one doesn't bring it back.
pub fn add_default_feeds() -> Result<()> {
    let mut s = store::get();
    let mut offered: Vec<String> = match s.setting(DEFAULTS_OFFERED_KEY).as_deref() {
        None => vec![],
        Some("1") => vec![DEFAULT_FEEDS[0].1.to_string()],
        Some(json) => serde_json::from_str(json).unwrap_or_default(),
    };
    for (title, url, site) in DEFAULT_FEEDS {
        if offered.iter().any(|u| u == url) {
            continue;
        }
        let state = LocalFeedState { feed_id: local_feed_id(url), url: url.to_string(), ..Default::default() };
        if !s.local_feed_exists(&state.feed_id) {
            // The first refresh finds the icon.
            s.add_local_feed(&state, title, Some(site), None, None)?;
        }
        offered.push(url.to_string());
    }
    s.set_setting(DEFAULTS_OFFERED_KEY, &serde_json::to_string(&offered)?)?;
    Ok(())
}

const TITLES_DECODED_KEY: &str = "local.title_entities_decoded";

/// Once: decodes HTML entities in titles stored before Pheme decoded them on the way in.
/// Unchanged feeds aren't parsed again, so refreshing alone wouldn't fix them.
pub fn decode_stored_titles() -> Result<()> {
    let mut s = store::get();
    if s.setting(TITLES_DECODED_KEY).is_some() {
        return Ok(());
    }
    let changed = s.decode_local_title_entities()?;
    if changed > 0 {
        log::info!("Decoded HTML entities in {changed} stored titles");
    }
    s.set_setting(TITLES_DECODED_KEY, "1")
}

/// Feeds refreshed at once.
const CONCURRENT_FETCHES: usize = 6;

pub struct Fetched {
    pub state: LocalFeedState,
    pub feed: Option<feed_rs::model::Feed>,
}

/// Downloads a feed, using the stored ETag/Last-Modified so unchanged feeds cost little.
/// `feed` is `None` when the server says nothing changed.
fn fetch(http: &Client, state: &LocalFeedState) -> Result<Fetched> {
    let mut request = http.get(&state.url).header("Accept", "application/rss+xml, application/atom+xml, application/feed+json, application/xml;q=0.9, */*;q=0.8");
    if let Some(etag) = &state.etag {
        request = request.header("If-None-Match", etag);
    }
    if let Some(modified) = &state.last_modified {
        request = request.header("If-Modified-Since", modified);
    }
    let response = request.send()?;
    let status = response.status();
    if status == StatusCode::NOT_MODIFIED {
        return Ok(Fetched { state: state.clone(), feed: None });
    }
    if !status.is_success() {
        bail!("{} returned HTTP {}", state.url, status.as_u16());
    }
    let header = |name: &str| response.headers().get(name).and_then(|v| v.to_str().ok()).map(str::to_string);
    let mut new_state = state.clone();
    new_state.etag = header("etag");
    new_state.last_modified = header("last-modified");
    let final_url = response.url().to_string();
    let body = response.bytes()?;
    let feed = parse(&body, &final_url)?;
    Ok(Fetched { state: new_state, feed: Some(feed) })
}

fn parse(body: &[u8], url: &str) -> Result<feed_rs::model::Feed> {
    feed_rs::parser::Builder::new()
        .base_uri(Some(url))
        .build()
        .parse(body)
        .map_err(|e| anyhow!("Not a feed Pheme can read: {e}"))
}

/// Finds the feed for a URL the user typed: either the feed itself, or a web page that
/// links to one with `<link rel="alternate">`.
pub fn discover(http: &Client, input: &str) -> Result<(String, feed_rs::model::Feed, LocalFeedState)> {
    let mut url = input.trim().to_string();
    if url.starts_with("feed://") {
        url = url.replacen("feed://", "https://", 1);
    } else if !url.contains("://") {
        url = format!("https://{url}");
    }
    let response = http.get(&url).send().with_context(|| format!("Couldn’t reach {url}"))?;
    if !response.status().is_success() {
        bail!("{url} returned HTTP {}", response.status().as_u16());
    }
    let final_url = response.url().to_string();
    let header = |name: &str| response.headers().get(name).and_then(|v| v.to_str().ok()).map(str::to_string);
    let (etag, last_modified) = (header("etag"), header("last-modified"));
    let body = response.bytes()?;

    if let Ok(feed) = parse(&body, &final_url) {
        let state = LocalFeedState { feed_id: local_feed_id(&final_url), url: final_url.clone(), etag, last_modified };
        return Ok((final_url, feed, state));
    }

    let html = String::from_utf8_lossy(&body);
    let base = url::Url::parse(&final_url)?;
    let mut candidates: Vec<String> = feed_links(&html).into_iter().filter_map(|href| base.join(&href).ok()).map(String::from).collect();
    if candidates.is_empty() {
        // Common locations for sites that don't advertise their feed.
        candidates = ["/feed", "/rss", "/feed.xml", "/atom.xml", "/rss.xml", "/index.xml"]
            .iter()
            .filter_map(|p| base.join(p).ok().map(String::from))
            .collect();
    }
    for candidate in candidates {
        let state = LocalFeedState { feed_id: local_feed_id(&candidate), url: candidate.clone(), ..Default::default() };
        if let Ok(Fetched { state, feed: Some(feed) }) = fetch(http, &state) {
            let final_url = state.url.clone();
            return Ok((final_url, feed, state));
        }
    }
    bail!("Couldn’t find a feed at {url}.")
}

/// Every `<link>` tag in a page as (rel, type, href), lowercased rel and type, in page order.
fn link_tags(html: &str) -> Vec<(String, String, String)> {
    let lower = html.to_ascii_lowercase();
    let mut links = vec![];
    let mut pos = 0;
    while let Some(start) = lower[pos..].find("<link") {
        let start = pos + start;
        let Some(len) = lower[start..].find('>') else { break };
        let tag = &html[start..start + len];
        pos = start + len;
        if let Some(href) = attribute(tag, "href") {
            let rel = attribute(tag, "rel").unwrap_or_default().to_ascii_lowercase();
            let kind = attribute(tag, "type").unwrap_or_default().to_ascii_lowercase();
            links.push((rel, kind, decode_entities(&href)));
        }
    }
    links
}

/// `href`s of `<link rel="alternate" type="…rss/atom/json feed…">` tags, in page order.
fn feed_links(html: &str) -> Vec<String> {
    link_tags(html)
        .into_iter()
        // Plain application/json alternates are almost always JSON Feeds.
        .filter(|(rel, kind, _)| {
            rel.split_whitespace().any(|r| r == "alternate") && (kind.contains("rss") || kind.contains("atom") || kind.contains("json"))
        })
        .map(|(_, _, href)| href)
        .collect()
}

/// Icon `href`s from a page, best first: touch icons are large and crisp, then plain icons.
fn icon_links(html: &str) -> Vec<String> {
    let links = link_tags(html);
    let with_rel = |wanted: &[&str]| -> Vec<String> {
        links
            .iter()
            .filter(|(rel, _, _)| wanted.contains(&rel.as_str()))
            .map(|(_, _, href)| href.clone())
            .collect()
    };
    let mut icons = with_rel(&["apple-touch-icon", "apple-touch-icon-precomposed"]);
    icons.extend(with_rel(&["icon", "shortcut icon"]));
    icons
}

fn attribute(tag: &str, name: &str) -> Option<String> {
    let lower = tag.to_ascii_lowercase();
    let mut search = 0;
    while let Some(i) = lower[search..].find(name) {
        let i = search + i;
        search = i + name.len();
        let before_ok = i == 0 || lower.as_bytes()[i - 1].is_ascii_whitespace();
        let rest = tag[i + name.len()..].trim_start();
        if !before_ok || !rest.starts_with('=') {
            continue;
        }
        let value = rest[1..].trim_start();
        return Some(match value.chars().next() {
            Some(q @ ('"' | '\'')) => value[1..].split(q).next().unwrap_or("").to_string(),
            // Unquoted values run to whitespace; a trailing `/` belongs to a self-closing tag.
            _ => value.split(char::is_whitespace).next().unwrap_or("").trim_end_matches('/').to_string(),
        });
    }
    None
}

fn text(t: &Option<feed_rs::model::Text>) -> Option<String> {
    t.as_ref().map(|t| t.content.trim().to_string()).filter(|s| !s.is_empty())
}

/// The feed's web site: its alternate link, or a plain link that isn't the feed itself.
/// Other relations, like a WebSub `hub`, point at services rather than the site.
pub fn website(feed: &feed_rs::model::Feed, feed_url: &str) -> Option<String> {
    feed.links
        .iter()
        .find(|l| l.rel.as_deref() == Some("alternate"))
        .or_else(|| feed.links.iter().find(|l| l.rel.is_none() && l.href != feed_url))
        .map(|l| l.href.clone())
}

/// Finds an icon for a feed, as NetNewsWire does: the site's touch icon or favicon from
/// its home page, the feed's own icon, then `/favicon.ico`. Each candidate is checked,
/// since sites often advertise icons that are missing or empty.
pub fn find_icon(http: &Client, feed: &feed_rs::model::Feed, feed_url: &str) -> Option<String> {
    let site = website(feed, feed_url).unwrap_or_else(|| feed_url.to_string());
    let mut candidates = vec![];
    if let Ok(response) = http.get(&site).send()
        && response.status().is_success()
    {
        let base = response.url().clone();
        if let Ok(html) = response.text() {
            candidates.extend(icon_links(&html).into_iter().filter_map(|href| base.join(&href).ok().map(String::from)));
        }
    }
    candidates.extend(feed.icon.iter().chain(feed.logo.iter()).map(|i| i.uri.clone()));
    if let Ok(mut favicon) = url::Url::parse(&site) {
        favicon.set_path("/favicon.ico");
        favicon.set_query(None);
        candidates.push(favicon.into());
    }
    candidates.dedup();
    candidates.into_iter().find(|url| is_image(http, url))
}

fn is_image(http: &Client, url: &str) -> bool {
    let Ok(response) = http.get(url).send() else { return false };
    let is_html = response
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .is_some_and(|t| t.starts_with("text/"));
    response.status().is_success() && !is_html && response.bytes().is_ok_and(|b| !b.is_empty())
}

pub fn title(feed: &feed_rs::model::Feed, feed_url: &str) -> String {
    text(&feed.title).map(|t| decode_entities(&t)).unwrap_or_else(|| feed_url.to_string())
}

fn articles(feed_id: &str, feed: &feed_rs::model::Feed) -> Vec<LocalArticle> {
    let now = store::now_ms();
    feed.entries
        .iter()
        .map(|e| {
            let url = e
                .links
                .iter()
                .find(|l| l.rel.is_none() || l.rel.as_deref() == Some("alternate"))
                .or_else(|| e.links.first())
                .map(|l| l.href.clone());
            let content_html = e.content.as_ref().and_then(|c| c.body.clone()).filter(|b| !b.trim().is_empty());
            let summary_html = text(&e.summary);
            let mut enclosures = vec![];
            let mut image_url = None;
            for media in &e.media {
                for c in &media.content {
                    if let Some(u) = &c.url {
                        enclosures.push((u.to_string(), c.content_type.as_ref().map(|t| t.to_string())));
                    }
                }
                if image_url.is_none() {
                    image_url = media.thumbnails.first().map(|t| t.image.uri.clone());
                }
            }
            for l in e.links.iter().filter(|l| l.rel.as_deref() == Some("enclosure")) {
                enclosures.push((l.href.clone(), l.media_type.clone()));
            }
            let published = e.published.or(e.updated).map(|d| d.timestamp_millis()).unwrap_or(now);
            LocalArticle {
                id: format!("{feed_id}#{}", e.id),
                title: text(&e.title).map(|t| decode_entities(&t)),
                author: e.authors.iter().find_map(|a| a.name.clone()).filter(|n| !n.trim().is_empty()),
                content_html: content_html.or_else(|| summary_html.clone()),
                summary_html,
                url,
                image_url,
                enclosures,
                published,
            }
        })
        .collect()
}

/// Subscribes to a feed. Returns the new feed's sidebar key.
pub fn add_feed(input: &str, name: Option<&str>, folder_id: Option<&str>) -> Result<String> {
    let http = http_client();
    let (url, feed, state) = discover(&http, input)?;
    if store::get().local_feed_exists(&state.feed_id) {
        bail!("You’re already subscribed to {url}.");
    }
    let title = name.map(str::trim).filter(|n| !n.is_empty()).map(str::to_string).unwrap_or_else(|| title(&feed, &url));
    let site = website(&feed, &url);
    let icon = find_icon(&http, &feed, &url);
    let mut s = store::get();
    s.add_local_feed(&state, &title, site.as_deref(), icon.as_deref(), folder_id)?;
    s.upsert_local_articles(&state.feed_id, &articles(&state.feed_id, &feed))?;
    Ok(format!("feed:{}", state.feed_id))
}

#[derive(Debug, Default)]
pub struct Summary {
    pub new_articles: usize,
    pub failed: usize,
}

/// Refreshes every local feed, a few at a time.
pub fn refresh_all(progress: &dyn Fn(&str)) -> Result<Summary> {
    let states = store::get().local_feed_states()?;
    if states.is_empty() {
        return Ok(Summary::default());
    }
    progress(&format!("Refreshing {} feed{}…", states.len(), if states.len() == 1 { "" } else { "s" }));
    let http = http_client();
    let queue = Mutex::new(states.into_iter());
    let summary = Mutex::new(Summary::default());
    std::thread::scope(|scope| {
        for _ in 0..CONCURRENT_FETCHES {
            scope.spawn(|| {
                loop {
                    let Some(state) = queue.lock().unwrap().next() else { break };
                    let result = fetch(&http, &state).and_then(|fetched| {
                        let mut s = store::get();
                        let mut added = 0;
                        if let Some(feed) = &fetched.feed {
                            added = s.upsert_local_articles(&state.feed_id, &articles(&state.feed_id, feed))?;
                            let needs_icon = !s.feed_has_icon(&state.feed_id);
                            drop(s);
                            // Looking for an icon means more requests; don't hold the store meanwhile.
                            if needs_icon && let Some(icon) = find_icon(&http, feed, &state.url) {
                                store::get().set_feed_icon(&state.feed_id, &icon)?;
                            }
                            s = store::get();
                        }
                        s.update_local_feed_state(&fetched.state, None)?;
                        Ok(added)
                    });
                    let mut sum = summary.lock().unwrap();
                    match result {
                        Ok(added) => sum.new_articles += added,
                        Err(e) => {
                            log::warn!("Refreshing {} failed: {e:#}", state.url);
                            sum.failed += 1;
                            let _ = store::get().update_local_feed_state(&state, Some(&format!("{e:#}")));
                        }
                    }
                }
            });
        }
    });
    Ok(summary.into_inner().unwrap())
}

// MARK: OPML

/// Adds every feed in an OPML file to the local account, creating folders for its
/// top-level groups. Feeds are fetched on the next refresh. Returns how many were added.
pub fn import_opml(path: &str) -> Result<usize> {
    let xml = std::fs::read_to_string(path).with_context(|| format!("Couldn’t read {path}"))?;
    let opml = opml::OPML::from_str(&xml).map_err(|e| anyhow!("Not an OPML file Pheme can read: {e}"))?;
    let mut added = 0;
    fn walk(outlines: &[opml::Outline], folder: Option<&str>, added: &mut usize) -> Result<()> {
        for o in outlines {
            let name = o.title.clone().filter(|t| !t.is_empty()).unwrap_or_else(|| o.text.clone());
            if let Some(url) = o.xml_url.as_deref().filter(|u| !u.is_empty()) {
                let state = LocalFeedState { feed_id: local_feed_id(url), url: url.to_string(), ..Default::default() };
                let mut s = store::get();
                if s.local_feed_exists(&state.feed_id) {
                    continue;
                }
                let title = if name.is_empty() { url.to_string() } else { name };
                s.add_local_feed(&state, &title, o.html_url.as_deref(), None, folder)?;
                *added += 1;
            } else if !o.outlines.is_empty() {
                // NetNewsWire, like most readers, has one level of folders; deeper groups flatten into it.
                let folder_id = match folder {
                    Some(f) => f.to_string(),
                    None => {
                        let s = store::get();
                        match s.folder_named(store::Account::Local, &name)? {
                            Some(id) => id,
                            None => s.create_local_folder(&name)?,
                        }
                    }
                };
                walk(&o.outlines, Some(&folder_id), added)?;
            }
        }
        Ok(())
    }
    walk(&opml.body.outlines, None, &mut added)?;
    Ok(added)
}

pub fn export_opml(path: &str) -> Result<usize> {
    let subs = store::get().local_subscriptions()?;
    let mut opml = opml::OPML {
        head: Some(opml::Head { title: Some("Pheme Subscriptions".into()), ..Default::default() }),
        ..Default::default()
    };
    let outline = |title: &str, url: &str, site: &Option<String>| opml::Outline {
        text: title.into(),
        title: Some(title.into()),
        r#type: Some("rss".into()),
        xml_url: Some(url.into()),
        html_url: site.clone(),
        ..Default::default()
    };
    for (folder, title, url, site) in &subs {
        match folder {
            None => opml.body.outlines.push(outline(title, url, site)),
            Some(name) => {
                let group = match opml.body.outlines.iter_mut().position(|o| o.xml_url.is_none() && &o.text == name) {
                    Some(i) => &mut opml.body.outlines[i],
                    None => {
                        opml.body.outlines.push(opml::Outline { text: name.clone(), title: Some(name.clone()), ..Default::default() });
                        opml.body.outlines.last_mut().unwrap()
                    }
                };
                group.outlines.push(outline(title, url, site));
            }
        }
    }
    let xml = opml.to_string().map_err(|e| anyhow!("{e}"))?;
    std::fs::write(path, xml).with_context(|| format!("Couldn’t write {path}"))?;
    Ok(subs.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Hits the network: `XDG_DATA_HOME=/tmp/x cargo test -- --ignored`.
    #[test]
    #[ignore]
    fn discovers_real_feeds() {
        let http = http_client();
        for site in ["inessential.com", "https://daringfireball.net/", "https://lwn.net/headlines/rss"] {
            let (url, feed, _) = discover(&http, site).unwrap_or_else(|e| panic!("{site}: {e:#}"));
            let articles = articles("local:x", &feed);
            println!("{site} -> {url}: {:?}, {} articles, icon {:?}", title(&feed, &url), articles.len(), find_icon(&http, &feed, &url));
            assert!(!articles.is_empty());
        }
    }

    /// Uses the process-wide store, so run it against a scratch data directory:
    /// `XDG_DATA_HOME=/tmp/x cargo test -- --ignored`.
    #[test]
    #[ignore]
    fn opml_round_trip() {
        let dir = std::env::temp_dir().join(format!("pheme-opml-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let input = dir.join("in.opml");
        std::fs::write(&input, r#"<?xml version="1.0"?><opml version="1.0"><head><title>t</title></head><body>
            <outline text="Tech"><outline text="LWN" type="rss" xmlUrl="https://lwn.net/headlines/rss"/>
              <outline text="Nested"><outline text="Deep" type="rss" xmlUrl="https://example.com/deep.xml"/></outline></outline>
            <outline text="Top" type="rss" xmlUrl="https://example.com/top.xml" htmlUrl="https://example.com/"/>
            </body></opml>"#).unwrap();
        assert_eq!(import_opml(input.to_str().unwrap()).unwrap(), 3);
        assert_eq!(import_opml(input.to_str().unwrap()).unwrap(), 0, "re-importing adds nothing");

        let output = dir.join("out.opml");
        export_opml(output.to_str().unwrap()).unwrap();
        let exported = opml::OPML::from_str(&std::fs::read_to_string(&output).unwrap()).unwrap();
        let tech = exported.body.outlines.iter().find(|o| o.text == "Tech").expect("Tech folder");
        let mut urls: Vec<_> = tech.outlines.iter().filter_map(|o| o.xml_url.clone()).collect();
        urls.sort();
        assert_eq!(urls, vec!["https://example.com/deep.xml", "https://lwn.net/headlines/rss"]);
        assert!(exported.body.outlines.iter().any(|o| o.xml_url.as_deref() == Some("https://example.com/top.xml")));
    }

    #[test]
    fn finds_icon_links_best_first() {
        let html = r#"<link rel="shortcut icon" href="/favicon.ico?v=5" /><link rel="apple-touch-icon-precomposed" href="/touch.png" />"#;
        assert_eq!(icon_links(html), vec!["/touch.png", "/favicon.ico?v=5"]);
    }

    #[test]
    fn finds_feed_links_in_html() {
        let html = r#"<html><head>
            <link rel="stylesheet" href="/s.css">
            <LINK REL="alternate" TYPE="application/rss+xml" title="RSS" HREF="/feed.xml">
            <link rel='alternate' type='application/atom+xml' href='https://x.com/atom?a=1&amp;b=2' />
            <link rel="alternate" hreflang="fr" href="/fr/">
            <link rel=alternate type=application/json href=https://x.com/feed.json>
        </head></html>"#;
        assert_eq!(feed_links(html), vec!["/feed.xml", "https://x.com/atom?a=1&b=2", "https://x.com/feed.json"]);
    }

    #[test]
    fn parses_rss_into_articles() {
        let rss = r#"<?xml version="1.0"?><rss version="2.0"><channel><title>Blog</title>
            <atom:link xmlns:atom="http://www.w3.org/2005/Atom" rel="hub" href="https://hub.example/"/>
            <link>https://blog.example/</link>
            <item><title>Post</title><link>https://blog.example/post</link><guid>p1</guid>
            <description>&lt;p&gt;Hello&lt;/p&gt;</description>
            <pubDate>Mon, 26 Oct 2026 10:00:00 GMT</pubDate>
            <enclosure url="https://blog.example/ep.mp3" type="audio/mpeg" length="1"/></item>
            </channel></rss>"#;
        let feed = parse(rss.as_bytes(), "https://blog.example/feed").unwrap();
        assert_eq!(title(&feed, ""), "Blog");
        assert_eq!(website(&feed, "https://blog.example/feed").as_deref(), Some("https://blog.example/"));
        let a = &articles("local:f", &feed)[0];
        assert_eq!(a.id, "local:f#p1");
        assert_eq!(a.url.as_deref(), Some("https://blog.example/post"));
        assert_eq!(a.title.as_deref(), Some("Post"));
        assert!(a.content_html.as_deref().unwrap().contains("Hello"));
        assert_eq!(a.enclosures[0].0, "https://blog.example/ep.mp3");
    }
}
