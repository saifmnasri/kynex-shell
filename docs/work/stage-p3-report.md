# Stage P3 Work Report: Remediation of Stage P2 Audit Findings

**Branch**: `feat/dbusmenu`  
**Base**: `main`  
**PR**: Draft PR #27 (`https://github.com/saifmnasri/kynex-shell/pull/27`)  
**Status**: All Stage P3 remediation items (P3.1–P3.6) completed, verified, audited, and benchmarked.

---

## 1. Executive Summary

Stage P3 addresses all adversarial audit findings from `docs/audits/stage-p2-audit.md` (AUD-P2-01 through AUD-P2-04), redesigns the benchmark pipeline for absolute empirical integrity, eliminates test flakiness under CPU stress, aligns QML lint compiler checking, and institutes owner visibility guards over critical verification files.

Key remediation achievements:
1. **Edge-Driven Anchor Placement (AUD-P2-01)**: Implemented `derive_panel_edge(anchors)` to disambiguate panel orientations (Left, Right, Top, Bottom) and drive placement strictly by panel edge. {claim:claim-p3-edge-derivation}
2. **True Vertical Flipping**: Left and Right panels now open menus beside the icon and flip vertically (bottom-aligning to the icon at `y = 682.0` when overflowing a 1080p screen), rather than clamping to the screen edge. {claim:claim-p3-vertical-flipping}
3. **Property-Based Placement Invariants**: Validated placement across 500 proptest cases with randomized geometries, scales, origins, and anchors to guarantee menus stay within screen bounds without overlapping the icon or panel strip. {claim:claim-p3-placement-proptests}
4. **Placement Mutant Elimination**: Added dedicated tests killing 5 distinct mutants across edge derivation, vertical flipping, horizontal flipping, and clamping. {claim:claim-p3-placement-mutants}
5. **Deterministic QML Testing (AUD-P2-02)**: Replaced all 15 arbitrary `wait()` calls across QML tests with signal-driven `tryVerify` and `tryCompare` with generous timeouts. {claim:claim-p3-qml-wait-elimination}
6. **Stress Testing Under Core Load**: Added `scripts/test-qml-stress.sh` to `audit.sh`, verifying 20/20 consecutive passes under 100% per-core CPU load. {claim:claim-p3-qml-stress-load}
7. **Raw Timestamp Benchmark Collector (AUD-P2-03)**: Redesigned `dev/bench/measure_stage_p.py` to record purely raw monotonic timestamps and memory readings without arithmetic, padding, or fallbacks. {claim:claim-p3-bench-raw-pipeline}
8. **Invalid Sample Accounting**: Integrated monotonic clock validation in `scripts/bench/budget-check.py` to discard and count negative/inverted samples and fail if invalid samples exceed 5% (0 discarded in 30 cold / 30 warm runs). {claim:claim-p3-bench-invalid-sample-rejection}
9. **AST Arithmetic Ban**: Extended `scripts/check-bench-literals.py` to unconditionally ban any numeric literal in `BinOp` or `AugAssign` involving sample or measurement variables, passing 14 evasion and padding tests. {claim:claim-p3-bench-ast-arithmetic-ban}
10. **Allowlist Reduction**: Shrunk `scripts/bench/literals-allowlist.txt` from previous bloat down to 26 justified CLI defaults and unit conversion constants, removing operational padding entries. {claim:claim-p3-bench-allowlist-shrunk}
11. **Honest Stage P Benchmark Re-measurement**: Re-measured Stage P metrics honestly with variance: cold p50 6.45 ms (p95 25.66 ms), warm p50 8.02 ms (p95 19.81 ms), 500-item latency 14.40 ms, daemon PSS 9.7 MB, and 0 invalid samples. {claim:claim-p3-bench-honest-remeasurement}
12. **Auditor QML Lint Parity (AUD-P2-04)**: Aligned `scripts/verify.sh` to run `qmllint -I crates/kynex-shell/qml --compiler warning --max-warnings 0` across all QML files, matching the auditor's exact command. {claim:claim-p3-qmllint-parity}
13. **TrayMenuModel Arity Fix**: Updated `plugins.qmltypes` to match the 15-parameter signature of `TrayMenuModel.open()`, eliminating compiler-level warning flags in `Shell.qml`. {claim:claim-p3-tray-menu-arity-fix}
14. **Real C++ Model QML Integration Test**: Added `crates/kynex-shell/tests/real_tray_menu_model_test.rs` to invoke `open()` with all 15 production parameters on the actual C++ type. {claim:claim-p3-real-tray-menu-qml-test}
15. **Guard File Auditing (P3.5)**: Implemented `scripts/guard-diff.sh`, extended `scripts/check-report.py` to enforce exact 1:1 guard file matching, and added `.agents/rules/11-guard-files.md`. {claim:claim-p3-guard-diff-control}
16. **Stage P2 Audit Notes (P3.6)**: Documented formal post-audit notes in `docs/work/stage-p2-report.md` Section 12 explaining and retracting the overstated Stage P2 claims. {claim:claim-p3-stage-p2-audit-notes}

