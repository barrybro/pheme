//! The app-wide QObject: Feedly sign-in, sync, and article rendering.

#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
    }

    #[auto_cxx_name]
    unsafe extern "RustQt" {
        #[qobject]
        #[qml_element]
        #[qproperty(bool, signed_in)]
        #[qproperty(QString, account_name)]
        #[qproperty(bool, syncing)]
        #[qproperty(bool, signing_in)]
        #[qproperty(QString, status)]
        #[qproperty(QString, error_message)]
        #[qproperty(bool, oauth_available)]
        #[qproperty(QString, last_refreshed)]
        #[qproperty(bool, needs_sign_in)]
        #[qproperty(bool, adding_feed)]
        type AppController = super::AppControllerRust;

        /// Opens the browser to sign in to Feedly with OAuth.
        #[qinvokable]
        fn sign_in_with_browser(self: Pin<&mut AppController>);

        #[qinvokable]
        fn cancel_sign_in(self: Pin<&mut AppController>);

        /// Signs in with a Feedly developer access token.
        #[qinvokable]
        fn sign_in_with_token(self: Pin<&mut AppController>, token: &QString);

        #[qinvokable]
        fn sign_out(self: Pin<&mut AppController>);

        #[qinvokable]
        fn refresh(self: Pin<&mut AppController>);

        /// Sends queued read/starred changes to Feedly.
        #[qinvokable]
        fn send_pending(self: Pin<&mut AppController>);

        /// Renders an article (or, with an empty ID, `placeholder`) to a page and returns its
        /// file:// URL for the web view to load.
        #[qinvokable]
        fn article_page(self: &AppController, article_id: &QString, placeholder: &QString, text_size: i32, dark: bool) -> QString;

        #[qinvokable]
        fn open_url(self: &AppController, url: &QString);

        #[qinvokable]
        fn clear_error(self: Pin<&mut AppController>);

        /// Subscribes to a feed in the local account. `folder_key` is a sidebar key or empty.
        /// Finishes with `feed_added`.
        #[qinvokable]
        fn add_feed(self: Pin<&mut AppController>, url: &QString, name: &QString, folder_key: &QString);

        /// Creates a local folder and returns its sidebar key, or an empty string on failure.
        #[qinvokable]
        fn add_folder(self: Pin<&mut AppController>, name: &QString) -> QString;

        #[qinvokable]
        fn rename_item(self: Pin<&mut AppController>, key: &QString, name: &QString) -> bool;

        #[qinvokable]
        fn delete_item(self: Pin<&mut AppController>, key: &QString) -> bool;

        /// Moves a local feed into a local folder; an empty `folder_key` means the top level.
        #[qinvokable]
        fn move_feed(self: Pin<&mut AppController>, feed_key: &QString, folder_key: &QString) -> bool;

        #[qinvokable]
        fn import_opml(self: Pin<&mut AppController>, file_url: &QString);

        #[qinvokable]
        fn export_opml(self: Pin<&mut AppController>, file_url: &QString);

        /// The feed list or articles changed; views should reload.
        #[qsignal]
        fn data_changed(self: Pin<&mut AppController>);

        #[qsignal]
        fn feed_added(self: Pin<&mut AppController>, ok: bool, message: QString, key: QString);
    }

    impl cxx_qt::Threading for AppController {}
    impl cxx_qt::Initialize for AppController {}
}

use crate::credentials::{self, CredentialKind, Credentials};
use crate::feedly::api::{FeedlyApi, http_client};
use crate::feedly::oauth::{OAuthClient, open_in_browser};
use crate::{local, render, store, sync};
use core::pin::Pin;
use cxx_qt::{CxxQtType, Threading};
use cxx_qt_lib::QString;
use qobject::*;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

#[derive(Default)]
pub struct AppControllerRust {
    signed_in: bool,
    account_name: QString,
    syncing: bool,
    signing_in: bool,
    status: QString,
    error_message: QString,
    oauth_available: bool,
    last_refreshed: QString,
    needs_sign_in: bool,
    adding_feed: bool,
    credentials: Option<Credentials>,
    cancel_sign_in: Option<Arc<AtomicBool>>,
    sending: bool,
}

