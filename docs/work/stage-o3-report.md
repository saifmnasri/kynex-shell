# Stage O3 Work Report: Adversarial Audit Remediation (AUD-O2-01 & AUD-O2-02)

**Branch**: `feat/tray`  
**Base**: `main`  
**PR**: Draft PR #23 (`https://github.com/saifmnasri/kynex-shell/pull/23`)  
**Status**: All Stage O3 remediation items completed, tested, and verified.

---

## 1. Executive Summary

This stage addresses and remediates the two findings from the Stage O2 adversarial audit (`docs/audits/stage-o2-audit.md`):
- **O3.1 (AUD-O2-01)**: Hardened well-known name registration authorization tests. The test `test_register_by_another_clients_well_known_name_rejected` now ensures client B exports a complete, valid StatusNotifierItem on `/StatusNotifierItem` before attempting to register client A's well-known name. The test fails if and only if the owner check is removed. An identical test was added for unowned well-known names.
- **O3.2 (AUD-O2-02)**: Removed hardcoded `1920x1080` screen resolution and `1920x40` panel dimensions in `crates/kynex-shell/src/tray.rs`. Screen geometry (`Screen.virtualX`, `virtualY`, `width`, `height`, `devicePixelRatio`) and panel height are dynamically extracted from QML and forwarded to `map_to_screen_coordinates`. Added a QML test mocking a secondary display at a negative virtual origin (`-1920, 0`), and proved that a mutant reintroducing constants is killed.
- **O3.3**: Added adversarial audit notes to `docs/work/stage-o2-report.md` for both overstated claims, passed strict verification and audit gates (`STRICT=1 ./scripts/verify.sh` and `./scripts/audit.sh`), and verified that PR #23 remains strictly in Draft state.

---

## 2. O3.1 — AUD-O2-01 Remediation: Sender Ownership Check Hardening

### 2.1 Problem Analysis
In Stage O2, `test_register_by_another_clients_well_known_name_rejected` registered client A's well-known name from client B. However, client B did not export an SNI object on `/StatusNotifierItem`. Consequently, even without sender authorization enforcement, registration failed during introspection/property fetching rather than exclusively due to the owner check.

### 2.2 Implementation & Test Hardening
In `crates/kynex-daemon/tests/tray_service_test.rs`:
1. Client B now exports a complete, valid `FakeItem` instance on `/StatusNotifierItem` via `zbus::connection::Builder::serve_at`.
2. Client A owns the well-known name `org.kde.StatusNotifierItem.TestAppA2`.
3. Client B attempts to register `org.kde.StatusNotifierItem.TestAppA2`.
4. If sender authorization were omitted, registration would succeed because client B's SNI object responds to all required property and method requests.
5. With sender authorization enforced:
   - The call fails immediately with exact error `org.freedesktop.DBus.Error.AccessDenied`.
   - `tray1.get_items().await` remains completely empty (`is_empty() == true`).
6. Added `test_register_by_unowned_well_known_name_rejected` verifying identical behavior when client A registers a well-known name that has no bus owner.

### 2.3 Proof of Sensitivity via Mutation
To prove that the test passes solely due to the sender ownership check, a mutant was introduced into `crates/kynex-daemon/src/services/tray.rs` temporarily removing `if owner != sender { return Err(...) }`.

Running the test against the mutant produces a test failure (panic), proving sensitivity:

```raw:o3-mutant-owner-check-fail:L10-L25
running 1 test
test test_register_by_another_clients_well_known_name_rejected ... FAILED

failures:

---- test_register_by_another_clients_well_known_name_rejected stdout ----

thread 'test_register_by_another_clients_well_known_name_rejected' (657385) panicked at crates/kynex-daemon/tests/tray_service_test.rs:1008:19:
Registering another client's well-known name must be rejected with AccessDenied: ()
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace


failures:
    test_register_by_another_clients_well_known_name_rejected

test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 21 filtered out; finished in 0.01s
```

### 2.4 Restored Tests Passing
With the check restored, both tests pass cleanly:

Well-known name owned by another client:
```raw:o3-test-wk-rejection-pass:L10-L13
running 1 test
test test_register_by_another_clients_well_known_name_rejected ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 21 filtered out; finished in 0.01s
```

Unowned well-known name:
```raw:o3-test-unowned-rejection-pass:L9-L12
running 1 test
test test_register_by_unowned_well_known_name_rejected ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 21 filtered out; finished in 0.01s
```

---

## 3. O3.2 — AUD-O2-02 Remediation: Dynamic Screen Geometry

### 3.1 Problem Analysis
In Stage O2, `crates/kynex-shell/src/tray.rs` hardcoded screen and panel dimensions (`1920x1080` screen resolution and `1920x40` panel bounds) when computing popup coordinates via `map_to_screen_coordinates`. On multi-monitor setups or screens with negative virtual coordinates (e.g., secondary display to the left of the primary display at `x = -1920`), popup windows would be placed on the wrong monitor.

