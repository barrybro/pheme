//! Local SQLite storage for folders, feeds, articles, and status changes not yet sent to Feedly.
//! Plays the role of NetNewsWire's ArticlesDatabase and SyncDatabase.

use crate::feedly::models::{Category, Entry, Subscription};
use crate::text::html_to_text;
use anyhow::Result;
use rusqlite::{Connection, OptionalExtension, params, params_from_iter};
use std::collections::HashSet;
use std::path::Path;
use std::sync::{Mutex, MutexGuard, OnceLock};

static STORE: OnceLock<Mutex<Store>> = OnceLock::new();

/// The process-wide store. Hold the guard only for short, non-network work.
pub fn get() -> MutexGuard<'static, Store> {
    STORE
        .get_or_init(|| {
            let path = crate::paths::data_dir().join("pheme.db");
            Mutex::new(Store::open(&path).expect("open database"))
        })
        .lock()
        .unwrap_or_else(|e| e.into_inner())
}

/// Articles older than this, read and unstarred, are cleaned up.
pub const RETENTION_DAYS: i64 = 90;

pub struct Store {
    conn: Connection,
}

#[derive(Debug, Clone)]
pub struct FolderRow {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone)]
pub struct FeedRow {
    pub id: String,
    pub title: String,
    pub icon_url: Option<String>,
    pub unread: i64,
}

#[derive(Debug, Clone)]
pub struct TimelineRow {
    pub id: String,
    pub feed_title: String,
    pub icon_url: Option<String>,
    pub title: String,
    pub summary: String,
    pub published: i64,
    pub unread: bool,
    pub starred: bool,
    pub image_url: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ArticleDetail {
    pub feed_title: String,
    pub feed_website: Option<String>,
    pub icon_url: Option<String>,
    pub title: Option<String>,
    pub author: Option<String>,
    pub content_html: Option<String>,
    pub url: Option<String>,
    pub published: i64,
    pub enclosures: Vec<(String, Option<String>)>,
}

/// Which account a folder or feed belongs to. Local items have IDs starting with `local`;
/// Feedly's IDs never do (`feed/…`, `user/…`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Account {
    Local,
    Feedly,
}

impl Account {
    pub fn of(id: &str) -> Account {
        if id.starts_with("local") { Account::Local } else { Account::Feedly }
    }

    /// SQL condition selecting this account's rows by an ID column.
    fn condition(self, column: &str) -> String {
        match self {
            Account::Local => format!("{column} LIKE 'local%'"),
            Account::Feedly => format!("{column} NOT LIKE 'local%'"),
        }
    }
}

pub fn local_feed_id(url: &str) -> String {
    format!("local:{url}")
}

/// An article from a locally fetched feed.
#[derive(Debug, Clone, Default)]
pub struct LocalArticle {
    pub id: String,
    pub title: Option<String>,
    pub author: Option<String>,
    pub content_html: Option<String>,
    pub summary_html: Option<String>,
    pub url: Option<String>,
    pub image_url: Option<String>,
    pub enclosures: Vec<(String, Option<String>)>,
    pub published: i64,
}

/// HTTP caching state for a locally fetched feed.
#[derive(Debug, Clone, Default)]
pub struct LocalFeedState {
    pub feed_id: String,
    pub url: String,
    pub etag: Option<String>,
    pub last_modified: Option<String>,
}

/// What the sidebar has selected; also what the timeline shows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Selection {
    Today,
    AllUnread,
    Starred,
    Folder(String),
    Feed(String),
}

impl Selection {
    pub fn parse(key: &str) -> Option<Self> {
        let (kind, id) = key.split_once(':')?;
        Some(match kind {
            "smart" => match id {
                "today" => Selection::Today,
                "unread" => Selection::AllUnread,
                "starred" => Selection::Starred,
                _ => return None,
            },
            "folder" => Selection::Folder(id.to_string()),
            "feed" => Selection::Feed(id.to_string()),
            _ => return None,
        })
    }

    /// SQL condition on `a` (articles) plus its parameters.
    fn condition(&self) -> (String, Vec<String>) {
        match self {
            Selection::Today => ("a.published >= ?".into(), vec![start_of_today_ms().to_string()]),
            Selection::AllUnread => ("a.unread = 1".into(), vec![]),
            Selection::Starred => ("a.starred = 1".into(), vec![]),
            Selection::Folder(id) => (
                "a.feed_id IN (SELECT feed_id FROM folder_feeds WHERE folder_id = ?)".into(),
                vec![id.clone()],
            ),
            Selection::Feed(id) => ("a.feed_id = ?".into(), vec![id.clone()]),
        }
    }
}

