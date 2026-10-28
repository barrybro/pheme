//! The article list for the sidebar's selection.

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
    }

    #[auto_cxx_name]
    unsafe extern "RustQt" {
        #[qobject]
        #[qml_element]
        #[base = QAbstractListModel]
        #[qproperty(QString, selection_key)]
        #[qproperty(bool, hide_read)]
        #[qproperty(QString, search_text)]
        #[qproperty(i32, count)]
        #[qproperty(i32, unread_count)]
        type TimelineModel = super::TimelineModelRust;

        #[inherit]
        fn begin_reset_model(self: Pin<&mut TimelineModel>);
        #[inherit]
        fn end_reset_model(self: Pin<&mut TimelineModel>);
        #[inherit]
        fn index(self: &TimelineModel, row: i32, column: i32, parent: &QModelIndex) -> QModelIndex;
        #[inherit]
        #[qsignal]
        fn data_changed(self: Pin<&mut TimelineModel>, top_left: &QModelIndex, bottom_right: &QModelIndex, roles: &QList_i32);

        #[cxx_override]
        fn data(self: &TimelineModel, index: &QModelIndex, role: i32) -> QVariant;
        #[cxx_override]
        fn role_names(self: &TimelineModel) -> QHash_i32_QByteArray;
        #[cxx_override]
        fn row_count(self: &TimelineModel, parent: &QModelIndex) -> i32;

        /// Re-queries the database for the current selection, filter, and search.
        #[qinvokable]
        fn reload(self: Pin<&mut TimelineModel>);
        /// Picks up read/starred changes made elsewhere without reordering rows.
        #[qinvokable]
        fn refresh_statuses(self: Pin<&mut TimelineModel>);

        /// These change local state and queue the change for Feedly. They return
        /// true when something changed, so QML can schedule a send.
        #[qinvokable]
        fn set_read(self: Pin<&mut TimelineModel>, row: i32, read: bool) -> bool;
        #[qinvokable]
        fn toggle_read(self: Pin<&mut TimelineModel>, row: i32) -> bool;
        #[qinvokable]
        fn toggle_starred(self: Pin<&mut TimelineModel>, row: i32) -> bool;
        #[qinvokable]
        fn mark_all_read(self: Pin<&mut TimelineModel>) -> bool;

        #[qinvokable]
        fn id_at(self: &TimelineModel, row: i32) -> QString;
        #[qinvokable]
        fn url_at(self: &TimelineModel, row: i32) -> QString;
        #[qinvokable]
        fn row_for_id(self: &TimelineModel, id: &QString) -> i32;
        /// The next unread row after `row`; -1 if none.
        #[qinvokable]
        fn next_unread_row(self: &TimelineModel, row: i32) -> i32;
    }
}

use crate::store::{self, Selection, StatusField, TimelineRow};
use crate::text::timeline_date;
use core::pin::Pin;
use cxx_qt::CxxQtType;
use cxx_qt_lib::{QByteArray, QHash, QHashPair_i32_QByteArray, QList, QModelIndex, QString, QVariant};
use qobject::*;

#[derive(Default)]
pub struct TimelineModelRust {
    selection_key: QString,
    hide_read: bool,
    search_text: QString,
    count: i32,
    unread_count: i32,
    rows: Vec<TimelineRow>,
}

mod role {
    pub const ID: i32 = 0x0101;
    pub const FEED_TITLE: i32 = 0x0102;
    pub const ICON_URL: i32 = 0x0103;
    pub const TITLE: i32 = 0x0104;
    pub const SUMMARY: i32 = 0x0105;
    pub const DATE: i32 = 0x0106;
    pub const UNREAD: i32 = 0x0107;
    pub const STARRED: i32 = 0x0108;
    pub const IMAGE_URL: i32 = 0x0109;
}

impl TimelineModel {
    fn update_unread_count(mut self: Pin<&mut Self>) {
        let n = self.rows.iter().filter(|r| r.unread).count() as i32;
        self.as_mut().set_unread_count(n);
    }

    fn row_changed(mut self: Pin<&mut Self>, row: usize) {
        let index = self.index(row as i32, 0, &QModelIndex::default());
        let mut roles = QList::<i32>::default();
        roles.append(role::UNREAD);
        roles.append(role::STARRED);
        self.as_mut().data_changed(&index, &index, &roles);
    }

    pub fn reload(mut self: Pin<&mut Self>) {
        let rows = match Selection::parse(&self.selection_key().to_string()) {
            Some(sel) => store::get()
                .timeline(&sel, *self.hide_read(), &self.search_text().to_string())
                .unwrap_or_else(|e| {
                    log::error!("Loading the timeline failed: {e:#}");
                    vec![]
                }),
            None => vec![],
        };
        self.as_mut().begin_reset_model();
        self.as_mut().rust_mut().rows = rows;
        self.as_mut().end_reset_model();
        let n = self.rows.len() as i32;
        self.as_mut().set_count(n);
        self.update_unread_count();
    }

