//! StatusNotifierItem and StatusNotifierHost registry with sender authentication,
//! resource bounding, and lifecycle tracking.

use std::collections::{HashMap, HashSet};

use crate::error::TrayError;
use crate::limits::{MAX_ITEMS_PER_SENDER, MAX_TOTAL_ITEMS};
use crate::pixmap::ValidatedPixmap;
use crate::status::ItemStatus;

/// Unique key identifying a registered tray item: `(unique_bus_name, object_path)`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ItemKey {
    /// Caller's unique connection bus name (e.g. `:1.42`).
    pub bus_name: String,
    /// D-Bus object path of the item (e.g. `/StatusNotifierItem`).
    pub object_path: String,
}

impl ItemKey {
    /// Creates a new item key.
    #[must_use]
    pub fn new(bus_name: impl Into<String>, object_path: impl Into<String>) -> Self {
        Self {
            bus_name: bus_name.into(),
            object_path: object_path.into(),
        }
    }

    /// Formats the key into a canonical string ID: `"<bus_name><object_path>"`.
    #[must_use]
    pub fn as_id_string(&self) -> String {
        format!("{}{}", self.bus_name, self.object_path)
    }
}

/// Fully sanitized and validated state of an active StatusNotifierItem.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ItemState {
    /// Item key identifying sender and object path.
    pub key: ItemKey,
    /// Sanitized item identifier string (`Id`).
    pub item_id: String,
    /// Sanitized title headline (`Title`).
    pub title: String,
    /// Operational status (`Passive`, `Active`, `NeedsAttention`).
    pub status: ItemStatus,
    /// Sanitized Freedesktop icon theme name (`IconName`).
    pub icon_name: Option<String>,
    /// Optional extra icon theme search path (`IconThemePath`).
    pub icon_theme_path: Option<String>,
    /// Validated icon pixmap converted to RGBA.
    pub icon_pixmap: Option<ValidatedPixmap>,
    /// Sanitized tooltip headline.
    pub tooltip_title: String,
    /// Sanitized tooltip body description.
    pub tooltip_text: String,
    /// Whether item is a menu-only item.
    pub is_menu: bool,
    /// Window ID if associated with a window.
    pub window_id: i32,
    /// Sanitized attention icon theme name (`AttentionIconName`).
    pub attention_icon_name: Option<String>,
    /// Validated attention icon pixmap converted to RGBA.
    pub attention_pixmap: Option<ValidatedPixmap>,
    /// Optional object path to dbusmenu service (`Menu`).
    pub menu_path: Option<String>,
}

/// Pure-Rust registry managing tray items and visualization hosts.
#[derive(Debug, Default)]
pub struct TrayRegistry {
    /// Tracked items keyed by `(bus_name, object_path)`.
    items: HashMap<ItemKey, ItemState>,
    /// Per-sender item counts.
    sender_counts: HashMap<String, usize>,
    /// Registered visualization hosts (unique bus names).
    hosts: HashSet<String>,
}

impl TrayRegistry {
    /// Creates an empty registry.
    #[must_use]
    pub fn new() -> Self {
        Self {
            items: HashMap::new(),
            sender_counts: HashMap::new(),
            hosts: HashSet::new(),
        }
    }

    /// Returns the number of currently tracked items.
    #[must_use]
    pub fn item_count(&self) -> usize {
        self.items.len()
    }

    /// Returns the number of registered hosts.
    #[must_use]
    pub fn host_count(&self) -> usize {
        self.hosts.len()
    }

    /// Returns whether at least one visualization host is registered.
    #[must_use]
    pub fn is_host_registered(&self) -> bool {
        !self.hosts.is_empty()
    }

    /// Registers a visualization host identified by its unique bus name.
    ///
    /// Returns `true` if this is the first host registered (triggering `HostRegistered` signal),
    /// or `false` if other hosts were already active.
    pub fn register_host(&mut self, host_bus_name: &str) -> bool {
        let was_empty = self.hosts.is_empty();
        self.hosts.insert(host_bus_name.to_string());
        was_empty
    }

    /// Unregisters a visualization host when its bus name vanishes.
    ///
    /// Returns `true` if the last host vanished (triggering `HostUnregistered` signal),
    /// or `false` otherwise.
    pub fn unregister_host(&mut self, host_bus_name: &str) -> bool {
        if self.hosts.remove(host_bus_name) {
            self.hosts.is_empty()
        } else {
            false
        }
    }