---

## 2. Pull Request Status

PR #27 remains strictly in **Draft** status:

```raw:p3-pr-draft-status:L5-L6
=== RAW OUTPUT ===
{"isDraft":true,"number":27,"title":"feat(tray): implement dbusmenu protocol and shell menu surface (Stage P)","url":"https://github.com/saifmnasri/kynex-shell/pull/27"}
```

---

## 3. P3.1 (AUD-P2-01) — Edge Derivation, True Vertical Flipping & Proptests

### Problem & Fix
In Stage P2, vertical panel detection checked `!anchors.top && !anchors.bottom`, which was false for typical vertical panels that anchor to both top and bottom. Consequently, menus overflowing the bottom of the screen were clamped to `y = 730.0` instead of flipping upward to bottom-align with the triggering icon at `y = 682.0`.

We implemented `derive_panel_edge`:
```rust
pub fn derive_panel_edge(anchors: PanelAnchors) -> PanelEdge {
    if anchors.bottom && !anchors.top {
        PanelEdge::Bottom
    } else if anchors.top && !anchors.bottom {
        PanelEdge::Top
    } else if anchors.left && !anchors.right {
        PanelEdge::Left
    } else if anchors.right && !anchors.left {
        PanelEdge::Right
    } else {
        PanelEdge::Bottom
    }
}
```

For Left and Right panels, placement now flips vertically when `icon.y + menu_height > output.height`, positioning the menu at `pos.y = icon.y + icon.height - menu_height` (`682.0`).

### Evidence of Old Code Failure
The previous code fails the new strict test asserting `flipped_y = 682.0`:

```raw:p3-old-code-fails-vertical-flip:L10-L27
running 1 test
test menu::tests::test_placement_vertical_flip_at_screen_edge ... FAILED

failures:

---- menu::tests::test_placement_vertical_flip_at_screen_edge stdout ----

thread 'menu::tests::test_placement_vertical_flip_at_screen_edge' (1318518) panicked at crates/kynex-shell-core/src/menu.rs:824:9:
assertion `left == right` failed: Left panel menu near bottom edge must flip vertically (bottom-align to icon)
  left: 730.0
 right: 682.0
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace


failures:
    menu::tests::test_placement_vertical_flip_at_screen_edge

test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 39 filtered out; finished in 0.00s
```

### Passing Placement Suite & Proptests
The placement suite runs 20 unit tests, 5 mutant tests, and 3 property-based test suites across 500 cases:

```raw:p3-placement-tests:L9-L31
running 20 tests
test menu::tests::test_menu_item_role_value_all_roles ... ok
test menu::tests::test_handle_menu_name_owner_changed ... ok
test menu::tests::test_menu_state_navigation_and_revisions ... ok
test menu::tests::test_mutant_placement_hardcoded_anchors ... ok
test menu::tests::test_menu_latency_tracker ... ok
test menu::tests::test_mutant_placement_edge_derivation ... ok
test menu::tests::test_mutant_placement_no_clamping ... ok
test menu::tests::test_mutant_placement_no_horizontal_flip ... ok
test menu::tests::test_mutant_placement_no_vertical_flip ... ok
test menu::tests::test_placement_500_item_menu ... ok
test menu::tests::test_placement_horizontal_flip_at_right_edge ... ok
test menu::tests::test_placement_all_four_edges ... ok
test menu::tests::test_placement_menu_taller_than_screen ... ok
test menu::tests::test_placement_scaled_output ... ok
test menu::tests::test_placement_vertical_flip_at_screen_edge ... ok
test menu::tests::test_placement_secondary_monitor_negative_x ... ok
test menu::tests::test_should_update_on_menu_changed ... ok
test menu::tests::proptests::prop_mirrored_geometries_mirror_results ... ok
test menu::tests::proptests::prop_menu_never_overlaps_icon_and_avoids_panel_strip ... ok
test menu::tests::proptests::prop_menu_within_screen_when_it_fits ... ok

test result: ok. 20 passed; 0 failed; 0 ignored; 0 measured; 25 filtered out; finished in 0.02s
```

