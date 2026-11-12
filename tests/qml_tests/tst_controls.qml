import QtQuick
import QtTest
import Kynex.Controls
import Kynex.Style
import Kynex.Shell

Item {
    id: root
    width: 800
    height: 800

    Column {
        anchors.fill: parent
        spacing: 20

        KynexButton {
            id: button
            text: "Submit Action"
        }

        KynexSwitch {
            id: toggleSwitch
            text: "Enable Option"
        }

        KynexSlider {
            id: slider
            value: 0.5
        }

        KynexTooltip {
            id: tooltip
            text: "Helpful hint"
            visible: false
        }

        KynexListItem {
            id: listItem
            text: "Primary item"
            secondaryText: "Detailed explanation"
        }

        KynexSearchField {
            id: searchField
            placeholderText: "Find programs..."
        }

        KynexMenuItem {
            id: menuItem
            text: "Menu Action"
            shortcutText: "Ctrl+M"
        }

        TrayMenuModel {
            id: mockMenuModel
        }

        KynexMenu {
            id: testMenu
            model: mockMenuModel
        }
    }

    SignalSpy {
        id: searchFieldAcceptedSpy
        target: searchField
        signalName: "accepted"
    }

    SignalSpy {
        id: searchFieldClearedSpy
        target: searchField
        signalName: "cleared"
    }

    SignalSpy {
        id: searchFieldTextEditedSpy
        target: searchField
        signalName: "textEdited"
    }

    SignalSpy {
        id: buttonClickedSpy
        target: button
        signalName: "clicked"
    }

    SignalSpy {
        id: buttonMiddleClickedSpy
        target: button
        signalName: "middleClicked"
    }

    SignalSpy {
        id: buttonPressedSpy
        target: button
        signalName: "pressed"
    }

    SignalSpy {
        id: buttonReleasedSpy
        target: button
        signalName: "released"
    }

    SignalSpy {
        id: switchToggledSpy
        target: toggleSwitch
        signalName: "toggled"
    }

    SignalSpy {
        id: sliderMovedSpy
        target: slider
        signalName: "moved"
    }

    SignalSpy {
        id: listItemClickedSpy
        target: listItem
        signalName: "clicked"
    }

    SignalSpy {
        id: listItemPressedSpy
        target: listItem
        signalName: "pressed"
    }

    SignalSpy {
        id: listItemReleasedSpy
        target: listItem
        signalName: "released"
    }

    SignalSpy {
        id: menuItemClickedSpy
        target: menuItem
        signalName: "clicked"
    }

    SignalSpy {
        id: menuItemPressedSpy
        target: menuItem
        signalName: "pressed"
    }

    SignalSpy {
        id: menuItemReleasedSpy
        target: menuItem
        signalName: "released"
    }

    TestCase {
        id: testCase
        name: "KynexControlsTests"
        when: windowShown

        readonly property var window: root.Window ? root.Window.window : null

        function waitFocus(item) {
            if (window) {
                if (!window.active) {
                    window.requestActivate();
                }
                tryVerify(() => window.active);
            }
            item.forceActiveFocus();
            if (item.enabled) {
                tryVerify(() => item.activeFocus);
            }
        }

        function init() {
            LayoutHelper.setRtl(false);
            root.forceActiveFocus();
            button.enabled = true;
            button.primary = false;
            buttonClickedSpy.clear();
            buttonPressedSpy.clear();
            buttonReleasedSpy.clear();
            buttonMiddleClickedSpy.clear();

            toggleSwitch.enabled = true;
            toggleSwitch.checked = false;
            switchToggledSpy.clear();

            slider.enabled = true;
            slider.value = 0.5;
            sliderMovedSpy.clear();

            tooltip.visible = false;

            listItem.enabled = true;
            listItem.selected = false;
            listItemClickedSpy.clear();
            listItemPressedSpy.clear();
            listItemReleasedSpy.clear();
        }

        // --- KynexButton Tests ---

        function test_button_defaults_and_a11y() {
            compare(button.text, "Submit Action");
            compare(button.Accessible.role, Accessible.Button);
            compare(button.Accessible.name, "Submit Action");
            verify(button.Accessible.focusable);
            verify(button.activeFocusOnTab);
            verify(button.LayoutMirroring.childrenInherit);
            compare(button.LayoutMirroring.enabled, Qt.locale().textDirection === Qt.RightToLeft);
            compare(button.opacity, 1.0);

            button.Accessible.pressAction();
            compare(buttonClickedSpy.count, 1);

            button.text = "";
            verify(!button.labelVisible);
            button.text = "Submit Action";
            verify(button.labelVisible);
        }

        function test_button_icon_and_letter_badge() {
            verify(!button.iconVisible);
            button.letterBadge = "B";
            verify(button.badgeVisible);
            let testIcon = Qt.resolvedUrl("test_icon.png");
            button.iconSource = testIcon;
            tryVerify(function() { return button.iconVisible; });
            compare(button.iconWidth, Style.typography.basePx + Style.spacing.unit);
            compare(button.iconHeight, Style.typography.basePx + Style.spacing.unit);
            verify(!button.badgeVisible);
            button.iconSource = "";
            verify(!button.iconVisible);
            verify(button.badgeVisible);
            button.letterBadge = "";
            verify(!button.badgeVisible);
        }

        function test_button_window_count_badge() {
            compare(button.windowCount, 0);
            verify(!button.countBadgeVisible);

            button.windowCount = 1;
            verify(!button.countBadgeVisible);

            button.windowCount = 2;
            verify(button.countBadgeVisible);
            compare(button.countBadgeText, "2");

            button.windowCount = 5;
            verify(button.countBadgeVisible);
            compare(button.countBadgeText, "5");

            button.windowCount = 0;
            verify(!button.countBadgeVisible);
        }

        function test_button_mouse_click() {
            mouseClick(button, button.width / 2, button.height / 2, Qt.LeftButton);
            compare(buttonClickedSpy.count, 1);
            compare(buttonPressedSpy.count, 1);
            compare(buttonReleasedSpy.count, 1);
            compare(buttonMiddleClickedSpy.count, 0);

            mouseClick(button, button.width / 2, button.height / 2, Qt.MiddleButton);
            compare(buttonClickedSpy.count, 1);
            compare(buttonPressedSpy.count, 1);
            compare(buttonReleasedSpy.count, 1);
            compare(buttonMiddleClickedSpy.count, 1);
        }

        function test_button_keyboard_space() {
            waitFocus(button);

            keyPress(Qt.Key_Space);
            compare(buttonPressedSpy.count, 1);
            verify(button.down);

            keyRelease(Qt.Key_Space);
            compare(buttonClickedSpy.count, 1);
            verify(!button.down);
        }

        function test_button_keyboard_return() {
            waitFocus(button);

            keyClick(Qt.Key_Return);
            compare(buttonClickedSpy.count, 1);

            waitFocus(button);
            keyClick(Qt.Key_Enter);
            compare(buttonClickedSpy.count, 2);
        }

        function test_button_focus_ring() {
            compare(button.borderWidth, 0);
            verify(button.borderColor !== Style.colors.focusRing);

            waitFocus(button);
            compare(button.borderColor, Style.colors.focusRing);
            compare(button.borderWidth, 1);
        }

        // --- KynexSwitch Tests ---

        function test_switch_defaults_and_a11y() {
            compare(toggleSwitch.text, "Enable Option");
            compare(toggleSwitch.checked, false);
            compare(toggleSwitch.Accessible.role, Accessible.CheckBox);
            compare(toggleSwitch.Accessible.checked, false);
            compare(toggleSwitch.Accessible.name, "Enable Option");
            verify(toggleSwitch.Accessible.focusable);
            verify(toggleSwitch.activeFocusOnTab);
            verify(toggleSwitch.LayoutMirroring.childrenInherit);
            compare(toggleSwitch.LayoutMirroring.enabled, Qt.locale().textDirection === Qt.RightToLeft);

            toggleSwitch.Accessible.pressAction();
            compare(toggleSwitch.checked, true);
            toggleSwitch.checked = false;

            toggleSwitch.text = "";
            verify(!toggleSwitch.labelVisible);
            toggleSwitch.text = "Enable Option";
            verify(toggleSwitch.labelVisible);
        }

        function test_switch_mouse_toggle() {
            mouseClick(toggleSwitch);
            compare(toggleSwitch.checked, true);
            compare(switchToggledSpy.count, 1);
            compare(toggleSwitch.Accessible.checked, true);

            mouseClick(toggleSwitch);
            compare(toggleSwitch.checked, false);
            compare(switchToggledSpy.count, 2);
            compare(toggleSwitch.Accessible.checked, false);
        }

        function test_switch_keyboard_toggle() {
            waitFocus(toggleSwitch);

            keyClick(Qt.Key_Space);
            compare(toggleSwitch.checked, true);
            compare(switchToggledSpy.count, 1);

            waitFocus(toggleSwitch);
            keyClick(Qt.Key_Return);
            compare(toggleSwitch.checked, false);
            compare(switchToggledSpy.count, 2);

            waitFocus(toggleSwitch);
            keyClick(Qt.Key_Enter);
            compare(toggleSwitch.checked, true);
            compare(switchToggledSpy.count, 3);

            toggleSwitch.checked = false;
            waitFocus(toggleSwitch);
            keyPress(Qt.Key_Space);
            verify(toggleSwitch.down);
            keyRelease(Qt.Key_Space);
            verify(!toggleSwitch.down);
        }

        function test_switch_focus_ring() {
            compare(toggleSwitch.borderWidth, 0);
            verify(toggleSwitch.borderColor !== Style.colors.focusRing);

            waitFocus(toggleSwitch);
            compare(toggleSwitch.borderColor, Style.colors.focusRing);
            compare(toggleSwitch.borderWidth, 1);
        }

        // --- KynexSlider Tests ---

        function test_slider_defaults_and_a11y() {
            compare(slider.value, 0.5);
            compare(slider.Accessible.role, Accessible.Slider);
            compare(slider.Accessible.name, "Slider");
            verify(slider.Accessible.focusable);
            verify(slider.activeFocusOnTab);
            verify(slider.LayoutMirroring.childrenInherit);
            compare(slider.LayoutMirroring.enabled, Qt.locale().textDirection === Qt.RightToLeft);

            slider.value = 0.5;
            slider.Accessible.increaseAction();
            verify(slider.value > 0.5);
            slider.Accessible.decreaseAction();
            compare(slider.value, 0.5);
        }

        function test_slider_keyboard_navigation() {
            waitFocus(slider);

            slider.value = 0.5;
            let initial = slider.value;
            keyClick(Qt.Key_Right);
            verify(slider.value > initial);
            compare(sliderMovedSpy.count, 1);

            let higher = slider.value;
            waitFocus(slider);
            keyClick(Qt.Key_Left);
            compare(slider.value, initial);
            compare(sliderMovedSpy.count, 2);

            let valBeforeUp = slider.value;
            waitFocus(slider);
            keyClick(Qt.Key_Up);
            verify(slider.value > valBeforeUp);

            let valBeforeDown = slider.value;
            waitFocus(slider);
            keyClick(Qt.Key_Down);
            verify(slider.value < valBeforeDown);

            slider.value = 0.5;
            let valBeforePageUp = slider.value;
            waitFocus(slider);
            keyClick(Qt.Key_PageUp);
            verify(slider.value > valBeforePageUp);

            let valBeforePageDown = slider.value;
            waitFocus(slider);
            keyClick(Qt.Key_PageDown);
            verify(slider.value < valBeforePageDown);

            waitFocus(slider);
            keyClick(Qt.Key_Home);
            compare(slider.value, 0.0);

            waitFocus(slider);
            keyClick(Qt.Key_End);
            compare(slider.value, 1.0);
        }

        function test_slider_mouse_and_wheel() {
            slider.value = 0.5;
            sliderMovedSpy.clear();

            mouseWheel(slider, slider.width / 2, slider.height / 2, 0, 120);
            verify(slider.value > 0.5);

            mouseWheel(slider, slider.width / 2, slider.height / 2, 0, -120);
            compare(slider.value, 0.5);

            mouseClick(slider, slider.width * 0.9, slider.height / 2);
            verify(slider.value > 0.7);

            mouseClick(slider, slider.width * 0.1, slider.height / 2);
            verify(slider.value < 0.3);
        }

        function test_slider_clamping() {
            slider.setValueClamped(1.5);
            compare(slider.value, 1.0);

            slider.setValueClamped(-0.5);
            compare(slider.value, 0.0);
        }

        function test_slider_focus_ring() {
            compare(slider.borderWidth, 1);
            compare(slider.borderColor, Style.colors.accent);

            waitFocus(slider);
            compare(slider.borderColor, Style.colors.focusRing);
            compare(slider.borderWidth, 1);
        }

        // --- KynexTooltip Tests ---

        function test_tooltip_properties_and_visibility() {
            compare(tooltip.text, "Helpful hint");
            compare(tooltip.Accessible.role, Accessible.ToolTip);
            compare(tooltip.Accessible.name, "Helpful hint");
            verify(tooltip.LayoutMirroring.childrenInherit);
            compare(tooltip.LayoutMirroring.enabled, Qt.locale().textDirection === Qt.RightToLeft);
            compare(tooltip.textFormat, Text.PlainText);
            compare(tooltip.opacity, 0.0);

            tooltip.visible = true;
            tryCompare(tooltip, "opacity", 1.0);
        }

        // --- KynexListItem Tests ---

        function test_listitem_defaults_and_a11y() {
            compare(listItem.text, "Primary item");
            compare(listItem.secondaryText, "Detailed explanation");
            compare(listItem.selected, false);
            compare(listItem.Accessible.role, Accessible.ListItem);
            compare(listItem.Accessible.name, "Primary item - Detailed explanation");
            verify(listItem.Accessible.focusable);
            compare(listItem.textFormat, Text.PlainText);
            compare(listItem.secondaryTextFormat, Text.PlainText);
            compare(listItem.badgeTextFormat, Text.PlainText);
            verify(listItem.activeFocusOnTab);
            verify(listItem.LayoutMirroring.childrenInherit);
            compare(listItem.LayoutMirroring.enabled, Qt.locale().textDirection === Qt.RightToLeft);

            listItem.Accessible.pressAction();
            compare(listItemClickedSpy.count, 1);

            listItem.text = "";
            verify(!listItem.labelVisible);
            listItem.text = "Primary item";
            verify(listItem.labelVisible);

            listItem.secondaryText = "";
            verify(!listItem.secondaryLabelVisible);
            listItem.secondaryText = "Detailed explanation";
            verify(listItem.secondaryLabelVisible);

            verify(!listItem.iconVisible);
            listItem.letterBadge = "K";
            verify(listItem.badgeVisible);
            let testIcon = Qt.resolvedUrl("test_icon.png");
            listItem.iconSource = testIcon;
            tryVerify(function() { return listItem.iconVisible; });
            compare(listItem.iconWidth, Style.typography.basePx + Style.spacing.unit);
            compare(listItem.iconHeight, Style.typography.basePx + Style.spacing.unit);
            verify(!listItem.badgeVisible);
            listItem.iconSource = "";
            verify(!listItem.iconVisible);
            verify(listItem.badgeVisible);
            listItem.letterBadge = "";
            verify(!listItem.badgeVisible);
        }

        function test_listitem_mouse_click() {
            mouseClick(listItem);
            compare(listItemClickedSpy.count, 1);
        }

        function test_listitem_keyboard_activation() {
            waitFocus(listItem);

            keyClick(Qt.Key_Return);
            compare(listItemClickedSpy.count, 1);

            waitFocus(listItem);
            keyClick(Qt.Key_Enter);
            compare(listItemClickedSpy.count, 2);

            waitFocus(listItem);
            keyClick(Qt.Key_Space);
            compare(listItemClickedSpy.count, 3);

            waitFocus(listItem);
            keyPress(Qt.Key_Space);
            verify(listItem.down);
            keyRelease(Qt.Key_Space);
            verify(!listItem.down);
        }

        function test_listitem_selection() {
            listItem.selected = true;
            compare(listItem.selected, true);
            listItem.selected = false;
            compare(listItem.selected, false);
        }

        function test_listitem_focus_ring() {
            compare(listItem.borderWidth, 0);
            verify(listItem.borderColor !== Style.colors.focusRing);

            waitFocus(listItem);
            compare(listItem.borderColor, Style.colors.focusRing);
            compare(listItem.borderWidth, 1);
        }

        function test_searchfield_defaults_and_a11y() {
            compare(searchField.text, "");
            compare(searchField.placeholderText, "Find programs...");
            compare(searchField.Accessible.role, Accessible.EditableText);
            compare(searchField.Accessible.name, "Find programs...");
            verify(searchField.Accessible.focusable);
            verify(searchField.activeFocusOnTab);
            verify(!searchField.hasText);
            verify(!searchField.clearButtonVisible);
            verify(searchField.LayoutMirroring.childrenInherit);
            compare(searchField.LayoutMirroring.enabled, Qt.locale().textDirection === Qt.RightToLeft);

            searchField.text = "sample";
            compare(searchField.Accessible.description, "sample");
            searchField.text = "";
            compare(searchField.clearAccessibleRole, Accessible.Button);
            compare(searchField.clearAccessibleName, "Clear search");
        }

        function test_searchfield_typing_and_editing() {
            searchFieldAcceptedSpy.clear();
            searchFieldTextEditedSpy.clear();

            searchField.forceFieldFocus();
            verify(searchField.activeFocus);

            keyClick(Qt.Key_K);
            keyClick(Qt.Key_A);
            keyClick(Qt.Key_T);
            keyClick(Qt.Key_E);
            compare(searchField.text, "kate");
            verify(searchField.hasText);
            verify(searchField.clearButtonVisible);
            verify(searchFieldTextEditedSpy.count > 0);

            keyClick(Qt.Key_Return);
            compare(searchFieldAcceptedSpy.count, 1);

            keyClick(Qt.Key_Enter);
            compare(searchFieldAcceptedSpy.count, 2);
        }

        function test_searchfield_clear_button() {
            button.forceActiveFocus();
            verify(!searchField.inputActiveFocus);

            searchField.text = "something";
            tryVerify(function() {
                return searchField.hasText && searchField.clearButtonVisible && searchField.clearButtonItem.x > 0;
            }, 5000);

            searchFieldClearedSpy.clear();
            mouseClick(searchField.clearMouseItem);
            tryCompare(searchField, "text", "", 5000);
            tryVerify(function() {
                return !searchField.hasText && !searchField.clearButtonVisible && searchField.inputActiveFocus;
            }, 5000);
            compare(searchFieldClearedSpy.count, 1);
        }

        function test_searchfield_focus_ring() {
            searchField.focus = false;
            button.forceActiveFocus();
            tryVerify(function() {
                return searchField.borderWidth === 0 && searchField.borderColor !== Style.colors.focusRing;
            }, 5000);

            searchField.forceFieldFocus();
            tryVerify(function() {
                return searchField.borderColor === Style.colors.focusRing && searchField.borderWidth === 1;
            }, 5000);
        }

        function send_all_inputs(item) {
            waitFocus(item);
            let keys = [
                Qt.Key_Right,
                Qt.Key_Left,
                Qt.Key_Up,
                Qt.Key_Down,
                Qt.Key_Home,
                Qt.Key_End,
                Qt.Key_PageUp,
                Qt.Key_PageDown,
                Qt.Key_Space,
                Qt.Key_Return,
                Qt.Key_Enter
            ];
            for (let i = 0; i < keys.length; ++i) {
                waitFocus(item);
                keyClick(keys[i]);
            }

            let cx = item.width / 2;
            let cy = item.height / 2;
            mousePress(item, cx, cy);
            mouseRelease(item, cx, cy);
            mouseClick(item, cx, cy);

            mouseDrag(item, item.width / 4, cy, item.width / 2, 0);

            mouseWheel(item, cx, cy, 0, 120);
            mouseWheel(item, cx, cy, 0, -120);
        }

        function test_generic_disabled_contract() {
            // 1. KynexSlider: start from mid-range 0.5
            slider.enabled = true;
            slider.value = 0.5;
            sliderMovedSpy.clear();

            slider.enabled = false;
            compare(slider.opacity, Style.opacity.disabled);
            compare(slider.value, 0.5);
            compare(slider.cursorShape, Qt.ArrowCursor);

            send_all_inputs(slider);
            compare(slider.value, 0.5);
            compare(sliderMovedSpy.count, 0);

            // Programmatic calls while disabled
            slider.decrease();
            compare(slider.value, 0.5);
            compare(sliderMovedSpy.count, 0);

            slider.increase();
            compare(slider.value, 0.5);
            compare(sliderMovedSpy.count, 0);

            slider.toMinimum();
            compare(slider.value, 0.5);
            compare(sliderMovedSpy.count, 0);

            slider.toMaximum();
            compare(slider.value, 0.5);
            compare(sliderMovedSpy.count, 0);

            slider.setInteractiveRatio(0.1);
            compare(slider.value, 0.5);
            compare(sliderMovedSpy.count, 0);

            // Accessible actions while disabled
            slider.Accessible.decreaseAction();
            compare(slider.value, 0.5);
            compare(sliderMovedSpy.count, 0);

            slider.Accessible.increaseAction();
            compare(slider.value, 0.5);
            compare(sliderMovedSpy.count, 0);

            // Positive control: when enabled, inputs and methods alter value and emit signals
            slider.enabled = true;
            compare(slider.cursorShape, Qt.PointingHandCursor);
            send_all_inputs(slider);
            verify(sliderMovedSpy.count > 0);

            sliderMovedSpy.clear();
            slider.value = 0.5;
            slider.decrease();
            verify(slider.value < 0.5);
            compare(sliderMovedSpy.count, 1);

            sliderMovedSpy.clear();
            slider.increase();
            compare(slider.value, 0.5);
            compare(sliderMovedSpy.count, 1);

            // 2. KynexSwitch: state 1 (checked = false)
            toggleSwitch.enabled = true;
            toggleSwitch.checked = false;
            switchToggledSpy.clear();

            toggleSwitch.enabled = false;
            compare(toggleSwitch.opacity, Style.opacity.disabled);
            compare(toggleSwitch.checked, false);
            compare(toggleSwitch.cursorShape, Qt.ArrowCursor);

            send_all_inputs(toggleSwitch);
            compare(toggleSwitch.checked, false);
            compare(switchToggledSpy.count, 0);

            // Programmatic call while disabled
            toggleSwitch.toggle();
            compare(toggleSwitch.checked, false);
            compare(switchToggledSpy.count, 0);

            // Accessible action while disabled
            toggleSwitch.Accessible.pressAction();
            compare(toggleSwitch.checked, false);
            compare(switchToggledSpy.count, 0);

            // Positive control
            toggleSwitch.enabled = true;
            compare(toggleSwitch.cursorShape, Qt.PointingHandCursor);
            send_all_inputs(toggleSwitch);
            verify(switchToggledSpy.count > 0);

            switchToggledSpy.clear();
            toggleSwitch.checked = false;
            toggleSwitch.toggle();
            compare(toggleSwitch.checked, true);
            compare(switchToggledSpy.count, 1);

            // 3. KynexSwitch: state 2 (checked = true)
            toggleSwitch.enabled = true;
            toggleSwitch.checked = true;
            switchToggledSpy.clear();

            toggleSwitch.enabled = false;
            compare(toggleSwitch.opacity, Style.opacity.disabled);
            compare(toggleSwitch.checked, true);

            send_all_inputs(toggleSwitch);
            compare(toggleSwitch.checked, true);
            compare(switchToggledSpy.count, 0);

            toggleSwitch.toggle();
            compare(toggleSwitch.checked, true);
            compare(switchToggledSpy.count, 0);

            toggleSwitch.Accessible.pressAction();
            compare(toggleSwitch.checked, true);
            compare(switchToggledSpy.count, 0);

            // 4. KynexButton
            button.enabled = true;
            buttonClickedSpy.clear();
            buttonPressedSpy.clear();
            buttonReleasedSpy.clear();
            buttonMiddleClickedSpy.clear();

            button.enabled = false;
            compare(button.opacity, Style.opacity.disabled);
            compare(button.cursorShape, Qt.ArrowCursor);

            send_all_inputs(button);
            compare(buttonClickedSpy.count, 0);
            compare(buttonPressedSpy.count, 0);
            compare(buttonReleasedSpy.count, 0);
            compare(buttonMiddleClickedSpy.count, 0);
            verify(!button.down);

            // Programmatic calls while disabled
            button.click();
            button.middleClick();
            button.press();
            button.release();
            compare(buttonClickedSpy.count, 0);
            compare(buttonPressedSpy.count, 0);
            compare(buttonReleasedSpy.count, 0);
            compare(buttonMiddleClickedSpy.count, 0);

            // Accessible action while disabled
            button.Accessible.pressAction();
            compare(buttonClickedSpy.count, 0);

            // Positive control
            button.enabled = true;
            compare(button.cursorShape, Qt.PointingHandCursor);
            send_all_inputs(button);
            verify(buttonClickedSpy.count > 0);

            buttonClickedSpy.clear();
            button.click();
            compare(buttonClickedSpy.count, 1);

            // 5. KynexListItem: state 1 (selected = false)
            listItem.enabled = true;
            listItem.selected = false;
            listItemClickedSpy.clear();
            listItemPressedSpy.clear();
            listItemReleasedSpy.clear();

            listItem.enabled = false;
            compare(listItem.opacity, Style.opacity.disabled);
            compare(listItem.selected, false);
            compare(listItem.cursorShape, Qt.ArrowCursor);

            send_all_inputs(listItem);
            compare(listItem.selected, false);
            compare(listItemClickedSpy.count, 0);
            compare(listItemPressedSpy.count, 0);
            compare(listItemReleasedSpy.count, 0);

            // Programmatic calls while disabled
            listItem.click();
            listItem.press();
            listItem.release();
            compare(listItemClickedSpy.count, 0);
            compare(listItemPressedSpy.count, 0);
            compare(listItemReleasedSpy.count, 0);

            // Accessible action while disabled
            listItem.Accessible.pressAction();
            compare(listItemClickedSpy.count, 0);

            // Positive control
            listItem.enabled = true;
            compare(listItem.cursorShape, Qt.PointingHandCursor);
            send_all_inputs(listItem);
            verify(listItemClickedSpy.count > 0);

            listItemClickedSpy.clear();
            listItem.click();
            compare(listItemClickedSpy.count, 1);

            // 6. KynexListItem: state 2 (selected = true)
            listItem.enabled = true;
            listItem.selected = true;
            listItemClickedSpy.clear();
            listItemPressedSpy.clear();
            listItemReleasedSpy.clear();

            listItem.enabled = false;
            compare(listItem.opacity, Style.opacity.disabled);
            compare(listItem.selected, true);

            send_all_inputs(listItem);
            compare(listItem.selected, true);
            compare(listItemClickedSpy.count, 0);
            compare(listItemPressedSpy.count, 0);
            compare(listItemReleasedSpy.count, 0);

            listItem.click();
            compare(listItem.selected, true);
            compare(listItemClickedSpy.count, 0);

            listItem.Accessible.pressAction();
            compare(listItem.selected, true);
            compare(listItemClickedSpy.count, 0);

            // 7. KynexSearchField
            searchField.enabled = true;
            searchField.text = "query text";
            searchFieldAcceptedSpy.clear();
            searchFieldClearedSpy.clear();
            searchFieldTextEditedSpy.clear();

            searchField.enabled = false;
            compare(searchField.opacity, Style.opacity.disabled);
            verify(!searchField.clearButtonVisible);
            verify(!searchField.inputEnabled);

            // Programmatic calls while disabled
            searchField.clear();
            compare(searchField.text, "query text");
            compare(searchFieldClearedSpy.count, 0);

            searchField.enabled = true;
            verify(searchField.inputEnabled);
            searchField.clear();

            // 8. KynexMenuItem
            menuItem.enabled = true;
            menuItemClickedSpy.clear();
            menuItemPressedSpy.clear();
            menuItemReleasedSpy.clear();

            menuItem.enabled = false;
            compare(menuItem.opacity, Style.opacity.disabled);
            compare(menuItem.cursorShape, Qt.ArrowCursor);

            send_all_inputs(menuItem);
            compare(menuItemClickedSpy.count, 0);
            compare(menuItemPressedSpy.count, 0);
            compare(menuItemReleasedSpy.count, 0);

            // Programmatic calls while disabled
            menuItem.click();
            menuItem.press();
            menuItem.release();
            compare(menuItemClickedSpy.count, 0);
            compare(menuItemPressedSpy.count, 0);
            compare(menuItemReleasedSpy.count, 0);

            // Accessible action while disabled
            menuItem.Accessible.pressAction();
            compare(menuItemClickedSpy.count, 0);

            // Positive control
            menuItem.enabled = true;
            menuItem.click();
            compare(menuItemClickedSpy.count, 1);
            menuItemClickedSpy.clear();
        }

        function test_menu_item_properties_and_features(): void {
            // Checkmark toggle
            menuItem.toggleType = 1;
            menuItem.toggleState = 0;
            compare(menuItem.checkmarkText, "");
            menuItem.toggleState = 1;
            compare(menuItem.checkmarkText, "✓");

            // Radio toggle
            menuItem.toggleType = 2;
            menuItem.toggleState = 0;
            compare(menuItem.checkmarkText, "○");
            menuItem.toggleState = 1;
            compare(menuItem.checkmarkText, "●");

            // Submenu indicator
            menuItem.hasSubmenu = true;
            compare(menuItem.submenuArrowText, "▶");

            // Dispositions
            menuItem.disposition = 0;
            compare(menuItem.textColor, Style.colors.text);
            menuItem.disposition = 1;
            compare(menuItem.textColor, Style.colors.textMuted);
            menuItem.disposition = 2;
            compare(menuItem.textColor, Style.colors.warning);
            menuItem.disposition = 3;
            compare(menuItem.textColor, Style.colors.danger);

            // Reset
            menuItem.toggleType = 0;
            menuItem.toggleState = 0;
            menuItem.hasSubmenu = false;
            menuItem.disposition = 0;

            // Accessible name and focusable
            compare(menuItem.Accessible.name, "Menu Action");
            compare(menuItem.activeFocusOnTab, true);
            compare(menuItem.Accessible.focusable, true);
            compare(menuItem.textFormat, Text.PlainText);

            menuItem.enabled = false;
            compare(menuItem.Accessible.focusable, false);
            menuItem.enabled = true;

            menuItem.kind = 1;
            compare(menuItem.activeFocusOnTab, false);
            compare(menuItem.Accessible.focusable, false);
            compare(menuItem.cursorShape, Qt.ArrowCursor);
            menuItem.kind = 0;
            compare(menuItem.cursorShape, Qt.PointingHandCursor);

            // Accessible pressAction
            menuItemClickedSpy.clear();
            menuItem.Accessible.pressAction();
            compare(menuItemClickedSpy.count, 1);
            menuItemClickedSpy.clear();

            // Down state tracking (driven by keyboard activeFocus + spaceDown)
            compare(menuItem.down, false);
            waitFocus(menuItem);
            menuItem.spaceDown = true;
            compare(menuItem.down, true);
            menuItem.spaceDown = false;
            compare(menuItem.down, false);

            menuItem.enabled = false;
            menuItem.spaceDown = true;
            compare(menuItem.down, false);
            menuItem.spaceDown = false;
            menuItem.enabled = true;
            root.forceActiveFocus();

            // Border color & width states
            menuItem.selected = false;
            root.forceActiveFocus();
            tryVerify(function() {
                return menuItem.borderWidth === 0 && menuItem.borderColor === Style.colors.surface;
            }, 5000);

            menuItem.selected = true;
            tryVerify(function() {
                return menuItem.borderWidth === 1 && menuItem.borderColor === Style.colors.accent;
            }, 5000);

            menuItem.selected = false;
            menuItem.forceActiveFocus();
            tryVerify(function() {
                return menuItem.borderWidth === 1 && menuItem.borderColor === Style.colors.focusRing;
            }, 5000);
            root.forceActiveFocus();

            // Icon visibility
            menuItem.iconSource = "";
            verify(!menuItem.iconVisible);
            let testIcon = Qt.resolvedUrl("test_icon.png");
            menuItem.iconSource = testIcon;
            tryVerify(function() { return menuItem.iconVisible; });
            menuItem.iconSource = "";
            tryVerify(function() { return !menuItem.iconVisible; });
        }

        function test_menu_separator(): void {
            let sep = Qt.createQmlObject(
                "import QtQuick; import Kynex.Controls; KynexMenuItem { kind: 1 }",
                root
            );
            verify(sep.isSeparator);
            compare(sep.Accessible.role, Accessible.Separator);
            compare(sep.cursorShape, Qt.ArrowCursor);
            verify(!sep.activeFocusOnTab);
            verify(!sep.Accessible.focusable);
            sep.click();
            sep.destroy();
        }

        function test_menu_container_and_model(): void {
            compare(testMenu.Accessible.role, Accessible.PopupMenu);
            compare(testMenu.Accessible.name, "Menu");
            compare(testMenu.LayoutMirroring.childrenInherit, true);
            compare(testMenu.LayoutMirroring.enabled, Qt.locale().textDirection === Qt.RightToLeft);

            testMenu.enabled = false;
            compare(testMenu.opacity, Style.opacity.disabled);
            testMenu.enabled = true;
            compare(testMenu.opacity, 1.0);

            mockMenuModel.clear();
            compare(testMenu.count, 0);

            mockMenuModel.addItem(1, "Open", "Ctrl+O", 0, 0, 0, 0, false, "document-open", true);
            mockMenuModel.addItem(2, "Disabled", "", 0, 0, 0, 0, false, "", false);
            tryVerify(function() { return testMenu.count === 2; });
            tryVerify(function() { return testMenu.itemAt(0) !== null && testMenu.itemAt(1) !== null; });

            let d0 = testMenu.itemAt(0);
            let d1 = testMenu.itemAt(1);

            tryCompare(d0, "enabled", true);
            tryCompare(d1, "enabled", false);
            compare(d0.iconSource, "image://kynex-icon/document-open");
            compare(d1.iconSource, "");

            testMenu.selectedIndex = 0;
            tryCompare(d0, "selected", true);
            tryCompare(d1, "selected", false);

            testMenu.selectedIndex = 1;
            tryCompare(d0, "selected", false);
            tryCompare(d1, "selected", true);

            testMenu.selectedIndex = -1;
            tryCompare(d0, "selected", false);
            tryCompare(d1, "selected", false);

            mockMenuModel.clear();
        }

        function test_controls_rtl_layout_mirroring(): void {
            // 1. Assert LTR baseline
            LayoutHelper.setRtl(false);
            compare(Qt.locale().textDirection, Qt.LeftToRight);

            // Re-instantiate buttons in LTR to check position ordering
            let testIcon = Qt.resolvedUrl("test_icon.png");
            let ltrButton = Qt.createQmlObject(
                "import QtQuick; import Kynex.Controls; KynexButton { text: 'Test'; iconSource: '" + testIcon + "'; width: 200 }",
                root
            );
            let ltrSwitch = Qt.createQmlObject(
                "import QtQuick; import Kynex.Controls; KynexSwitch { text: 'Toggle Option'; width: 200 }",
                root
            );
            let ltrItem = Qt.createQmlObject(
                "import QtQuick; import Kynex.Controls; KynexListItem { text: 'List Item'; iconSource: '" + testIcon + "'; width: 200 }",
                root
            );
            let ltrSlider = Qt.createQmlObject(
                "import QtQuick; import Kynex.Controls; KynexSlider { width: 200 }",
                root
            );
            let ltrTooltip = Qt.createQmlObject(
                "import QtQuick; import Kynex.Controls; KynexTooltip { text: 'Hint'; width: 100 }",
                root
            );
            let ltrSearch = Qt.createQmlObject(
                "import QtQuick; import Kynex.Controls; KynexSearchField { text: 'Query'; width: 200 }",
                root
            );
            let ltrMenuItem = Qt.createQmlObject(
                "import QtQuick; import Kynex.Controls; KynexMenuItem { text: 'Menu'; hasSubmenu: true; width: 200 }",
                root
            );

            tryVerify(function() {
                return !ltrButton.LayoutMirroring.enabled && ltrButton.iconX < ltrButton.labelX;
            }, 5000);

            verify(!ltrButton.LayoutMirroring.enabled, "LTR button LayoutMirroring must be disabled");
            verify(ltrButton.iconX < ltrButton.labelX, "In LTR, icon must be to the left of label");

            verify(!ltrSwitch.LayoutMirroring.enabled, "LTR switch LayoutMirroring must be disabled");
            verify(ltrSwitch.trackX < ltrSwitch.labelX, "In LTR, track must be to the left of label");

            verify(!ltrItem.LayoutMirroring.enabled, "LTR list item LayoutMirroring must be disabled");
            verify(ltrItem.iconX < ltrItem.labelX, "In LTR, icon must be to the left of label");

            verify(!ltrSlider.LayoutMirroring.enabled, "LTR slider LayoutMirroring must be disabled");
            verify(!ltrTooltip.LayoutMirroring.enabled, "LTR tooltip LayoutMirroring must be disabled");
            verify(!ltrSearch.LayoutMirroring.enabled, "LTR search LayoutMirroring must be disabled");
            verify(!ltrMenuItem.LayoutMirroring.enabled, "LTR menu item LayoutMirroring must be disabled");
            compare(ltrMenuItem.submenuArrowText, "▶");

            let ltrMenu = Qt.createQmlObject(
                "import QtQuick; import Kynex.Controls; KynexMenu {}",
                root
            );
            verify(!ltrMenu.LayoutMirroring.enabled, "LTR menu LayoutMirroring must be disabled");
            verify(ltrMenu.LayoutMirroring.childrenInherit, "LTR menu LayoutMirroring childrenInherit must be true");
            ltrMenu.destroy();

            ltrButton.destroy();
            ltrSwitch.destroy();
            ltrItem.destroy();
            ltrSlider.destroy();
            ltrTooltip.destroy();
            ltrSearch.destroy();
            ltrMenuItem.destroy();

            // 2. Switch to RTL locale and verify LayoutMirroring enabled and mirrored positions
            LayoutHelper.setRtl(true);
            compare(Qt.locale().textDirection, Qt.RightToLeft);

            let rtlButton = Qt.createQmlObject(
                "import QtQuick; import Kynex.Controls; KynexButton { text: 'Test'; iconSource: '" + testIcon + "'; width: 200 }",
                root
            );
            let rtlSwitch = Qt.createQmlObject(
                "import QtQuick; import Kynex.Controls; KynexSwitch { text: 'Toggle Option'; width: 200 }",
                root
            );
            let rtlItem = Qt.createQmlObject(
                "import QtQuick; import Kynex.Controls; KynexListItem { text: 'List Item'; iconSource: '" + testIcon + "'; width: 200 }",
                root
            );
            let rtlSlider = Qt.createQmlObject(
                "import QtQuick; import Kynex.Controls; KynexSlider { width: 200 }",
                root
            );
            let rtlTooltip = Qt.createQmlObject(
                "import QtQuick; import Kynex.Controls; KynexTooltip { text: 'Hint'; width: 100 }",
                root
            );
            let rtlSearch = Qt.createQmlObject(
                "import QtQuick; import Kynex.Controls; KynexSearchField { text: 'Query'; width: 200 }",
                root
            );
            let rtlMenuItem = Qt.createQmlObject(
                "import QtQuick; import Kynex.Controls; KynexMenuItem { text: 'Menu'; hasSubmenu: true; width: 200 }",
                root
            );

            tryVerify(function() {
                return rtlButton.LayoutMirroring.enabled &&
                       rtlSwitch.LayoutMirroring.enabled &&
                       rtlItem.LayoutMirroring.enabled &&
                       rtlSlider.LayoutMirroring.enabled &&
                       rtlTooltip.LayoutMirroring.enabled &&
                       rtlSearch.LayoutMirroring.enabled &&
                       rtlMenuItem.LayoutMirroring.enabled &&
                       rtlButton.iconX > rtlButton.labelX;
            }, 5000);

            verify(rtlButton.LayoutMirroring.enabled, "RTL button LayoutMirroring must be enabled");
            verify(rtlButton.iconX > rtlButton.labelX, "In RTL, icon must be mirrored to the right of label");

            verify(rtlSwitch.LayoutMirroring.enabled, "RTL switch LayoutMirroring must be enabled");
            verify(rtlSwitch.trackX > rtlSwitch.labelX, "In RTL, track must be mirrored to the right of label");

            verify(rtlItem.LayoutMirroring.enabled, "RTL list item LayoutMirroring must be enabled");
            verify(rtlItem.iconX > rtlItem.labelX, "In RTL, icon must be mirrored to the right of label");

            verify(rtlSlider.LayoutMirroring.enabled, "RTL slider LayoutMirroring must be enabled");
            verify(rtlTooltip.LayoutMirroring.enabled, "RTL tooltip LayoutMirroring must be enabled");
            verify(rtlSearch.LayoutMirroring.enabled, "RTL search LayoutMirroring must be enabled");
            verify(rtlMenuItem.LayoutMirroring.enabled, "RTL menu item LayoutMirroring must be enabled");
            compare(rtlMenuItem.submenuArrowText, "◀");

            let rtlMenu = Qt.createQmlObject(
                "import QtQuick; import Kynex.Controls; KynexMenu {}",
                root
            );
            verify(rtlMenu.LayoutMirroring.enabled, "RTL menu LayoutMirroring must be enabled");
            verify(rtlMenu.LayoutMirroring.childrenInherit, "RTL menu LayoutMirroring childrenInherit must be true");
            rtlMenu.destroy();

            rtlButton.destroy();
            rtlSwitch.destroy();
            rtlItem.destroy();
            rtlSlider.destroy();
            rtlTooltip.destroy();
            rtlSearch.destroy();
            rtlMenuItem.destroy();

            // Restore LTR
            LayoutHelper.setRtl(false);
        }

        function test_controls_token_bindings(): void {
            // 1. KynexButton: non-primary vs primary token colors
            button.primary = false;
            button.focus = false;
            root.forceActiveFocus(); // unfocus button
            tryCompare(button, "backgroundColor", Style.colors.surfaceRaised, 5000);
            compare(button.textColor, Style.colors.text);
            compare(button.borderColor, Style.colors.surfaceRaised);

            button.primary = true;
            tryCompare(button, "backgroundColor", Style.colors.accent, 5000);
            compare(button.textColor, Style.colors.surface);
            compare(button.borderColor, Style.colors.surfaceRaised);
            button.primary = false;
            tryCompare(button, "backgroundColor", Style.colors.surfaceRaised, 5000);

            // 2. KynexSwitch: indicator token colors
            toggleSwitch.checked = false;
            tryCompare(toggleSwitch, "trackColor", Style.colors.surfaceRaised);
            tryCompare(toggleSwitch, "thumbColor", Style.colors.text);

            toggleSwitch.checked = true;
            tryCompare(toggleSwitch, "trackColor", Style.colors.accent);
            tryCompare(toggleSwitch, "thumbColor", Style.colors.surface);
            toggleSwitch.checked = false;
            tryCompare(toggleSwitch, "trackColor", Style.colors.surfaceRaised);

            // 3. KynexSlider: track and handle token colors
            compare(slider.trackColor, Style.colors.surfaceRaised);
            compare(slider.filledTrackColor, Style.colors.accent);
            compare(slider.handleColor, Style.colors.surfaceRaised);
            compare(slider.handleBorderColor, Style.colors.accent);

            // 4. KynexTooltip: token colors
            compare(tooltip.backgroundColor, Style.colors.surfaceRaised);
            compare(tooltip.textColor, Style.colors.text);

            // 5. KynexListItem: selection token colors
            listItem.selected = false;
            compare(listItem.backgroundColor, Style.colors.surface);
            compare(listItem.textColor, Style.colors.text);

            listItem.selected = true;
            compare(listItem.backgroundColor, Style.colors.surfaceRaised);
            compare(listItem.textColor, Style.colors.text);
            listItem.selected = false;

            // 6. KynexSearchField: token colors
            compare(searchField.backgroundColor, Style.colors.surface);
            compare(searchField.textColor, Style.colors.text);
        }
    }
}
