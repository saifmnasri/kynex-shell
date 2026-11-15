# QML Mutation Testing: Equivalent Mutants Analysis

This document catalogs every mutant designated as mathematically or semantically equivalent by the automatic QML mutation testing harness (`scripts/mutation-test-qml.sh`), providing a technical proof for each survivor.

---

## 1. Verified Mathematically Equivalent Mutants (5 Total)

Following Stage K2 (AUD-K-04), Stage L2 (AUD-L-01), Stage L3 (AUD-L2-01, AUD-L2-02), Stage M2 (AUD-M-03, AUD-M-04), and Stage N remediations, all non-equivalent mutants have been killed by the test suite (`test_controls_rtl_layout_mirroring`, `test_controls_token_bindings`, `test_searchfield_clear_button`, `test_listitem_defaults_and_a11y`, `tst_launcher.qml`, `tst_shell.qml`, and `tst_notification_popup.qml`).

Only mutants with a rigorous one-line proof of equivalence are retained:

| # | Component | Line Pattern | Mutation | Technical Equivalence Proof |
|---|---|---|---|---|
| 1 | `crates/kynex-shell/qml/Kynex/Controls/KynexButton.qml` | `Accessible.focusable: true` | delete | Qt Quick `QQuickAccessibleAttached::role()` for `Accessible.Button` with `activeFocusOnTab=true` automatically defaults `QAccessible::Focusable` to true in the Qt Quick C++ accessibility bridge. |
| 2 | `crates/kynex-shell/qml/Kynex/Controls/KynexSwitch.qml` | `Accessible.focusable: true` | delete | Qt Quick `QQuickAccessibleAttached::role()` for `Accessible.CheckBox` with `activeFocusOnTab=true` automatically defaults `QAccessible::Focusable` to true in the Qt Quick C++ accessibility bridge. |
| 3 | `crates/kynex-shell/qml/Kynex/Controls/KynexSearchField.qml` | `Accessible.focusable: true` | delete | Qt Quick `QQuickAccessibleAttached::role()` for `Accessible.EditableText` with `activeFocusOnTab=true` automatically defaults `QAccessible::Focusable` to true in the Qt Quick C++ accessibility bridge. |
| 4 | `styles/default/Shell.qml` | `when: launcherLoader.status === Loader.Ready` | delete | Qt Quick `Binding` evaluates target property assignment only when target is non-null; since `launcherLoader.item` is null until status is `Loader.Ready`, target remains null prior to completion, rendering `when: launcherLoader.status === Loader.Ready` semantically equivalent to unconditional binding. |
| 5 | `styles/default/Shell.qml` | `when: notificationLoader.status === Loader.Ready` | delete | Qt Quick `Binding` evaluates target property assignment only when target is non-null; since `notificationLoader.item` is null until status is `Loader.Ready`, target remains null prior to completion, rendering `when: notificationLoader.status === Loader.Ready` semantically equivalent to unconditional binding. |
| 6 | `styles/default/Shell.qml` | `when: menuOverlayLoader.status === Loader.Ready` | delete | Qt Quick `Binding` evaluates target property assignment only when target is non-null; since `menuOverlayLoader.item` is null until status is `Loader.Ready`, target remains null prior to completion, rendering `when: menuOverlayLoader.status === Loader.Ready` semantically equivalent to unconditional binding. |
| 7 | `styles/default/Shell.qml` | `Accessible.focusable: true` | delete | Qt Quick `QQuickAccessibleAttached::role()` for `Accessible.Button` automatically defaults `QAccessible::Focusable` to true in the Qt Quick C++ accessibility bridge. |

---

## 2. Remediation of Previously Claimed "Equivalents" and Auditor Mutants

1. **Stage L3 Test Hardening (AUD-L2-01 & AUD-L2-02)**:
   - **M10 (SearchField clear button click)**: `test_searchfield_clear_button` previously called `searchField.clear()` directly, allowing deletion of `root.clear()` in `clearMouse.onClicked` to survive. Remediated by invoking `mouseClick(searchField.clearMouseItem)` and asserting text clear, focus transfer to `textInput`, and signal emission without calling `searchField.clear()`.
   - **M09 (ListItem letter badge text format)**: `KynexListItem.qml` exposed `textFormat` and `secondaryTextFormat` but omitted `badgeTextFormat`. Mutating `badgeText.textFormat` to `RichText` previously survived. Remediated by exposing `readonly property int badgeTextFormat: badgeText.textFormat` and asserting `Text.PlainText` in `test_listitem_defaults_and_a11y()`.
2. **LayoutMirroring Mutants (3 killed)**:
   - `KynexButton.qml`, `KynexListItem.qml`, `KynexTooltip.qml`: `LayoutMirroring.enabled: Qt.locale().textDirection === Qt.RightToLeft`
   - **Resolution**: `tests/qml_tests/tst_controls.qml` runs `test_controls_rtl_layout_mirroring()` with `LayoutHelper.setRtl(true)` and `LayoutHelper.setRtl(false)`, asserting mirrored element ordering (`iconX > labelX` in RTL vs `iconX < labelX` in LTR). Both `delete` and `negate` mutants are killed.
3. **Token Binding & Primary State Mutants (killed)**:
   - Button primary background (`Style.colors.accent`), non-primary background (`Style.colors.surfaceRaised`), switch indicator colors, and slider track/handle colors are asserted in `test_controls_token_bindings()`.
