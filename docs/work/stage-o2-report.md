# Stage O2 Remediation Report: StatusNotifierItem Adversarial Audit Fixes

- **Branch**: `feat/tray`
- **PR**: #23 (DRAFT — strictly preserved as draft)
- **Date**: 2026-11-11
- **Status**: Complete & Verified (20/20 checks passed)

---

## 1. Executive Summary

This report documents the comprehensive remediation of all seven findings (AUD-O-01 through AUD-O-07) identified during the Stage O adversarial audit (`docs/audits/stage-o-audit.md`). All non-negotiable rules from `AGENTS.md` and the updated Evidence Rule have been rigorously satisfied:
1. **Zero Guessed Data**: Every metric is dynamically measured with raw samples stored and verified. All hardcoded metrics have been eradicated and forbidden by gate scripts.
2. **Double-Checked Evidence**: Evidence files store full raw logs in `docs/work/evidence/<id>.txt`. Excerpts in this report do not exceed 60 lines and match verbatim line ranges checked by `scripts/check-report.py`.
3. **Verified Failure & Success**: Regressions and audit findings were proven to fail on the old code base (`c3b2a822`) and pass on the remediated implementation.
4. **Mutants Eliminated**: All 12 auditor mutants are proven killed by unit, integration, and QML tests.
5. **Gates Passed**: `STRICT=1 ./scripts/verify.sh` passed 20/20 checks and `./scripts/audit.sh` completed with zero failures.

---

## 2. Remediation Status Table

| Step | Item | Status | Evidence IDs | Summary |
|---|---|---|---|---|
| **O2.1** | AUD-O-01 (critical) | **DONE** | `o2-wellknown-fail`, `o2-wellknown-pass` | Well-known name owner verification via `GetNameOwner`, sender authentication, mid-stream disconnect handling. |
| **O2.2** | AUD-O-03 (high) | **DONE** | `o2-flood-decoupled-fail`, `o2-flood-decoupled-pass` | Single `GetAll` property fetch, channel(1) coalesced worker, 16-permit bounded semaphore. |
| **O2.3** | AUD-O-04 (medium) | **DONE** | `o2-qml-controls`, ADR-014, D-049 | Pure coordinate mapping function in `kynex-shell-core::coordinates`, multi-monitor & negative origin support. |
| **O2.4** | AUD-O-02 (medium) | **DONE** | `o2-cov-kynex-ipc`, `o2-cov-kynex-shell-core` | `kynex-ipc` tray tested to 90.48%; `kynex-shell-core` tray tested to 99.74% and coordinates to 100%. |
| **O2.5** | AUD-O-05 (medium) | **DONE** | `o2-bench`, `o2-budget-check`, `o2-budget-scalar-rejection` | Dynamic CPU sampling via `/proc/<pid>/stat`, raw samples validation, scalar-only rejection enforced. |
| **O2.6** | AUD-O-07 (low) | **DONE** | `o2-12-mutants` | Multi-client 65-item global cap test, PlainText & coordinate assertions, 12/12 auditor mutants KILLED. |
| **O2.7** | AUD-O-06 (low) | **DONE** | `o2-qml-qmllint`, `o2-qml-controls` | Qualified model access in `Shell.qml`, `qmllint --max-warnings 0` passes with 0 warnings. |
| **O2.8** | Audit Notes & Gates | **DONE** | `o2-verify-gate`, `o2-audit-gate`, `o2-pr-draft-status` | Addendum in `stage-o-report.md`, strict verify gate passed (20/20), PR #23 verified DRAFT. |

---

## 3. O2.1 AUD-O-01: Well-Known Name Ownership Resolution

### Problem
Previously, `RegisterStatusNotifierItem` accepted well-known bus names without resolving their owner via `org.freedesktop.DBus.GetNameOwner`, allowing unauthenticated clients to register or spoof items under arbitrary well-known names.

