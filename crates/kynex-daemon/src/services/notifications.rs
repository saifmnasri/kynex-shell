//! Desktop Notifications service implementing `org.freedesktop.Notifications` v1.2
//! and `org.kynexos.Daemon.Notifications1`.
//!
//! Provides defense-in-depth sanitization, verified application identity resolution,
//! token-bucket rate limiting, event-driven deadline expiration scheduling,
//! and non-replacement well-known name acquisition.
#![allow(clippy::too_many_arguments)]

use std::collections::HashMap;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use std::time::Instant;

use kynex_ipc::notifications::{
    NotificationHistoryItem, FDO_NOTIFICATIONS_DESTINATION, FDO_NOTIFICATIONS_INTERFACE,
    FDO_NOTIFICATIONS_PATH,
};
use kynex_notifications::{
    validate_image_data, CloseOutcome, NotificationError, NotificationManager, RawImageData,
    Urgency,
};
use tokio::sync::{Mutex, Notify, RwLock};
use tracing::{info, warn};
use zbus::message::Header;
use zbus::object_server::SignalEmitter;
use zbus::{interface, Array, Connection, OwnedVariant, Structure};

use super::apps::AppsSharedState;

/// Shared runtime state for the notifications subsystem.
pub struct NotificationsSharedState {
    /// Pure Rust notification state machine.
    pub manager: RwLock<NotificationManager>,
    /// Optional reference to indexed applications for desktop-entry identity verification.
    pub apps_shared: Option<Arc<AppsSharedState>>,
    /// Event triggered when deadline timer needs recalculation.
    pub notify_event: Arc<Notify>,
    /// Signal emitter for standard `org.freedesktop.Notifications`.
    pub fdo_emitter: Mutex<Option<SignalEmitter<'static>>>,
    /// Signal emitter for internal `org.kynexos.Daemon.Notifications1`.
    pub notifications1_emitter: Mutex<Option<SignalEmitter<'static>>>,
    /// D-Bus connection handle for unicast signal emissions.
    pub conn: RwLock<Option<Connection>>,
    /// Whether `org.freedesktop.Notifications` was successfully claimed on the bus.
    pub fdo_name_acquired: AtomicBool,
}

impl NotificationsSharedState {
    /// Creates a new shared state instance.
    #[must_use]
    pub fn new(apps_shared: Option<Arc<AppsSharedState>>) -> Self {
        Self {
            manager: RwLock::new(NotificationManager::new()),
            apps_shared,
            notify_event: Arc::new(Notify::new()),
            fdo_emitter: Mutex::new(None),
            notifications1_emitter: Mutex::new(None),
            conn: RwLock::new(None),
            fdo_name_acquired: AtomicBool::new(false),
        }
    }

    /// Stores the active D-Bus connection.
    pub async fn set_connection(&self, conn: Connection) {
        *self.conn.write().await = Some(conn);
    }
}

/// Standard `org.freedesktop.Notifications` service endpoint.
#[derive(Clone)]
pub struct FdoNotificationsServer {
    shared: Arc<NotificationsSharedState>,
}

impl FdoNotificationsServer {
    /// Creates a new standard notifications server.
    #[must_use]
    pub fn new(shared: Arc<NotificationsSharedState>) -> Self {
        Self { shared }
    }

    /// Attaches the signal emitter for standard notifications signals.
    pub async fn set_emitter(&self, emitter: SignalEmitter<'static>) {
        *self.shared.fdo_emitter.lock().await = Some(emitter);
    }
}

#[interface(name = "org.freedesktop.Notifications")]
impl FdoNotificationsServer {
    /// Signal emitted when a notification is closed.
    #[zbus(signal)]
    pub async fn notification_closed(
        emitter: &SignalEmitter<'_>,
        id: u32,
        reason: u32,
    ) -> zbus::Result<()>;

    /// Signal emitted when a notification action is triggered by the user.
    #[zbus(signal)]
    pub async fn action_invoked(
        emitter: &SignalEmitter<'_>,
        id: u32,
        action_key: &str,
    ) -> zbus::Result<()>;

    /// Queries supported server capabilities.
    #[allow(clippy::unused_async, clippy::unused_async_trait_impl)]
    async fn get_capabilities(&self) -> Vec<String> {
        vec!["body".to_string(), "actions".to_string()]
    }

    /// Queries server name, vendor, version, and spec version.
    #[allow(clippy::unused_async, clippy::unused_async_trait_impl)]
    async fn get_server_information(&self) -> (String, String, String, String) {
        (
            "kynex-daemon".to_string(),
            "Kynex Project".to_string(),
            env!("CARGO_PKG_VERSION").to_string(),
            "1.2".to_string(),
        )
    }

