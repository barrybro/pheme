//! The source list: smart feeds, then the Feedly account's folders and feeds, flattened
//! into rows with a depth so QML can draw disclosure triangles like NetNewsWire's outline view.

#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!(<QtCore/QAbstractListModel>);
        type QAbstractListModel;

        include!("cxx-qt-lib/qmodelindex.h");
        type QModelIndex = cxx_qt_lib::QModelIndex;
        include!("cxx-qt-lib/qvariant.h");
        type QVariant = cxx_qt_lib::QVariant;
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
        include!("cxx-qt-lib/qhash.h");
        type QHash_i32_QByteArray = cxx_qt_lib::QHash<cxx_qt_lib::QHashPair_i32_QByteArray>;
        include!("cxx-qt-lib/qlist.h");
        type QList_i32 = cxx_qt_lib::QList<i32>;
        type QList_QVariant = cxx_qt_lib::QList<cxx_qt_lib::QVariant>;
    }

    #[auto_cxx_name]
    unsafe extern "RustQt" {
        #[qobject]
        #[qml_element]
        #[base = QAbstractListModel]
        /// The Feedly section's title; when empty, there's no Feedly section.
        #[qproperty(QString, account_title)]
        type SidebarModel = super::SidebarModelRust;

        #[inherit]
        fn begin_reset_model(self: Pin<&mut SidebarModel>);
        #[inherit]
        fn end_reset_model(self: Pin<&mut SidebarModel>);
        #[inherit]
        fn index(self: &SidebarModel, row: i32, column: i32, parent: &QModelIndex) -> QModelIndex;
        #[inherit]
        #[qsignal]
        fn data_changed(self: Pin<&mut SidebarModel>, top_left: &QModelIndex, bottom_right: &QModelIndex, roles: &QList_i32);

        #[cxx_override]
        fn data(self: &SidebarModel, index: &QModelIndex, role: i32) -> QVariant;
        #[cxx_override]
        fn role_names(self: &SidebarModel) -> QHash_i32_QByteArray;
        #[cxx_override]
        fn row_count(self: &SidebarModel, parent: &QModelIndex) -> i32;

        /// Rebuilds the rows from the database.
        #[qinvokable]
        fn reload(self: Pin<&mut SidebarModel>);
        /// Updates unread counts in place, keeping selection and scroll position.
        #[qinvokable]
        fn refresh_counts(self: Pin<&mut SidebarModel>);
        #[qinvokable]
        fn toggle_expanded(self: Pin<&mut SidebarModel>, row: i32);
        #[qinvokable]
        fn set_expanded(self: Pin<&mut SidebarModel>, row: i32, expanded: bool);
        #[qinvokable]
        fn row_for_key(self: &SidebarModel, key: &QString) -> i32;
        #[qinvokable]
        fn key_at(self: &SidebarModel, row: i32) -> QString;
        #[qinvokable]
        fn title_at(self: &SidebarModel, row: i32) -> QString;
        /// Local folders as `[{key, title}]`, for folder pickers.
        #[qinvokable]
        fn local_folders(self: &SidebarModel) -> QList_QVariant;
        /// The local folder containing the row's feed, or the row itself if it's a local folder.
        #[qinvokable]
        fn local_folder_key_for(self: &SidebarModel, row: i32) -> QString;
        /// The next row with unread articles after `row`, wrapping around; -1 if none.
        #[qinvokable]
        fn next_unread_row(self: &SidebarModel, row: i32) -> i32;
    }
}

use crate::store::{self, Account, Selection};
use core::pin::Pin;
use cxx_qt::CxxQtType;
use cxx_qt_lib::{QByteArray, QHash, QHashPair_i32_QByteArray, QList, QMap, QMapPair_QString_QVariant, QModelIndex, QString, QVariant};
use qobject::*;
use std::collections::HashSet;

const EXPANDED_KEY: &str = "sidebar.expanded_folders";

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Smart,
    Header,
    Folder,
    Feed,
}

impl Kind {
    fn name(self) -> &'static str {
        match self {
            Kind::Smart => "smart",
            Kind::Header => "header",
            Kind::Folder => "folder",
            Kind::Feed => "feed",
        }
    }
}

#[derive(Clone)]
struct Row {
    kind: Kind,
    key: String,
    title: String,
    unread: i64,
    depth: i32,
    icon_url: String,
    /// For feeds, the folder they're shown in; for folders, their own ID.
    folder: Option<String>,
}

#[derive(Default)]
pub struct SidebarModelRust {
    account_title: QString,
    /// Every row, including those inside collapsed folders.
    all: Vec<Row>,
    /// The rows currently shown.
    rows: Vec<Row>,
    expanded: HashSet<String>,
    loaded_expanded: bool,
}

mod role {
    pub const KIND: i32 = 0x0101;
    pub const KEY: i32 = 0x0102;
    pub const TITLE: i32 = 0x0103;
    pub const UNREAD: i32 = 0x0104;
    pub const DEPTH: i32 = 0x0105;
    pub const EXPANDED: i32 = 0x0106;
    pub const ICON_URL: i32 = 0x0107;
    pub const ACCOUNT: i32 = 0x0108;
}