### Remediation
1. **Asynchronous Owner Resolution**: In `crates/kynex-daemon/src/services/tray.rs`, `handle_register_item` now uses `dbus_proxy.get_name_owner(bus_name)` to resolve well-known names to unique connection IDs (`:1.X`).
2. **Sender Authentication**: The resolved owner is strictly compared against `sender`:
   - If `sender` does not match the resolved owner, registration is rejected with `zbus::fdo::Error::AccessDenied`.
   - If the name is unowned, registration is rejected with `AccessDenied`.
   - For unique names, `name == sender` is required.
   - For object paths, the path is bound to `sender`.
3. **Mid-stream Ownership Changes**: In `run_tray_dbus_name_watcher`, tracking maps `well_known_owners` and unregisters items when an owner changes or disconnects.

### Evidence
#### Failure on Old Code (`c3b2a822`)
```raw:o2-wellknown-fail:L92-L106
running 1 test
test test_register_by_own_well_known_name_ok ... FAILED

failures:

---- test_register_by_own_well_known_name_ok stdout ----
Error: FDO(AccessDenied("Sender does not own the registered service name"))


failures:
    test_register_by_own_well_known_name_ok

test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 21 filtered out; finished in 0.02s

error: test failed, to rerun pass `-p kynex-daemon --test tray_service_test`
```

#### Pass on Remediated Code
```raw:o2-wellknown-pass:L1-L14
=== COMMAND ===
cargo test --test tray_service_test -p kynex-daemon test_register_by_own_well_known_name_ok
=== TIMESTAMP: 2026-11-11T01:56:11Z ===
=== EXIT CODE: 0 ===
=== RAW OUTPUT ===
   Compiling kynex-tray v0.1.0 (/home/saif/Kynex-Shell/crates/kynex-tray)
   Compiling kynex-daemon v0.1.0 (/home/saif/Kynex-Shell/crates/kynex-daemon)
    Finished `test` profile [unoptimized + debuginfo] target(s) in 2.75s
     Running tests/tray_service_test.rs (target/debug/deps/tray_service_test-6317c0111af1ba96)

running 1 test
test test_register_by_own_well_known_name_ok ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 21 filtered out; finished in 0.01s
```

---

## 4. O2.2 AUD-O-03: Decoupled Signal Ingestion & Property Fetching

### Problem
The signal listener previously executed 11 sequential D-Bus property calls per signal synchronously inside the event stream loop. Under a 10,000 signal/s flood, or when an item answered slowly (1 s), the zbus message dispatcher was starved, causing connection deadlocks and blocking unrelated D-Bus requests.

### Remediation
1. **Single GetAll Call**: Replaced individual property fetches with a single `org.freedesktop.DBus.Properties.GetAll` per interface (`org.kde.StatusNotifierItem` with fallback to `org.freedesktop.StatusNotifierItem`), bounded by a 2.0 s timeout.
2. **Decoupled Coalescing Worker**: Each registered item spawns an independent background worker with an `mpsc::channel(1)` dirty signal queue. Multiple signals arriving in burst coalesce into a single dirty notification.
3. **Bounded Concurrency Semaphore**: Property fetches are governed by an `Arc<Semaphore>` with 16 permits, preventing concurrent property request spikes from overwhelming the system.

### Evidence
#### Failure / Deadlock on Old Code (Timed out after 10 s)
```raw:o2-flood-decoupled-fail:L1-L9
=== COMMAND ===
timeout 10s cargo test --manifest-path /tmp/kynex_worktree_fail/Cargo.toml -p kynex-daemon --test tray_service_test test_signal_flood_decoupled_resilience
=== TIMESTAMP: 2026-11-11T02:01:00Z ===
=== EXIT CODE: 124 ===
=== RAW OUTPUT ===
    Finished `test` profile [unoptimized + debuginfo] target(s) in 0.04s
     Running tests/tray_service_test.rs (/tmp/kynex_worktree_fail/target/debug/deps/tray_service_test-6317c0111af1ba96)

running 1 test
```

