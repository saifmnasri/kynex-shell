//! Pure-Rust notification state machine, monotonic ID allocation, and queueing.

use std::collections::{HashMap, HashSet, VecDeque};
use std::time::{Duration, Instant};

use crate::error::NotificationError;
use crate::image::RawImageData;
use crate::limits::{
    DEFAULT_EXPIRE_TIMEOUT_MS, MAX_EXPIRE_TIMEOUT_MS, MAX_HISTORY_ENTRIES, MAX_VISIBLE_POPUPS,
};
use crate::rate_limit::RateLimiter;
use crate::sanitize::{
    sanitize_app_name, sanitize_body, sanitize_summary, validate_and_sanitize_actions,
};

/// Notification urgency level defined in the Freedesktop specification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default)]
pub enum Urgency {
    /// Low urgency (background, non-intrusive).
    Low = 0,
    /// Standard notification urgency.
    #[default]
    Normal = 1,
    /// Critical urgency (persists until acknowledged, bypasses Do Not Disturb).
    Critical = 2,
}

impl Urgency {
    /// Converts a raw byte hint to an `Urgency`.
    #[must_use]
    pub fn from_byte(b: u8) -> Self {
        match b {
            0 => Self::Low,
            2 => Self::Critical,
            _ => Self::Normal,
        }
    }

    /// Converts an `Urgency` to a standard byte value.
    #[must_use]
    pub fn as_byte(self) -> u8 {
        self as u8
    }
}

/// Standard notification close reasons defined in the Freedesktop specification.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CloseReason {
    /// The notification expired due to timeout.
    Expired = 1,
    /// The notification was dismissed by the user.
    DismissedByUser = 2,
    /// The notification was closed by a call to `CloseNotification`.
    CloseNotification = 3,
    /// Undefined / reserved close reason.
    Undefined = 4,
}

impl CloseReason {
    /// Converts `CloseReason` to the standard D-Bus u32 code.
    #[must_use]
    pub fn as_u32(self) -> u32 {
        self as u32
    }
}

/// In-memory representation of an active or historical notification.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Notification {
    /// Monotonic non-zero identifier.
    pub id: u32,
    /// Unique D-Bus connection name of the creator (e.g. `:1.42`).
    pub sender: String,
    /// Sanitized application name.
    pub app_name: String,
    /// Whether `app_name` was verified against the desktop application index.
    pub app_name_verified: bool,
    /// Icon theme name or icon path.
    pub app_icon: String,
    /// Sanitized single-line summary.
    pub summary: String,
    /// Sanitized body text (markup stripped).
    pub body: String,
    /// List of action pairs: (key, label).
    pub actions: Vec<(String, String)>,
    /// Urgency level.
    pub urgency: Urgency,
    /// Original expiration timeout in milliseconds.
    pub timeout_ms: i32,
    /// Whether notification should bypass history.
    pub transient: bool,
    /// Whether clicking an action leaves the notification visible.
    pub resident: bool,
    /// Timestamp when notification was created or updated.
    pub created_at: Instant,
    /// Calculated expiration deadline, or None if persistent.
    pub expires_at: Option<Instant>,
    /// Remaining time when paused (e.g. while hovered).
    pub paused_remaining: Option<Duration>,
    /// Optional validated embedded raw image data.
    pub image: Option<RawImageData>,
}

/// Result returned when a notification is successfully processed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotifyOutcome {
    /// The allocated or reused notification ID.
    pub id: u32,
    /// Whether the notification is an update of an existing notification.
    pub is_update: bool,
    /// Whether the notification should be immediately displayed on the desktop popup surface.
    pub show_popup: bool,
    /// The complete notification payload.
    pub notification: Notification,
}

/// Result returned when a notification closes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CloseOutcome {
    /// ID of the closed notification.
    pub id: u32,
    /// Original creator D-Bus unique bus name.
    pub sender: String,
    /// Standard close reason code.
    pub reason: CloseReason,
    /// Next notification from the pending queue promoted to visible (if any).
    pub promoted: Option<Notification>,
}