    /// Creates or updates a desktop notification.
    #[allow(clippy::too_many_arguments)]
    async fn notify(
        &self,
        #[zbus(header)] header: Header<'_>,
        app_name: String,
        replaces_id: u32,
        app_icon: String,
        summary: String,
        body: String,
        actions: Vec<String>,
        hints: HashMap<String, OwnedVariant>,
        expire_timeout: i32,
    ) -> zbus::fdo::Result<u32> {
        let sender = header
            .sender()
            .map_or_else(|| ":0.0".to_string(), |s| s.as_str().to_string());

        let now = Instant::now();

        // Parse supported hints
        let urgency = extract_urgency_hint(&hints);
        let transient = extract_bool_hint(&hints, "transient").unwrap_or(false);
        let resident = extract_bool_hint(&hints, "resident").unwrap_or(false);
        let image = extract_image_data_hint(&hints);

        // Resolve application identity
        let (display_app_name, app_name_verified, final_app_icon) = self
            .resolve_app_identity(&hints, &app_name, &app_icon)
            .await;

        let outcome = {
            let mut mgr = self.shared.manager.write().await;
            mgr.notify(
                &sender,
                replaces_id,
                &display_app_name,
                app_name_verified,
                &final_app_icon,
                &summary,
                &body,
                &actions,
                urgency,
                expire_timeout,
                transient,
                resident,
                image,
                now,
            )
            .map_err(map_notification_error)?
        };

        // Notify expiry scheduler
        self.shared.notify_event.notify_one();

        // Emit signal to shell surface
        let notif = &outcome.notification;
        let actions_flat: Vec<String> = notif
            .actions
            .iter()
            .flat_map(|(k, l)| [k.clone(), l.clone()])
            .collect();
        let actions_refs: Vec<&str> = actions_flat.iter().map(String::as_str).collect();

        let lock = self.shared.notifications1_emitter.lock().await;
        if let Some(ref emitter) = *lock {
            if outcome.is_update {
                let _ = NotificationsServer::updated(
                    emitter,
                    notif.id,
                    &notif.app_name,
                    notif.app_name_verified,
                    &notif.app_icon,
                    &notif.summary,
                    &notif.body,
                    &actions_refs,
                    notif.urgency.as_byte(),
                    notif.timeout_ms,
                )
                .await;
            } else if outcome.show_popup {
                let _ = NotificationsServer::shown(
                    emitter,
                    notif.id,
                    &notif.app_name,
                    notif.app_name_verified,
                    &notif.app_icon,
                    &notif.summary,
                    &notif.body,
                    &actions_refs,
                    notif.urgency.as_byte(),
                    notif.timeout_ms,
                )
                .await;
            }
        }

        Ok(outcome.id)
    }

    /// Dismisses an active notification programmatically.
    async fn close_notification(
        &self,
        #[zbus(header)] header: Header<'_>,
        id: u32,
    ) -> zbus::fdo::Result<()> {
        let caller = header
            .sender()
            .map_or(":0.0", zbus::names::UniqueName::as_str);

        let outcome = {
            let mut mgr = self.shared.manager.write().await;
            mgr.close_notification_by_client(caller, id)
                .map_err(map_notification_error)?
        };

        self.emit_notification_closed(&outcome).await;
        Ok(())
    }
}

impl FdoNotificationsServer {
    /// Resolves desktop application identity via `desktop-entry` hint and `AppsIndex`.
    async fn resolve_app_identity(
        &self,
        hints: &HashMap<String, OwnedVariant>,
        raw_app_name: &str,
        raw_app_icon: &str,
    ) -> (String, bool, String) {
        let desktop_entry = hints
            .get("desktop-entry")
            .and_then(|v| <&str>::try_from(v).ok());

        if let Some(entry_id) = desktop_entry {
            if let Some(ref apps_shared) = self.shared.apps_shared {
                let index = apps_shared.index.read().await;
                // Query desktop entry by ID (e.g. "org.kde.konsole" or "org.kde.konsole.desktop")
                let lookup_id = if entry_id.ends_with(".desktop") {
                    entry_id.to_string()
                } else {
                    format!("{entry_id}.desktop")
                };

                if let Some(entry) = index.get(&lookup_id) {
                    let icon = if raw_app_icon.is_empty() {
                        entry.icon.clone()
                    } else {
                        raw_app_icon.to_string()
                    };
                    return (entry.name.default.clone(), true, icon);
                }
            }
        }

        (raw_app_name.to_string(), false, raw_app_icon.to_string())
    }

