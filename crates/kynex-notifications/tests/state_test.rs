//! Comprehensive state machine, queueing, timeout, and authorization tests.
#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::similar_names,
    clippy::assert_is_empty,
    clippy::too_many_lines
)]

use std::time::{Duration, Instant};

use kynex_notifications::{
    CloseReason, NotificationError, NotificationManager, Urgency, DEFAULT_EXPIRE_TIMEOUT_MS,
    MAX_EXPIRE_TIMEOUT_MS, MAX_HISTORY_ENTRIES, MAX_VISIBLE_POPUPS,
};

#[test]
fn test_monotonic_ids_and_non_zero() {
    let mut manager = NotificationManager::new();
    let now = Instant::now();

    for expected_id in 1..=10 {
        let outcome = manager
            .notify(
                ":1.100",
                0,
                "app",
                false,
                "",
                "Summary",
                "Body",
                &[],
                Urgency::Normal,
                5000,
                false,
                false,
                None,
                now,
            )
            .expect("notify");
        assert_eq!(outcome.id, expected_id);
        assert!(!outcome.is_update);
    }
}

#[test]
fn test_replaces_id_same_sender_succeeds() {
    let mut manager = NotificationManager::new();
    let now = Instant::now();

    let initial = manager
        .notify(
            ":1.100",
            0,
            "app",
            false,
            "",
            "Original",
            "Body",
            &[],
            Urgency::Normal,
            5000,
            false,
            false,
            None,
            now,
        )
        .expect("initial notify");
    let original_id = initial.id;

    let update = manager
        .notify(
            ":1.100",
            original_id,
            "app",
            false,
            "",
            "Updated Summary",
            "Updated Body",
            &[],
            Urgency::Normal,
            5000,
            false,
            false,
            None,
            now,
        )
        .expect("update notify");

    assert_eq!(update.id, original_id);
    assert!(update.is_update);
    assert_eq!(update.notification.summary, "Updated Summary");
}

#[test]
fn test_replaces_id_different_sender_rejected() {
    let mut manager = NotificationManager::new();
    let now = Instant::now();

    let initial = manager
        .notify(
            ":1.100",
            0,
            "app1",
            false,
            "",
            "Summary",
            "Body",
            &[],
            Urgency::Normal,
            5000,
            false,
            false,
            None,
            now,
        )
        .expect("notify sender 1");

    // Sender 2 tries to replace sender 1's notification
    let err = manager
        .notify(
            ":1.200",
            initial.id,
            "app2",
            false,
            "",
            "Hijack attempt",
            "Body",
            &[],
            Urgency::Normal,
            5000,
            false,
            false,
            None,
            now,
        )
        .expect_err("should reject replacement by different sender");

    assert_eq!(err, NotificationError::SenderMismatch);
}

#[test]
fn test_close_notification_authorization() {
    let mut manager = NotificationManager::new();
    let now = Instant::now();

    let notif = manager
        .notify(
            ":1.100",
            0,
            "app1",
            false,
            "",
            "Summary",
            "Body",
            &[],
            Urgency::Normal,
            5000,
            false,
            false,
            None,
            now,
        )
        .expect("notify");

    // Unauthorized client attempts CloseNotification
    let err = manager
        .close_notification_by_client(":1.999", notif.id)
        .expect_err("unauthorized close must fail");
    assert_eq!(err, NotificationError::SenderMismatch);

    // Authorized client closes successfully
    let close = manager
        .close_notification_by_client(":1.100", notif.id)
        .expect("authorized close succeeds");
    assert_eq!(close.id, notif.id);
    assert_eq!(close.reason, CloseReason::CloseNotification);
    assert_eq!(close.reason.as_u32(), 3);
}