/// Central pure-Rust state machine for notification dispatch, queueing, and history.
#[derive(Debug)]
pub struct NotificationManager {
    next_id: u32,
    active_notifications: HashMap<u32, Notification>,
    visible_popups: VecDeque<u32>,
    pending_queue: VecDeque<u32>,
    history: VecDeque<Notification>,
    history_ids: HashSet<u32>,
    rate_limiter: RateLimiter,
    dnd_enabled: bool,
}

impl Default for NotificationManager {
    fn default() -> Self {
        Self::new()
    }
}

impl NotificationManager {
    /// Initializes an empty notification manager.
    #[must_use]
    pub fn new() -> Self {
        Self {
            next_id: 1,
            active_notifications: HashMap::new(),
            visible_popups: VecDeque::with_capacity(MAX_VISIBLE_POPUPS),
            pending_queue: VecDeque::new(),
            history: VecDeque::with_capacity(MAX_HISTORY_ENTRIES),
            history_ids: HashSet::with_capacity(MAX_HISTORY_ENTRIES),
            rate_limiter: RateLimiter::new(),
            dnd_enabled: false,
        }
    }

    /// Checks and sets the Do Not Disturb (DND) state.
    pub fn set_do_not_disturb(&mut self, enabled: bool) {
        self.dnd_enabled = enabled;
    }

    /// Returns the current Do Not Disturb state.
    #[must_use]
    pub fn do_not_disturb(&self) -> bool {
        self.dnd_enabled
    }

    /// Allocates the next monotonic non-zero u32 ID with safe wraparound and collision skipping.
    fn allocate_id(&mut self) -> u32 {
        loop {
            let candidate = self.next_id;
            // Advance with wraparound skipping 0
            self.next_id = if self.next_id == u32::MAX {
                1
            } else {
                self.next_id + 1
            };

            if candidate != 0
                && !self.active_notifications.contains_key(&candidate)
                && !self.history_ids.contains(&candidate)
            {
                return candidate;
            }
        }
    }

    /// Dispatches a notification from a D-Bus client.
    ///
    /// # Errors
    ///
    /// Returns [`NotificationError::LimitsExceeded`] if rate limits or field counts are exceeded,
    /// [`NotificationError::SenderMismatch`] if attempting to replace a notification owned by another sender,
    /// or [`NotificationError::InvalidArgs`] if action formatting is malformed.
    #[allow(clippy::too_many_arguments)]
    pub fn notify(
        &mut self,
        sender: &str,
        replaces_id: u32,
        app_name: &str,
        app_name_verified: bool,
        app_icon: &str,
        summary: &str,
        body: &str,
        raw_actions: &[String],
        urgency: Urgency,
        timeout_ms: i32,
        transient: bool,
        resident: bool,
        image: Option<RawImageData>,
        now: Instant,
    ) -> Result<NotifyOutcome, NotificationError> {
        // Enforce per-sender rate limiting
        self.rate_limiter.check_and_consume(sender, now)?;

        // Sanitize textual inputs
        let sanitized_app_name = sanitize_app_name(app_name);
        let sanitized_summary = sanitize_summary(summary);
        let sanitized_body = sanitize_body(body);
        let sanitized_actions = validate_and_sanitize_actions(raw_actions)?;

        // Calculate expiration deadline
        let expires_at = if urgency == Urgency::Critical || timeout_ms == 0 {
            None // Persist until dismissed
        } else if timeout_ms == -1 {
            Some(
                now + Duration::from_millis(
                    u64::try_from(DEFAULT_EXPIRE_TIMEOUT_MS).unwrap_or(5000),
                ),
            )
        } else if timeout_ms > 0 {
            let clamped = timeout_ms.min(MAX_EXPIRE_TIMEOUT_MS);
            let millis = u64::try_from(clamped).unwrap_or(5000);
            Some(now + Duration::from_millis(millis))
        } else {
            // Unrecognized negative values fall back to default
            Some(
                now + Duration::from_millis(
                    u64::try_from(DEFAULT_EXPIRE_TIMEOUT_MS).unwrap_or(5000),
                ),
            )
        };

        // Check replacement semantics
        let (id, is_update) = if replaces_id != 0 {
            if let Some(existing) = self.active_notifications.get(&replaces_id) {
                // Sender must match the original creator
                if existing.sender != sender {
                    return Err(NotificationError::SenderMismatch);
                }
                (replaces_id, true)
            } else {
                // Check if the id is currently owned by another sender in history
                if let Some(hist) = self.history.iter().find(|n| n.id == replaces_id) {
                    if hist.sender != sender {
                        return Err(NotificationError::SenderMismatch);
                    }
                    // If previously owned by same sender, remove from history and recreate
                    self.history_ids.remove(&replaces_id);
                    self.history.retain(|n| n.id != replaces_id);
                }
                // Spec v1.2 §4.1: If replaces_id is not 0, the returned value is the same value as replaces_id.
                (replaces_id, false)
            }
        } else {
            (self.allocate_id(), false)
        };

        let notification = Notification {
            id,
            sender: sender.to_string(),
            app_name: sanitized_app_name,
            app_name_verified,
            app_icon: app_icon.to_string(),
            summary: sanitized_summary,
            body: sanitized_body,
            actions: sanitized_actions,
            urgency,
            timeout_ms,
            transient,
            resident,
            created_at: now,
            expires_at,
            paused_remaining: None,
            image,
        };

        // Determine popup display eligibility
        let suppress_popup = self.dnd_enabled && urgency != Urgency::Critical;

        let mut show_popup = false;
        if is_update {
            // Already active: update in-place
            self.active_notifications.insert(id, notification.clone());
            if self.visible_popups.contains(&id) {
                show_popup = true;
            }
        } else {
            self.active_notifications.insert(id, notification.clone());
            if suppress_popup {
                if !transient {
                    // If suppressed by DND, immediately record in history if not transient
                    self.record_history(notification.clone());
                }
            } else if self.visible_popups.len() < MAX_VISIBLE_POPUPS {
                self.visible_popups.push_back(id);
                show_popup = true;
            } else {
                self.pending_queue.push_back(id);
            }
        }

        Ok(NotifyOutcome {
            id,
            is_update,
            show_popup,
            notification,
        })
    }