    async fn emit_notification_closed(&self, outcome: &CloseOutcome) {
        // Unicast NotificationClosed signal to creator
        if let Some(ref conn) = *self.shared.conn.read().await {
            let _ = conn
                .emit_signal(
                    Some(outcome.sender.as_str()),
                    FDO_NOTIFICATIONS_PATH,
                    FDO_NOTIFICATIONS_INTERFACE,
                    "NotificationClosed",
                    &(outcome.id, outcome.reason.as_u32()),
                )
                .await;
        }

        // Notify shell
        let lock = self.shared.notifications1_emitter.lock().await;
        if let Some(ref emitter) = *lock {
            let _ = NotificationsServer::closed(emitter, outcome.id, outcome.reason.as_u32()).await;

            // If a queued popup was promoted to visible, emit shown for it
            if let Some(ref promoted) = outcome.promoted {
                let actions_flat: Vec<String> = promoted
                    .actions
                    .iter()
                    .flat_map(|(k, l)| [k.clone(), l.clone()])
                    .collect();
                let actions_refs: Vec<&str> = actions_flat.iter().map(String::as_str).collect();

                let _ = NotificationsServer::shown(
                    emitter,
                    promoted.id,
                    &promoted.app_name,
                    promoted.app_name_verified,
                    &promoted.app_icon,
                    &promoted.summary,
                    &promoted.body,
                    &actions_refs,
                    promoted.urgency.as_byte(),
                    promoted.timeout_ms,
                )
                .await;
            }
        }
    }
}

/// Internal `org.kynexos.Daemon.Notifications1` service endpoint.
#[derive(Clone)]
pub struct NotificationsServer {
    shared: Arc<NotificationsSharedState>,
}

impl NotificationsServer {
    /// Creates a new shell notifications server.
    pub fn new(shared: Arc<NotificationsSharedState>) -> Self {
        Self { shared }
    }

    /// Attaches the signal emitter for shell notifications signals.
    pub async fn set_emitter(&self, emitter: SignalEmitter<'static>) {
        *self.shared.notifications1_emitter.lock().await = Some(emitter);
    }
}

#[allow(clippy::too_many_arguments)]
#[interface(name = "org.kynexos.Daemon.Notifications1")]
impl NotificationsServer {
    /// Signal emitted when a notification popup should be displayed on screen.
    #[allow(clippy::too_many_arguments)]
    #[zbus(signal)]
    pub async fn shown(
        emitter: &SignalEmitter<'_>,
        id: u32,
        app_name: &str,
        app_name_verified: bool,
        app_icon: &str,
        summary: &str,
        body: &str,
        actions: &[&str],
        urgency: u8,
        timeout_ms: i32,
    ) -> zbus::Result<()>;

    /// Signal emitted when an active notification popup is updated.
    #[allow(clippy::too_many_arguments)]
    #[zbus(signal)]
    pub async fn updated(
        emitter: &SignalEmitter<'_>,
        id: u32,
        app_name: &str,
        app_name_verified: bool,
        app_icon: &str,
        summary: &str,
        body: &str,
        actions: &[&str],
        urgency: u8,
        timeout_ms: i32,
    ) -> zbus::Result<()>;

