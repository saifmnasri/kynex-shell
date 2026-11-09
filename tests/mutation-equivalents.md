# QML Mutation Testing: Equivalent Mutants Analysis

This document catalogs every mutant designated as mathematically or semantically equivalent by the automatic QML mutation testing harness (`scripts/mutation-test-qml.sh`), providing a technical proof for each survivor.

---

## 1. Verified Mathematically Equivalent Mutants (3 Total)

Following Stage K2 (AUD-K-04), Stage L2 (AUD-L-01), and Stage L3 (AUD-L2-01, AUD-L2-02) remediations, all non-equivalent mutants have been killed by the test suite (`test_controls_rtl_layout_mirroring`, `test_controls_token_bindings`, `test_searchfield_clear_button`, `test_listitem_defaults_and_a11y`, and `tst_launcher.qml`).

Only mutants with a rigorous one-line proof of equivalence are retained:

| # | Component | Line Pattern | Mutation | Technical Equivalence Proof |
|---|---|---|---|---|
| 1 | `crates/kynex-shell/qml/Kynex/Controls/KynexButton.qml` | `Accessible.focusable: true` | delete | Qt Quick `QQuickAccessibleAttached::role()` for `Accessible.Button` with `activeFocusOnTab=true` automatically defaults `QAccessible::Focusable` to true in the Qt Quick C++ accessibility bridge. |
| 2 | `crates/kynex-shell/qml/Kynex/Controls/KynexSwitch.qml` | `Accessible.focusable: true` | delete | Qt Quick `QQuickAccessibleAttached::role()` for `Accessible.CheckBox` with `activeFocusOnTab=true` automatically defaults `QAccessible::Focusable` to true in the Qt Quick C++ accessibility bridge. |
| 3 | `crates/kynex-shell/qml/Kynex/Controls/KynexSearchField.qml` | `Accessible.focusable: true` | delete | Qt Quick `QQuickAccessibleAttached::role()` for `Accessible.EditableText` with `activeFocusOnTab=true` automatically defaults `QAccessible::Focusable` to true in the Qt Quick C++ accessibility bridge. |

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
4. **Surface Shell Scope (14 eliminated)**:
   - `styles/default/Shell.qml` panel surface mutants were purged from the base controls mutation harness (`scripts/mutation-test-qml.sh`), matching the verification gate definition (`adversarial mutation test (qml controls)`).

---

## 3. Summary Statistics

- **Target Files**: `crates/kynex-shell/qml/Kynex/Controls/*.qml` and `styles/default/Launcher.qml` (7 files)
- **Total Mutants Generated**: 230
- **Mutants Killed**: 227 / 230 (**98.7%**)
- **Truly Equivalent Survivors**: 3 / 230 (**1.3%**)
- **Unjustified Survivors**: 0
- **Status**: 100% of non-equivalent mutants killed.