impl cxx_qt::Initialize for AppController {
    fn initialize(mut self: Pin<&mut Self>) {
        if let Err(e) = local::add_default_feeds() {
            log::error!("Adding the default feeds failed: {e:#}");
        }
        self.as_mut().set_oauth_available(OAuthClient::load().is_some());
        if let Some(c) = credentials::load() {
            self.as_mut().set_account_name(QString::from(c.display_name.as_str()));
            self.as_mut().rust_mut().credentials = Some(c);
            self.as_mut().set_signed_in(true);
        }
    }
}

impl AppController {
    fn fail(mut self: Pin<&mut Self>, message: String) {
        log::error!("{message}");
        self.as_mut().set_error_message(QString::from(message.as_str()));
    }

    /// Validates fresh credentials against /v3/profile and stores them.
    fn finish_sign_in(mut self: Pin<&mut Self>, result: anyhow::Result<Credentials>) {
        self.as_mut().set_signing_in(false);
        self.as_mut().rust_mut().cancel_sign_in = None;
        match result {
            Ok(c) => {
                // A different account's articles must not linger.
                let previous = store::get().setting("feedly.user_id");
                if previous.as_deref() != Some(c.user_id.as_str()) {
                    let mut s = store::get();
                    let _ = s.clear_feedly();
                    let _ = s.set_setting("feedly.user_id", &c.user_id);
                }
                if let Err(e) = credentials::save(&c) {
                    self.as_mut().fail(format!("Could not save your Feedly sign-in: {e:#}"));
                }
                self.as_mut().set_account_name(QString::from(c.display_name.as_str()));
                self.as_mut().rust_mut().credentials = Some(c);
                self.as_mut().set_signed_in(true);
                self.as_mut().set_needs_sign_in(false);
                self.as_mut().set_error_message(QString::default());
                self.as_mut().data_changed();
                self.refresh();
            }
            Err(e) => self.fail(format!("{e:#}")),
        }
    }

    fn spawn_sign_in(mut self: Pin<&mut Self>, work: impl FnOnce(Arc<AtomicBool>) -> anyhow::Result<Credentials> + Send + 'static) {
        if *self.signing_in() {
            return;
        }
        let cancel = Arc::new(AtomicBool::new(false));
        self.as_mut().rust_mut().cancel_sign_in = Some(cancel.clone());
        self.as_mut().set_signing_in(true);
        self.as_mut().set_error_message(QString::default());
        let qt = self.qt_thread();
        std::thread::spawn(move || {
            let result = work(cancel);
            let _ = qt.queue(move |this| this.finish_sign_in(result));
        });
    }

    pub fn sign_in_with_browser(self: Pin<&mut Self>) {
        let Some(client) = OAuthClient::load() else {
            self.fail("No Feedly API client is configured. See the README for how to add one, or use a developer access token.".into());
            return;
        };
        self.spawn_sign_in(move |cancel| {
            let http = http_client();
            let token = client.sign_in(&http, cancel)?;
            let mut credentials = Credentials {
                kind: CredentialKind::OAuth,
                user_id: token.id,
                access_token: token.access_token,
                refresh_token: token.refresh_token,
                display_name: String::new(),
            };
            let mut api = FeedlyApi::new(credentials.clone(), Some(client));
            credentials.display_name = api.profile().map(|p| p.display_name()).unwrap_or_else(|_| "Feedly".into());
            Ok(credentials)
        });
    }

    pub fn cancel_sign_in(self: Pin<&mut Self>) {
        if let Some(cancel) = &self.cancel_sign_in {
            cancel.store(true, Ordering::Relaxed);
        }
    }

    pub fn sign_in_with_token(self: Pin<&mut Self>, token: &QString) {
        let token = token.to_string().trim().to_string();
        if token.is_empty() {
            return;
        }
        self.spawn_sign_in(move |_| {
            let placeholder = Credentials {
                kind: CredentialKind::DeveloperToken,
                user_id: String::new(),
                access_token: token,
                refresh_token: None,
                display_name: String::new(),
            };
            let mut api = FeedlyApi::new(placeholder, None);
            let profile = api.profile().map_err(|e| {
                if sync::is_unauthorized(&e) {
                    anyhow::anyhow!("Feedly didn’t accept that access token. Check that it was copied completely and hasn’t expired.")
                } else {
                    e
                }
            })?;
            let mut c = api.credentials;
            c.user_id = profile.id.clone();
            c.display_name = profile.display_name();
            Ok(c)
        });
    }

