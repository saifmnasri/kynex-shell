//! Notification list model exposing desktop popups to QML via `cxx-qt`.
#![allow(
    clippy::chunks_exact_to_as_chunks,
    clippy::missing_errors_doc,
    clippy::too_many_arguments,
    clippy::doc_markdown,
    clippy::needless_pass_by_value,
    clippy::redundant_closure_for_method_calls
)]
//!
//! Subclasses `QAbstractListModel` to provide a reactive, asynchronous data model
//! for visible notification popups adhering to N3.2:
//! - Up to 5 visible popups with app name, icon, summary, body, actions, urgency.
//! - Non-blocking asynchronous D-Bus communication with automatic daemon reconnection.
//! - Hover pauses expiration, action triggers unicast dispatch.

use core::pin::Pin;

use cxx_qt::{CxxQtType, Threading};
use cxx_qt_lib::{
    QByteArray, QHash, QHashPair_i32_QByteArray, QModelIndex, QString, QStringList, QVariant,
};
use futures_util::StreamExt;
use kynex_ipc::notifications::{Notifications1Proxy, NOTIFICATIONS_INTERFACE, NOTIFICATIONS_PATH};
use kynex_ipc::DAEMON_DESTINATION;
use tracing::{info, warn};
use zbus::fdo::DBusProxy;
use zbus::message::Type;
use zbus::{MatchRule, MessageStream};

/// Role ID for notification ID.
pub const ROLE_ID: i32 = 257;
/// Role ID for application display name.
pub const ROLE_APP_NAME: i32 = 258;
/// Role ID for application identity verification flag.
pub const ROLE_APP_NAME_VERIFIED: i32 = 259;
/// Role ID for icon name or image path.
pub const ROLE_APP_ICON: i32 = 260;
/// Role ID for fallback letter badge.
pub const ROLE_LETTER_BADGE: i32 = 261;
/// Role ID for notification summary.
pub const ROLE_SUMMARY: i32 = 262;
/// Role ID for notification body.
pub const ROLE_BODY: i32 = 263;
/// Role ID for JSON-serialized actions list.
pub const ROLE_ACTIONS_JSON: i32 = 264;
/// Role ID for urgency level (0=low, 1=normal, 2=critical).
pub const ROLE_URGENCY: i32 = 265;
/// Role ID for timeout in milliseconds.
pub const ROLE_TIMEOUT_MS: i32 = 266;
/// Role ID for string list of action keys.
pub const ROLE_ACTIONS_KEYS: i32 = 267;
/// Role ID for string list of action labels.
pub const ROLE_ACTIONS_LABELS: i32 = 268;
/// Role ID for whether a "default" action exists.
pub const ROLE_HAS_DEFAULT_ACTION: i32 = 269;

/// Pure logic representation of a visible notification popup item.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotificationUiItem {
    /// Monotonic notification ID.
    pub id: u32,
    /// Display application name.
    pub app_name: String,
    /// Whether application name was verified against desktop entry index.
    pub app_name_verified: bool,
    /// Icon theme name or image path.
    pub app_icon: String,
    /// Fallback single-character letter badge.
    pub letter_badge: String,
    /// Notification summary headline.
    pub summary: String,
    /// Notification body text.
    pub body: String,
    /// JSON-serialized array of actions.
    pub actions_json: String,
    /// List of action keys excluding default.
    pub action_keys: Vec<String>,
    /// List of action display labels excluding default.
    pub action_labels: Vec<String>,
    /// Whether a "default" action was specified.
    pub has_default_action: bool,
    /// Urgency level (0=low, 1=normal, 2=critical).
    pub urgency: u8,
    /// Timeout in milliseconds.
    pub timeout_ms: i32,
}

/// Pure logic collection representing visible popups.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NotificationUiState {
    /// Active visible popup items (at most 5).
    pub items: Vec<NotificationUiItem>,
}

impl NotificationUiState {
    /// Inserts or updates an item in the visible popup list.
    pub fn upsert(&mut self, item: NotificationUiItem) {
        if let Some(pos) = self.items.iter().position(|it| it.id == item.id) {
            self.items[pos] = item;
        } else {
            if self.items.len() >= 5 {
                self.items.pop();
            }
            self.items.insert(0, item);
        }
    }

    /// Removes an item by notification ID.
    pub fn remove(&mut self, id: u32) -> bool {
        if let Some(pos) = self.items.iter().position(|it| it.id == id) {
            self.items.remove(pos);
            true
        } else {
            false
        }
    }
}

/// Computes a fallback single-character letter badge for an application name.
#[must_use]
pub fn compute_letter_badge(app_name: &str) -> String {
    for c in app_name.chars() {
        if c.is_alphanumeric() {
            return c.to_uppercase().to_string();
        }
    }
    "?".to_string()
}