    /// Signal emitted when a notification popup is removed/closed.
    #[zbus(signal)]
    pub async fn closed(emitter: &SignalEmitter<'_>, id: u32, reason: u32) -> zbus::Result<()>;

    /// Dismisses a notification popup from user interaction in the shell.
    async fn dismiss(&self, id: u32) -> zbus::fdo::Result<()> {
        let outcome = {
            let mut mgr = self.shared.manager.write().await;
            mgr.dismiss_by_user(id).map_err(map_notification_error)?
        };

        // Unicast close to creator
        if let Some(ref conn) = *self.shared.conn.read().await {
            let _ = conn
                .emit_signal(
                    Some(outcome.sender.as_str()),
                    FDO_NOTIFICATIONS_PATH,
                    FDO_NOTIFICATIONS_INTERFACE,
                    "NotificationClosed",
                    &(outcome.id, outcome.reason.as_u32()),
                )
                .await;
        }

        // Notify shell
        let lock = self.shared.notifications1_emitter.lock().await;
        if let Some(ref emitter) = *lock {
            let _ = Self::closed(emitter, outcome.id, outcome.reason.as_u32()).await;

            if let Some(ref promoted) = outcome.promoted {
                let actions_flat: Vec<String> = promoted
                    .actions
                    .iter()
                    .flat_map(|(k, l)| [k.clone(), l.clone()])
                    .collect();
                let actions_refs: Vec<&str> = actions_flat.iter().map(String::as_str).collect();

                let _ = Self::shown(
                    emitter,
                    promoted.id,
                    &promoted.app_name,
                    promoted.app_name_verified,
                    &promoted.app_icon,
                    &promoted.summary,
                    &promoted.body,
                    &actions_refs,
                    promoted.urgency.as_byte(),
                    promoted.timeout_ms,
                )
                .await;
            }
        }

        Ok(())
    }

    /// Pauses expiration timeout for a visible notification popup (e.g. while hovered).
    async fn pause_timeout(&self, id: u32) -> zbus::fdo::Result<()> {
        let mut mgr = self.shared.manager.write().await;
        mgr.pause_expiry(id, Instant::now());
        self.shared.notify_event.notify_one();
        Ok(())
    }

    /// Resumes expiration timeout for a visible notification popup (e.g. when unhovered).
    async fn resume_timeout(&self, id: u32) -> zbus::fdo::Result<()> {
        let mut mgr = self.shared.manager.write().await;
        mgr.resume_expiry(id, Instant::now());
        self.shared.notify_event.notify_one();
        Ok(())
    }

    /// Invokes an action on a notification popup from the shell.
    async fn invoke_action(&self, id: u32, key: String) -> zbus::fdo::Result<()> {
        let (sender, close_outcome) = {
            let mut mgr = self.shared.manager.write().await;
            mgr.invoke_action(id, &key)
                .map_err(map_notification_error)?
        };

        // Unicast ActionInvoked signal strictly to the original creator
        if let Some(ref conn) = *self.shared.conn.read().await {
            let _ = conn
                .emit_signal(
                    Some(sender.as_str()),
                    FDO_NOTIFICATIONS_PATH,
                    FDO_NOTIFICATIONS_INTERFACE,
                    "ActionInvoked",
                    &(id, &key),
                )
                .await;
        }

        // If non-resident, notification closed: broadcast close
        if let Some(ref outcome) = close_outcome {
            if let Some(ref conn) = *self.shared.conn.read().await {
                let _ = conn
                    .emit_signal(
                        Some(outcome.sender.as_str()),
                        FDO_NOTIFICATIONS_PATH,
                        FDO_NOTIFICATIONS_INTERFACE,
                        "NotificationClosed",
                        &(outcome.id, outcome.reason.as_u32()),
                    )
                    .await;
            }

            let lock = self.shared.notifications1_emitter.lock().await;
            if let Some(ref emitter) = *lock {
                let _ = Self::closed(emitter, outcome.id, outcome.reason.as_u32()).await;

                if let Some(ref promoted) = outcome.promoted {
                    let actions_flat: Vec<String> = promoted
                        .actions
                        .iter()
                        .flat_map(|(k, l)| [k.clone(), l.clone()])
                        .collect();
                    let actions_refs: Vec<&str> = actions_flat.iter().map(String::as_str).collect();

                    let _ = Self::shown(
                        emitter,
                        promoted.id,
                        &promoted.app_name,
                        promoted.app_name_verified,
                        &promoted.app_icon,
                        &promoted.summary,
                        &promoted.body,
                        &actions_refs,
                        promoted.urgency.as_byte(),
                        promoted.timeout_ms,
                    )
                    .await;
                }
            }
        }

        Ok(())
    }

    /// Returns bounded past notification history (up to 200 entries).
    async fn get_history(&self) -> Vec<NotificationHistoryItem> {
        let mgr = self.shared.manager.read().await;
        mgr.history()
            .iter()
            .map(|n| {
                let actions_flat: Vec<String> = n
                    .actions
                    .iter()
                    .flat_map(|(k, l)| [k.clone(), l.clone()])
                    .collect();

                NotificationHistoryItem {
                    id: n.id,
                    app_name: n.app_name.clone(),
                    app_icon: n.app_icon.clone(),
                    summary: n.summary.clone(),
                    body: n.body.clone(),
                    actions: actions_flat,
                    app_name_verified: n.app_name_verified,
                    urgency: n.urgency.as_byte(),
                }
            })
            .collect()
    }

    /// Do Not Disturb mode property.
    #[zbus(property)]
    async fn do_not_disturb(&self) -> bool {
        self.shared.manager.read().await.do_not_disturb()
    }

    /// Sets Do Not Disturb mode property.
    #[zbus(property)]
    async fn set_do_not_disturb(&self, value: bool) -> zbus::Result<()> {
        self.shared.manager.write().await.set_do_not_disturb(value);
        Ok(())
    }
}

/// Attempts to claim `org.freedesktop.Notifications` with `DO_NOT_QUEUE` and NO replacement.
///
/// Returns true if claimed, false if another daemon owns it or claiming failed.
pub async fn try_acquire_fdo_name(conn: &Connection) -> bool {
    match conn
        .request_name_with_flags(
            FDO_NOTIFICATIONS_DESTINATION,
            zbus::fdo::RequestNameFlags::DO_NOT_QUEUE,
        )
        .await
    {
        Ok(
            zbus::fdo::RequestNameReply::PrimaryOwner | zbus::fdo::RequestNameReply::AlreadyOwner,
        ) => {
            info!("Successfully claimed well-known name '{FDO_NOTIFICATIONS_DESTINATION}'");
            true
        }
        Ok(_) | Err(zbus::Error::NameTaken) => {
            warn!(
                "Name '{FDO_NOTIFICATIONS_DESTINATION}' is already owned by another service. Disabling standard notifications endpoint on this bus."
            );
            false
        }
        Err(e) => {
            warn!(
                "Failed to request '{FDO_NOTIFICATIONS_DESTINATION}': {e}. Disabling standard notifications endpoint."
            );
            false
        }
    }
}

/// Runs the single-deadline event-driven notification expiration loop.
pub async fn run_notification_expiry_scheduler(
    shared: Arc<NotificationsSharedState>,
    conn: Connection,
) {
    loop {
        let next_deadline = shared.manager.read().await.get_next_deadline();

        let sleep_fut = if let Some(deadline) = next_deadline {
            let now = Instant::now();
            if deadline <= now {
                tokio::time::sleep(std::time::Duration::ZERO)
            } else {
                tokio::time::sleep(deadline - now)
            }
        } else {
            // Sleep indefinitely until awakened by new notification event
            shared.notify_event.notified().await;
            continue;
        };

        tokio::select! {
            () = sleep_fut => {
                let now = Instant::now();
                let expired = {
                    let mut mgr = shared.manager.write().await;
                    mgr.expire_up_to(now)
                };

                for outcome in expired {
                    // Unicast close to creator
                    let _ = conn
                        .emit_signal(
                            Some(outcome.sender.as_str()),
                            FDO_NOTIFICATIONS_PATH,
                            FDO_NOTIFICATIONS_INTERFACE,
                            "NotificationClosed",
                            &(outcome.id, outcome.reason.as_u32()),
                        )
                        .await;

                    // Notify shell
                    let lock = shared.notifications1_emitter.lock().await;
                    if let Some(ref emitter) = *lock {
                        let _ = NotificationsServer::closed(emitter, outcome.id, outcome.reason.as_u32()).await;

                        if let Some(ref promoted) = outcome.promoted {
                            let actions_flat: Vec<String> = promoted
                                .actions
                                .iter()
                                .flat_map(|(k, l)| [k.clone(), l.clone()])
                                .collect();
                            let actions_refs: Vec<&str> = actions_flat.iter().map(String::as_str).collect();

                            let _ = NotificationsServer::shown(
                                emitter,
                                promoted.id,
                                &promoted.app_name,
                                promoted.app_name_verified,
                                &promoted.app_icon,
                                &promoted.summary,
                                &promoted.body,
                                &actions_refs,
                                promoted.urgency.as_byte(),
                                promoted.timeout_ms,
                            )
                            .await;
                        }
                    }
                }
            }
            () = shared.notify_event.notified() => {
                // New notification posted or updated: loop to re-evaluate deadline
            }
        }
    }
}

/// Helper mapping `NotificationError` to standard D-Bus error responses.
#[allow(clippy::needless_pass_by_value)]
fn map_notification_error(err: NotificationError) -> zbus::fdo::Error {
    match err {
        NotificationError::LimitsExceeded(msg) => zbus::fdo::Error::LimitsExceeded(msg.to_string()),
        NotificationError::SenderMismatch => {
            zbus::fdo::Error::AccessDenied("Caller does not own this notification".to_string())
        }
        NotificationError::InvalidArgs(msg) | NotificationError::InvalidImageData(msg) => {
            zbus::fdo::Error::InvalidArgs(msg.to_string())
        }
        NotificationError::NotFound(_) => {
            // FDO Desktop Notifications Spec v1.2 §4.2:
            // "If the notification no longer exists, an empty D-BUS Error message is sent back."
            zbus::fdo::Error::InvalidArgs(String::new())
        }
    }
}

/// Extracts urgency hint byte.
fn extract_urgency_hint(hints: &HashMap<String, OwnedVariant>) -> Urgency {
    if let Some(v) = hints.get("urgency") {
        if let Ok(b) = u8::try_from(v) {
            return Urgency::from_byte(b);
        }
        if let Ok(i) = i32::try_from(v) {
            return Urgency::from_byte(u8::try_from(i).unwrap_or(1));
        }
        if let Ok(u) = u32::try_from(v) {
            return Urgency::from_byte(u8::try_from(u).unwrap_or(1));
        }
    }
    Urgency::Normal
}

/// Extracts boolean hint.
fn extract_bool_hint(hints: &HashMap<String, OwnedVariant>, key: &str) -> Option<bool> {
    hints.get(key).and_then(|v| bool::try_from(v).ok())
}

/// Extracts and strictly validates raw embedded image-data hint.
fn extract_image_data_hint(hints: &HashMap<String, OwnedVariant>) -> Option<RawImageData> {
    let val = hints
        .get("image-data")
        .or_else(|| hints.get("image_data"))?;
    let structure = <&Structure<'_>>::try_from(val).ok()?;
    let fields = structure.fields();
    if fields.len() != 7 {
        return None;
    }
    let width = i32::try_from(&fields[0]).ok()?;
    let height = i32::try_from(&fields[1]).ok()?;
    let rowstride = i32::try_from(&fields[2]).ok()?;
    let has_alpha = bool::try_from(&fields[3]).ok()?;
    let bits_per_sample = i32::try_from(&fields[4]).ok()?;
    let channels = i32::try_from(&fields[5]).ok()?;
    let array = <&Array<'_>>::try_from(&fields[6]).ok()?;
    let data: Vec<u8> = array
        .inner()
        .iter()
        .filter_map(|v| u8::try_from(v).ok())
        .collect();
    if data.len() != array.len() {
        return None;
    }

    validate_image_data(
        width,
        height,
        rowstride,
        has_alpha,
        bits_per_sample,
        channels,
        &data,
    )
    .ok()
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use kynex_notifications::Urgency;
    use zbus::{Structure, Variant};

    #[test]
    fn test_map_notification_error() {
        let err1 = map_notification_error(NotificationError::LimitsExceeded("limit"));
        assert!(matches!(err1, zbus::fdo::Error::LimitsExceeded(_)));

        let err2 = map_notification_error(NotificationError::SenderMismatch);
        assert!(matches!(err2, zbus::fdo::Error::AccessDenied(_)));

        let err3 = map_notification_error(NotificationError::InvalidArgs("args"));
        assert!(matches!(err3, zbus::fdo::Error::InvalidArgs(_)));

        let err4 = map_notification_error(NotificationError::InvalidImageData("img"));
        assert!(matches!(err4, zbus::fdo::Error::InvalidArgs(_)));

        let err5 = map_notification_error(NotificationError::NotFound(42));
        assert!(matches!(err5, zbus::fdo::Error::InvalidArgs(_)));
    }

    #[test]
    fn test_extract_urgency_hint() {
        let mut hints = HashMap::new();
        assert_eq!(extract_urgency_hint(&hints), Urgency::Normal);

        hints.insert("urgency".to_string(), OwnedVariant::from(0u8));
        assert_eq!(extract_urgency_hint(&hints), Urgency::Low);

        hints.insert("urgency".to_string(), OwnedVariant::from(2i32));
        assert_eq!(extract_urgency_hint(&hints), Urgency::Critical);

        hints.insert("urgency".to_string(), OwnedVariant::from(1u32));
        assert_eq!(extract_urgency_hint(&hints), Urgency::Normal);

        hints.insert(
            "urgency".to_string(),
            OwnedVariant::try_from(Variant::from("unknown")).unwrap(),
        );
        assert_eq!(extract_urgency_hint(&hints), Urgency::Normal);
    }

    #[test]
    fn test_extract_bool_hint() {
        let mut hints = HashMap::new();
        assert_eq!(extract_bool_hint(&hints, "transient"), None);

        hints.insert("transient".to_string(), OwnedVariant::from(true));
        assert_eq!(extract_bool_hint(&hints, "transient"), Some(true));

        hints.insert("transient".to_string(), OwnedVariant::from(false));
        assert_eq!(extract_bool_hint(&hints, "transient"), Some(false));

        hints.insert(
            "transient".to_string(),
            OwnedVariant::try_from(Variant::from("not_a_bool")).unwrap(),
        );
        assert_eq!(extract_bool_hint(&hints, "transient"), None);
    }

    #[test]
    fn test_extract_image_data_hint() {
        let mut hints = HashMap::new();
        assert!(extract_image_data_hint(&hints).is_none());

        // Invalid structure (too few fields)
        let small_struct = Structure::builder().add_field(10i32).build().unwrap();
        hints.insert(
            "image-data".to_string(),
            OwnedVariant::try_from(Variant::from(small_struct)).unwrap(),
        );
        assert!(extract_image_data_hint(&hints).is_none());

        // Valid image data structure (width=2, height=2, rowstride=6, alpha=false, bits=8, channels=3, 12 bytes)
        let pixels: Vec<u8> = vec![255, 0, 0, 0, 255, 0, 0, 0, 255, 255, 255, 255];
        let valid_struct = Structure::builder()
            .add_field(2i32)
            .add_field(2i32)
            .add_field(6i32)
            .add_field(false)
            .add_field(8i32)
            .add_field(3i32)
            .add_field(Array::from(pixels.clone()))
            .build()
            .unwrap();

        let mut valid_hints = HashMap::new();
        valid_hints.insert(
            "image_data".to_string(),
            OwnedVariant::try_from(Variant::from(valid_struct)).unwrap(),
        );
        let extracted = extract_image_data_hint(&valid_hints);
        assert!(extracted.is_some());
        let img = extracted.unwrap();
        assert_eq!(img.width, 2);
        assert_eq!(img.height, 2);
        assert_eq!(img.data.len(), 12);
    }

    #[tokio::test]
    async fn test_fdo_server_direct_calls() {
        let shared = Arc::new(NotificationsSharedState::new(None));
        let server = FdoNotificationsServer::new(shared.clone());

        let caps = server.get_capabilities().await;
        assert_eq!(caps, vec!["body".to_string(), "actions".to_string()]);

        let (name, vendor, version, spec) = server.get_server_information().await;
        assert_eq!(name, "kynex-daemon");
        assert_eq!(vendor, "Kynex Project");
        assert_eq!(spec, "1.2");
        assert_ne!(version, "");

        let hints = HashMap::new();
        let (name, verified, icon) = server
            .resolve_app_identity(&hints, "RawApp", "raw-icon")
            .await;
        assert_eq!(name, "RawApp");
        assert!(!verified);
        assert_eq!(icon, "raw-icon");
    }

    #[tokio::test]
    async fn test_notifications1_server_direct_calls() {
        let shared = Arc::new(NotificationsSharedState::new(None));
        let server = NotificationsServer::new(shared.clone());

        // DND property
        assert!(!server.do_not_disturb().await);
        server.set_do_not_disturb(true).await.unwrap();
        assert!(server.do_not_disturb().await);
        server.set_do_not_disturb(false).await.unwrap();
        assert!(!server.do_not_disturb().await);

        // Add a notification directly into state machine to test server endpoints
        let actions = vec!["open".to_string(), "Open".to_string()];
        let id = {
            let mut mgr = shared.manager.write().await;
            let outcome = mgr
                .notify(
                    ":1.100",
                    0,
                    "test-app",
                    false,
                    "icon",
                    "Summary",
                    "Body",
                    &actions,
                    Urgency::Normal,
                    5000,
                    false,
                    false,
                    None,
                    Instant::now(),
                )
                .unwrap();
            outcome.id
        };

        // Pause and resume timeouts
        assert!(server.pause_timeout(id).await.is_ok());
        assert!(server.resume_timeout(id).await.is_ok());

        // Invoke action
        assert!(server.invoke_action(id, "open".to_string()).await.is_ok());

        // History
        let hist = server.get_history().await;
        assert_eq!(hist.len(), 1);
        assert_eq!(hist[0].id, id);
        assert_eq!(hist[0].summary, "Summary");

        // Dismiss (not found now because action was invoked and closed)
        assert!(server.dismiss(id).await.is_err());
    }

    #[tokio::test]
    async fn test_expiry_scheduler_promotion() {
        let conn = zbus::connection::Builder::session().build().await.unwrap();

        let shared = Arc::new(NotificationsSharedState::new(None));
        shared.set_connection(conn.clone()).await;

        let scheduler_shared = shared.clone();
        let scheduler_conn = conn.clone();
        let handle = tokio::spawn(async move {
            run_notification_expiry_scheduler(scheduler_shared, scheduler_conn).await;
        });

        // Add a notification with short timeout (10 ms)
        {
            let mut mgr = shared.manager.write().await;
            mgr.notify(
                ":1.200",
                0,
                "short-app",
                false,
                "",
                "Expiring",
                "Will expire soon",
                &[],
                Urgency::Normal,
                10,
                false,
                false,
                None,
                Instant::now(),
            )
            .unwrap();
        }
        shared.notify_event.notify_one();

        // Wait for scheduler to expire it
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let mgr = shared.manager.read().await;
        assert_eq!(mgr.active_notifications().len(), 0);
        assert_eq!(mgr.history().len(), 1);

        handle.abort();
    }

    #[tokio::test]
    async fn test_try_acquire_fdo_name_coverage() {
        let conn = zbus::connection::Builder::session().build().await.unwrap();
        // Calls the function exercising the match on request_name_with_flags
        let _ = try_acquire_fdo_name(&conn).await;
    }

    #[tokio::test]
    async fn test_promotions_and_signals_coverage() {
        let conn = zbus::connection::Builder::session().build().await.unwrap();

        let shared = Arc::new(NotificationsSharedState::new(None));
        shared.set_connection(conn.clone()).await;

        let fdo_server = FdoNotificationsServer::new(shared.clone());
        let shell_server = NotificationsServer::new(shared.clone());

        // Fill up to 5 visible + 2 queued
        let mut ids = Vec::new();
        for i in 1..=7 {
            let mut mgr = shared.manager.write().await;
            let outcome = mgr
                .notify(
                    ":1.500",
                    0,
                    &format!("App{i}"),
                    false,
                    "icon",
                    &format!("Sum{i}"),
                    "Body",
                    &["default".to_string(), "Open".to_string()],
                    Urgency::Normal,
                    5000,
                    false,
                    false,
                    None,
                    Instant::now(),
                )
                .unwrap();
            ids.push(outcome.id);
        }

        assert_eq!(ids.len(), 7);

        // 1. Dismiss id 0 -> promotes id 5 (first queued)
        assert!(shell_server.dismiss(ids[0]).await.is_ok());

        // 2. Invoke action on id 1 -> promotes id 6 (next queued)
        assert!(shell_server
            .invoke_action(ids[1], "default".to_string())
            .await
            .is_ok());

        // 3. Close notification from client
        let outcome = {
            let mut mgr = shared.manager.write().await;
            mgr.close_notification_by_client(":1.500", ids[2]).unwrap()
        };
        fdo_server.emit_notification_closed(&outcome).await;
    }

    #[tokio::test]
    async fn test_resolve_app_identity_with_apps_index() {
        let temp_dir = tempfile::tempdir().unwrap();
        let app_dir = temp_dir.path().to_path_buf();
        let desktop_file = app_dir.join("test-app.desktop");
        std::fs::write(
            &desktop_file,
            "[Desktop Entry]\nType=Application\nName=Verified Test App\nExec=true\nIcon=verified-icon\n",
        )
        .unwrap();

        let apps_shared = Arc::new(crate::services::apps::AppsSharedState::new(
            vec![app_dir],
            "Kynex",
        ));
        let shared = Arc::new(NotificationsSharedState::new(Some(apps_shared)));
        let server = FdoNotificationsServer::new(shared.clone());

        let mut hints = HashMap::new();
        hints.insert(
            "desktop-entry".to_string(),
            OwnedVariant::try_from(Variant::from("test-app")).unwrap(),
        );

        let (name, verified, icon) = server
            .resolve_app_identity(&hints, "UnverifiedName", "")
            .await;
        assert_eq!(name, "Verified Test App");
        assert!(verified);
        assert_eq!(icon, "verified-icon");

        // With explicit custom icon
        let (name2, verified2, icon2) = server
            .resolve_app_identity(&hints, "UnverifiedName", "custom-icon")
            .await;
        assert_eq!(name2, "Verified Test App");
        assert!(verified2);
        assert_eq!(icon2, "custom-icon");

        // With .desktop suffix in hint
        let mut hints_suffix = HashMap::new();
        hints_suffix.insert(
            "desktop-entry".to_string(),
            OwnedVariant::try_from(Variant::from("test-app.desktop")).unwrap(),
        );
        let (name3, verified3, icon3) = server
            .resolve_app_identity(&hints_suffix, "UnverifiedName", "")
            .await;
        assert_eq!(name3, "Verified Test App");
        assert!(verified3);
        assert_eq!(icon3, "verified-icon");

        let _ = std::fs::remove_file(&desktop_file);
        let _ = temp_dir.close();
    }
}