    /// Validates that a client's registration argument resolves to the sender's own connection bus name.
    ///
    /// # Sender Verification Invariant
    /// - If `service_or_path` starts with `/`, it represents an object path on the sender's connection.
    ///   Object path must be non-empty, start with `/`, and contain no whitespace or NUL bytes.
    /// - If `service_or_path` does not start with `/`:
    ///   - If it contains a `/` (e.g. `service/path`), service and path are split.
    ///   - Otherwise, path defaults to `/StatusNotifierItem`.
    ///   - The service name must resolve to `sender_bus_name`:
    ///     - If it starts with `:`, it must exactly match `sender_bus_name`.
    ///     - If it is a well-known name, `owner_lookup(well_known_name)` must return `Some(sender_bus_name)`.
    ///
    /// # Errors
    ///
    /// Returns [`TrayError::SenderMismatch`] if the service name does not resolve to the caller.
    /// Returns [`TrayError::InvalidObjectPath`] if the object path is malformed.
    pub fn resolve_registration<F>(
        sender_bus_name: &str,
        service_or_path: &str,
        owner_lookup: F,
    ) -> Result<ItemKey, TrayError>
    where
        F: FnOnce(&str) -> Option<String>,
    {
        let trimmed = service_or_path.trim();
        if trimmed.is_empty() {
            return Err(TrayError::InvalidObjectPath(
                "Service or path argument cannot be empty".to_string(),
            ));
        }

        let (target_service, object_path) = if trimmed.starts_with('/') {
            // Path-only form: sender is caller's unique bus name
            (sender_bus_name.to_string(), trimmed.to_string())
        } else if let Some(idx) = trimmed.find('/') {
            let (svc, path) = trimmed.split_at(idx);
            (svc.to_string(), path.to_string())
        } else {
            (trimmed.to_string(), "/StatusNotifierItem".to_string())
        };

        // Validate object path format
        if !object_path.starts_with('/') || object_path.contains(' ') || object_path.contains('\0')
        {
            return Err(TrayError::InvalidObjectPath(object_path));
        }

        // Verify sender ownership
        if target_service.starts_with(':') {
            if target_service != sender_bus_name {
                return Err(TrayError::SenderMismatch);
            }
        } else {
            // Well-known name lookup
            let owner = owner_lookup(&target_service);
            if owner.as_deref() != Some(sender_bus_name) {
                return Err(TrayError::SenderMismatch);
            }
        }

        Ok(ItemKey::new(sender_bus_name, object_path))
    }

    /// Verifies that registering an item under `key` does not violate capacity limits.
    ///
    /// # Errors
    ///
    /// Returns [`TrayError::TotalLimitExceeded`] if total items >= [`MAX_TOTAL_ITEMS`].
    /// Returns [`TrayError::SenderLimitExceeded`] if sender items >= [`MAX_ITEMS_PER_SENDER`].
    pub fn check_capacity(&self, key: &ItemKey) -> Result<(), TrayError> {
        // If updating an already existing item, capacity is not consumed
        if self.items.contains_key(key) {
            return Ok(());
        }

        if self.items.len() >= MAX_TOTAL_ITEMS {
            return Err(TrayError::TotalLimitExceeded(MAX_TOTAL_ITEMS));
        }

        let current_sender_count = self.sender_counts.get(&key.bus_name).copied().unwrap_or(0);
        if current_sender_count >= MAX_ITEMS_PER_SENDER {
            return Err(TrayError::SenderLimitExceeded(MAX_ITEMS_PER_SENDER));
        }

        Ok(())
    }

    /// Inserts or updates an item in the registry.
    ///
    /// # Errors
    ///
    /// Returns an error if capacity limits are exceeded.
    pub fn upsert(&mut self, state: ItemState) -> Result<(), TrayError> {
        self.check_capacity(&state.key)?;

        if !self.items.contains_key(&state.key) {
            *self
                .sender_counts
                .entry(state.key.bus_name.clone())
                .or_insert(0) += 1;
        }

        self.items.insert(state.key.clone(), state);
        Ok(())
    }

    /// Unregisters an item by its key.
    ///
    /// Returns the removed [`ItemState`], or `None` if not found.
    pub fn unregister(&mut self, key: &ItemKey) -> Option<ItemState> {
        if let Some(removed) = self.items.remove(key) {
            if let Some(count) = self.sender_counts.get_mut(&key.bus_name) {
                if *count > 1 {
                    *count -= 1;
                } else {
                    self.sender_counts.remove(&key.bus_name);
                }
            }
            Some(removed)
        } else {
            None
        }
    }

    /// Removes all items registered by a sender (e.g. when `NameOwnerChanged` indicates disconnect).
    ///
    /// Returns a list of all removed items.
    pub fn remove_sender(&mut self, sender_bus_name: &str) -> Vec<ItemState> {
        self.sender_counts.remove(sender_bus_name);

        let keys_to_remove: Vec<ItemKey> = self
            .items
            .keys()
            .filter(|k| k.bus_name == sender_bus_name)
            .cloned()
            .collect();

        let mut removed = Vec::with_capacity(keys_to_remove.len());
        for key in keys_to_remove {
            if let Some(item) = self.items.remove(&key) {
                removed.push(item);
            }
        }
        removed
    }