fn escape_json_str(s: &str, out: &mut String) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 32 => {
                use std::fmt::Write;
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

/// Serializes flat action pairs `[key1, label1, ...]` into a JSON string `[{"key":"...","label":"..."}]`.
#[must_use]
pub fn format_actions_json(actions: &[String]) -> String {
    let mut out = String::from("[");
    let mut first = true;
    for chunk in actions.chunks_exact(2) {
        if !first {
            out.push(',');
        }
        first = false;
        out.push_str("{\"key\":");
        escape_json_str(&chunk[0], &mut out);
        out.push_str(",\"label\":");
        escape_json_str(&chunk[1], &mut out);
        out.push('}');
    }
    out.push(']');
    out
}

/// Converts a notification popup item and role ID into a `QVariant`.
#[must_use]
pub fn notification_item_role_variant(item: &NotificationUiItem, role: i32) -> QVariant {
    match role {
        ROLE_ID => QVariant::from(&(i32::try_from(item.id).unwrap_or(0))),
        ROLE_APP_NAME => QVariant::from(&QString::from(&item.app_name)),
        ROLE_APP_NAME_VERIFIED => QVariant::from(&item.app_name_verified),
        ROLE_APP_ICON => QVariant::from(&QString::from(&item.app_icon)),
        ROLE_LETTER_BADGE => QVariant::from(&QString::from(&item.letter_badge)),
        ROLE_SUMMARY => QVariant::from(&QString::from(&item.summary)),
        ROLE_BODY => QVariant::from(&QString::from(&item.body)),
        ROLE_ACTIONS_JSON => QVariant::from(&QString::from(&item.actions_json)),
        ROLE_ACTIONS_KEYS => {
            let mut list = QStringList::default();
            for k in &item.action_keys {
                list.append_clone(&QString::from(k));
            }
            QVariant::from(&list)
        }
        ROLE_ACTIONS_LABELS => {
            let mut list = QStringList::default();
            for l in &item.action_labels {
                list.append_clone(&QString::from(l));
            }
            QVariant::from(&list)
        }
        ROLE_HAS_DEFAULT_ACTION => QVariant::from(&item.has_default_action),
        ROLE_URGENCY => QVariant::from(&(i32::from(item.urgency))),
        ROLE_TIMEOUT_MS => QVariant::from(&item.timeout_ms),
        _ => QVariant::default(),
    }
}

/// Constructs the role names hash map mapping role IDs to QML property strings.
#[must_use]
pub fn create_notification_role_names() -> QHash<QHashPair_i32_QByteArray> {
    let mut roles = QHash::<QHashPair_i32_QByteArray>::default();
    roles.insert(ROLE_ID, QByteArray::from("id"));
    roles.insert(ROLE_APP_NAME, QByteArray::from("appName"));
    roles.insert(ROLE_APP_NAME_VERIFIED, QByteArray::from("appNameVerified"));
    roles.insert(ROLE_APP_ICON, QByteArray::from("appIcon"));
    roles.insert(ROLE_LETTER_BADGE, QByteArray::from("letterBadge"));
    roles.insert(ROLE_SUMMARY, QByteArray::from("summary"));
    roles.insert(ROLE_BODY, QByteArray::from("body"));
    roles.insert(ROLE_ACTIONS_JSON, QByteArray::from("actionsJson"));
    roles.insert(ROLE_ACTIONS_KEYS, QByteArray::from("actionsKeys"));
    roles.insert(ROLE_ACTIONS_LABELS, QByteArray::from("actionsLabels"));
    roles.insert(
        ROLE_HAS_DEFAULT_ACTION,
        QByteArray::from("hasDefaultAction"),
    );
    roles.insert(ROLE_URGENCY, QByteArray::from("urgency"));
    roles.insert(ROLE_TIMEOUT_MS, QByteArray::from("timeoutMs"));
    roles
}

/// CXX-Qt bridge module for `NotificationModel`.
#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qmodelindex.h");
        /// Model index type.
        type QModelIndex = cxx_qt_lib::QModelIndex;
        include!("cxx-qt-lib/qvariant.h");
        /// Variant type.
        type QVariant = cxx_qt_lib::QVariant;
        include!("cxx-qt-lib/qhash_i32_QByteArray.h");
        /// Hash map of integer to byte array.
        type QHash_i32_QByteArray = cxx_qt_lib::QHash<cxx_qt_lib::QHashPair_i32_QByteArray>;
        include!("cxx-qt-lib/qstring.h");
        /// QString type.
        type QString = cxx_qt_lib::QString;
        include!("cxx-qt-lib/qstringlist.h");
        /// QStringList type.
        type QStringList = cxx_qt_lib::QStringList;
    }

    unsafe extern "C++Qt" {
        include!(<QtCore/QAbstractListModel>);
        /// Qt QAbstractListModel base class.
        #[qobject]
        type QAbstractListModel;
    }

    extern "RustQt" {
        /// Notification popup list model for QML presentation.
        #[qobject]
        #[base = QAbstractListModel]
        #[qml_element]
        #[qproperty(i32, count)]
        type NotificationModel = super::NotificationModelRust;

        /// Initializes background D-Bus listeners.
        #[qinvokable]
        fn init(self: Pin<&mut Self>);

        /// Returns the number of items in the model.
        #[qinvokable]
        #[cxx_override]
        #[cxx_name = "rowCount"]
        fn row_count(self: &NotificationModel, parent: &QModelIndex) -> i32;

        /// Returns role data for a given model index and role.
        #[qinvokable]
        #[cxx_override]
        fn data(self: &NotificationModel, index: &QModelIndex, role: i32) -> QVariant;

        /// Returns role names mapping for QML property bindings.
        #[qinvokable]
        #[cxx_override]
        #[cxx_name = "roleNames"]
        fn role_names(self: &NotificationModel) -> QHash_i32_QByteArray;

        /// Dismisses a notification popup.
        #[qinvokable]
        fn dismiss(self: Pin<&mut Self>, id: u32);

        /// Invokes an action on a notification.
        #[qinvokable]
        #[cxx_name = "invokeAction"]
        fn invoke_action(self: Pin<&mut Self>, id: u32, key: &QString);

        /// Pauses expiration timeout for a notification (e.g. while hovered).
        #[qinvokable]
        fn pause(self: Pin<&mut Self>, id: u32);

        /// Resumes expiration timeout for a notification (e.g. when unhovered).
        #[qinvokable]
        fn resume(self: Pin<&mut Self>, id: u32);

        /// Records the first frame rendered after a notification popup was shown.
        #[qinvokable]
        #[cxx_name = "recordFirstFrame"]
        fn record_first_frame(self: Pin<&mut Self>);
    }

    unsafe extern "RustQt" {
        /// Inherited beginResetModel from QAbstractListModel.
        #[inherit]
        #[cxx_name = "beginResetModel"]
        fn begin_reset_model(self: Pin<&mut NotificationModel>);

        /// Inherited endResetModel from QAbstractListModel.
        #[inherit]
        #[cxx_name = "endResetModel"]
        fn end_reset_model(self: Pin<&mut NotificationModel>);
    }

    impl cxx_qt::Threading for NotificationModel {}
}