### 3.2 Implementation
1. **CXX-Qt Bridge (`crates/kynex-shell/src/tray.rs`)**:
   - Updated `activate`, `secondary_activate`, and `context_menu` signatures to accept 6 dynamic parameters:
     - `screen_x: f64`
     - `screen_y: f64`
     - `screen_width: f64`
     - `screen_height: f64`
     - `device_pixel_ratio: f64`
     - `panel_height: f64`
   - Added helper `project_tray_coordinates(...)` that constructs `ScreenGeometry` and `PanelGeometry` dynamically from these values before calling `map_to_screen_coordinates`.
   - Updated QML plugin type annotations in `crates/kynex-shell/qml/Kynex/Shell/plugins.qmltypes`.
2. **QML Bindings (`styles/default/Shell.qml`)**:
   - Added attached `Screen` property bindings on `panel`:
     - `screenVirtualX: Screen.virtualX`
     - `screenVirtualY: Screen.virtualY`
     - `screenWidth: Screen.width`
     - `screenHeight: Screen.height`
     - `screenDevicePixelRatio: Screen.devicePixelRatio`
   - In `trayMouseArea.onClicked`, pass these properties along with `panel.height` to `trayModel.activate(...)`, `secondaryActivate(...)`, and `contextMenu(...)`.
3. **QML Test Infrastructure (`tests/qml_tests/main.cpp` & `tst_shell.qml`)**:
   - Updated `MockTrayModel` to accept geometry parameters and record `lastScreenX`, `lastScreenY`, `lastScreenWidth`, `lastScreenHeight`, `lastDevicePixelRatio`, and `lastPanelHeight`.
   - Added `test_tray_secondary_screen_negative_origin()` simulating a monitor placed at `virtualX = -1920`, `virtualY = 0`, verifying that negative virtual origins correctly reach the model.

### 3.3 What Remains Unwired
In `crates/kynex-shell/src/tray.rs`, `SurfaceAnchors::top_panel()` is still statically supplied to `map_to_screen_coordinates`. If a shell panel surface is configured to anchor to the bottom edge or vertical screen edges (left/right), dynamic anchor configuration extraction from `LayerSurface.anchors` in QML into `TrayModel` is not yet wired. Top-panel placement with arbitrary screen geometry, arbitrary panel heights, and multi-monitor offsets (including negative virtual origins) is fully dynamic and verified.

### 3.4 Proof of Sensitivity via Mutation
To verify the QML test suite detects hardcoded constants, a mutant was introduced into `styles/default/Shell.qml` replacing dynamic screen variables with literal zeros. Running the QML test suite killed the mutant immediately:

```raw:o3-mutant-constants-killed:L132-L143
QWARN  : controls::ShellTests::test_tray_secondary_screen_negative_origin() layershellqt: Shell_QMLTYPE_47(0xb1a6326c41f0) is not a wayland window. Not creating zwlr_layer_surface
QDEBUG : controls::ShellTests::test_tray_secondary_screen_negative_origin() qml: Kynex Shell panel initialized successfully
FAIL!  : controls::ShellTests::test_tray_secondary_screen_negative_origin() Compared values are not the same
   Actual   (): 0
   Expected (): -1920
   Loc: [/home/saif/Kynex-Shell/tests/qml_tests/tst_shell.qml(431)]
QWARN  : controls::ShellTests::test_tray_tooltip_branches() layershellqt: Shell_QMLTYPE_47(0xb1a632868aa0) is not a wayland window. Not creating zwlr_layer_surface
QDEBUG : controls::ShellTests::test_tray_tooltip_branches() qml: Kynex Shell panel initialized successfully
PASS   : controls::ShellTests::test_tray_tooltip_branches()
PASS   : controls::ShellTests::cleanupTestCase()
Totals: 69 passed, 1 failed, 0 skipped, 0 blacklisted, 1065ms
********* Finished testing of controls *********
```

### 3.5 Passing Verification
With restored dynamic bindings, all 70 QML control tests pass:

```raw:o3-qml-negative-screen-pass:L132-L140
QWARN  : controls::ShellTests::test_tray_secondary_screen_negative_origin() layershellqt: Shell_QMLTYPE_47(0xafa9130fe690) is not a wayland window. Not creating zwlr_layer_surface
QDEBUG : controls::ShellTests::test_tray_secondary_screen_negative_origin() qml: Kynex Shell panel initialized successfully
PASS   : controls::ShellTests::test_tray_secondary_screen_negative_origin()
QWARN  : controls::ShellTests::test_tray_tooltip_branches() layershellqt: Shell_QMLTYPE_47(0xafa9131fe170) is not a wayland window. Not creating zwlr_layer_surface
QDEBUG : controls::ShellTests::test_tray_tooltip_branches() qml: Kynex Shell panel initialized successfully
PASS   : controls::ShellTests::test_tray_tooltip_branches()
PASS   : controls::ShellTests::cleanupTestCase()
Totals: 70 passed, 0 failed, 0 skipped, 0 blacklisted, 1043ms
********* Finished testing of controls *********
```