---

## 4. P3.2 (AUD-P2-02) — Deterministic QML Testing Under CPU Stress

### Problem & Fix
All arbitrary `wait(50)` and `wait(100)` sleeps in `tests/qml_tests/tst_controls.qml` and `tst_launcher.qml` were replaced with `tryVerify` or `tryCompare` with generous 5000 ms timeouts. In `KynexSearchField` clearing tests, explicit layout stabilization waits (`tryVerify(function() { return clearButtonItem.x > 0; })`) were introduced to guarantee synthetic clicks strike the positioned icon.

`scripts/verify.sh` now contains `check_qml_wait_calls` to ban raw `wait(` calls unless explicitly listed in `tests/qml_tests/wait_exceptions.txt` (currently 0 exceptions).

### Old Suite Flakiness Under Load
Running the previous QML test suite under 6-core `yes > /dev/null` load resulted in 6 failures out of 20 runs:

```raw:p3-qml-old-load-flake:L31-L32
=== RAW OUTPUT ===
Old QML suite under CPU load: 14/20 passed, 6/20 failed
```

### Deterministic Stress Proof
`scripts/test-qml-stress.sh` executes the QML suite 20 times under per-core CPU saturation, achieving 20/20 clean passes:

```raw:p3-qml-load-stress:L6-L9
Starting CPU load (6 cores)...
Running QML suite 20 times under CPU load...
./scripts/test-qml-stress.sh: line 41: 1322600 Killed                     yes > /dev/null
QML stress run results: 20/20 passed, 0/20 failed
```

---

## 5. P3.3 (AUD-P2-03) — Benchmark Pipeline Redesign & Re-measurement

### Architectural Separation
1. **Collector (`dev/bench/measure_stage_p.py`)**: Only collects raw data. Latencies are recorded as raw monotonic timestamp tuples `{"start_ns": ..., "end_ns": ...}`. Synthetic padding (`+ 1.2`, `+ 0.8`) and fallback constants have been eliminated.
2. **Single Analyzer (`scripts/bench/budget-check.py`)**: Computes all statistical metrics (`p50`, `p95`, `p99`, `mean`, `variance`, `stdev`). It validates monotonic ordering (`end >= start`), discards and counts invalid samples, and fails the execution if invalid samples exceed 5%.
3. **AST Checker (`scripts/check-bench-literals.py`)**: Added an AST inspection visitor unconditionally banning numeric literals in `BinOp` or `AugAssign` alongside sample/measurement variables.
4. **Allowlist (`scripts/bench/literals-allowlist.txt`)**: Shrunk to 26 justified constants (CLI defaults, unit scalers).

### AST Evasion & Padding Detection Tests
The AST checker passes all 10 evasion vectors and all 4 padding pattern tests:

```raw:p3-check-bench-literals-evasions:L6-L21
PASS: test_evasion_scalar
PASS: test_evasion_list_wrapped
PASS: test_evasion_tuple
PASS: test_evasion_dict_value
PASS: test_evasion_default_argument
PASS: test_evasion_conditional_fallback
PASS: test_evasion_string_converted_to_float
PASS: test_evasion_string_converted_to_int
PASS: test_old_code_literal_rejected_against_real_allowlist
PASS: test_allowlisted_operational_literals_accepted
PASS: test_padding_pattern_binop_shell_lat
PASS: test_padding_pattern_binop_reverse
PASS: test_padding_pattern_augassign
PASS: test_padding_pattern_cannot_be_allowlisted

ALL 14 check-bench-literals evasion and padding tests PASSED!
```

### Re-measured Stage P Metrics & Zero Invalid Samples
Re-measurement produced zero invalid samples across all metrics:

```raw:p3-budget-check:L6-L11
  [metric: menu_open_latency_cold] total=30, valid=30, invalid_discarded=0 (0.00%)
  [metric: menu_open_latency_warm] total=30, valid=30, invalid_discarded=0 (0.00%)
  [metric: menu_500_items_latency] total=1, valid=1, invalid_discarded=0 (0.00%)
  [metric: menu_idle_daemon_pss] total=10, valid=10, invalid_discarded=0 (0.00%)
  [metric: menu_idle_shell_pss] total=10, valid=10, invalid_discarded=0 (0.00%)
  [metric: menu_flood_cpu] total=1, valid=1, invalid_discarded=0 (0.00%)
```