/// Backing data for `NotificationModel`.
#[derive(Default)]
pub struct NotificationModelRust {
    pub(crate) state: NotificationUiState,
    pub(crate) count: i32,
    pub(crate) first_frame_pending: bool,
    pub(crate) last_shown_time: Option<std::time::Instant>,
}

impl NotificationModelRust {
    /// Sets state directly.
    pub fn set_state(&mut self, state: NotificationUiState) {
        self.count = i32::try_from(state.items.len()).unwrap_or(0);
        self.state = state;
    }

    /// Accesses item at row.
    #[must_use]
    pub fn item_at(&self, row: usize) -> Option<&NotificationUiItem> {
        self.state.items.get(row)
    }

    /// Returns row count for the model.
    #[must_use]
    pub fn row_count(&self) -> i32 {
        self.count
    }

    /// Returns role data for a given row and role.
    #[must_use]
    pub fn data(&self, row: i32, role: i32) -> QVariant {
        let Ok(row_idx) = usize::try_from(row) else {
            return QVariant::default();
        };

        let items = &self.state.items;
        if row_idx >= items.len() {
            return QVariant::default();
        }

        notification_item_role_variant(&items[row_idx], role)
    }

    /// Records the first frame rendered after a notification popup was shown.
    pub fn record_first_frame(&mut self) -> Option<f64> {
        if self.first_frame_pending {
            self.first_frame_pending = false;
            let elapsed_ms = self
                .last_shown_time
                .map_or(0.0, |t| t.elapsed().as_secs_f64() * 1000.0);
            tracing::info!(
                target: "kynex_perf",
                event = "notification_first_frame_swapped",
                latency_ms = elapsed_ms,
                "Notification first frame swapped"
            );
            Some(elapsed_ms)
        } else {
            None
        }
    }
}

impl qobject::NotificationModel {
    /// Initializes background listeners connecting to `kynex-daemon`.
    pub fn init(self: Pin<&mut Self>) {
        let qt_thread = self.qt_thread();
        tokio::spawn(async move {
            run_notification_listener(qt_thread).await;
        });
    }

    /// Returns row count for the model.
    #[must_use]
    pub fn row_count(&self, _parent: &QModelIndex) -> i32 {
        self.rust().row_count()
    }

    /// Returns role data for a given index.
    #[must_use]
    pub fn data(&self, index: &QModelIndex, role: i32) -> QVariant {
        self.rust().data(index.row(), role)
    }

    /// Returns role names mapping for QML.
    #[must_use]
    pub fn role_names(&self) -> QHash<QHashPair_i32_QByteArray> {
        create_notification_role_names()
    }