#[test]
fn test_timeout_semantics() {
    let mut manager = NotificationManager::new();
    let now = Instant::now();

    // -1 = default 5000 ms
    let def = manager
        .notify(
            ":1.101",
            0,
            "app",
            false,
            "",
            "S",
            "B",
            &[],
            Urgency::Normal,
            -1,
            false,
            false,
            None,
            now,
        )
        .expect("notify default");
    assert_eq!(
        def.notification.expires_at,
        Some(now + Duration::from_millis(DEFAULT_EXPIRE_TIMEOUT_MS as u64))
    );

    // 0 = persist until dismissed
    let pers = manager
        .notify(
            ":1.102",
            0,
            "app",
            false,
            "",
            "S",
            "B",
            &[],
            Urgency::Normal,
            0,
            false,
            false,
            None,
            now,
        )
        .expect("notify persist");
    assert_eq!(pers.notification.expires_at, None);

    // Critical urgency = persists until dismissed regardless of timeout
    let crit = manager
        .notify(
            ":1.103",
            0,
            "app",
            false,
            "",
            "S",
            "B",
            &[],
            Urgency::Critical,
            2000,
            false,
            false,
            None,
            now,
        )
        .expect("notify critical");
    assert_eq!(crit.notification.expires_at, None);

    // Huge positive value = capped at MAX_EXPIRE_TIMEOUT_MS (24 h)
    let huge = manager
        .notify(
            ":1.104",
            0,
            "app",
            false,
            "",
            "S",
            "B",
            &[],
            Urgency::Normal,
            i32::MAX,
            false,
            false,
            None,
            now,
        )
        .expect("notify huge");
    assert_eq!(
        huge.notification.expires_at,
        Some(now + Duration::from_millis(MAX_EXPIRE_TIMEOUT_MS as u64))
    );

    // Unrecognized negative value (-99) = fallback to default 5000 ms
    let neg = manager
        .notify(
            ":1.105",
            0,
            "app",
            false,
            "",
            "S",
            "B",
            &[],
            Urgency::Normal,
            -99,
            false,
            false,
            None,
            now,
        )
        .expect("notify neg");
    assert_eq!(
        neg.notification.expires_at,
        Some(now + Duration::from_millis(DEFAULT_EXPIRE_TIMEOUT_MS as u64))
    );
}

#[test]
fn test_do_not_disturb_suppresses_popups_except_critical() {
    let mut manager = NotificationManager::new();
    manager.set_do_not_disturb(true);
    assert!(manager.do_not_disturb());

    let now = Instant::now();

    // Normal urgency: suppressed from popup, added to history
    let normal = manager
        .notify(
            ":1.100",
            0,
            "app",
            false,
            "",
            "Normal",
            "B",
            &[],
            Urgency::Normal,
            5000,
            false,
            false,
            None,
            now,
        )
        .expect("notify normal");
    assert!(!normal.show_popup);
    assert_eq!(manager.visible_popups().len(), 0);
    assert_eq!(manager.history().len(), 1);

    // Critical urgency: bypasses DND and displays popup
    let crit = manager
        .notify(
            ":1.100",
            0,
            "app",
            false,
            "",
            "Critical",
            "B",
            &[],
            Urgency::Critical,
            5000,
            false,
            false,
            None,
            now,
        )
        .expect("notify critical");
    assert!(crit.show_popup);
    assert_eq!(manager.visible_popups().len(), 1);
    assert_eq!(manager.visible_popups()[0], crit.id);
}

#[test]
fn test_visible_stack_bounding_and_queue_promotion() {
    let mut manager = NotificationManager::new();
    let now = Instant::now();

    // Send 5 notifications (fills visible stack)
    let mut ids = Vec::new();
    for i in 1..=MAX_VISIBLE_POPUPS {
        let n = manager
            .notify(
                &format!(":1.{i}"),
                0,
                "app",
                false,
                "",
                &format!("Summary {i}"),
                "Body",
                &[],
                Urgency::Normal,
                5000,
                false,
                false,
                None,
                now,
            )
            .expect("notify");
        assert!(n.show_popup);
        ids.push(n.id);
    }
    assert_eq!(manager.visible_popups().len(), 5);

    // 6th notification: queued in pending queue, show_popup is false
    let sixth = manager
        .notify(
            ":1.106",
            0,
            "app",
            false,
            "",
            "Summary 6",
            "Body",
            &[],
            Urgency::Normal,
            5000,
            false,
            false,
            None,
            now,
        )
        .expect("notify 6th");
    assert!(!sixth.show_popup);
    assert_eq!(manager.visible_popups().len(), 5);

    // Dismiss one visible notification: 6th is promoted into visible stack!
    let dismissed = manager.dismiss_by_user(ids[0]).expect("dismiss first");
    assert_eq!(dismissed.id, ids[0]);
    assert_eq!(dismissed.reason, CloseReason::DismissedByUser);
    assert_eq!(dismissed.reason.as_u32(), 2);
    assert!(dismissed.promoted.is_some());
    assert_eq!(dismissed.promoted.as_ref().map(|p| p.id), Some(sixth.id));

    assert_eq!(manager.visible_popups().len(), 5);
    assert!(manager.visible_popups().contains(&sixth.id));
}