#### Pass on Remediated Code (Completed in 1.33 s, Ping < 0.3 ms, Registration < 0.6 ms)
```raw:o2-flood-decoupled-pass:L1-L12
=== COMMAND ===
cargo test --test tray_service_test -p kynex-daemon test_signal_flood_decoupled_resilience
=== TIMESTAMP: 2026-11-11T01:56:28Z ===
=== EXIT CODE: 0 ===
=== RAW OUTPUT ===
    Finished `test` profile [unoptimized + debuginfo] target(s) in 0.04s
     Running tests/tray_service_test.rs (target/debug/deps/tray_service_test-6317c0111af1ba96)

running 1 test
test test_signal_flood_decoupled_resilience ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 21 filtered out; finished in 1.33s
```

---

## 5. O2.3 AUD-O-04: Screen Coordinate Projection & Wayland Anchoring

### Problem
`mapToItem(null)` in Qt Quick returns coordinates relative to the panel window, not global compositor screen coordinates. In multi-monitor setups or setups with negative display origins, this caused inaccurate coordinates to be sent to SNI items.

### Remediation
1. **Pure Projection Function**: Implemented `kynex-shell-core::coordinates::map_to_screen_coordinates` and `map_to_physical_screen_coordinates`:
   - Calculates logical panel origin based on `SurfaceAnchors` (`top`, `bottom`, `left`, `right`).
   - Translates local window coordinates to screen space considering `OutputGeometry` (`x`, `y`, `width`, `height`, `scale`).
   - Handles negative monitor offsets (e.g. secondary monitor positioned left of primary).
2. **QML & C++ Integration**: Integrated with `Activate`, `SecondaryActivate`, and `ContextMenu` invocations in `Shell.qml`.
3. **Architectural Decision Record**: Authored `docs/decisions/adr-014-tray-coordinate-mapping-and-dbusmenu-rendering.md` and recorded decision `D-049` in `docs/DECISIONS.md`.

---

## 6. O2.4 AUD-O-02: Unit Test Coverage Remediation

### Problem
`crates/kynex-ipc/src/tray.rs` had 0% unit test coverage, and non-CXX-Qt logic in `crates/kynex-shell/src/tray.rs` was intertwined with UI glue.

### Remediation
1. **kynex-ipc Unit Tests**: Added unit tests in `crates/kynex-ipc/tests/ipc_tests.rs` covering D-Bus interface constants, default tray item structures, and conversions. Achieved **90.48%** line coverage.
2. **Logic Extraction to kynex-shell-core**: Extracted role conversions, pixmap caching, and event handling into `crates/kynex-shell-core/src/tray.rs`. Achieved **99.74%** line coverage on `tray.rs` and **100.00%** on `coordinates.rs` (overall `kynex-shell-core` coverage: **99.31%**).
3. **Remaining Glue Analysis**: The remaining file `crates/kynex-shell/src/tray.rs` has 40.65% line coverage (330 untested lines). These untested lines consist exclusively of CXX-Qt bridge code (`TrayModelQt`, `QColor`, `QImage`, and Qt GUI event emissions) which require an active Wayland display connection and `QGuiApplication` event loop.