    pub fn refresh_statuses(mut self: Pin<&mut Self>) {
        let Some(sel) = Selection::parse(&self.selection_key().to_string()) else { return };
        // Without the read filter, so rows read elsewhere don't vanish mid-reading.
        let Ok(fresh) = store::get().timeline(&sel, false, &self.search_text().to_string()) else { return };
        let by_id: std::collections::HashMap<&str, &TimelineRow> = fresh.iter().map(|r| (r.id.as_str(), r)).collect();
        let mut changed = vec![];
        {
            let mut rust = self.as_mut().rust_mut();
            for (i, row) in rust.rows.iter_mut().enumerate() {
                if let Some(f) = by_id.get(row.id.as_str())
                    && (f.unread != row.unread || f.starred != row.starred)
                {
                    row.unread = f.unread;
                    row.starred = f.starred;
                    changed.push(i);
                }
            }
        }
        for i in changed {
            self.as_mut().row_changed(i);
        }
        self.update_unread_count();
    }

    fn apply(mut self: Pin<&mut Self>, rows: &[usize], field: StatusField, flag: bool) -> bool {
        let ids: Vec<String> = rows.iter().filter_map(|&i| self.rows.get(i)).map(|r| r.id.clone()).collect();
        if ids.is_empty() {
            return false;
        }
        let changed = match store::get().set_status(&ids, field, flag) {
            Ok(changed) => changed,
            Err(e) => {
                log::error!("Updating article status failed: {e:#}");
                return false;
            }
        };
        if changed.is_empty() {
            return false;
        }
        for &i in rows {
            {
                let mut rust = self.as_mut().rust_mut();
                let row = &mut rust.rows[i];
                match field {
                    StatusField::Read => row.unread = !flag,
                    StatusField::Starred => row.starred = flag,
                }
            }
            self.as_mut().row_changed(i);
        }
        self.update_unread_count();
        true
    }

    pub fn set_read(self: Pin<&mut Self>, row: i32, read: bool) -> bool {
        if row < 0 || row as usize >= self.rows.len() {
            return false;
        }
        self.apply(&[row as usize], StatusField::Read, read)
    }

    pub fn toggle_read(self: Pin<&mut Self>, row: i32) -> bool {
        let Some(r) = self.rows.get(row as usize) else { return false };
        let read = r.unread;
        self.apply(&[row as usize], StatusField::Read, read)
    }

    pub fn toggle_starred(self: Pin<&mut Self>, row: i32) -> bool {
        let Some(r) = self.rows.get(row as usize) else { return false };
        let starred = !r.starred;
        self.apply(&[row as usize], StatusField::Starred, starred)
    }

    pub fn mark_all_read(self: Pin<&mut Self>) -> bool {
        let rows: Vec<usize> = self.rows.iter().enumerate().filter(|(_, r)| r.unread).map(|(i, _)| i).collect();
        self.apply(&rows, StatusField::Read, true)
    }

    pub fn id_at(&self, row: i32) -> QString {
        self.rows.get(row as usize).map(|r| QString::from(r.id.as_str())).unwrap_or_default()
    }

    pub fn url_at(&self, row: i32) -> QString {
        let Some(r) = self.rows.get(row as usize) else { return QString::default() };
        let url = store::get().article(&r.id).ok().flatten().and_then(|a| a.url).unwrap_or_default();
        QString::from(url.as_str())
    }

    pub fn row_for_id(&self, id: &QString) -> i32 {
        let id = id.to_string();
        self.rows.iter().position(|r| r.id == id).map(|i| i as i32).unwrap_or(-1)
    }

    pub fn next_unread_row(&self, row: i32) -> i32 {
        let start = if row < 0 { 0 } else { row as usize + 1 };
        self.rows.iter().skip(start).position(|r| r.unread).map(|i| (i + start) as i32).unwrap_or(-1)
    }

    pub fn data(&self, index: &QModelIndex, role: i32) -> QVariant {
        let Some(r) = self.rows.get(index.row() as usize) else { return QVariant::default() };
        let s = |v: &str| QVariant::from(&QString::from(v));
        match role {
            role::ID => s(&r.id),
            role::FEED_TITLE => s(&r.feed_title),
            role::ICON_URL => s(r.icon_url.as_deref().unwrap_or("")),
            role::TITLE => s(&r.title),
            role::SUMMARY => s(&r.summary),
            role::DATE => s(&timeline_date(r.published)),
            role::UNREAD => QVariant::from(&r.unread),
            role::STARRED => QVariant::from(&r.starred),
            role::IMAGE_URL => s(r.image_url.as_deref().unwrap_or("")),
            _ => QVariant::default(),
        }
    }

    pub fn role_names(&self) -> QHash<QHashPair_i32_QByteArray> {
        let mut roles = QHash::<QHashPair_i32_QByteArray>::default();
        for (id, name) in [
            (role::ID, "articleId"),
            (role::FEED_TITLE, "feedTitle"),
            (role::ICON_URL, "iconUrl"),
            (role::TITLE, "title"),
            (role::SUMMARY, "summary"),
            (role::DATE, "dateText"),
            (role::UNREAD, "unread"),
            (role::STARRED, "starred"),
            (role::IMAGE_URL, "imageUrl"),
        ] {
            roles.insert(id, QByteArray::from(name));
        }
        roles
    }

    pub fn row_count(&self, _parent: &QModelIndex) -> i32 {
        self.rows.len() as i32
    }
}