    /// Dismisses a notification popup.
    pub fn dismiss(self: Pin<&mut Self>, id: u32) {
        tokio::spawn(async move {
            let _ = send_dismiss_call(id).await;
        });
    }

    /// Invokes an action on a notification.
    pub fn invoke_action(self: Pin<&mut Self>, id: u32, key: &QString) {
        let key_str = key.to_string();
        tokio::spawn(async move {
            let _ = send_invoke_action_call(id, &key_str).await;
        });
    }

    /// Pauses expiration timeout for a notification.
    pub fn pause(self: Pin<&mut Self>, id: u32) {
        tokio::spawn(async move {
            let _ = send_pause_call(id).await;
        });
    }

    /// Resumes expiration timeout for a notification.
    pub fn resume(self: Pin<&mut Self>, id: u32) {
        tokio::spawn(async move {
            let _ = send_resume_call(id).await;
        });
    }

    /// Records the first frame rendered after a notification popup was shown.
    pub fn record_first_frame(mut self: Pin<&mut Self>) {
        self.as_mut().rust_mut().record_first_frame();
    }
}

/// Sends Dismiss D-Bus call to daemon.
pub async fn send_dismiss_call(id: u32) -> zbus::Result<()> {
    let conn = zbus::connection::Builder::session().build().await?;
    let proxy = Notifications1Proxy::new(&conn).await?;
    proxy.dismiss(id).await
}

/// Sends InvokeAction D-Bus call to daemon.
pub async fn send_invoke_action_call(id: u32, key: &str) -> zbus::Result<()> {
    let conn = zbus::connection::Builder::session().build().await?;
    let proxy = Notifications1Proxy::new(&conn).await?;
    proxy.invoke_action(id, key).await
}

/// Sends PauseTimeout D-Bus call to daemon.
pub async fn send_pause_call(id: u32) -> zbus::Result<()> {
    let conn = zbus::connection::Builder::session().build().await?;
    let proxy = Notifications1Proxy::new(&conn).await?;
    proxy.pause_timeout(id).await
}

/// Sends ResumeTimeout D-Bus call to daemon.
pub async fn send_resume_call(id: u32) -> zbus::Result<()> {
    let conn = zbus::connection::Builder::session().build().await?;
    let proxy = Notifications1Proxy::new(&conn).await?;
    proxy.resume_timeout(id).await
}

/// Dispatches updated notification state to the Qt GUI thread.
fn dispatch_state(
    qt_thread: &cxx_qt::CxxQtThread<qobject::NotificationModel>,
    state: NotificationUiState,
) {
    let count = i32::try_from(state.items.len()).unwrap_or(0);
    let now = std::time::Instant::now();
    let _ = qt_thread.queue(move |mut qobj| {
        qobj.as_mut().begin_reset_model();
        let mut r = qobj.as_mut().rust_mut();
        r.state = state;
        r.first_frame_pending = true;
        r.last_shown_time = Some(now);
        qobj.as_mut().set_count(count);
        qobj.as_mut().end_reset_model();
    });
}

/// Parses incoming notification fields into a presentation item.
#[must_use]
pub fn parse_shown_notification(
    id: u32,
    app_name: String,
    app_name_verified: bool,
    app_icon: String,
    summary: String,
    body: String,
    actions: Vec<String>,
    urgency: u8,
    timeout_ms: i32,
) -> NotificationUiItem {
    let letter_badge = compute_letter_badge(&app_name);
    let actions_json = format_actions_json(&actions);
    let mut action_keys = Vec::new();
    let mut action_labels = Vec::new();
    let mut has_default_action = false;
    for chunk in actions.chunks_exact(2) {
        if chunk[0] == "default" {
            has_default_action = true;
        } else {
            action_keys.push(chunk[0].clone());
            action_labels.push(chunk[1].clone());
        }
    }

    NotificationUiItem {
        id,
        app_name,
        app_name_verified,
        app_icon,
        letter_badge,
        summary,
        body,
        actions_json,
        action_keys,
        action_labels,
        has_default_action,
        urgency,
        timeout_ms,
    }
}

/// Initializes a match-rule signal stream on `NOTIFICATIONS_PATH`.
async fn init_signal_stream(conn: &zbus::Connection) -> Option<MessageStream> {
    let rule = MatchRule::builder()
        .msg_type(Type::Signal)
        .path(NOTIFICATIONS_PATH)
        .interface(NOTIFICATIONS_INTERFACE)
        .build()
        .ok()?;
    MessageStream::for_match_rule(rule, conn, None).await.ok()
}