#[test]
fn test_history_bounding_and_eviction() {
    let mut manager = NotificationManager::new();
    let now = Instant::now();

    // Populate history past MAX_HISTORY_ENTRIES (200)
    for i in 1..=210 {
        let n = manager
            .notify(
                &format!(":1.{}", i % 50),
                0,
                "app",
                false,
                "",
                &format!("Summary {i}"),
                "Body",
                &[],
                Urgency::Normal,
                1000,
                false,
                false,
                None,
                now,
            )
            .expect("notify");
        let _ = manager.dismiss_by_user(n.id);
    }

    assert_eq!(manager.history().len(), MAX_HISTORY_ENTRIES);
    // Oldest IDs (1..=10) should have been evicted
    assert!(!manager.history().iter().any(|n| n.id == 1));
    assert!(manager.history().iter().any(|n| n.id == 210));
}

#[test]
fn test_action_invocation_unicast_and_residence() {
    let mut manager = NotificationManager::new();
    let now = Instant::now();

    // Non-resident notification
    let actions = vec![
        "default".to_string(),
        "Open".to_string(),
        "reply".to_string(),
        "Reply".to_string(),
    ];
    let notif = manager
        .notify(
            ":1.123",
            0,
            "chat",
            false,
            "",
            "New Message",
            "Hello",
            &actions,
            Urgency::Normal,
            5000,
            false,
            false, // resident = false
            None,
            now,
        )
        .expect("notify");

    // Invoking action returns original sender and closes non-resident popup
    let (target_sender, close_outcome) = manager
        .invoke_action(notif.id, "reply")
        .expect("invoke action");
    assert_eq!(target_sender, ":1.123");
    assert!(close_outcome.is_some());
    let close = close_outcome.expect("close outcome");
    assert_eq!(close.reason, CloseReason::DismissedByUser);

    // Resident notification
    let resident_notif = manager
        .notify(
            ":1.456",
            0,
            "music",
            false,
            "",
            "Now Playing",
            "Track",
            &actions,
            Urgency::Normal,
            5000,
            false,
            true, // resident = true
            None,
            now,
        )
        .expect("notify resident");

    let (res_sender, res_close) = manager
        .invoke_action(resident_notif.id, "default")
        .expect("invoke resident action");
    assert_eq!(res_sender, ":1.456");
    assert!(res_close.is_none()); // resident popup remains open!
}

#[test]
fn test_single_deadline_expiry_scheduler() {
    let mut manager = NotificationManager::new();
    let start = Instant::now();

    // Popup A: expires in 3000 ms
    let notif_a = manager
        .notify(
            ":1.100",
            0,
            "a",
            false,
            "",
            "A",
            "A",
            &[],
            Urgency::Normal,
            3000,
            false,
            false,
            None,
            start,
        )
        .expect("notif a");

    // Popup B: expires in 1000 ms
    let notif_b = manager
        .notify(
            ":1.101",
            0,
            "b",
            false,
            "",
            "B",
            "B",
            &[],
            Urgency::Normal,
            1000,
            false,
            false,
            None,
            start,
        )
        .expect("notif b");

    // Earliest deadline should be notif_b at start + 1000 ms
    let next_deadline = manager.get_next_deadline().expect("next deadline exists");
    assert_eq!(next_deadline, notif_b.notification.expires_at.unwrap());

    // Advance 500 ms: neither expired
    let expired_500 = manager.expire_up_to(start + Duration::from_millis(500));
    assert!(expired_500.is_empty());

    // Advance 1500 ms: notif_b expires (reason 1)
    let expired_1500 = manager.expire_up_to(start + Duration::from_millis(1500));
    assert_eq!(expired_1500.len(), 1);
    assert_eq!(expired_1500[0].id, notif_b.id);
    assert_eq!(expired_1500[0].reason, CloseReason::Expired);
    assert_eq!(expired_1500[0].reason.as_u32(), 1);

    // New earliest deadline should now be notif_a at start + 3000 ms
    let new_deadline = manager
        .get_next_deadline()
        .expect("deadline after notif_b expired");
    assert_eq!(new_deadline, notif_a.notification.expires_at.unwrap());

    // Advance 3500 ms: notif_a expires
    let expired_3500 = manager.expire_up_to(start + Duration::from_millis(3500));
    assert_eq!(expired_3500.len(), 1);
    assert_eq!(expired_3500[0].id, notif_a.id);

    // No visible popups remaining: deadline is None (idle zero wakeups)
    assert_eq!(manager.get_next_deadline(), None);
}