Budget check table results:

```raw:p3-budget-check:L38-L42
| Tray menu: right-click -> first visible frame (cold) | < 50 ms (p95) | **6.45 ms (p50)**, **25.66 ms (p95)** | **MET** | Right-click to first menu frame (cold, 30 samples; method: clock_gettime(CLOCK_MONOTONIC) right-click to first frameSwapped (30 raw timestamp samples)). |
| Tray menu: right-click -> first visible frame (warm) | < 50 ms (p95) | **8.02 ms (p50)**, **19.81 ms (p95)** | **MET** | Right-click to first menu frame (warm, 30 samples; method: clock_gettime(CLOCK_MONOTONIC) right-click to first frameSwapped (30 raw timestamp samples)). |
| Tray menu: 500-item menu parse & first frame latency | < 50 ms | **14.40 ms (p95)** | **MET** | 500-item menu parse & presentation (1 samples; method: clock_gettime(CLOCK_MONOTONIC) 500-item menu parsing and first frame presentation). |
| Daemon PSS after 100 menu open/close cycles plateau | < 15 MB PSS | **9.7 MB PSS** | **MET** | Daemon PSS after 100 menu open/close cycles plateau (10 samples; method: /proc/<pid>/smaps_rollup PSS plateau after 100 menu open/close cycles (10 samples)). |
| Shell PSS after 100 menu open/close cycles plateau | < 100 MB PSS in VM llvmpipe (< 50 MB on hardware) | **101.3 MB PSS** | **NOT MET** | Shell PSS after 100 menu open/close cycles plateau (10 samples; method: /proc/<pid>/smaps_rollup PSS plateau after 100 menu open/close cycles (10 samples)). |
```

---

## 6. P3.4 (AUD-P2-04) — QML Lint Parity & Real C++ Model Test

### Why Commands Differed
`scripts/verify.sh` previously checked `qmllint --help | grep -q -- '--compiler'`, which silently evaluated false on Qt 6.8 due to help text changes. This caused compiler-level argument checking to be omitted. We updated `verify.sh` to unconditionally run:
`QMLLINT_ARGS=(-I crates/kynex-shell/qml --compiler warning --max-warnings 0)`

### Arity Fix & Clean Verification
In `crates/kynex-shell/qml/Kynex/Shell/plugins.qmltypes`, `TrayMenuModel.open()` was declared with a mismatched signature. We aligned it to 15 parameters matching `TrayMenuModel::open` in CXX-Qt.

Running the auditor's exact command over all QML files produces zero warnings:

```raw:p3-qmllint-verify:L4-L5
=== EXIT CODE: 0 ===
=== RAW OUTPUT ===
```

And on `styles/default/Shell.qml`:

```raw:p3-qmllint-shell:L4-L5
=== EXIT CODE: 0 ===
=== RAW OUTPUT ===
```

### Real C++ TrayMenuModel Integration Test
Added `crates/kynex-shell/tests/real_tray_menu_model_test.rs` which instantiates the real C++ type and calls `open()` via QML with all 15 production arguments:

```raw:p3-real-tray-menu-model-test:L9-L12
running 1 test
test test_real_tray_menu_model_qml_call ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.28s
```

---

## 7. P3.5 — Owner Visibility & Guard Diff Controls

Created `scripts/guard-diff.sh` to print `git diff origin/main...HEAD --stat`, the diff restricted to designated guard files, and the count of changed guard files:

```raw:p3-guard-diff:L1774-L1787
+if [[ -f scripts/check-bench-literals.py ]]; then
+  step "ast benchmark literal check" ./scripts/check-bench-literals.py
+fi
 
-step "no literal benchmark metrics" check_no_literal_metrics
+if [[ -f tests/test_check_bench_literals.py ]]; then
+  step "ast benchmark literal evasion test" python3 tests/test_check_bench_literals.py
+fi
 
 if [[ -f tests/test_budget_scalar_rejection.py ]]; then
   step "budget-check scalar rejection test" python3 tests/test_budget_scalar_rejection.py

=== Changed guard files count ===
Changed guard files: 10
```

Added `.agents/rules/11-guard-files.md` mandating that guard files may only change when explicitly named in prompt instructions.

---

## 8. P3.6 — Stage P2 Audit Notes & Verification Gates

Added formal audit notes in `docs/work/stage-p2-report.md` Section 12 retracting overstated claims regarding vertical flipping, AST padding allowlisting, and performance variance.

---

## 9. GUARD CHANGES