/// NetNewsWire calls its local account "On My Mac".
const LOCAL_TITLE: &str = "On My Computer";

fn build_rows(feedly_title: Option<&str>) -> anyhow::Result<Vec<Row>> {
    let s = store::get();
    let smart = |key: &str, title: &str, sel: Selection| -> anyhow::Result<Row> {
        Ok(Row {
            kind: Kind::Smart,
            key: format!("smart:{key}"),
            title: title.into(),
            unread: s.unread_count(&sel)?,
            depth: 0,
            icon_url: String::new(),
            folder: None,
        })
    };
    let mut rows = vec![
        smart("today", "Today", Selection::Today)?,
        smart("unread", "All Unread", Selection::AllUnread)?,
        smart("starred", "Starred", Selection::Starred)?,
    ];
    let feed_row = |f: store::FeedRow, depth: i32, folder: Option<String>| Row {
        kind: Kind::Feed,
        key: format!("feed:{}", f.id),
        title: f.title,
        unread: f.unread,
        depth,
        icon_url: f.icon_url.unwrap_or_default(),
        folder,
    };
    let mut sections = vec![(Account::Local, "account:local", LOCAL_TITLE)];
    if let Some(title) = feedly_title {
        sections.push((Account::Feedly, "account:feedly", title));
    }
    for (account, key, title) in sections {
        rows.push(Row {
            kind: Kind::Header,
            key: key.into(),
            title: title.into(),
            unread: 0,
            depth: 0,
            icon_url: String::new(),
            folder: None,
        });
        for folder in s.folders(account)? {
            let feeds = s.feeds_in_folder(&folder.id)?;
            let unread = s.unread_count(&Selection::Folder(folder.id.clone()))?;
            rows.push(Row {
                kind: Kind::Folder,
                key: format!("folder:{}", folder.id),
                title: folder.name,
                unread,
                depth: 0,
                icon_url: String::new(),
                folder: Some(folder.id.clone()),
            });
            rows.extend(feeds.into_iter().map(|f| feed_row(f, 1, Some(folder.id.clone()))));
        }
        rows.extend(s.unfoldered_feeds(account)?.into_iter().map(|f| feed_row(f, 0, None)));
    }
    Ok(rows)
}

impl SidebarModel {
    fn visible(&self, all: &[Row]) -> Vec<Row> {
        all.iter()
            .filter(|r| match (&r.kind, &r.folder) {
                (Kind::Feed, Some(folder)) => self.expanded.contains(folder),
                _ => true,
            })
            .cloned()
            .collect()
    }

    fn load_expanded(mut self: Pin<&mut Self>) {
        if self.loaded_expanded {
            return;
        }
        let saved = store::get().setting(EXPANDED_KEY);
        let expanded: HashSet<String> = saved.and_then(|v| serde_json::from_str(&v).ok()).unwrap_or_default();
        let mut rust = self.as_mut().rust_mut();
        rust.expanded = expanded;
        rust.loaded_expanded = true;
    }

    fn save_expanded(&self) {
        if let Ok(json) = serde_json::to_string(&self.expanded) {
            let _ = store::get().set_setting(EXPANDED_KEY, &json);
        }
    }

    pub fn reload(mut self: Pin<&mut Self>) {
        self.as_mut().load_expanded();
        let title = self.account_title().to_string();
        let all = match build_rows(Some(title.as_str()).filter(|t| !t.is_empty())) {
            Ok(rows) => rows,
            Err(e) => {
                log::error!("Loading the sidebar failed: {e:#}");
                return;
            }
        };
        let rows = self.visible(&all);
        self.as_mut().begin_reset_model();
        {
            let mut rust = self.as_mut().rust_mut();
            rust.all = all;
            rust.rows = rows;
        }
        self.as_mut().end_reset_model();
    }

    pub fn refresh_counts(mut self: Pin<&mut Self>) {
        let title = self.account_title().to_string();
        let Ok(all) = build_rows(Some(title.as_str()).filter(|t| !t.is_empty())) else { return };
        let same_shape = all.len() == self.all.len() && all.iter().zip(self.all.iter()).all(|(a, b)| a.key == b.key);
        if !same_shape {
            self.reload();
            return;
        }
        let fresh = self.visible(&all);
        let mut changed_rows = vec![];
        for (i, (new, old)) in fresh.iter().zip(self.rows.iter()).enumerate() {
            if new.unread != old.unread || new.title != old.title {
                changed_rows.push(i);
            }
        }
        {
            let mut rust = self.as_mut().rust_mut();
            rust.all = all;
            rust.rows = fresh;
        }
        let mut roles = QList::<i32>::default();
        roles.append(role::UNREAD);
        roles.append(role::TITLE);
        for i in changed_rows {
            let index = self.index(i as i32, 0, &QModelIndex::default());
            self.as_mut().data_changed(&index, &index, &roles);
        }
    }

