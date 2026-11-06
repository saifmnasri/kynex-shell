import QtQuick
import QtTest
import Kynex.Controls
import Kynex.Style

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

    TestCase {
        name: "KynexControlsTests"
        when: windowShown

        function init() {
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
            button.forceActiveFocus();
            verify(button.activeFocus);

            keyPress(Qt.Key_Space);
            compare(buttonPressedSpy.count, 1);
            verify(button.down);

            keyRelease(Qt.Key_Space);
            compare(buttonClickedSpy.count, 1);
            verify(!button.down);
        }

        function test_button_keyboard_return() {
            button.forceActiveFocus();
            verify(button.activeFocus);

            keyClick(Qt.Key_Return);
            compare(buttonClickedSpy.count, 1);

            keyClick(Qt.Key_Enter);
            compare(buttonClickedSpy.count, 2);
        }

        function test_button_focus_ring() {
            compare(button.borderWidth, 0);
            verify(button.borderColor !== Style.colors.focusRing);

            button.forceActiveFocus();
            verify(button.activeFocus);
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
            toggleSwitch.forceActiveFocus();
            verify(toggleSwitch.activeFocus);

            keyClick(Qt.Key_Space);
            compare(toggleSwitch.checked, true);
            compare(switchToggledSpy.count, 1);

            keyClick(Qt.Key_Return);
            compare(toggleSwitch.checked, false);
            compare(switchToggledSpy.count, 2);

            keyClick(Qt.Key_Enter);
            compare(toggleSwitch.checked, true);
            compare(switchToggledSpy.count, 3);

            toggleSwitch.checked = false;
            keyPress(Qt.Key_Space);
            verify(toggleSwitch.down);
            keyRelease(Qt.Key_Space);
            verify(!toggleSwitch.down);
        }

        function test_switch_focus_ring() {
            compare(toggleSwitch.borderWidth, 0);
            verify(toggleSwitch.borderColor !== Style.colors.focusRing);

            toggleSwitch.forceActiveFocus();
            verify(toggleSwitch.activeFocus);
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
            slider.forceActiveFocus();
            verify(slider.activeFocus);

            slider.value = 0.5;
            let initial = slider.value;
            keyClick(Qt.Key_Right);
            verify(slider.value > initial);
            compare(sliderMovedSpy.count, 1);

            let higher = slider.value;
            keyClick(Qt.Key_Left);
            compare(slider.value, initial);
            compare(sliderMovedSpy.count, 2);

            let valBeforeUp = slider.value;
            keyClick(Qt.Key_Up);
            verify(slider.value > valBeforeUp);

            let valBeforeDown = slider.value;
            keyClick(Qt.Key_Down);
            verify(slider.value < valBeforeDown);

            slider.value = 0.5;
            let valBeforePageUp = slider.value;
            keyClick(Qt.Key_PageUp);
            verify(slider.value > valBeforePageUp);

            let valBeforePageDown = slider.value;
            keyClick(Qt.Key_PageDown);
            verify(slider.value < valBeforePageDown);

            keyClick(Qt.Key_Home);
            compare(slider.value, 0.0);

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

            slider.forceActiveFocus();
            verify(slider.activeFocus);
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
        }

        function test_listitem_mouse_click() {
            mouseClick(listItem);
            compare(listItemClickedSpy.count, 1);
        }

        function test_listitem_keyboard_activation() {
            listItem.forceActiveFocus();
            verify(listItem.activeFocus);

            keyClick(Qt.Key_Return);
            compare(listItemClickedSpy.count, 1);

            keyClick(Qt.Key_Enter);
            compare(listItemClickedSpy.count, 2);

            keyClick(Qt.Key_Space);
            compare(listItemClickedSpy.count, 3);

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

            listItem.forceActiveFocus();
            verify(listItem.activeFocus);
            compare(listItem.borderColor, Style.colors.focusRing);
            compare(listItem.borderWidth, 1);
        }

        function send_all_inputs(item) {
            item.forceActiveFocus();
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

            send_all_inputs(slider);
            compare(slider.value, 0.5);
            compare(sliderMovedSpy.count, 0);

            // Positive control: when enabled, inputs MUST alter value and emit signals
            slider.enabled = true;
            send_all_inputs(slider);
            verify(sliderMovedSpy.count > 0);

            // 2. KynexSwitch: state 1 (checked = false)
            toggleSwitch.enabled = true;
            toggleSwitch.checked = false;
            switchToggledSpy.clear();

            toggleSwitch.enabled = false;
            compare(toggleSwitch.opacity, Style.opacity.disabled);
            compare(toggleSwitch.checked, false);

            send_all_inputs(toggleSwitch);
            compare(toggleSwitch.checked, false);
            compare(switchToggledSpy.count, 0);

            // Positive control
            toggleSwitch.enabled = true;
            send_all_inputs(toggleSwitch);
            verify(switchToggledSpy.count > 0);

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

            // 4. KynexButton
            button.enabled = true;
            buttonClickedSpy.clear();
            buttonPressedSpy.clear();

            button.enabled = false;
            compare(button.opacity, Style.opacity.disabled);

            send_all_inputs(button);
            compare(buttonClickedSpy.count, 0);
            compare(buttonPressedSpy.count, 0);
            verify(!button.down);

            // Positive control
            button.enabled = true;
            send_all_inputs(button);
            verify(buttonClickedSpy.count > 0);

            // 5. KynexListItem: state 1 (selected = false)
            listItem.enabled = true;
            listItem.selected = false;
            listItemClickedSpy.clear();

            listItem.enabled = false;
            compare(listItem.opacity, Style.opacity.disabled);
            compare(listItem.selected, false);

            send_all_inputs(listItem);
            compare(listItem.selected, false);
            compare(listItemClickedSpy.count, 0);

            // Positive control
            listItem.enabled = true;
            send_all_inputs(listItem);
            verify(listItemClickedSpy.count > 0);

            // 6. KynexListItem: state 2 (selected = true)
            listItem.enabled = true;
            listItem.selected = true;
            listItemClickedSpy.clear();

            listItem.enabled = false;
            compare(listItem.opacity, Style.opacity.disabled);
            compare(listItem.selected, true);

            send_all_inputs(listItem);
            compare(listItem.selected, true);
            compare(listItemClickedSpy.count, 0);
        }
    }
}