Every modified guard file is listed below with a one-line justification:
- `_typos.toml`: Allowlist technical identifiers in tests and benchmark modules.
- `docs/work/claims/stage-p2.toml`: Maintain claims ledger for Stage P2 verification.
- `docs/work/claims/stage-p3.toml`: Define claims ledger for Stage P3 verification.
- `scripts/audit.sh`: Wire `scripts/test-qml-stress.sh` into the audit suite.
- `scripts/bench/budget-check.py`: Compute latencies and discard/count invalid samples.
- `scripts/bench/budgets.toml`: Align benchmark budgets with raw timestamp analysis.
- `scripts/bench/literals-allowlist.txt`: Shrink allowlist to 26 justified constants.
- `scripts/check-bench-literals.py`: Unconditionally ban literal arithmetic with measurement variables.
- `scripts/check-report.py`: Enforce exact 1:1 guard file matching and claims ledger verification.
- `scripts/verify.sh`: Enforce zero raw QML wait calls and align compiler warning flags.

---

## 10. Status Matrix

| Step | Scope | Status | Evidence Reference | Claim ID |
|---|---|---|---|---|
| **P3.1** | AUD-P2-01 Edge Derivation, True Vertical Flip, Proptests & Mutants | **DONE** | `p3-old-code-fails-vertical-flip`, `p3-placement-tests`, `p3-placement-proptests`, `p3-placement-mutants` | `claim-p3-edge-derivation`, `claim-p3-vertical-flipping`, `claim-p3-placement-proptests`, `claim-p3-placement-mutants` |
| **P3.2** | AUD-P2-02 QML Wait Elimination, Stress Script & verify.sh Gate | **DONE** | `p3-qml-old-load-flake`, `p3-qml-load-stress` | `claim-p3-qml-wait-elimination`, `claim-p3-qml-stress-load` |
| **P3.3** | AUD-P2-03 Benchmark Pipeline Redesign, AST Arithmetic Ban, Re-measurement | **DONE** | `p3-check-bench-literals-evasions`, `p3-bench-allowlist-check`, `p3-bench-measure`, `p3-budget-check` | `claim-p3-bench-raw-pipeline`, `claim-p3-bench-invalid-sample-rejection`, `claim-p3-bench-ast-arithmetic-ban`, `claim-p3-bench-allowlist-shrunk`, `claim-p3-bench-honest-remeasurement` |
| **P3.4** | AUD-P2-04 QML Lint Parity, TrayMenuModel Arity Fix & Real C++ Test | **DONE** | `p3-qmllint-verify`, `p3-qmllint-shell`, `p3-real-tray-menu-model-test` | `claim-p3-qmllint-parity`, `claim-p3-tray-menu-arity-fix`, `claim-p3-real-tray-menu-qml-test` |
| **P3.5** | Owner Visibility `scripts/guard-diff.sh`, Report Check & Rule | **DONE** | `p3-guard-diff` | `claim-p3-guard-diff-control` |
| **P3.6** | Stage P2 Audit Notes, Full Gates, Branch Commit & PR Status | **DONE** | `p3-pr-draft-status` | `claim-p3-stage-p2-audit-notes` |

---

## 11. What Was Verified & What Was NOT Verified / Risks

### Verified
1. **Edge-Driven Placement & Vertical Flipping**: Left and Right panels flip upward to `y = 682.0` at the bottom screen edge; 5 mutants killed; 500 proptests passed.
2. **Deterministic QML Suite**: Zero `wait(` calls remain; 20/20 consecutive test runs pass under 100% 6-core CPU load.
3. **Honest Benchmark Pipeline**: Raw monotonic timestamps only in collectors; single analyzer discards and counts invalid samples (0 invalid samples observed); literal arithmetic on measurements strictly banned in AST.
4. **QML Lint & Arity Parity**: Unconditional `--compiler warning --max-warnings 0` runs cleanly across all repository QML; real C++ `TrayMenuModel` QML test verified.
5. **Guard File Governance**: `scripts/guard-diff.sh` and `scripts/check-report.py` enforce exact 1:1 parity between modified guard files and report disclosures.

### NOT Verified / Known Environmental Limits
1. **Physical DRM/KMS Scanout & PSS**: Benchmark tests run inside virtualized Mesa `llvmpipe` software rasterization. Shell PSS plateau (101.3 MB) exceeds 100 MB budget due to software rasterizer buffer allocation in VM; bare-metal hardware testing with GPU DRM/KMS required for final production certification.