/// Handles an incoming notification signal message, updating UI state.
/// Returns whether the state was updated.
pub fn handle_notification_signal_message(
    state: &mut NotificationUiState,
    msg: &zbus::Message,
) -> bool {
    let header = msg.header();
    let Some(member) = header.member() else {
        return false;
    };
    match member.as_str() {
        "Shown" | "Updated" => {
            type ShownTuple = (
                u32,
                String,
                bool,
                String,
                String,
                String,
                Vec<String>,
                u8,
                i32,
            );

            if let Ok((
                id,
                app_name,
                app_name_verified,
                app_icon,
                summary,
                body,
                actions,
                urgency,
                timeout_ms,
            )) = msg.body().deserialize::<ShownTuple>()
            {
                let item = parse_shown_notification(
                    id,
                    app_name,
                    app_name_verified,
                    app_icon,
                    summary,
                    body,
                    actions,
                    urgency,
                    timeout_ms,
                );
                state.upsert(item);
                true
            } else {
                false
            }
        }
        "Closed" => {
            if let Ok((id, _reason)) = msg.body().deserialize::<(u32, u32)>() {
                state.remove(id)
            } else {
                false
            }
        }
        _ => false,
    }
}

/// Handles a NameOwnerChanged event for the daemon destination.
/// Returns whether the daemon disconnected and state should be reset.
pub fn handle_name_owner_changed_event(
    state: &mut NotificationUiState,
    new_owner: Option<&str>,
) -> bool {
    if new_owner.as_ref().is_none_or(|o| o.is_empty()) {
        info!("kynex-daemon left the bus; resetting notifications state");
        state.items.clear();
        true
    } else {
        false
    }
}

/// Event loop for notification D-Bus signals and name changes.
pub async fn run_notification_listener_loop<F>(
    conn: &zbus::Connection,
    mut on_state_change: F,
    mut shutdown: tokio::sync::watch::Receiver<bool>,
) where
    F: FnMut(NotificationUiState),
{
    let dbus_proxy = match DBusProxy::new(conn).await {
        Ok(p) => p,
        Err(err) => {
            warn!("NotificationModel failed to create DBusProxy: {err}");
            return;
        }
    };

    let mut name_owner_stream = match dbus_proxy
        .receive_name_owner_changed_with_args(&[(0, DAEMON_DESTINATION)])
        .await
    {
        Ok(s) => Some(s),
        Err(err) => {
            warn!("NotificationModel failed to register for NameOwnerChanged: {err}");
            None
        }
    };

    let mut signal_stream = init_signal_stream(conn).await;
    let mut state = NotificationUiState::default();

    loop {
        tokio::select! {
            _ = shutdown.changed() => {
                if *shutdown.borrow() {
                    break;
                }
            }

            Some(msg) = async {
                if let Some(ref mut s) = signal_stream {
                    s.next().await
                } else {
                    std::future::pending().await
                }
            } => {
                if let Ok(msg) = msg {
                    if handle_notification_signal_message(&mut state, &msg) {
                        on_state_change(state.clone());
                    }
                }
            }

            Some(name_event) = async {
                if let Some(ref mut s) = name_owner_stream {
                    s.next().await
                } else {
                    std::future::pending().await
                }
            } => {
                if let Ok(args) = name_event.args() {
                    let new_owner = args.new_owner();
                    let new_owner_str = new_owner.as_ref().map(|o| o.as_str());
                    if handle_name_owner_changed_event(&mut state, new_owner_str) {
                        on_state_change(state.clone());
                        signal_stream = None;
                    } else {
                        info!("kynex-daemon registered on bus; reconnecting notification stream");
                        signal_stream = init_signal_stream(conn).await;
                    }
                }
            }
        }
    }
}

/// Background asynchronous listener connecting to `org.kynexos.Daemon.Notifications1`.
pub async fn run_notification_listener(qt_thread: cxx_qt::CxxQtThread<qobject::NotificationModel>) {
    let session_conn = match zbus::connection::Builder::session().build().await {
        Ok(c) => c,
        Err(err) => {
            warn!("NotificationModel failed to connect to session bus: {err}");
            return;
        }
    };

    let (_tx, rx) = tokio::sync::watch::channel(false);
    run_notification_listener_loop(
        &session_conn,
        move |state| {
            dispatch_state(&qt_thread, state);
        },
        rx,
    )
    .await;
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::too_many_lines,
    clippy::manual_string_new
)]
mod tests {
    use super::*;

    #[test]
    fn test_compute_letter_badge() {
        assert_eq!(compute_letter_badge("Firefox"), "F");
        assert_eq!(compute_letter_badge(" terminal"), "T");
        assert_eq!(compute_letter_badge("123"), "1");
        assert_eq!(compute_letter_badge("!@#"), "?");
        assert_eq!(compute_letter_badge(""), "?");
    }

    #[test]
    fn test_format_actions_json() {
        let actions = vec![
            "open".to_string(),
            "Open File".to_string(),
            "close".to_string(),
            "Dismiss".to_string(),
        ];
        let json = format_actions_json(&actions);
        assert_eq!(
            json,
            r#"[{"key":"open","label":"Open File"},{"key":"close","label":"Dismiss"}]"#
        );

        let empty: Vec<String> = vec![];
        assert_eq!(format_actions_json(&empty), "[]");
    }

