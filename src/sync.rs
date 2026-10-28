//! Feedly sync, following the phases of NetNewsWire's FeedlyAccountDelegate.refreshAll:
//! send local status changes, refresh the feed list, fetch new articles, then mirror the
//! unread and starred sets, downloading any articles those sets mention that we lack.
//!
//! Runs on a background thread. The store lock is held only around local writes.

use crate::credentials::{self, Credentials};
use crate::feedly::api::{ENTRIES_CHUNK_SIZE, FeedlyApi, MARK_CHUNK_SIZE, MarkAction};
use crate::feedly::models::{global_all, global_saved};
use crate::feedly::oauth::OAuthClient;
use crate::store::{self, StatusField, now_ms};
use anyhow::Result;

const DAY_MS: i64 = 24 * 60 * 60 * 1000;
/// How far back the first sync reaches.
const INITIAL_FETCH_DAYS: i64 = 14;
/// Re-fetch a little overlap to cover clock skew; re-fetching is idempotent.
const FETCH_OVERLAP_MS: i64 = 5 * 60 * 1000;
const STREAM_PAGE_SIZE: usize = 250;
const MAX_STREAM_PAGES: usize = 20;
const MAX_ID_PAGES: usize = 10;
/// Cap on articles downloaded by ID per sync; the rest come next time.
const MAX_MISSING_DOWNLOADS: usize = 5000;

const LAST_FETCH_KEY: &str = "feedly.last_fetch_ms";

#[derive(Debug, Default, Clone)]
pub struct Summary {
    pub statuses_sent: usize,
    pub feeds: usize,
    pub articles_downloaded: usize,
}

impl Summary {
    pub fn message(&self) -> String {
        let mut parts = vec![];
        if self.articles_downloaded > 0 {
            parts.push(format!("{} new or updated article{}", self.articles_downloaded, plural(self.articles_downloaded)));
        }
        if self.statuses_sent > 0 {
            parts.push(format!("{} change{} sent", self.statuses_sent, plural(self.statuses_sent)));
        }
        if parts.is_empty() { "Up to date".into() } else { parts.join(", ") }
    }
}

fn plural(n: usize) -> &'static str {
    if n == 1 { "" } else { "s" }
}

pub type Progress<'a> = &'a dyn Fn(&str);

pub fn refresh_all(credentials: Credentials, progress: Progress) -> Result<Summary> {
    let mut api = FeedlyApi::new(credentials, OAuthClient::load());
    let result = run(&mut api, progress);
    if api.credentials_changed
        && let Err(e) = credentials::save(&api.credentials)
    {
        log::error!("Could not save refreshed Feedly credentials: {e:#}");
    }
    result
}

fn run(api: &mut FeedlyApi, progress: Progress) -> Result<Summary> {
    let user_id = api.credentials.user_id.clone();
    let start = now_ms();
    let mut summary = Summary::default();

    progress("Sending changes…");
    match send_statuses(api) {
        Ok(n) => summary.statuses_sent = n,
        // A failed send must not block fetching new articles, except for auth and rate limits.
        Err(e) if is_fatal(&e) => return Err(e),
        Err(e) => log::warn!("Continuing sync despite status send failure: {e:#}"),
    }

    progress("Refreshing feeds…");
    let subscriptions = api.subscriptions()?;
    let categories = api.categories()?;
    summary.feeds = subscriptions.len();
    store::get().replace_feed_list(&subscriptions, &categories)?;

    progress("Downloading articles…");
    let last_fetch: Option<i64> = store::get().setting(LAST_FETCH_KEY).and_then(|v| v.parse().ok());
    let floor = start - INITIAL_FETCH_DAYS * DAY_MS;
    let newer_than = last_fetch.map(|t| (t - FETCH_OVERLAP_MS).max(start - store::RETENTION_DAYS * DAY_MS)).unwrap_or(floor);
    let all = global_all(&user_id);
    let mut continuation: Option<String> = None;
    let mut truncated = true;
    for _ in 0..MAX_STREAM_PAGES {
        let page = api.stream_contents(&all, continuation.as_deref(), Some(newer_than), STREAM_PAGE_SIZE)?;
        summary.articles_downloaded += store::get().upsert_entries(&page.items)?;
        continuation = page.continuation.filter(|c| !c.is_empty());
        if continuation.is_none() {
            truncated = false;
            break;
        }
    }

    progress("Syncing unread articles…");
    let (unread, unread_truncated) = api.all_stream_ids(&all, Some(start - store::RETENTION_DAYS * DAY_MS), true, MAX_ID_PAGES)?;
    progress("Syncing starred articles…");
    let (starred, starred_truncated) = api.all_stream_ids(&global_saved(&user_id), None, false, MAX_ID_PAGES)?;

    // Articles that are unread or starred remotely but not here yet — usually older ones.
    let mut wanted = unread.clone();
    wanted.extend(starred.iter().cloned());
    let missing = store::get().missing_ids(&wanted)?;
    if !missing.is_empty() {
        progress("Downloading missing articles…");
    }
    for chunk in missing.chunks(ENTRIES_CHUNK_SIZE).take(MAX_MISSING_DOWNLOADS / ENTRIES_CHUNK_SIZE) {
        let entries = api.entries(chunk)?;
        summary.articles_downloaded += store::get().upsert_entries(&entries)?;
    }

    {
        let mut s = store::get();
        // A truncated walk isn't authoritative about absence.
        s.apply_unread_ids(&unread, !unread_truncated)?;
        s.apply_starred_ids(&starred, !starred_truncated)?;
        s.delete_old_articles()?;
        // Don't advance the watermark past a window we didn't finish walking.
        if !truncated {
            s.set_setting(LAST_FETCH_KEY, &start.to_string())?;
        }
    }

    Ok(summary)
}

/// Sends queued read/starred changes in Feedly-sized batches. Returns how many were sent.
pub fn send_statuses(api: &mut FeedlyApi) -> Result<usize> {
    let pending = store::get().pending()?;
    let mut sent = 0;
    for (field, flag, action) in [
        (StatusField::Read, true, MarkAction::Read),
        (StatusField::Read, false, MarkAction::Unread),
        (StatusField::Starred, true, MarkAction::Saved),
        (StatusField::Starred, false, MarkAction::Unsaved),
    ] {
        let ids: Vec<String> = pending
            .iter()
            .filter(|(_, f, v)| *f == field && *v == flag)
            .map(|(id, _, _)| id.clone())
            .collect();
        for chunk in ids.chunks(MARK_CHUNK_SIZE) {
            api.mark(chunk, action)?;
            store::get().clear_pending(chunk, field, flag)?;
            sent += chunk.len();
        }
    }
    Ok(sent)
}

pub fn send_pending(credentials: Credentials) -> Result<usize> {
    let mut api = FeedlyApi::new(credentials, OAuthClient::load());
    let result = send_statuses(&mut api);
    if api.credentials_changed {
        let _ = credentials::save(&api.credentials);
    }
    result
}

pub fn is_fatal(e: &anyhow::Error) -> bool {
    use crate::feedly::api::FeedlyError;
    matches!(e.downcast_ref::<FeedlyError>(), Some(FeedlyError::Unauthorized | FeedlyError::RateLimited))
}

pub fn is_unauthorized(e: &anyhow::Error) -> bool {
    use crate::feedly::api::FeedlyError;
    matches!(e.downcast_ref::<FeedlyError>(), Some(FeedlyError::Unauthorized))
}