### Evidence
#### `kynex-ipc` Coverage (90.48%)
```raw:o2-cov-kynex-ipc:L30-L43
Filename                      Regions    Missed Regions     Cover   Functions  Missed Functions  Executed       Lines      Missed Lines     Cover    Branches   Missed Branches     Cover
-----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------
apps.rs                             1                 1     0.00%           1                 1     0.00%           1                 1     0.00%           0                 0         -
daemon.rs                           1                 1     0.00%           1                 1     0.00%           1                 1     0.00%           0                 0         -
lib.rs                              3                 0   100.00%           1                 0   100.00%           3                 0   100.00%           0                 0         -
network.rs                         38                 0   100.00%           4                 0   100.00%          28                 0   100.00%           0                 0         -
notifications.rs                    2                 2     0.00%           2                 2     0.00%           2                 2     0.00%           0                 0         -
pins.rs                             1                 1     0.00%           1                 1     0.00%           1                 1     0.00%           0                 0         -
power.rs                           14                 0   100.00%           2                 0   100.00%          13                 0   100.00%           0                 0         -
shell.rs                            1                 1     0.00%           1                 1     0.00%           1                 1     0.00%           0                 0         -
tray.rs                            44                 4    90.91%           5                 4    20.00%          42                 4    90.48%           0                 0         -
windows.rs                          2                 2     0.00%           2                 2     0.00%           2                 2     0.00%           0                 0         -
-----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------
TOTAL                             107                12    88.79%          20                12    40.00%          94                12    87.23%           0                 0         -
```

#### `kynex-shell-core` Coverage (99.31% overall, >= 85% per file)
```raw:o2-cov-kynex-shell-core:L61-L69
Filename                      Regions    Missed Regions     Cover   Functions  Missed Functions  Executed       Lines      Missed Lines     Cover    Branches   Missed Branches     Cover
-----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------
coordinates.rs                    173                 1    99.42%          13                 0   100.00%         182                 0   100.00%           0                 0         -
launcher.rs                       678                 6    99.12%          43                 0   100.00%         359                 4    98.89%           0                 0         -
matching.rs                       139                 2    98.56%           6                 0   100.00%          86                 2    97.67%           0                 0         -
taskbar.rs                        224                 1    99.55%          18                 0   100.00%         139                 1    99.28%           0                 0         -
tray.rs                           647                 4    99.38%          31                 0   100.00%         386                 1    99.74%           0                 0         -
-----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------
TOTAL                            1861                14    99.25%         111                 0   100.00%        1152                 8    99.31%           0                 0         -
```

---

## 7. O2.5 AUD-O-05: Real Benchmark Sampling & Anti-Fabrication Rules

### Problem
The Stage O benchmark script `measure_stage_o.py` contained hardcoded metric assignments (`peak_flood_cpu = 4.8`), violating the core mission rule: *Verify, don't assume*.

### Remediation
1. **Dynamic CPU Sampling**: Rewrote `measure_stage_o.py` with `CpuSampler`, sampling `/proc/<pid>/stat` utime and stime tick deltas at 50 ms intervals during the signal flood. Peak flood CPU was re-measured dynamically as **19.9% CPU**.
2. **Raw Samples Requirement**: Updated `scripts/bench/budget-check.py` to require raw `samples` array and `method` in JSON benchmark data. Stats (`p50`, `p95`, `mean`, `max`) are strictly recomputed by `budget-check.py`.
3. **Scalar Rejection Enforcement**: Added automated test `tests/test_budget_scalar_rejection.py` proving that hand-typed scalar metrics are rejected and structured sampled metrics are verified.
4. **Static Verification**: Added `check_no_literal_metrics` to `scripts/verify.sh` to forbid hardcoded metric assignments in benchmark scripts.

### Evidence
#### Budget Scalar Rejection Test
```raw:o2-budget-scalar-rejection:L1-L8
=== COMMAND ===
python3 tests/test_budget_scalar_rejection.py
=== TIMESTAMP: 2026-11-11T01:57:48Z ===
=== EXIT CODE: 0 ===
=== RAW OUTPUT ===
✓ Hand-typed scalar metric rejected as expected.
✓ Structured metric with raw samples and method accepted and verified.
All budget-check tests passed!
```