    pub fn set_expanded(mut self: Pin<&mut Self>, row: i32, expanded: bool) {
        let Some(r) = self.rows.get(row as usize).cloned() else { return };
        let (Kind::Folder, Some(folder)) = (r.kind, r.folder) else { return };
        if self.expanded.contains(&folder) == expanded {
            return;
        }
        {
            let mut rust = self.as_mut().rust_mut();
            if expanded {
                rust.expanded.insert(folder);
            } else {
                rust.expanded.remove(&folder);
            }
        }
        self.save_expanded();
        let rows = self.visible(&self.all);
        self.as_mut().begin_reset_model();
        self.as_mut().rust_mut().rows = rows;
        self.as_mut().end_reset_model();
    }

    pub fn toggle_expanded(self: Pin<&mut Self>, row: i32) {
        let Some(r) = self.rows.get(row as usize) else { return };
        let Some(folder) = r.folder.clone().filter(|_| r.kind == Kind::Folder) else { return };
        let expanded = !self.expanded.contains(&folder);
        self.set_expanded(row, expanded);
    }

    pub fn row_for_key(&self, key: &QString) -> i32 {
        let key = key.to_string();
        self.rows.iter().position(|r| r.key == key).map(|i| i as i32).unwrap_or(-1)
    }

    pub fn key_at(&self, row: i32) -> QString {
        self.rows.get(row as usize).map(|r| QString::from(r.key.as_str())).unwrap_or_default()
    }

    pub fn title_at(&self, row: i32) -> QString {
        self.rows.get(row as usize).map(|r| QString::from(r.title.as_str())).unwrap_or_default()
    }

    pub fn local_folders(&self) -> QList<QVariant> {
        let mut list = QList::<QVariant>::default();
        for r in self.all.iter().filter(|r| r.kind == Kind::Folder && Account::of(r.folder.as_deref().unwrap_or("")) == Account::Local) {
            let mut map = QMap::<QMapPair_QString_QVariant>::default();
            map.insert(QString::from("key"), QVariant::from(&QString::from(r.key.as_str())));
            map.insert(QString::from("title"), QVariant::from(&QString::from(r.title.as_str())));
            list.append(QVariant::from(&map));
        }
        list
    }

    pub fn local_folder_key_for(&self, row: i32) -> QString {
        let Some(r) = self.rows.get(row as usize) else { return QString::default() };
        match &r.folder {
            Some(folder) if Account::of(folder) == Account::Local => QString::from(format!("folder:{folder}").as_str()),
            _ => QString::default(),
        }
    }

    pub fn next_unread_row(&self, row: i32) -> i32 {
        let n = self.rows.len();
        if n == 0 {
            return -1;
        }
        let start = if row < 0 { 0 } else { row as usize + 1 };
        // Only feeds: folders and smart feeds overlap with them.
        (0..n)
            .map(|i| (start + i) % n)
            .find(|&i| self.rows[i].kind == Kind::Feed && self.rows[i].unread > 0)
            .map(|i| i as i32)
            .unwrap_or(-1)
    }

    pub fn data(&self, index: &QModelIndex, role: i32) -> QVariant {
        let Some(r) = self.rows.get(index.row() as usize) else { return QVariant::default() };
        match role {
            role::KIND => QVariant::from(&QString::from(r.kind.name())),
            role::KEY => QVariant::from(&QString::from(r.key.as_str())),
            role::TITLE => QVariant::from(&QString::from(r.title.as_str())),
            role::UNREAD => QVariant::from(&(r.unread as i32)),
            role::DEPTH => QVariant::from(&r.depth),
            role::EXPANDED => {
                let expanded = r.folder.as_ref().is_some_and(|f| r.kind == Kind::Folder && self.expanded.contains(f));
                QVariant::from(&expanded)
            }
            role::ICON_URL => QVariant::from(&QString::from(r.icon_url.as_str())),
            role::ACCOUNT => {
                let account = match r.key.split_once(':') {
                    Some(("account", which)) => which,
                    Some((_, id)) if Account::of(id) == Account::Local => "local",
                    Some(("feed" | "folder", _)) => "feedly",
                    _ => "",
                };
                QVariant::from(&QString::from(account))
            }
            _ => QVariant::default(),
        }
    }

    pub fn role_names(&self) -> QHash<QHashPair_i32_QByteArray> {
        let mut roles = QHash::<QHashPair_i32_QByteArray>::default();
        roles.insert(role::KIND, QByteArray::from("kind"));
        roles.insert(role::KEY, QByteArray::from("itemKey"));
        roles.insert(role::TITLE, QByteArray::from("title"));
        roles.insert(role::UNREAD, QByteArray::from("unreadCount"));
        roles.insert(role::DEPTH, QByteArray::from("depth"));
        roles.insert(role::EXPANDED, QByteArray::from("expanded"));
        roles.insert(role::ICON_URL, QByteArray::from("iconUrl"));
        roles.insert(role::ACCOUNT, QByteArray::from("account"));
        roles
    }

    pub fn row_count(&self, _parent: &QModelIndex) -> i32 {
        self.rows.len() as i32
    }
}