#[test]
fn test_urgency_conversions() {
    assert_eq!(Urgency::from_byte(0), Urgency::Low);
    assert_eq!(Urgency::from_byte(1), Urgency::Normal);
    assert_eq!(Urgency::from_byte(2), Urgency::Critical);
    assert_eq!(Urgency::from_byte(99), Urgency::Normal);

    assert_eq!(Urgency::Low.as_byte(), 0);
    assert_eq!(Urgency::Normal.as_byte(), 1);
    assert_eq!(Urgency::Critical.as_byte(), 2);
}

#[test]
fn test_state_edge_cases_and_accessors() {
    let mut manager = NotificationManager::default();
    let now = Instant::now();

    // Transient notification: does not record to history on close
    let transient_notif = manager
        .notify(
            ":1.100",
            0,
            "app",
            false,
            "",
            "Transient",
            "B",
            &[],
            Urgency::Normal,
            5000,
            true,
            false,
            None,
            now,
        )
        .expect("transient notify");
    assert!(manager
        .active_notifications()
        .contains_key(&transient_notif.id));
    let _ = manager.dismiss_by_user(transient_notif.id);
    assert_eq!(manager.history().len(), 0); // Not recorded in history!

    // Replaces_id for non-existent notification allocates fresh ID
    let fresh = manager
        .notify(
            ":1.100",
            99999,
            "app",
            false,
            "",
            "Fresh",
            "B",
            &[],
            Urgency::Normal,
            5000,
            false,
            false,
            None,
            now,
        )
        .expect("fresh notify");
    assert_ne!(fresh.id, 99999);

    // Close non-existent notification returns NotFound
    assert_eq!(
        manager.close_notification_by_client(":1.100", 88888),
        Err(NotificationError::NotFound(88888))
    );
    assert_eq!(
        manager.dismiss_by_user(88888),
        Err(NotificationError::NotFound(88888))
    );

    // Invoke action on non-existent notification returns NotFound
    assert_eq!(
        manager.invoke_action(88888, "default"),
        Err(NotificationError::NotFound(88888))
    );

    // Invoke non-existent action key returns InvalidArgs
    let active = manager
        .notify(
            ":1.100",
            0,
            "app",
            false,
            "",
            "S",
            "B",
            &["open".to_string(), "Open".to_string()],
            Urgency::Normal,
            5000,
            false,
            false,
            None,
            now,
        )
        .expect("notify");
    assert_eq!(
        manager.invoke_action(active.id, "invalid_key"),
        Err(NotificationError::InvalidArgs("Action key not found"))
    );

    // Remove sender cleans up rate limiter
    manager.remove_sender(":1.100");
}

#[test]
fn test_id_wraparound_and_collision_skipping() {
    let mut manager = NotificationManager::new();
    let now = Instant::now();

    // Set next_id to u32::MAX
    manager.set_next_id_for_test(u32::MAX);

    let res_max = manager
        .notify(
            ":1.100",
            0,
            "app",
            false,
            "",
            "Max ID",
            "B",
            &[],
            Urgency::Normal,
            5000,
            false,
            false,
            None,
            now,
        )
        .expect("notify max");
    assert_eq!(res_max.id, u32::MAX);

    // Occupy ID 1 to force collision skipping
    let _ = manager
        .notify(
            ":1.100",
            0,
            "app",
            false,
            "",
            "First after wrap",
            "B",
            &[],
            Urgency::Normal,
            5000,
            false,
            false,
            None,
            now,
        )
        .expect("notify 1");

    // Manually force next_id to wrap to 1 when 1 is already taken
    manager.set_next_id_for_test(u32::MAX);
    let res_skip = manager
        .notify(
            ":1.100",
            0,
            "app",
            false,
            "",
            "Skip Collision",
            "B",
            &[],
            Urgency::Normal,
            5000,
            false,
            false,
            None,
            now,
        )
        .expect("notify skip");
    assert_eq!(res_skip.id, 2);
}