/// "Today" means the last 24 hours, as in NetNewsWire.
fn start_of_today_ms() -> i64 {
    chrono::Utc::now().timestamp_millis() - 24 * 60 * 60 * 1000
}

pub fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatusField {
    Read,
    Starred,
}

impl StatusField {
    fn as_str(self) -> &'static str {
        match self {
            StatusField::Read => "read",
            StatusField::Starred => "starred",
        }
    }
}

const SCHEMA: &str = r#"
PRAGMA journal_mode = WAL;
PRAGMA foreign_keys = ON;
CREATE TABLE IF NOT EXISTS folders (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    sort_order INTEGER NOT NULL DEFAULT 0
);
CREATE TABLE IF NOT EXISTS feeds (
    id TEXT PRIMARY KEY,
    title TEXT NOT NULL,
    website TEXT,
    icon_url TEXT
);
CREATE TABLE IF NOT EXISTS folder_feeds (
    folder_id TEXT NOT NULL REFERENCES folders(id) ON DELETE CASCADE,
    feed_id TEXT NOT NULL REFERENCES feeds(id) ON DELETE CASCADE,
    PRIMARY KEY (folder_id, feed_id)
);
CREATE TABLE IF NOT EXISTS articles (
    id TEXT PRIMARY KEY,
    feed_id TEXT NOT NULL,
    title TEXT,
    author TEXT,
    content_html TEXT,
    summary_text TEXT,
    url TEXT,
    image_url TEXT,
    enclosures TEXT,
    published INTEGER NOT NULL,
    crawled INTEGER NOT NULL,
    unread INTEGER NOT NULL DEFAULT 1,
    starred INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX IF NOT EXISTS articles_feed ON articles(feed_id, published);
CREATE INDEX IF NOT EXISTS articles_published ON articles(published);
CREATE INDEX IF NOT EXISTS articles_unread ON articles(unread);
CREATE TABLE IF NOT EXISTS pending_status (
    article_id TEXT NOT NULL,
    field TEXT NOT NULL,
    flag INTEGER NOT NULL,
    PRIMARY KEY (article_id, field)
);
CREATE TABLE IF NOT EXISTS local_feeds (
    feed_id TEXT PRIMARY KEY REFERENCES feeds(id) ON DELETE CASCADE,
    url TEXT NOT NULL,
    etag TEXT,
    last_modified TEXT,
    last_error TEXT
);
CREATE TABLE IF NOT EXISTS settings (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL
);
"#;

impl Store {
    pub fn open(path: &Path) -> Result<Self> {
        let conn = Connection::open(path)?;
        conn.execute_batch(SCHEMA)?;
        Ok(Self { conn })
    }

    #[cfg(test)]
    pub fn in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        conn.execute_batch(SCHEMA)?;
        Ok(Self { conn })
    }

    // MARK: Settings

    pub fn setting(&self, key: &str) -> Option<String> {
        self.conn
            .query_row("SELECT value FROM settings WHERE key = ?", [key], |r| r.get(0))
            .optional()
            .ok()
            .flatten()
    }

    pub fn set_setting(&self, key: &str, value: &str) -> Result<()> {
        self.conn.execute(
            "INSERT INTO settings(key, value) VALUES(?, ?) ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            [key, value],
        )?;
        Ok(())
    }

    /// Forgets the Feedly account's data; used when signing out. Local feeds stay.
    pub fn clear_feedly(&mut self) -> Result<()> {
        self.conn.execute_batch(
            "DELETE FROM folder_feeds WHERE folder_id NOT LIKE 'local%';
             DELETE FROM folders WHERE id NOT LIKE 'local%';
             DELETE FROM articles WHERE feed_id NOT LIKE 'local%';
             DELETE FROM feeds WHERE id NOT LIKE 'local%';
             DELETE FROM pending_status; DELETE FROM settings WHERE key LIKE 'feedly.%';",
        )?;
        Ok(())
    }

    // MARK: Feed list

    /// Mirrors Feedly's subscriptions and categories. Feeds and folders that vanished remotely are removed.
    pub fn replace_feed_list(&mut self, subs: &[Subscription], categories: &[Category]) -> Result<()> {
        let tx = self.conn.transaction()?;
        tx.execute("DELETE FROM folder_feeds WHERE folder_id NOT LIKE 'local%'", [])?;
        tx.execute("DELETE FROM folders WHERE id NOT LIKE 'local%'", [])?;
        for (i, c) in categories.iter().enumerate() {
            // Global categories (global.all, global.must…) aren't user folders.
            if c.id.contains("/category/global.") {
                continue;
            }
            let name = c.label.clone().unwrap_or_else(|| "Untitled".into());
            tx.execute("INSERT OR REPLACE INTO folders(id, name, sort_order) VALUES(?, ?, ?)", params![c.id, name, i as i64])?;
        }
        let mut keep = HashSet::new();
        for s in subs {
            keep.insert(s.id.clone());
            let title = s
                .title
                .clone()
                .filter(|t| !t.trim().is_empty())
                .unwrap_or_else(|| crate::feedly::models::feed_url_from_id(&s.id).to_string());
            let icon = s.icon_url.clone().or_else(|| s.visual_url.clone());
            tx.execute(
                "INSERT INTO feeds(id, title, website, icon_url) VALUES(?, ?, ?, ?)
                 ON CONFLICT(id) DO UPDATE SET title = excluded.title, website = excluded.website, icon_url = excluded.icon_url",
                params![s.id, title, s.website, icon],
            )?;
            for c in &s.categories {
                if c.id.contains("/category/global.") {
                    continue;
                }
                let name = c.label.clone().unwrap_or_else(|| "Untitled".into());
                tx.execute("INSERT OR IGNORE INTO folders(id, name, sort_order) VALUES(?, ?, 9999)", params![c.id, name])?;
                tx.execute("INSERT OR IGNORE INTO folder_feeds(folder_id, feed_id) VALUES(?, ?)", params![c.id, s.id])?;
            }
        }
        let existing: Vec<String> = {
            let mut stmt = tx.prepare("SELECT id FROM feeds WHERE id NOT LIKE 'local%'")?;
            stmt.query_map([], |r| r.get(0))?.collect::<Result<_, _>>()?
        };
        for id in existing.iter().filter(|id| !keep.contains(*id)) {
            tx.execute("DELETE FROM feeds WHERE id = ?", [id])?;
            tx.execute("DELETE FROM articles WHERE feed_id = ? AND starred = 0", [id])?;
        }
        tx.commit()?;
        Ok(())
    }

    pub fn folders(&self, account: Account) -> Result<Vec<FolderRow>> {
        let sql = format!(
            "SELECT id, name FROM folders WHERE {} ORDER BY sort_order, name COLLATE NOCASE",
            account.condition("id")
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map([], |r| Ok(FolderRow { id: r.get(0)?, name: r.get(1)? }))?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    fn feeds_where(&self, condition: &str, args: &[&str]) -> Result<Vec<FeedRow>> {
        let sql = format!(
            "SELECT f.id, f.title, f.icon_url,
                    (SELECT COUNT(*) FROM articles a WHERE a.feed_id = f.id AND a.unread = 1)
             FROM feeds f WHERE {condition} ORDER BY f.title COLLATE NOCASE"
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(params_from_iter(args), |r| {
            Ok(FeedRow { id: r.get(0)?, title: r.get(1)?, icon_url: r.get(2)?, unread: r.get(3)? })
        })?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    pub fn feeds_in_folder(&self, folder_id: &str) -> Result<Vec<FeedRow>> {
        self.feeds_where("f.id IN (SELECT feed_id FROM folder_feeds WHERE folder_id = ?)", &[folder_id])
    }

    pub fn unfoldered_feeds(&self, account: Account) -> Result<Vec<FeedRow>> {
        let condition = format!("f.id NOT IN (SELECT feed_id FROM folder_feeds) AND {}", account.condition("f.id"));
        self.feeds_where(&condition, &[])
    }

    pub fn unread_count(&self, selection: &Selection) -> Result<i64> {
        let (cond, args) = selection.condition();
        let sql = format!("SELECT COUNT(*) FROM articles a WHERE {cond} AND a.unread = 1");
        Ok(self.conn.query_row(&sql, params_from_iter(args), |r| r.get(0))?)
    }

    // MARK: Articles

    /// Inserts or updates entries. Read/starred state from Feedly is applied unless a
    /// local change for that article is still waiting to be sent.
    pub fn upsert_entries(&mut self, entries: &[Entry]) -> Result<usize> {
        let tx = self.conn.transaction()?;
        let mut count = 0;
        {
            let mut stmt = tx.prepare(
                "INSERT INTO articles(id, feed_id, title, author, content_html, summary_text, url, image_url,
                                      enclosures, published, crawled, unread, starred)
                 VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
                 ON CONFLICT(id) DO UPDATE SET
                    title = excluded.title, author = excluded.author, content_html = excluded.content_html,
                    summary_text = excluded.summary_text, url = excluded.url, image_url = excluded.image_url,
                    enclosures = excluded.enclosures, published = excluded.published,
                    unread = CASE WHEN EXISTS(SELECT 1 FROM pending_status p WHERE p.article_id = ?1 AND p.field = 'read')
                             THEN articles.unread ELSE excluded.unread END,
                    starred = CASE WHEN EXISTS(SELECT 1 FROM pending_status p WHERE p.article_id = ?1 AND p.field = 'starred')
                             THEN articles.starred ELSE excluded.starred END",
            )?;
            for e in entries {
                let Some(feed_id) = e.feed_id() else { continue };
                let content = e.content_html();
                let summary_source = e.summary_html().or_else(|| content.clone()).unwrap_or_default();
                let summary = html_to_text(&summary_source, 300);
                let enclosures: Vec<(String, Option<String>)> =
                    e.enclosures().into_iter().map(|l| (l.href, l.mime_type)).collect();
                stmt.execute(params![
                    e.id,
                    feed_id,
                    e.clean_title(),
                    e.author,
                    content,
                    summary,
                    e.external_url(),
                    e.image_url(),
                    serde_json::to_string(&enclosures)?,
                    e.date_published(),
                    e.crawled,
                    e.unread,
                    e.is_saved(),
                ])?;
                count += 1;
            }
        }
        tx.commit()?;
        Ok(count)
    }

    fn load_id_set(tx: &rusqlite::Transaction, ids: &[String]) -> Result<()> {
        tx.execute_batch("CREATE TEMP TABLE IF NOT EXISTS id_set(id TEXT PRIMARY KEY); DELETE FROM id_set;")?;
        let mut stmt = tx.prepare("INSERT OR IGNORE INTO id_set(id) VALUES(?)")?;
        for id in ids {
            stmt.execute([id])?;
        }
        Ok(())
    }

    /// Mirrors Feedly's unread set. When `authoritative`, local unread articles absent
    /// from the set become read. Pending local edits are left alone.
    pub fn apply_unread_ids(&mut self, ids: &[String], authoritative: bool) -> Result<()> {
        let tx = self.conn.transaction()?;
        Self::load_id_set(&tx, ids)?;
        // Only Feedly's articles: Feedly knows nothing about local feeds.
        let not_pending = "id NOT LIKE 'local%' AND id NOT IN (SELECT article_id FROM pending_status WHERE field = 'read')";
        tx.execute(&format!("UPDATE articles SET unread = 1 WHERE unread = 0 AND id IN (SELECT id FROM id_set) AND {not_pending}"), [])?;
        if authoritative {
            tx.execute(&format!("UPDATE articles SET unread = 0 WHERE unread = 1 AND id NOT IN (SELECT id FROM id_set) AND {not_pending}"), [])?;
        }
        tx.commit()?;
        Ok(())
    }

    pub fn apply_starred_ids(&mut self, ids: &[String], authoritative: bool) -> Result<()> {
        let tx = self.conn.transaction()?;
        Self::load_id_set(&tx, ids)?;
        // Only Feedly's articles: Feedly knows nothing about local feeds.
        let not_pending = "id NOT LIKE 'local%' AND id NOT IN (SELECT article_id FROM pending_status WHERE field = 'starred')";
        tx.execute(&format!("UPDATE articles SET starred = 1 WHERE starred = 0 AND id IN (SELECT id FROM id_set) AND {not_pending}"), [])?;
        if authoritative {
            tx.execute(&format!("UPDATE articles SET starred = 0 WHERE starred = 1 AND id NOT IN (SELECT id FROM id_set) AND {not_pending}"), [])?;
        }
        tx.commit()?;
        Ok(())
    }

    /// The subset of `ids` not stored locally.
    pub fn missing_ids(&mut self, ids: &[String]) -> Result<Vec<String>> {
        let tx = self.conn.transaction()?;
        Self::load_id_set(&tx, ids)?;
        let missing = {
            let mut stmt = tx.prepare("SELECT id FROM id_set WHERE id NOT IN (SELECT id FROM articles)")?;
            stmt.query_map([], |r| r.get(0))?.collect::<Result<Vec<String>, _>>()?
        };
        tx.commit()?;
        Ok(missing)
    }

    pub fn delete_old_articles(&self) -> Result<usize> {
        let cutoff = now_ms() - RETENTION_DAYS * 24 * 60 * 60 * 1000;
        Ok(self.conn.execute(
            "DELETE FROM articles WHERE published < ? AND unread = 0 AND starred = 0
             AND id NOT IN (SELECT article_id FROM pending_status)",
            [cutoff],
        )?)
    }

    pub fn timeline(&self, selection: &Selection, hide_read: bool, search: &str) -> Result<Vec<TimelineRow>> {
        let (cond, mut args) = selection.condition();
        let mut sql = format!(
            "SELECT a.id, COALESCE(f.title, ''), f.icon_url, COALESCE(a.title, ''),
                    COALESCE(a.summary_text, ''), a.published, a.unread, a.starred, a.image_url
             FROM articles a LEFT JOIN feeds f ON f.id = a.feed_id
             WHERE {cond}"
        );
        // Starred articles stay visible with the read filter on, as in NetNewsWire's Starred feed.
        if hide_read && *selection != Selection::Starred {
            sql.push_str(" AND a.unread = 1");
        }
        let search = search.trim();
        if !search.is_empty() {
            sql.push_str(" AND (a.title LIKE ? ESCAPE '\\' OR a.summary_text LIKE ? ESCAPE '\\' OR a.author LIKE ? ESCAPE '\\')");
            let pattern = format!("%{}%", search.replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_"));
            args.extend([pattern.clone(), pattern.clone(), pattern]);
        }
        sql.push_str(" ORDER BY a.published DESC LIMIT 5000");
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(params_from_iter(args), |r| {
            Ok(TimelineRow {
                id: r.get(0)?,
                feed_title: r.get(1)?,
                icon_url: r.get(2)?,
                title: r.get(3)?,
                summary: r.get(4)?,
                published: r.get(5)?,
                unread: r.get(6)?,
                starred: r.get(7)?,
                image_url: r.get(8)?,
            })
        })?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    pub fn article(&self, id: &str) -> Result<Option<ArticleDetail>> {
        Ok(self
            .conn
            .query_row(
                "SELECT COALESCE(f.title, ''), f.website, f.icon_url, a.title, a.author, a.content_html,
                        a.url, a.published, a.enclosures
                 FROM articles a LEFT JOIN feeds f ON f.id = a.feed_id WHERE a.id = ?",
                [id],
                |r| {
                    let enclosures: Option<String> = r.get(8)?;
                    Ok(ArticleDetail {
                        feed_title: r.get(0)?,
                        feed_website: r.get(1)?,
                        icon_url: r.get(2)?,
                        title: r.get(3)?,
                        author: r.get(4)?,
                        content_html: r.get(5)?,
                        url: r.get(6)?,
                        published: r.get(7)?,
                        enclosures: enclosures.and_then(|e| serde_json::from_str(&e).ok()).unwrap_or_default(),
                    })
                },
            )
            .optional()?)
    }

    // MARK: Local feeds

    pub fn local_feed_exists(&self, feed_id: &str) -> bool {
        self.conn
            .query_row("SELECT 1 FROM feeds WHERE id = ?", [feed_id], |_| Ok(()))
            .optional()
            .ok()
            .flatten()
            .is_some()
    }

    pub fn add_local_feed(&mut self, state: &LocalFeedState, title: &str, website: Option<&str>, icon_url: Option<&str>, folder_id: Option<&str>) -> Result<()> {
        let tx = self.conn.transaction()?;
        tx.execute(
            "INSERT INTO feeds(id, title, website, icon_url) VALUES(?, ?, ?, ?)
             ON CONFLICT(id) DO UPDATE SET title = excluded.title, website = excluded.website, icon_url = excluded.icon_url",
            params![state.feed_id, title, website, icon_url],
        )?;
        tx.execute(
            "INSERT OR REPLACE INTO local_feeds(feed_id, url, etag, last_modified) VALUES(?, ?, ?, ?)",
            params![state.feed_id, state.url, state.etag, state.last_modified],
        )?;
        tx.execute("DELETE FROM folder_feeds WHERE feed_id = ?", [&state.feed_id])?;
        if let Some(folder) = folder_id {
            tx.execute("INSERT INTO folder_feeds(folder_id, feed_id) VALUES(?, ?)", params![folder, state.feed_id])?;
        }
        tx.commit()?;
        Ok(())
    }

    pub fn local_feed_states(&self) -> Result<Vec<LocalFeedState>> {
        let mut stmt = self.conn.prepare("SELECT feed_id, url, etag, last_modified FROM local_feeds")?;
        let rows = stmt.query_map([], |r| {
            Ok(LocalFeedState { feed_id: r.get(0)?, url: r.get(1)?, etag: r.get(2)?, last_modified: r.get(3)? })
        })?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    pub fn update_local_feed_state(&self, state: &LocalFeedState, error: Option<&str>) -> Result<()> {
        self.conn.execute(
            "UPDATE local_feeds SET etag = ?, last_modified = ?, last_error = ? WHERE feed_id = ?",
            params![state.etag, state.last_modified, error, state.feed_id],
        )?;
        Ok(())
    }

    pub fn feed_has_icon(&self, feed_id: &str) -> bool {
        self.conn
            .query_row("SELECT icon_url IS NOT NULL FROM feeds WHERE id = ?", [feed_id], |r| r.get(0))
            .unwrap_or(false)
    }

    pub fn set_feed_icon(&self, feed_id: &str, icon_url: &str) -> Result<()> {
        self.conn.execute("UPDATE feeds SET icon_url = ? WHERE id = ? AND icon_url IS NULL", params![icon_url, feed_id])?;
        Ok(())
    }

    /// Stores a local feed's articles. New ones arrive unread; existing ones keep their
    /// read/starred state. Articles older than the retention window aren't added.
    pub fn upsert_local_articles(&mut self, feed_id: &str, articles: &[LocalArticle]) -> Result<usize> {
        let cutoff = now_ms() - RETENTION_DAYS * 24 * 60 * 60 * 1000;
        let tx = self.conn.transaction()?;
        let mut added = 0;
        {
            let mut exists = tx.prepare("SELECT 1 FROM articles WHERE id = ?")?;
            let mut insert = tx.prepare(
                "INSERT INTO articles(id, feed_id, title, author, content_html, summary_text, url, image_url,
                                      enclosures, published, crawled, unread, starred)
                 VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, 1, 0)
                 ON CONFLICT(id) DO UPDATE SET
                    title = excluded.title, author = excluded.author, content_html = excluded.content_html,
                    summary_text = excluded.summary_text, url = excluded.url, image_url = excluded.image_url,
                    enclosures = excluded.enclosures",
            )?;
            let now = now_ms();
            for a in articles {
                let is_new = exists.query_row([&a.id], |_| Ok(())).optional()?.is_none();
                if is_new && a.published < cutoff {
                    continue;
                }
                let summary_source = a.summary_html.clone().or_else(|| a.content_html.clone()).unwrap_or_default();
                insert.execute(params![
                    a.id,
                    feed_id,
                    a.title,
                    a.author,
                    a.content_html,
                    html_to_text(&summary_source, 300),
                    a.url,
                    a.image_url,
                    serde_json::to_string(&a.enclosures)?,
                    a.published.min(now),
                    now,
                ])?;
                if is_new {
                    added += 1;
                }
            }
        }
        tx.commit()?;
        Ok(added)
    }

    pub fn create_local_folder(&self, name: &str) -> Result<String> {
        use rand::RngExt;
        let id = format!("local-folder:{:016x}", rand::rng().random::<u64>());
        self.conn.execute(
            "INSERT INTO folders(id, name, sort_order) VALUES(?, ?, 0)",
            params![id, name],
        )?;
        Ok(id)
    }

    pub fn rename_local(&self, id: &str, name: &str) -> Result<()> {
        if Account::of(id) != Account::Local {
            anyhow::bail!("Only items under On My Computer can be renamed in Pheme.");
        }
        self.conn.execute("UPDATE folders SET name = ? WHERE id = ?", params![name, id])?;
        self.conn.execute("UPDATE feeds SET title = ? WHERE id = ?", params![name, id])?;
        Ok(())
    }

    /// Deletes a local feed, or a local folder along with its feeds, and their articles.
    pub fn delete_local(&mut self, id: &str) -> Result<()> {
        if Account::of(id) != Account::Local {
            anyhow::bail!("Only items under On My Computer can be deleted in Pheme.");
        }
        let tx = self.conn.transaction()?;
        let feeds: Vec<String> = {
            let mut stmt = tx.prepare("SELECT feed_id FROM folder_feeds WHERE folder_id = ?")?;
            stmt.query_map([id], |r| r.get(0))?.collect::<Result<_, _>>()?
        };
        for feed in feeds.iter().map(String::as_str).chain(std::iter::once(id)) {
            tx.execute("DELETE FROM articles WHERE feed_id = ?", [feed])?;
            tx.execute("DELETE FROM feeds WHERE id = ?", [feed])?;
        }
        tx.execute("DELETE FROM folders WHERE id = ?", [id])?;
        tx.commit()?;
        Ok(())
    }

    /// Moves a local feed into a local folder, or to the top level with `None`.
    pub fn move_local_feed(&self, feed_id: &str, folder_id: Option<&str>) -> Result<()> {
        self.conn.execute("DELETE FROM folder_feeds WHERE feed_id = ?", [feed_id])?;
        if let Some(folder) = folder_id {
            self.conn.execute("INSERT INTO folder_feeds(folder_id, feed_id) VALUES(?, ?)", params![folder, feed_id])?;
        }
        Ok(())
    }

    pub fn folder_named(&self, account: Account, name: &str) -> Result<Option<String>> {
        let sql = format!("SELECT id FROM folders WHERE name = ? AND {}", account.condition("id"));
        Ok(self.conn.query_row(&sql, [name], |r| r.get(0)).optional()?)
    }

    /// Local subscriptions for OPML export: (folder name, feed title, feed URL, website).
    pub fn local_subscriptions(&self) -> Result<Vec<(Option<String>, String, String, Option<String>)>> {
        let mut stmt = self.conn.prepare(
            "SELECT fo.name, f.title, lf.url, f.website
             FROM local_feeds lf JOIN feeds f ON f.id = lf.feed_id
             LEFT JOIN folder_feeds ff ON ff.feed_id = f.id
             LEFT JOIN folders fo ON fo.id = ff.folder_id
             ORDER BY fo.name COLLATE NOCASE, f.title COLLATE NOCASE",
        )?;
        let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    // MARK: Status changes

    /// Sets a status locally and queues it for Feedly. Returns the IDs that actually changed.
    pub fn set_status(&mut self, ids: &[String], field: StatusField, flag: bool) -> Result<Vec<String>> {
        let tx = self.conn.transaction()?;
        let mut changed = Vec::new();
        {
            let (column, value) = match field {
                StatusField::Read => ("unread", !flag),
                StatusField::Starred => ("starred", flag),
            };
            let mut update = tx.prepare(&format!("UPDATE articles SET {column} = ?1 WHERE id = ?2 AND {column} != ?1"))?;
            let mut queue = tx.prepare(
                "INSERT INTO pending_status(article_id, field, flag) VALUES(?, ?, ?)
                 ON CONFLICT(article_id, field) DO UPDATE SET flag = excluded.flag",
            )?;
            for id in ids {
                if update.execute(params![value, id])? > 0 {
                    if Account::of(id) == Account::Feedly {
                        queue.execute(params![id, field.as_str(), flag])?;
                    }
                    changed.push(id.clone());
                }
            }
        }
        tx.commit()?;
        Ok(changed)
    }

    /// Pending changes as (article_id, field, flag).
    pub fn pending(&self) -> Result<Vec<(String, StatusField, bool)>> {
        let mut stmt = self.conn.prepare("SELECT article_id, field, flag FROM pending_status")?;
        let rows = stmt.query_map([], |r| {
            let field: String = r.get(1)?;
            let field = if field == "read" { StatusField::Read } else { StatusField::Starred };
            Ok((r.get(0)?, field, r.get(2)?))
        })?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    /// Removes sent changes — but only if they weren't changed again while sending.
    pub fn clear_pending(&mut self, ids: &[String], field: StatusField, flag: bool) -> Result<()> {
        let tx = self.conn.transaction()?;
        {
            let mut stmt = tx.prepare("DELETE FROM pending_status WHERE article_id = ? AND field = ? AND flag = ?")?;
            for id in ids {
                stmt.execute(params![id, field.as_str(), flag])?;
            }
        }
        tx.commit()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(id: &str, feed: &str, unread: bool) -> Entry {
        serde_json::from_value(serde_json::json!({
            "id": id, "crawled": now_ms(), "published": now_ms(), "unread": unread,
            "title": format!("Title {id}"), "origin": {"streamId": feed},
            "summary": {"content": "<p>Hello &amp; welcome</p>"}
        }))
        .unwrap()
    }

    #[test]
    fn pending_changes_survive_remote_state() {
        let mut s = Store::in_memory().unwrap();
        s.upsert_entries(&[entry("a", "feed/x", true), entry("b", "feed/x", true)]).unwrap();
        assert_eq!(s.set_status(&["a".into()], StatusField::Read, true).unwrap(), vec!["a".to_string()]);

        // Feedly still says both are unread; the pending local read must win.
        s.upsert_entries(&[entry("a", "feed/x", true)]).unwrap();
        s.apply_unread_ids(&["a".into(), "b".into()], true).unwrap();
        assert_eq!(s.unread_count(&Selection::Feed("feed/x".into())).unwrap(), 1);

        s.clear_pending(&["a".into()], StatusField::Read, true).unwrap();
        s.apply_unread_ids(&["b".into()], true).unwrap();
        let unread: Vec<String> = s.timeline(&Selection::AllUnread, false, "").unwrap().into_iter().map(|r| r.id).collect();
        assert_eq!(unread, vec!["b".to_string()]);
    }

    #[test]
    fn timeline_filters_and_searches() {
        let mut s = Store::in_memory().unwrap();
        s.upsert_entries(&[entry("a", "feed/x", true), entry("b", "feed/x", false)]).unwrap();
        let sel = Selection::Feed("feed/x".into());
        assert_eq!(s.timeline(&sel, false, "").unwrap().len(), 2);
        assert_eq!(s.timeline(&sel, true, "").unwrap().len(), 1);
        assert_eq!(s.timeline(&sel, false, "Title b").unwrap()[0].id, "b");
        assert_eq!(s.timeline(&sel, false, "").unwrap()[0].summary, "Hello & welcome");
        assert_eq!(s.missing_ids(&["a".into(), "z".into()]).unwrap(), vec!["z".to_string()]);
    }

    #[test]
    fn local_articles_are_independent_of_feedly() {
        let mut s = Store::in_memory().unwrap();
        let feed = local_feed_id("https://example.com/feed.xml");
        let state = LocalFeedState { feed_id: feed.clone(), url: "https://example.com/feed.xml".into(), ..Default::default() };
        s.add_local_feed(&state, "Example", None, None, None).unwrap();
        let article = LocalArticle { id: format!("{feed}#1"), title: Some("Hi".into()), published: now_ms(), ..Default::default() };
        assert_eq!(s.upsert_local_articles(&feed, &[article.clone()]).unwrap(), 1);

        // Reading it locally queues nothing for Feedly, and re-fetching keeps it read.
        s.set_status(&[article.id.clone()], StatusField::Read, true).unwrap();
        assert!(s.pending().unwrap().is_empty());
        assert_eq!(s.upsert_local_articles(&feed, &[article.clone()]).unwrap(), 0);
        s.set_status(&[article.id.clone()], StatusField::Read, false).unwrap();

        // Feedly's authoritative unread set must not touch local articles.
        s.apply_unread_ids(&[], true).unwrap();
        assert_eq!(s.unread_count(&Selection::Feed(feed.clone())).unwrap(), 1);
        s.set_setting("local.default_feeds_added", "1").unwrap();
        s.clear_feedly().unwrap();
        assert_eq!(s.unfoldered_feeds(Account::Local).unwrap().len(), 1);
        assert!(s.setting("local.default_feeds_added").is_some(), "signing out of Feedly must not re-add default feeds");

        // Very old articles aren't imported.
        let old = LocalArticle { id: format!("{feed}#old"), published: 0, ..Default::default() };
        assert_eq!(s.upsert_local_articles(&feed, &[old]).unwrap(), 0);

        let folder = s.create_local_folder("Blogs").unwrap();
        s.move_local_feed(&feed, Some(&folder)).unwrap();
        assert_eq!(s.feeds_in_folder(&folder).unwrap().len(), 1);
        s.delete_local(&folder).unwrap();
        assert!(s.unfoldered_feeds(Account::Local).unwrap().is_empty());
        assert_eq!(s.unread_count(&Selection::AllUnread).unwrap(), 0);
    }
}