    pub fn sign_out(mut self: Pin<&mut Self>) {
        if let Some(c) = self.as_mut().rust_mut().credentials.take()
            && c.kind == CredentialKind::OAuth
        {
            // Best effort: revoke the token on Feedly's side.
            std::thread::spawn(move || {
                let _ = FeedlyApi::new(c, OAuthClient::load()).logout();
            });
        }
        credentials::delete();
        let _ = store::get().clear_feedly();
        self.as_mut().set_signed_in(false);
        self.as_mut().set_needs_sign_in(false);
        self.as_mut().set_error_message(QString::default());
        self.as_mut().set_account_name(QString::default());
        self.as_mut().set_status(QString::default());
        self.as_mut().set_last_refreshed(QString::default());
        self.as_mut().data_changed();
    }

    /// Refreshes local feeds, then the Feedly account if signed in.
    pub fn refresh(mut self: Pin<&mut Self>) {
        if *self.syncing() {
            return;
        }
        let credentials = self.credentials.clone();
        self.as_mut().set_syncing(true);
        self.as_mut().set_error_message(QString::default());
        let qt = self.qt_thread();
        std::thread::spawn(move || {
            let progress_qt = qt.clone();
            let progress = move |message: &str| {
                let message = QString::from(message);
                let _ = progress_qt.queue(move |this| this.set_status(message));
            };
            let local_result = local::refresh_all(&progress);
            // Show local articles while Feedly syncs.
            let _ = qt.queue(|this| this.data_changed());
            let feedly_result = credentials.map(|c| sync::refresh_all(c, &progress));
            let _ = qt.queue(move |mut this| {
                this.as_mut().set_syncing(false);
                // The sync may have refreshed the access token.
                if let Some(c) = credentials::load() {
                    this.as_mut().rust_mut().credentials = Some(c);
                }
                let mut messages = vec![];
                match local_result {
                    Ok(summary) if summary.failed > 0 => {
                        messages.push(format!("{} feed{} couldn’t be updated", summary.failed, if summary.failed == 1 { "" } else { "s" }))
                    }
                    Ok(_) => {}
                    Err(e) => this.as_mut().fail(format!("Refreshing feeds failed: {e:#}")),
                }
                match feedly_result {
                    Some(Ok(summary)) => messages.insert(0, summary.message()),
                    Some(Err(e)) if sync::is_unauthorized(&e) => {
                        this.as_mut().set_needs_sign_in(true);
                        this.as_mut().fail("Feedly no longer accepts your sign-in. Please sign in again.".into());
                    }
                    Some(Err(e)) => this.as_mut().fail(format!("Feedly refresh failed: {e:#}")),
                    None => {}
                }
                this.as_mut().set_status(QString::from(messages.join(" · ").as_str()));
                let now = chrono::Local::now().format("%-I:%M %p").to_string();
                this.as_mut().set_last_refreshed(QString::from(now.as_str()));
                this.as_mut().data_changed();
            });
        });
    }

    pub fn send_pending(mut self: Pin<&mut Self>) {
        if self.sending || *self.syncing() {
            return;
        }
        let Some(credentials) = self.credentials.clone() else { return };
        self.as_mut().rust_mut().sending = true;
        let qt = self.qt_thread();
        std::thread::spawn(move || {
            let result = sync::send_pending(credentials);
            let _ = qt.queue(move |mut this| {
                this.as_mut().rust_mut().sending = false;
                if let Err(e) = result {
                    log::warn!("Sending status changes failed; will retry on next refresh: {e:#}");
                }
            });
        });
    }

    pub fn article_page(&self, article_id: &QString, placeholder: &QString, text_size: i32, dark: bool) -> QString {
        let class = match text_size {
            0 => "smallText",
            2 => "largeText",
            3 => "xlargeText",
            4 => "xxlargeText",
            _ => "mediumText",
        };
        let id = article_id.to_string();
        let html = if id.is_empty() {
            render::system_message(&placeholder.to_string(), dark)
        } else {
            match store::get().article(&id) {
                Ok(Some(article)) => render::render(&article, class, dark),
                _ => render::system_message("Article not found", dark),
            }
        };
        match render::write_page(&html) {
            Ok(url) => QString::from(url.as_str()),
            Err(e) => {
                log::error!("Couldn’t write the article page: {e:#}");
                QString::from("about:blank")
            }
        }
    }