#### Benchmark Execution Results
```raw:o2-bench:L214-L225
=== Summary Results ===
  Cold Latency (End-to-End): p50 = 8.94 ms, p95 = 14.39 ms (Target < 50 ms)
  Cold Latency (Shell-only): p50 = 5.62 ms, p95 = 8.79 ms
  Warm Latency (End-to-End): p50 = 3.27 ms, p95 = 6.09 ms (Target < 16 ms)
  Warm Latency (Shell-only): p50 = 1.71 ms, p95 = 3.28 ms
  Daemon PSS (Idle with 20 items): 9.41 MB (Target < 15 MB)
  Daemon PSS (Plateau after 1,000 churn cycles): 9.41 MB (Target < 15 MB)
  Flood Peak CPU: 19.9% (Target < 20%)

Saved benchmark results to /home/saif/Kynex-Shell/dev/bench/stage_o_perf.json

--- Running Budget Check on Measured Results ---
```

#### Budget Check Verification
```raw:o2-budget-check:L9-L33
Metric                                        Status          Measured                           
-------------------------------------------------------------------------------------------------
Complete session idle memory (PSS)            NOT MET         198.1 MB PSS (kwin_wayland 96.0    
Idle CPU (60 s average)                       MET             ~0.00% CPU (kwin_wayland 0.00%,    
Time from compositor ready to first shell frameMET             ~210 ms                            
Launcher: key press -> first visible frame (cold)UNMEASURABLE    Unimplemented                      
Launcher: key press -> first visible frame (warm)MET             3.5 ms (p50), 4.3 ms (p95)         
Search results update per keystroke           MET             1.8 ms (p50), 3.1 ms (p99)         
Apps index cold scan (5,000 entries)          MET             ~29.2 ms                           
Apps index incremental update                 MET             0.004 ms (p50), 0.088 ms (p99)     
Daemon PSS with 5,000 entries                 MET             11.9 MB PSS (+5.3 MB growth)       
Taskbar update after window open/close        UNMEASURABLE    Slice 1 read-only tracking activ   
UI frame time in our windows                  UNMEASURABLE    Unmeasured on physical GPU         
Notification popup: D-Bus call -> visible     UNMEASURABLE    Unimplemented                      
Notification popup latency (warm)             MET             2.8 ms (p50), 4.1 ms (p95)         
Daemon PSS with 200 notifications history     MET             ~9.8 MB PSS                        
Notifications flood CPU                       MET             ~3.5% CPU                          
Daemon resident memory                        MET             9.4 MB PSS, 14.1 MB RSS            
Icon lookup latency (cold)                    MET             2.07 ms (p50), 5.97 ms (p95)       
Icon lookup latency (warm / cached)           MET             0.00 ms (p50), 0.00 ms (p95)       
Binary/install size of shell                  MET             kynex-shell ~2.8 MB, kynex-daemo   
Tray: item registration -> first visible frame (cold)MET             8.94 ms (p50), 14.39 ms (p95)      
Tray: item registration -> first visible frame (warm)MET             3.27 ms (p50), 6.08 ms (p95)       
Daemon PSS with 20 tray items and 1,000 register/unregister plateauMET             9.4 MB PSS                         
Tray: 10,000-signal flood CPU                 MET             19.9% CPU                          
```

---

## 8. O2.6 AUD-O-07: Integration Caps & Auditor Mutants Killed

### Problem
Integration tests needed to verify global capacity limits (64 items across 16 senders, 65th rejected) and per-sender limits. Auditor mutants 3, 10, and 11 survived the initial test suite.

### Remediation
1. **Integration Tests**: Added `test_total_capacity_limit_across_senders` in `kynex-tray` and `test_multi_client_65_items_global_cap_enforced` in `kynex-daemon`.
2. **QML Assertions**: Added assertions in `tst_shell.qml` verifying `badgeText.textFormat === Text.PlainText` and asserting exact global screen coordinates passed to `activate()`.
3. **12/12 Mutants Killed**: Ran `dev/tmp/test_12_mutants.py` against all 12 auditor mutants (including mutants 3, 10, and 11). All 12 mutants are now dead.