    #[test]
    fn test_notification_ui_state_upsert_and_remove() {
        let mut state = NotificationUiState::default();
        let item1 = NotificationUiItem {
            id: 1,
            app_name: "App 1".to_string(),
            app_name_verified: true,
            app_icon: "".to_string(),
            letter_badge: "A".to_string(),
            summary: "Sum 1".to_string(),
            body: "Body 1".to_string(),
            actions_json: "[]".to_string(),
            action_keys: vec![],
            action_labels: vec![],
            has_default_action: false,
            urgency: 1,
            timeout_ms: 5000,
        };
        state.upsert(item1.clone());
        assert_eq!(state.items.len(), 1);
        assert_eq!(state.items[0].id, 1);

        // Update item 1
        let mut item1_updated = item1;
        item1_updated.summary = "Updated Sum".to_string();
        state.upsert(item1_updated);
        assert_eq!(state.items.len(), 1);
        assert_eq!(state.items[0].summary, "Updated Sum");

        // Add 5 more items (enforce cap of 5)
        for i in 2..=6 {
            state.upsert(NotificationUiItem {
                id: i,
                app_name: format!("App {i}"),
                app_name_verified: false,
                app_icon: "".to_string(),
                letter_badge: "A".to_string(),
                summary: format!("Sum {i}"),
                body: "Body".to_string(),
                actions_json: "[]".to_string(),
                action_keys: vec![],
                action_labels: vec![],
                has_default_action: false,
                urgency: 1,
                timeout_ms: 5000,
            });
        }
        assert_eq!(state.items.len(), 5);
        assert_eq!(state.items[0].id, 6);

        // Remove item
        assert!(state.remove(6));
        assert_eq!(state.items.len(), 4);
        assert!(!state.remove(999));
    }

    #[test]
    fn test_escape_json_str_special_characters() {
        let mut out = String::new();
        escape_json_str(
            "quote: \" slash: \\ newline: \n cr: \r tab: \t control: \x07 end",
            &mut out,
        );
        assert!(out.contains("\\\""));
        assert!(out.contains("\\\\"));
        assert!(out.contains("\\n"));
        assert!(out.contains("\\r"));
        assert!(out.contains("\\t"));
        assert!(out.contains("\\u0007"));
    }

    #[test]
    fn test_parse_shown_notification_and_default_action() {
        let actions = vec![
            "default".to_string(),
            "Open".to_string(),
            "reply".to_string(),
            "Quick Reply".to_string(),
        ];
        let item = parse_shown_notification(
            42,
            "MailApp".to_string(),
            true,
            "mail-icon".to_string(),
            "New Mail".to_string(),
            "Message body".to_string(),
            actions,
            2,
            0,
        );

        assert_eq!(item.id, 42);
        assert_eq!(item.app_name, "MailApp");
        assert!(item.app_name_verified);
        assert_eq!(item.app_icon, "mail-icon");
        assert_eq!(item.letter_badge, "M");
        assert!(item.has_default_action);
        assert_eq!(item.action_keys, vec!["reply".to_string()]);
        assert_eq!(item.action_labels, vec!["Quick Reply".to_string()]);
        assert_eq!(item.urgency, 2);
        assert_eq!(item.timeout_ms, 0);
    }

    #[test]
    fn test_notification_model_rust_state_and_items() {
        let mut model_rust = NotificationModelRust::default();
        assert_eq!(model_rust.count, 0);
        assert!(model_rust.item_at(0).is_none());

        let mut state = NotificationUiState::default();
        let item = parse_shown_notification(
            10,
            "Test".to_string(),
            false,
            "".to_string(),
            "S".to_string(),
            "B".to_string(),
            Vec::<String>::new(),
            1,
            5000,
        );
        state.upsert(item);
        model_rust.set_state(state);

        assert_eq!(model_rust.count, 1);
        assert!(model_rust.item_at(0).is_some());
        assert_eq!(model_rust.item_at(0).unwrap().id, 10);
        assert!(model_rust.item_at(1).is_none());
    }

    #[test]
    fn test_role_constants_uniqueness() {
        let roles = [
            ROLE_ID,
            ROLE_APP_NAME,
            ROLE_APP_NAME_VERIFIED,
            ROLE_APP_ICON,
            ROLE_LETTER_BADGE,
            ROLE_SUMMARY,
            ROLE_BODY,
            ROLE_ACTIONS_JSON,
            ROLE_URGENCY,
            ROLE_TIMEOUT_MS,
            ROLE_ACTIONS_KEYS,
            ROLE_ACTIONS_LABELS,
            ROLE_HAS_DEFAULT_ACTION,
        ];
        let mut set = std::collections::HashSet::new();
        for r in roles {
            assert!(set.insert(r), "Duplicate role: {r}");
        }
        assert_eq!(set.len(), 13);
    }