    /// Closes a notification explicitly via D-Bus `CloseNotification`.
    ///
    /// Requires that the caller matches the original sender.
    ///
    /// # Errors
    ///
    /// Returns [`NotificationError::NotFound`] if the notification does not exist,
    /// or [`NotificationError::SenderMismatch`] if caller does not own the notification.
    pub fn close_notification_by_client(
        &mut self,
        caller: &str,
        id: u32,
    ) -> Result<CloseOutcome, NotificationError> {
        let notification = self
            .active_notifications
            .get(&id)
            .ok_or(NotificationError::NotFound(id))?;

        if notification.sender != caller {
            return Err(NotificationError::SenderMismatch);
        }

        self.close_internal(id, CloseReason::CloseNotification)
    }

    /// Closes a notification dismissed by user interaction on the shell surface.
    ///
    /// # Errors
    ///
    /// Returns [`NotificationError::NotFound`] if the notification does not exist.
    pub fn dismiss_by_user(&mut self, id: u32) -> Result<CloseOutcome, NotificationError> {
        self.close_internal(id, CloseReason::DismissedByUser)
    }

    /// Invokes an action on a notification.
    ///
    /// Returns the original creator's unique bus name and the invoked key.
    /// Closes the popup if the notification is not `resident`.
    ///
    /// # Errors
    ///
    /// Returns [`NotificationError::NotFound`] if the notification does not exist,
    /// or [`NotificationError::InvalidArgs`] if the action key is invalid.
    pub fn invoke_action(
        &mut self,
        id: u32,
        action_key: &str,
    ) -> Result<(String, Option<CloseOutcome>), NotificationError> {
        let (sender, resident, has_action) = {
            let notification = self
                .active_notifications
                .get(&id)
                .ok_or(NotificationError::NotFound(id))?;

            let has_action = action_key == "default"
                || notification.actions.iter().any(|(k, _)| k == action_key);

            (
                notification.sender.clone(),
                notification.resident,
                has_action,
            )
        };

        if !has_action {
            return Err(NotificationError::InvalidArgs("Action key not found"));
        }

        let close_outcome = if resident {
            None
        } else {
            Some(self.close_internal(id, CloseReason::DismissedByUser)?)
        };

        Ok((sender, close_outcome))
    }