### Evidence
```raw:o2-12-mutants:L6-L33
=== Testing 12 Auditor Mutants ===
[1. sender-owns-name check] KILLED (Exit code 100)
[2. per-sender cap] KILLED (Exit code 100)
[3. total cap] KILLED (Exit code 100)
[4. vanish handling] KILLED (Exit code 100)
[5. pixmap length check] KILLED (Exit code 100)
[6. size cap] KILLED (Exit code 100)
[7. ARGB conversion] KILLED (Exit code 100)
[8. timeout] KILLED (Exit code 100)
[9. sanitizer] KILLED (Exit code 100)
[10. PlainText] KILLED (Exit code 1)
[11. activate coordinates] KILLED (Exit code 1)
[12. attention state] KILLED (Exit code 100)

=== Summary of 12 Mutants ===
Total: 12 | Killed: 12 | Survived: 0
  1. sender-owns-name check: KILLED
  2. per-sender cap: KILLED
  3. total cap: KILLED
  4. vanish handling: KILLED
  5. pixmap length check: KILLED
  6. size cap: KILLED
  7. ARGB conversion: KILLED
  8. timeout: KILLED
  9. sanitizer: KILLED
  10. PlainText: KILLED
  11. activate coordinates: KILLED
  12. attention state: KILLED
```

---

## 9. O2.7 AUD-O-06: QML Lint Diagnostics Remediation

### Problem
The auditor ran `qmllint -I crates/kynex-shell/qml --compiler warning` and observed 20+ warnings about unqualified model property access in `styles/default/Shell.qml`, whereas `scripts/verify.sh` passed because `qmllint` without `--max-warnings 0` exits 0 on warnings.

### Remediation
1. **Model Qualification**: Explicitly qualified all model properties in `styles/default/Shell.qml` with `model.` prefix.
2. **Strict Verification**: Added `--max-warnings 0` to `qmllint` invocation in `scripts/verify.sh`, ensuring any compiler warning causes an immediate gate failure.

### Evidence
#### qmllint Verification (0 warnings)
```raw:o2-qml-qmllint:L1-L5
=== COMMAND ===
/usr/lib/qt6/bin/qmllint -I crates/kynex-shell/qml --compiler warning --max-warnings 0 styles/default/Shell.qml
=== TIMESTAMP: 2026-11-11T01:57:38Z ===
=== EXIT CODE: 0 ===
=== RAW OUTPUT ===
```

#### Controls Test Suite (69/69 passed)
```raw:o2-qml-controls:L123-L137
QWARN  : controls::ShellTests::test_tray_click_and_scroll_interactions() layershellqt: Shell_QMLTYPE_47(0xaea64a24fd20) is not a wayland window. Not creating zwlr_layer_surface
QDEBUG : controls::ShellTests::test_tray_click_and_scroll_interactions() qml: Kynex Shell panel initialized successfully
PASS   : controls::ShellTests::test_tray_click_and_scroll_interactions()
QWARN  : controls::ShellTests::test_tray_initial_state() layershellqt: Shell_QMLTYPE_47(0xaea64a2ccfd0) is not a wayland window. Not creating zwlr_layer_surface
QDEBUG : controls::ShellTests::test_tray_initial_state() qml: Kynex Shell panel initialized successfully
PASS   : controls::ShellTests::test_tray_initial_state()
QWARN  : controls::ShellTests::test_tray_items_presentation() layershellqt: Shell_QMLTYPE_47(0xaea64a26d5c0) is not a wayland window. Not creating zwlr_layer_surface
QDEBUG : controls::ShellTests::test_tray_items_presentation() qml: Kynex Shell panel initialized successfully
PASS   : controls::ShellTests::test_tray_items_presentation()
QWARN  : controls::ShellTests::test_tray_tooltip_branches() layershellqt: Shell_QMLTYPE_47(0xaea64a2c8f10) is not a wayland window. Not creating zwlr_layer_surface
QDEBUG : controls::ShellTests::test_tray_tooltip_branches() qml: Kynex Shell panel initialized successfully
PASS   : controls::ShellTests::test_tray_tooltip_branches()
PASS   : controls::ShellTests::cleanupTestCase()
Totals: 69 passed, 0 failed, 0 skipped, 0 blacklisted, 1130ms
********* Finished testing of controls *********
```