    #[test]
    fn test_notification_item_role_variant_all_roles() {
        let item = parse_shown_notification(
            99,
            "TestApp".to_string(),
            true,
            "test-icon".to_string(),
            "Summary".to_string(),
            "Body".to_string(),
            vec![
                "default".to_string(),
                "Open".to_string(),
                "k1".to_string(),
                "L1".to_string(),
            ],
            1,
            5000,
        );

        let v_id = notification_item_role_variant(&item, ROLE_ID);
        assert_eq!(v_id.value::<i32>(), Some(99));

        let v_name = notification_item_role_variant(&item, ROLE_APP_NAME);
        assert_eq!(v_name.value::<QString>().unwrap().to_string(), "TestApp");

        let v_verified = notification_item_role_variant(&item, ROLE_APP_NAME_VERIFIED);
        assert_eq!(v_verified.value::<bool>(), Some(true));

        let v_icon = notification_item_role_variant(&item, ROLE_APP_ICON);
        assert_eq!(v_icon.value::<QString>().unwrap().to_string(), "test-icon");

        let v_badge = notification_item_role_variant(&item, ROLE_LETTER_BADGE);
        assert_eq!(v_badge.value::<QString>().unwrap().to_string(), "T");

        let v_sum = notification_item_role_variant(&item, ROLE_SUMMARY);
        assert_eq!(v_sum.value::<QString>().unwrap().to_string(), "Summary");

        let v_body = notification_item_role_variant(&item, ROLE_BODY);
        assert_eq!(v_body.value::<QString>().unwrap().to_string(), "Body");

        let v_json = notification_item_role_variant(&item, ROLE_ACTIONS_JSON);
        assert!(v_json
            .value::<QString>()
            .unwrap()
            .to_string()
            .contains("\"key\":\"default\""));

        let v_keys = notification_item_role_variant(&item, ROLE_ACTIONS_KEYS);
        assert_eq!(v_keys.value::<QStringList>().unwrap().len(), 1);

        let v_labels = notification_item_role_variant(&item, ROLE_ACTIONS_LABELS);
        assert_eq!(v_labels.value::<QStringList>().unwrap().len(), 1);

        let v_def = notification_item_role_variant(&item, ROLE_HAS_DEFAULT_ACTION);
        assert_eq!(v_def.value::<bool>(), Some(true));

        let v_urg = notification_item_role_variant(&item, ROLE_URGENCY);
        assert_eq!(v_urg.value::<i32>(), Some(1));

        let v_time = notification_item_role_variant(&item, ROLE_TIMEOUT_MS);
        assert_eq!(v_time.value::<i32>(), Some(5000));

        let v_unknown = notification_item_role_variant(&item, 9999);
        assert!(v_unknown.is_null());
    }

    #[test]
    fn test_create_notification_role_names() {
        let hash = create_notification_role_names();
        assert_eq!(hash.len(), 13);
        assert_eq!(hash.get(&ROLE_ID).unwrap().to_string(), "id");
        assert_eq!(hash.get(&ROLE_APP_NAME).unwrap().to_string(), "appName");
        assert_eq!(
            hash.get(&ROLE_APP_NAME_VERIFIED).unwrap().to_string(),
            "appNameVerified"
        );
        assert_eq!(hash.get(&ROLE_TIMEOUT_MS).unwrap().to_string(), "timeoutMs");
    }

    #[tokio::test]
    async fn test_init_signal_stream_and_proxy_calls() {
        if let Ok(conn) = zbus::connection::Builder::session().build().await {
            let stream = init_signal_stream(&conn).await;
            assert!(stream.is_some());

            let (tx, rx) = tokio::sync::watch::channel(false);
            let (state_tx, mut state_rx) = tokio::sync::mpsc::channel(10);
            let conn_clone = conn.clone();
            let handle = tokio::spawn(async move {
                run_notification_listener_loop(
                    &conn_clone,
                    move |state| {
                        let _ = state_tx.try_send(state);
                    },
                    rx,
                )
                .await;
            });

            // Emit a Shown signal to exercise listener loop stream reception
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            let shown_tuple = (
                9999u32,
                "LoopTestApp".to_string(),
                true,
                "icon".to_string(),
                "Loop Summary".to_string(),
                "Loop Body".to_string(),
                Vec::<String>::new(),
                1u8,
                5000i32,
            );
            let _ = conn
                .emit_signal(
                    Option::<&str>::None,
                    NOTIFICATIONS_PATH,
                    NOTIFICATIONS_INTERFACE,
                    "Shown",
                    &shown_tuple,
                )
                .await;

            let _ =
                tokio::time::timeout(std::time::Duration::from_millis(500), state_rx.recv()).await;

            let _ = tx.send(true);
            let _ = handle.await;

            let _ = send_dismiss_call(999).await;
            let _ = send_invoke_action_call(999, "default").await;
            let _ = send_pause_call(999).await;
            let _ = send_resume_call(999).await;
        }
    }

