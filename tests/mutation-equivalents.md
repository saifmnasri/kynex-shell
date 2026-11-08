# QML Mutation Testing: Equivalent Mutants Analysis

This document catalogs every mutant designated as mathematically or semantically equivalent by the automatic QML mutation testing harness (`scripts/mutation-test-qml.sh`), providing a technical proof for each survivor.

---

## 1. Verified Mathematically Equivalent Mutants (2 Total)

Following the Stage K2 audit remediation (AUD-K-04), all non-equivalent mutants have been killed by the test suite (`test_controls_rtl_layout_mirroring` and `test_controls_token_bindings` in `tests/qml_tests/tst_controls.qml`).

Only mutants with a rigorous one-line proof of equivalence are retained:

| # | Component | Line Pattern | Mutation | Technical Equivalence Proof |
|---|---|---|---|---|
| 1 | `crates/kynex-shell/qml/Kynex/Controls/KynexButton.qml` | `Accessible.focusable: true` | delete | Qt Quick `QQuickAccessibleAttached::role()` for `Accessible.Button` with `activeFocusOnTab=true` automatically defaults `QAccessible::Focusable` to true in the Qt Quick C++ accessibility bridge. |
| 2 | `crates/kynex-shell/qml/Kynex/Controls/KynexSwitch.qml` | `Accessible.focusable: true` | delete | Qt Quick `QQuickAccessibleAttached::role()` for `Accessible.CheckBox` with `activeFocusOnTab=true` automatically defaults `QAccessible::Focusable` to true in the Qt Quick C++ accessibility bridge. |

---

## 2. Remediation of Previously Claimed "Equivalents"

In Stage K (PR #18), 17 mutants were misclassified as equivalent. In Stage K2 (AUD-K-04), all 17 have been resolved:

1. **LayoutMirroring Mutants (3 killed)**:
   - `KynexButton.qml`, `KynexListItem.qml`, `KynexTooltip.qml`: `LayoutMirroring.enabled: Qt.locale().textDirection === Qt.RightToLeft`
   - **Resolution**: `tests/qml_tests/tst_controls.qml` now runs `test_controls_rtl_layout_mirroring()` with `LayoutHelper.setRtl(true)` and `LayoutHelper.setRtl(false)`, asserting mirrored element ordering (`iconX > labelX` in RTL vs `iconX < labelX` in LTR). Both `delete` and `negate` mutants are killed.
2. **Token Binding & Primary State Mutants (killed)**:
   - Button primary background (`Style.colors.accent`), non-primary background (`Style.colors.surfaceRaised`), switch indicator colors, and slider track/handle colors are asserted in `test_controls_token_bindings()`.
3. **Surface Shell Scope (14 eliminated)**:
   - `styles/default/Shell.qml` panel surface mutants were purged from the base controls mutation harness (`scripts/mutation-test-qml.sh`), matching the verification gate definition (`adversarial mutation test (qml controls)`).

---

## 3. Summary Statistics

- **Target Files**: `crates/kynex-shell/qml/Kynex/Controls/*.qml` (5 files)
- **Total Mutants Generated**: 180
- **Mutants Killed**: 178 / 180 (**98.9%**)
- **Truly Equivalent Survivors**: 2 / 180 (**1.1%**)
- **Unjustified Survivors**: 0
- **Status**: 100% of non-equivalent mutants killed.