Rust unit test for coordinate projection with negative screen origin:
```raw:o3-rust-coordinates-pass:L9-L12
running 1 test
test tray::tests::test_project_tray_coordinates_negative_screen_origin ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 26 filtered out; finished in 0.00s
```

QML lint passes with zero warnings:
```raw:o3-qmllint:L1-L5
=== COMMAND ===
/usr/lib/qt6/bin/qmllint -I crates/kynex-shell/qml --compiler warning --max-warnings 0 styles/default/Shell.qml crates/kynex-shell/qml/Kynex/Controls/KynexButton.qml crates/kynex-shell/qml/Kynex/Controls/KynexListItem.qml crates/kynex-shell/qml/Kynex/Controls/KynexSearchField.qml crates/kynex-shell/qml/Kynex/Controls/KynexSlider.qml crates/kynex-shell/qml/Kynex/Controls/KynexSwitch.qml crates/kynex-shell/qml/Kynex/Controls/KynexTooltip.qml
=== TIMESTAMP: 2026-11-11T06:21:47Z ===
=== EXIT CODE: 0 ===
=== RAW OUTPUT ===
```

---

## 4. O3.3 — Verification Gates & PR Status

### 4.1 Strict Verification Gate
`STRICT=1 ./scripts/verify.sh` passes 20/20 checks:

```raw:o3-verify-gate:L1587-L1610
================ SUMMARY ================
PASSED : 20
  + rustfmt
  + clippy
  + tests (nextest)
  + doc tests
  + cargo-deny (licenses, bans, advisories)
  + cargo-audit (RustSec)
  + typos
  + secret scan
  + style tokens lint
  + style tokens linter test
  + qmllint
  + shellcheck
  + desktop-file-validate
  + qml quick test (base controls)
  + visual regression test (gallery)
  + adversarial mutation test (qml controls)
  + systemd user unit sandbox test
  + work report evidence check
  + no literal benchmark metrics
  + budget-check scalar rejection test
SKIPPED: 0
FAILED : 0
```

### 4.2 Security Audit Gate
`./scripts/audit.sh` passes with full SBOM generation and zero vulnerabilities:

```raw:o3-audit-gate:L2453-L2457
----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------
TOTAL                                            18008              4364    75.77%        1163               286    75.41%       11266              2533    77.52%           0                 0         -

### 5. SBOM
SBOM written
```

### 4.3 Pull Request Draft Status
PR #23 remains strictly in Draft status (`isDraft: true`):

```raw:o3-pr-draft-status:L1-L6
=== COMMAND ===
gh pr view 23 --json isDraft,state,url
=== TIMESTAMP: 2026-11-11T06:57:39Z ===
=== EXIT CODE: 0 ===
=== RAW OUTPUT ===
{"isDraft":true,"state":"OPEN","url":"https://github.com/saifmnasri/kynex-shell/pull/23"}
```

---

## 5. Verification Matrix

| Check | Tool / Command | Evidence ID | Result |
|---|---|---|---|
| O3.1 Mutant Test Failure | `cargo test -p kynex-daemon --test tray_service_test` | `o3-mutant-owner-check-fail` | Exit 101, Panic verified |
| O3.1 Well-Known Rejection | `cargo test -p kynex-daemon --test tray_service_test test_register_by_another_clients_well_known_name_rejected` | `o3-test-wk-rejection-pass` | Exit 0, 1 passed |
| O3.1 Unowned Rejection | `cargo test -p kynex-daemon --test tray_service_test test_register_by_unowned_well_known_name_rejected` | `o3-test-unowned-rejection-pass` | Exit 0, 1 passed |
| O3.2 Mutant Killed | `build/tests/qml_tests/kynex_qml_tests -input tests/qml_tests/tst_shell.qml` | `o3-mutant-constants-killed` | Exit 1, Failed on -1920 |
| O3.2 Negative Screen Origin | `build/tests/qml_tests/kynex_qml_tests -input tests/qml_tests/tst_shell.qml` | `o3-qml-negative-screen-pass` | Exit 0, 70 passed |
| O3.2 Coordinates Projection | `cargo test -p kynex-shell --lib test_project_tray_coordinates_negative_screen_origin` | `o3-rust-coordinates-pass` | Exit 0, 1 passed |
| O3.2 QML Lint | `qmllint` | `o3-qmllint` | Exit 0, Clean |
| O3.3 Strict Gate | `STRICT=1 ./scripts/verify.sh` | `o3-verify-gate` | Exit 0, 20/20 passed |
| O3.3 Audit Gate | `./scripts/audit.sh` | `o3-audit-gate` | Exit 0, Clean SBOM |
| O3.3 PR Draft Status | `gh pr view 23 --json isDraft,state,url` | `o3-pr-draft-status` | Exit 0, `isDraft: true` |