    #[test]
    fn test_notification_model_rust_row_count_data_and_record_first_frame() {
        let mut model_rust = NotificationModelRust::default();
        assert_eq!(model_rust.row_count(), 0);
        assert!(model_rust.data(0, ROLE_SUMMARY).is_null());
        assert!(model_rust.data(-1, ROLE_SUMMARY).is_null());
        assert!(model_rust.record_first_frame().is_none());

        let mut state = NotificationUiState::default();
        let item = parse_shown_notification(
            55,
            "Editor".to_string(),
            true,
            "edit".to_string(),
            "Saved".to_string(),
            "File saved".to_string(),
            Vec::<String>::new(),
            1,
            5000,
        );
        state.upsert(item);
        model_rust.set_state(state);

        assert_eq!(model_rust.row_count(), 1);
        let summary_var = model_rust.data(0, ROLE_SUMMARY);
        assert_eq!(summary_var.value::<QString>().unwrap().to_string(), "Saved");
        assert!(model_rust.data(1, ROLE_SUMMARY).is_null());

        model_rust.first_frame_pending = true;
        model_rust.last_shown_time = Some(std::time::Instant::now());
        let latency = model_rust.record_first_frame();
        assert!(latency.is_some());
        assert!(!model_rust.first_frame_pending);
        assert!(model_rust.record_first_frame().is_none());
    }

    #[test]
    fn test_handle_notification_signal_message_and_name_owner() {
        let mut state = NotificationUiState::default();

        let shown_tuple = (
            202u32,
            "Terminal".to_string(),
            false,
            "term".to_string(),
            "Command finished".to_string(),
            "Exit code 0".to_string(),
            vec!["default".to_string(), "Open".to_string()],
            0u8,
            3000i32,
        );

        let shown_msg = zbus::Message::signal(NOTIFICATIONS_PATH, NOTIFICATIONS_INTERFACE, "Shown")
            .build(&shown_tuple)
            .unwrap();

        assert!(handle_notification_signal_message(&mut state, &shown_msg));
        assert_eq!(state.items.len(), 1);
        assert_eq!(state.items[0].id, 202);
        assert_eq!(state.items[0].summary, "Command finished");

        let updated_tuple = (
            202u32,
            "Terminal".to_string(),
            true,
            "term".to_string(),
            "Command finished (updated)".to_string(),
            "Exit code 0".to_string(),
            Vec::<String>::new(),
            1u8,
            3000i32,
        );
        let updated_msg =
            zbus::Message::signal(NOTIFICATIONS_PATH, NOTIFICATIONS_INTERFACE, "Updated")
                .build(&updated_tuple)
                .unwrap();

        assert!(handle_notification_signal_message(&mut state, &updated_msg));
        assert_eq!(state.items.len(), 1);
        assert_eq!(state.items[0].summary, "Command finished (updated)");

        let closed_msg =
            zbus::Message::signal(NOTIFICATIONS_PATH, NOTIFICATIONS_INTERFACE, "Closed")
                .build(&(202u32, 1u32))
                .unwrap();

        assert!(handle_notification_signal_message(&mut state, &closed_msg));
        assert_eq!(state.items.len(), 0);

        let unknown_msg =
            zbus::Message::signal(NOTIFICATIONS_PATH, NOTIFICATIONS_INTERFACE, "UnknownSignal")
                .build(&())
                .unwrap();
        assert!(!handle_notification_signal_message(
            &mut state,
            &unknown_msg
        ));

        let bad_shown = zbus::Message::signal(NOTIFICATIONS_PATH, NOTIFICATIONS_INTERFACE, "Shown")
            .build(&123u32)
            .unwrap();
        assert!(!handle_notification_signal_message(&mut state, &bad_shown));

        let bad_closed =
            zbus::Message::signal(NOTIFICATIONS_PATH, NOTIFICATIONS_INTERFACE, "Closed")
                .build(&"bad")
                .unwrap();
        assert!(!handle_notification_signal_message(&mut state, &bad_closed));

        // Name owner changed events
        state.upsert(parse_shown_notification(
            1,
            "A".to_string(),
            false,
            "".to_string(),
            "S".to_string(),
            "B".to_string(),
            Vec::<String>::new(),
            1,
            5000,
        ));
        assert_eq!(state.items.len(), 1);

        assert!(!handle_name_owner_changed_event(&mut state, Some(":1.999")));
        assert_eq!(state.items.len(), 1);

        assert!(handle_name_owner_changed_event(&mut state, None));
        assert_eq!(state.items.len(), 0);

        state.upsert(parse_shown_notification(
            2,
            "B".to_string(),
            false,
            "".to_string(),
            "S".to_string(),
            "B".to_string(),
            Vec::<String>::new(),
            1,
            5000,
        ));
        assert!(handle_name_owner_changed_event(&mut state, Some("")));
        assert_eq!(state.items.len(), 0);
    }
}