    /// Retrieves an item by its key.
    #[must_use]
    pub fn get(&self, key: &ItemKey) -> Option<&ItemState> {
        self.items.get(key)
    }

    /// Retrieves a mutable reference to an item by its key.
    pub fn get_mut(&mut self, key: &ItemKey) -> Option<&mut ItemState> {
        self.items.get_mut(key)
    }

    /// Returns an iterator over all currently tracked items.
    pub fn iter(&self) -> impl Iterator<Item = &ItemState> {
        self.items.values()
    }

    /// Returns the list of registered item identifiers formatted for the watcher property
    /// `RegisteredStatusNotifierItems`.
    #[must_use]
    pub fn registered_item_ids(&self) -> Vec<String> {
        let mut ids: Vec<String> = self.items.keys().map(ItemKey::as_id_string).collect();
        ids.sort();
        ids
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sender_resolution() {
        let caller = ":1.100";

        // Path only -> resolves to caller
        let key = TrayRegistry::resolve_registration(caller, "/StatusNotifierItem", |_| None);
        assert_eq!(key, Ok(ItemKey::new(":1.100", "/StatusNotifierItem")));

        // Matching unique name
        let key2 = TrayRegistry::resolve_registration(caller, ":1.100", |_| None);
        assert_eq!(key2, Ok(ItemKey::new(":1.100", "/StatusNotifierItem")));

        // Mismatched unique name -> error
        let key_spoof = TrayRegistry::resolve_registration(caller, ":1.200", |_| None);
        assert_eq!(key_spoof, Err(TrayError::SenderMismatch));

        // Well-known name owned by caller -> success
        let key_wk =
            TrayRegistry::resolve_registration(caller, "org.kde.StatusNotifierItem-1-1", |name| {
                if name == "org.kde.StatusNotifierItem-1-1" {
                    Some(":1.100".to_string())
                } else {
                    None
                }
            });
        assert_eq!(key_wk, Ok(ItemKey::new(":1.100", "/StatusNotifierItem")));

        // Well-known name owned by someone else -> rejected
        let key_wk_spoof =
            TrayRegistry::resolve_registration(caller, "org.kde.StatusNotifierItem-1-1", |_| {
                Some(":1.999".to_string())
            });
        assert_eq!(key_wk_spoof, Err(TrayError::SenderMismatch));
    }

    #[test]
    fn test_capacity_limits() {
        let mut reg = TrayRegistry::new();
        let caller = ":1.10";

        // 4 items by same sender
        for i in 0..4 {
            let key = ItemKey::new(caller, format!("/item/{i}"));
            let item = ItemState {
                key: key.clone(),
                item_id: format!("id-{i}"),
                title: "Title".to_string(),
                status: ItemStatus::Active,
                icon_name: None,
                icon_theme_path: None,
                icon_pixmap: None,
                tooltip_title: String::new(),
                tooltip_text: String::new(),
                is_menu: false,
                window_id: 0,
                attention_icon_name: None,
                attention_pixmap: None,
                menu_path: None,
            };
            assert!(reg.upsert(item).is_ok());
        }

        // 5th item by same sender rejected
        let key5 = ItemKey::new(caller, "/item/4");
        let item5 = ItemState {
            key: key5,
            item_id: "id-4".to_string(),
            title: "Title".to_string(),
            status: ItemStatus::Active,
            icon_name: None,
            icon_theme_path: None,
            icon_pixmap: None,
            tooltip_title: String::new(),
            tooltip_text: String::new(),
            is_menu: false,
            window_id: 0,
            attention_icon_name: None,
            attention_pixmap: None,
            menu_path: None,
        };
        assert_eq!(
            reg.upsert(item5),
            Err(TrayError::SenderLimitExceeded(MAX_ITEMS_PER_SENDER))
        );

        // Unregister one item allows a new one
        assert!(reg.unregister(&ItemKey::new(caller, "/item/0")).is_some());
        let item_replacement = ItemState {
            key: ItemKey::new(caller, "/item/replacement"),
            item_id: "id-rep".to_string(),
            title: "Title".to_string(),
            status: ItemStatus::Active,
            icon_name: None,
            icon_theme_path: None,
            icon_pixmap: None,
            tooltip_title: String::new(),
            tooltip_text: String::new(),
            is_menu: false,
            window_id: 0,
            attention_icon_name: None,
            attention_pixmap: None,
            menu_path: None,
        };
        assert!(reg.upsert(item_replacement).is_ok());
    }

    #[test]
    fn test_total_capacity_limit_across_senders() {
        let mut reg = TrayRegistry::new();
        // 16 distinct senders register 4 items each = 64 items total
        for s in 0..16 {
            let sender = format!(":1.{s}");
            for i in 0..4 {
                let key = ItemKey::new(&sender, format!("/item/{i}"));
                let item = ItemState {
                    key,
                    item_id: format!("id-{s}-{i}"),
                    title: "Title".to_string(),
                    status: ItemStatus::Active,
                    icon_name: None,
                    icon_theme_path: None,
                    icon_pixmap: None,
                    tooltip_title: String::new(),
                    tooltip_text: String::new(),
                    is_menu: false,
                    window_id: 0,
                    attention_icon_name: None,
                    attention_pixmap: None,
                    menu_path: None,
                };
                assert!(reg.upsert(item).is_ok());
            }
        }
        assert_eq!(reg.item_count(), MAX_TOTAL_ITEMS);

        // 17th sender tries to register 1 item -> rejected with TotalLimitExceeded
        let key65 = ItemKey::new(":1.16", "/item/0");
        let item65 = ItemState {
            key: key65,
            item_id: "id-65".to_string(),
            title: "Title".to_string(),
            status: ItemStatus::Active,
            icon_name: None,
            icon_theme_path: None,
            icon_pixmap: None,
            tooltip_title: String::new(),
            tooltip_text: String::new(),
            is_menu: false,
            window_id: 0,
            attention_icon_name: None,
            attention_pixmap: None,
            menu_path: None,
        };
        assert_eq!(
            reg.upsert(item65),
            Err(TrayError::TotalLimitExceeded(MAX_TOTAL_ITEMS))
        );
    }

    #[test]
    fn test_host_registration_lifecycle() {
        let mut reg = TrayRegistry::new();
        assert!(!reg.is_host_registered());

        assert!(reg.register_host(":1.100")); // first host
        assert!(reg.is_host_registered());
        assert!(!reg.register_host(":1.101")); // second host

        assert!(!reg.unregister_host(":1.100")); // still 1 host left
        assert!(reg.is_host_registered());
        assert!(reg.unregister_host(":1.101")); // last host unregistered
        assert!(!reg.is_host_registered());
    }

    #[test]
    fn test_remove_sender() {
        let mut reg = TrayRegistry::new();
        let caller = ":1.50";
        for i in 0..3 {
            let key = ItemKey::new(caller, format!("/item/{i}"));
            let item = ItemState {
                key,
                item_id: format!("id-{i}"),
                title: "Title".to_string(),
                status: ItemStatus::Active,
                icon_name: None,
                icon_theme_path: None,
                icon_pixmap: None,
                tooltip_title: String::new(),
                tooltip_text: String::new(),
                is_menu: false,
                window_id: 0,
                attention_icon_name: None,
                attention_pixmap: None,
                menu_path: None,
            };
            assert!(reg.upsert(item).is_ok());
        }
        assert_eq!(reg.item_count(), 3);

        let removed = reg.remove_sender(caller);
        assert_eq!(removed.len(), 3);
        assert_eq!(reg.item_count(), 0);
    }

    #[test]
    fn test_unregister_get_and_iter() {
        let mut reg = TrayRegistry::new();
        let key = ItemKey::new(":1.10", "/test");
        let item = ItemState {
            key: key.clone(),
            item_id: "test".to_string(),
            title: "Original".to_string(),
            status: ItemStatus::Active,
            icon_name: None,
            icon_theme_path: None,
            icon_pixmap: None,
            tooltip_title: String::new(),
            tooltip_text: String::new(),
            is_menu: false,
            window_id: 0,
            attention_icon_name: None,
            attention_pixmap: None,
            menu_path: None,
        };

        assert!(reg.upsert(item).is_ok());
        assert_eq!(reg.item_count(), 1);
        assert_eq!(reg.get(&key).map(|i| i.title.as_str()), Some("Original"));

        if let Some(mut_item) = reg.get_mut(&key) {
            mut_item.title = "Modified".to_string();
        }
        assert_eq!(reg.get(&key).map(|i| i.title.as_str()), Some("Modified"));

        let items_iter: Vec<_> = reg.iter().collect();
        assert_eq!(items_iter.len(), 1);

        assert!(reg.register_host(":1.host1"));
        assert_eq!(reg.host_count(), 1);

        assert!(reg.unregister(&key).is_some());
        assert_eq!(reg.item_count(), 0);
        assert!(reg.unregister(&key).is_none());
    }
}