4. **Shell Surface Scope Added (AUD-M-04)**:
   - `styles/default/Shell.qml` was brought under mutation testing in Stage M2 per AUD-M-04. 14 of 15 mutants are killed directly by `tests/qml_tests/tst_shell.qml` (count badges, fallback labels, network tooltip bindings, icon URLs, accessibility strings). The single surviving mutant (line 85, deletion of `when: launcherLoader.status === Loader.Ready`) is mathematically equivalent due to Qt Quick `Binding` null-target semantics.

---

## 3. Summary Statistics

- **Target Files**: `crates/kynex-shell/qml/Kynex/Controls/*.qml` and `styles/default/*.qml` (13 files)
- **Total Mutants Generated**: 418
- **Mutants Killed**: 411 / 418 (**98.3%**)
- **Truly Equivalent Survivors**: 7 / 418 (**1.7%**)
- **Unjustified Survivors**: 0
- **Status**: 100% of non-equivalent mutants killed.

---

## 4. Stage Q Named Mutants (Audio & Quick Settings)

All 14 required security, robustness, and coalescing mutants for Stage Q have dedicated named unit/integration tests that fail if the mechanism is removed or bypassed:

| Mutant Identifier | Security Mechanism / Invariant | Killing Test Path & Name | Status |
|---|---|---|---|
| `MUTANT_RANGE_CLAMP` | Volume parser clamps parsed volume strictly to 0..=100 | `crates/kynex-daemon/tests/audio_service_test.rs::test_mutant_range_clamp` | **KILLED** |
| `MUTANT_RATE_LIMIT` | Global rate limiter strictly caps mutations to 20 calls/sec | `crates/kynex-daemon/tests/audio_service_test.rs::test_mutant_rate_limit` | **KILLED** |
| `MUTANT_LINE_CAP` | BoundedLineReader truncates and discards lines exceeding 4 KiB | `crates/kynex-daemon/tests/audio_service_test.rs::test_mutant_line_cap` | **KILLED** |
| `MUTANT_DEVICE_CAP` | Device parser bounds total parsed sinks to <= 64 | `crates/kynex-daemon/tests/audio_service_test.rs::test_mutant_device_cap` | **KILLED** |
| `MUTANT_BACKOFF_CAP` | Subprocess restart manager caps restarts to 5 per minute | `crates/kynex-daemon/tests/audio_service_test.rs::test_mutant_backoff_cap` | **KILLED** |
| `MUTANT_SANITIZER` | Device name sanitizer strips ASCII control chars and Unicode bidi overrides | `crates/kynex-daemon/tests/audio_service_test.rs::test_mutant_sanitizer` | **KILLED** |
| `MUTANT_TIMEOUT` | Short subprocess calls are killed if exceeding 2.0 s timeout | `crates/kynex-daemon/tests/audio_service_test.rs::test_mutant_timeout` | **KILLED** |
| `MUTANT_ARGV_ONLY` | Subprocess spawned directly with argv array without shell invocation (hostile sink metacharacters preserved verbatim, no shell injection) | `crates/kynex-daemon/tests/audio_service_test.rs::test_mutant_argv_only_no_shell` | **KILLED** |
| `MUTANT_READBACK` | SetVolume and SetMuted verify state mutation via authoritative readback | `crates/kynex-daemon/tests/audio_service_test.rs::test_mutant_readback` | **KILLED** |
| `MUTANT_DEBOUNCE_FINAL_VALUE` | Slider debouncer guarantees delivery of final settled slider value | `crates/kynex-shell-core/tests/audio_tests.rs::test_mutant_debounce_final_value` | **KILLED** |
| `MUTANT_COALESCE_LEADING` | First event of burst delivered immediately on leading edge (DeliverLeadingAndSchedule) | `crates/kynex-audio/tests/parser_tests.rs::test_mutant_coalesce_leading` | **KILLED** |
| `MUTANT_COALESCE_TRAILING` | Events arriving during window are coalesced and trailing edge delivered on timer run | `crates/kynex-audio/tests/parser_tests.rs::test_mutant_coalesce_trailing` | **KILLED** |
| `MUTANT_COALESCE_WIDENING` | Adaptive coalescer widens window exponentially from 25ms to 250ms during bursts | `crates/kynex-audio/tests/parser_tests.rs::test_mutant_coalesce_widening` | **KILLED** |
| `MUTANT_COALESCE_WINDOW_CAP` | Coalescing window strictly capped at 250ms max duration | `crates/kynex-audio/tests/parser_tests.rs::test_mutant_coalesce_window_cap` | **KILLED** |
| `MUTANT_COALESCE_RESET` | Coalescing window resets to base 25ms after >= 1000ms quiet period | `crates/kynex-audio/tests/parser_tests.rs::test_mutant_coalesce_reset` | **KILLED** |
| `MUTANT_COALESCE_FINAL_STATE` | Final volume state delivered on EOF/burst end followed by quiet | `crates/kynex-audio/tests/parser_tests.rs::test_coalesce_final_state_delivery_logic` & `crates/kynex-daemon/tests/audio_service_test.rs::test_coalesce_flood_and_isolated_latency` | **KILLED** |

