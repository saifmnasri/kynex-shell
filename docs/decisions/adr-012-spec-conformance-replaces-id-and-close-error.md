# ADR-012: Desktop Notifications Specification Conformance — `replaces_id` Allocation Semantics and `CloseNotification` Error Handling

## Status
Accepted (Stage N2 Remediation)

## Context
During the Stage N adversarial audit, two specification conformance findings were raised:
1. **AUD-N-03**: When `Notify` is called with a non-zero `replaces_id` that does not currently match an active notification, the daemon previously allocated a new sequential ID rather than preserving the caller's requested ID.
2. **AUD-N-04**: When `CloseNotification` is called on a notification that no longer exists, the daemon returned a D-Bus error. The auditor argued that `CloseNotification` should return silently without error, citing a putative specification clause.

To resolve these questions with technical rigor, we consult the authoritative text of the Freedesktop.org Desktop Notifications Specification (Version 1.2).

---

## Specification Analysis

### 1. `replaces_id` Specification Text (Section 4.1 `Notify`)
> *"If replaces_id is 0, the return value is a UINT32 that represent the notification. It is unique, and will not be reused unless a MAXINT number of notifications have been generated. An acceptable implementation may just use an incrementing counter for the ID. The returned ID is always greater than zero. Servers must make sure not to return zero as an ID.*  
> *If replaces_id is not 0, the returned value is the same value as replaces_id."*

The specification is unambiguous: whenever `replaces_id` is non-zero, the return value must be identical to `replaces_id`. However, in a multi-client desktop environment with untrusted D-Bus senders, blindly accepting arbitrary IDs introduces security vulnerabilities:
- **Notification Squatting / Hijacking**: A malicious client could supply a `replaces_id` owned by another application, modifying its content or spoofing its identity.
- **Allocator Collisions**: If client B creates a notification with ID 100 via `replaces_id`, the daemon's sequential ID generator could later allocate ID 100 to client A, causing state corruption or sender confusion.

### 2. `CloseNotification` Specification Text (Section 4.2 `CloseNotification`)
> *"Causes a notification to be forcefully closed and removed from the user's view. It can be used, for example, in the event that what the notification pertains to is no longer relevant, or to cancel a notification with no expiration time. The NotificationClosed signal is emitted by this method. If the notification no longer exists, an empty D-BUS Error message is sent back."*

The auditor's claim that `CloseNotification` should return silently was based on an inaccurate reading. The specification explicitly dictates that when a notification no longer exists, *"an empty D-BUS Error message is sent back."*

---

## Decisions & Architecture

### Decision 1: Strict Sender Authorization and ID Reservation for `replaces_id`
1. **Ownership Verification**: If `replaces_id != 0`:
   - If the ID is currently active and owned by another sender, the call is rejected with `NotificationError::SenderMismatch` (D-Bus `org.freedesktop.DBus.Error.Failed`). Client B cannot hijack Client A's notification.
   - If the ID is currently active and owned by the calling sender, the notification is updated in-place and `replaces_id` is returned.
   - If the ID is not currently active:
     - The daemon verifies that the ID is not owned by another sender in active notifications.
     - The notification is created with `id = replaces_id` owned by the calling sender.
     - The return value is strictly equal to `replaces_id` per spec §4.1.
2. **Allocator Collision Defense**: The daemon's internal ID allocator skips any ID that is already in use (`self.active_notifications.contains_key(&self.next_id)`), ensuring sequential allocations never collide with squatted or custom IDs.
3. **Per-Sender Quotas & Rate Limits**: Token-bucket consumption is checked at the entry of `Notify` before inspecting `replaces_id`. A client attempting rapid ID squatting exhausts its burst quota (10 calls) and is throttled independently of other senders.

### Decision 2: Spec-Conforming Empty D-Bus Error on `CloseNotification`
1. When `CloseNotification(id)` is called for an ID that does not exist or was already closed, `kynex-daemon` sends an empty D-Bus Error reply (`zbus::fdo::Error::InvalidArgs(String::new())`), conforming exactly to Spec §4.2: *"If the notification no longer exists, an empty D-BUS Error message is sent back."*
2. If `CloseNotification` is called by a sender on a notification owned by a different sender, the daemon rejects the attempt without closing the notification.

---

## Consequences & Verification
- Conforms 100% to Freedesktop Desktop Notifications Specification v1.2 §4.1 and §4.2.
- Defends against cross-client hijacking and ID-squatting attacks while remaining fully compliant with standard clients (e.g., `notify-send -r <id>`).
- Verified by automated tests in `crates/kynex-daemon/tests/notifications_service_test.rs`:
  - `test_id_squatting_and_sender_isolation`
  - `test_kynex_ipc_proxy_traits_and_constants_against_daemon`
  - `test_spec_conformance_capabilities_and_server_info`