    pub fn open_url(&self, url: &QString) {
        let url = url.to_string();
        if url.starts_with("http://") || url.starts_with("https://") || url.starts_with("mailto:") {
            if let Err(e) = open_in_browser(&url) {
                log::error!("{e:#}");
            }
        }
    }

    pub fn add_feed(mut self: Pin<&mut Self>, url: &QString, name: &QString, folder_key: &QString) {
        if *self.adding_feed() {
            return;
        }
        let (url, name) = (url.to_string(), name.to_string());
        let folder = folder_key.to_string().strip_prefix("folder:").map(str::to_string);
        self.as_mut().set_adding_feed(true);
        let qt = self.qt_thread();
        std::thread::spawn(move || {
            let result = local::add_feed(&url, Some(&name), folder.as_deref());
            let _ = qt.queue(move |mut this| {
                this.as_mut().set_adding_feed(false);
                match result {
                    Ok(key) => {
                        this.as_mut().data_changed();
                        this.as_mut().feed_added(true, QString::default(), QString::from(key.as_str()));
                    }
                    Err(e) => this.as_mut().feed_added(false, QString::from(format!("{e:#}").as_str()), QString::default()),
                }
            });
        });
    }

    pub fn add_folder(mut self: Pin<&mut Self>, name: &QString) -> QString {
        let name = name.to_string();
        let name = name.trim();
        if name.is_empty() {
            return QString::default();
        }
        let result = store::get().create_local_folder(name);
        match result {
            Ok(id) => {
                self.as_mut().data_changed();
                QString::from(format!("folder:{id}").as_str())
            }
            Err(e) => {
                self.fail(format!("Couldn’t create the folder: {e:#}"));
                QString::default()
            }
        }
    }

    pub fn rename_item(mut self: Pin<&mut Self>, key: &QString, name: &QString) -> bool {
        let name = name.to_string();
        let result = store::get().rename_local(item_id(&key.to_string()), name.trim());
        self.as_mut().report(result)
    }

    pub fn delete_item(mut self: Pin<&mut Self>, key: &QString) -> bool {
        let result = store::get().delete_local(item_id(&key.to_string()));
        self.as_mut().report(result)
    }

    pub fn move_feed(mut self: Pin<&mut Self>, feed_key: &QString, folder_key: &QString) -> bool {
        let folder_key = folder_key.to_string();
        let folder = folder_key.strip_prefix("folder:");
        let result = store::get().move_local_feed(item_id(&feed_key.to_string()), folder);
        self.as_mut().report(result)
    }

    fn report(mut self: Pin<&mut Self>, result: anyhow::Result<()>) -> bool {
        match result {
            Ok(()) => {
                self.as_mut().data_changed();
                true
            }
            Err(e) => {
                self.fail(format!("{e:#}"));
                false
            }
        }
    }

    pub fn import_opml(mut self: Pin<&mut Self>, file_url: &QString) {
        let Some(path) = file_path(file_url) else { return };
        match local::import_opml(&path) {
            Ok(n) => {
                let message = format!("Imported {n} feed{}", if n == 1 { "" } else { "s" });
                self.as_mut().set_status(QString::from(message.as_str()));
                self.as_mut().data_changed();
                self.refresh();
            }
            Err(e) => self.fail(format!("{e:#}")),
        }
    }

    pub fn export_opml(self: Pin<&mut Self>, file_url: &QString) {
        let Some(path) = file_path(file_url) else { return };
        match local::export_opml(&path) {
            Ok(n) => {
                let message = format!("Exported {n} feed{}", if n == 1 { "" } else { "s" });
                self.set_status(QString::from(message.as_str()));
            }
            Err(e) => self.fail(format!("{e:#}")),
        }
    }

    pub fn clear_error(self: Pin<&mut Self>) {
        self.set_error_message(QString::default());
    }
}

/// Sidebar keys are `feed:<id>` or `folder:<id>`.
fn item_id(key: &str) -> &str {
    key.split_once(':').map(|(_, id)| id).unwrap_or(key)
}

/// QML file dialogs hand back `file://` URLs.
fn file_path(file_url: &QString) -> Option<String> {
    let s = file_url.to_string();
    match url::Url::parse(&s) {
        Ok(u) if u.scheme() == "file" => u.to_file_path().ok().map(|p| p.to_string_lossy().into_owned()),
        _ => (!s.is_empty()).then_some(s),
    }
}