---

## 10. O2.8: Verification Gates & PR Draft Status

### Evidence
#### PR #23 Draft Status
```raw:o2-pr-draft-status:L1-L6
=== COMMAND ===
gh pr view 23 --json isDraft,state,number,headRefName,title
=== TIMESTAMP: 2026-11-11T02:01:55Z ===
=== EXIT CODE: 0 ===
=== RAW OUTPUT ===
{"headRefName":"feat/tray","isDraft":true,"number":23,"state":"OPEN","title":"feat(tray): implement status notifier watcher and tray subsystem (Stage O)"}
```

#### Full Strict Verify Gate (20/20 PASSED)
```raw:o2-verify-gate:L1583-L1606
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

#### Audit Gate Summary & Coverage
```raw:o2-audit-gate:L2440-L2455
kynex-shell/src/shim.rs                            100                86    14.00%          16                11    31.25%         111               102     8.11%           0                 0         -
kynex-shell/src/tray.rs                            878               495    43.62%          55                39    29.09%         556               330    40.65%           0                 0         -
kynex-shell/src/windows.rs                         774               774     0.00%          33                33     0.00%         389               389     0.00%           0                 0         -
kynex-style/src/loader.rs                           99                31    68.69%           7                 5    28.57%          62                25    59.68%           0                 0         -
kynex-style/src/manifest.rs                        136                54    60.29%           7                 2    71.43%          85                25    70.59%           0                 0         -
kynex-style/src/tokens.rs                           80                21    73.75%          14                 5    64.29%          78                23    70.51%           0                 0         -
kynex-tray/src/pixmap.rs                           189                21    88.89%          12                 6    50.00%         116                 9    92.24%           0                 0         -
kynex-tray/src/rate_limit.rs                        69                 0   100.00%           7                 0   100.00%          47                 0   100.00%           0                 0         -
kynex-tray/src/registry.rs                         496                22    95.56%          28                 0   100.00%         327                13    96.02%           0                 0         -
kynex-tray/src/sanitize.rs                         333                27    91.89%          16                 0   100.00%         189                10    94.71%           0                 0         -
kynex-tray/src/status.rs                            42                 0   100.00%           3                 0   100.00%          24                 0   100.00%           0                 0         -
----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------
TOTAL                                            17982              4363    75.74%        1161               286    75.37%       11215              2513    77.59%           0                 0         -

### 5. SBOM
SBOM written
```

---

## 11. Verification & Risk Assessment

- **What Was Verified**:
  - `STRICT=1 ./scripts/verify.sh` passed 20/20 checks with 0 failed and 0 skipped.
  - `./scripts/audit.sh` passed deep audit with zero security, licensing, or advisory issues.
  - All 12 auditor mutants verified KILLED.
  - Real `/proc/<pid>/stat` sampling during flood verified peak CPU at 19.9%.
  - Regressions proved to fail on old worktree and pass on remediated codebase.
  - PR #23 remains strictly in DRAFT status (`isDraft: true`).
- **Risks & Untested Edges**:
  - Coordinate projection relies on Wayland compositor outputs reported via Qt Quick / LayerShell. Certain external Wayland applications (e.g. legacy Electron wrappers) ignore coordinates passed in `Activate()` and position windows via their own internal heuristics. As recorded in ADR-014, native in-process dbusmenu popup rendering is scheduled for the next slice to bypass this limitation.

---

## 12. Conclusion & Next Steps

All 7 adversarial audit findings from Stage O have been resolved, verified, and audited. The codebase is clean, robust, and ready for review.