    /// Internal helper to close a notification, record history, and promote queued popups.
    fn close_internal(
        &mut self,
        id: u32,
        reason: CloseReason,
    ) -> Result<CloseOutcome, NotificationError> {
        let notification = self
            .active_notifications
            .remove(&id)
            .ok_or(NotificationError::NotFound(id))?;

        // Remove from visible popups or pending queue
        self.visible_popups.retain(|&item_id| item_id != id);
        self.pending_queue.retain(|&item_id| item_id != id);

        // Record in history if not transient
        if !notification.transient {
            self.record_history(notification.clone());
        }

        // Promote the next pending popup if a visible slot opened
        let mut promoted = None;
        while self.visible_popups.len() < MAX_VISIBLE_POPUPS {
            if let Some(next_id) = self.pending_queue.pop_front() {
                if let Some(next_notif) = self.active_notifications.get(&next_id) {
                    self.visible_popups.push_back(next_id);
                    promoted = Some(next_notif.clone());
                    break;
                }
            } else {
                break;
            }
        }

        Ok(CloseOutcome {
            id,
            sender: notification.sender,
            reason,
            promoted,
        })
    }

    /// Appends a notification to the bounded history ring buffer.
    fn record_history(&mut self, notification: Notification) {
        if self.history.len() >= MAX_HISTORY_ENTRIES {
            if let Some(evicted) = self.history.pop_front() {
                self.history_ids.remove(&evicted.id);
            }
        }
        self.history_ids.insert(notification.id);
        self.history.push_back(notification);
    }

    /// Pauses expiration deadline for a notification if active.
    pub fn pause_expiry(&mut self, id: u32, now: Instant) {
        if let Some(n) = self.active_notifications.get_mut(&id) {
            if let Some(deadline) = n.expires_at.take() {
                let remaining = deadline.saturating_duration_since(now);
                n.paused_remaining = Some(remaining);
            }
        }
    }

    /// Resumes expiration deadline for a notification if paused.
    pub fn resume_expiry(&mut self, id: u32, now: Instant) {
        if let Some(n) = self.active_notifications.get_mut(&id) {
            if let Some(remaining) = n.paused_remaining.take() {
                n.expires_at = Some(now + remaining);
            }
        }
    }

    /// Returns the next earliest expiration deadline among all visible popups, or None.
    #[must_use]
    pub fn get_next_deadline(&self) -> Option<Instant> {
        self.visible_popups
            .iter()
            .filter_map(|id| self.active_notifications.get(id))
            .filter_map(|n| n.expires_at)
            .min()
    }

    /// Expires all visible notifications whose deadline is at or before `now`.
    ///
    /// Returns the close outcomes for all expired notifications.
    pub fn expire_up_to(&mut self, now: Instant) -> Vec<CloseOutcome> {
        let expired_ids: Vec<u32> = self
            .visible_popups
            .iter()
            .copied()
            .filter(|id| {
                if let Some(n) = self.active_notifications.get(id) {
                    if let Some(deadline) = n.expires_at {
                        deadline <= now
                    } else {
                        false
                    }
                } else {
                    false
                }
            })
            .collect();

        let mut outcomes = Vec::with_capacity(expired_ids.len());
        for id in expired_ids {
            if let Ok(outcome) = self.close_internal(id, CloseReason::Expired) {
                outcomes.push(outcome);
            }
        }
        outcomes
    }

    /// Returns a slice of the retained notification history.
    #[must_use]
    pub fn history(&self) -> &VecDeque<Notification> {
        &self.history
    }

    /// Returns all currently active notifications.
    #[must_use]
    pub fn active_notifications(&self) -> &HashMap<u32, Notification> {
        &self.active_notifications
    }

    /// Returns the currently visible popup IDs.
    #[must_use]
    pub fn visible_popups(&self) -> &VecDeque<u32> {
        &self.visible_popups
    }

    /// Handles a disconnected D-Bus sender.
    pub fn remove_sender(&mut self, sender: &str) {
        self.rate_limiter.remove_sender(sender);
    }

    /// Sets next_id for wraparound testing.
    #[doc(hidden)]
    pub fn set_next_id_for_test(&mut self, id: u32) {
        self.next_id = id;
    }
}
