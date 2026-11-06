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
        id: buttonPressedSpy
        target: button
        signalName: "pressed"
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
            compare(button.opacity, 1.0);
        }

        function test_button_mouse_click() {
            mouseClick(button);
            compare(buttonClickedSpy.count, 1);
            compare(buttonPressedSpy.count, 1);
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
        }

        function test_button_disabled() {
            button.enabled = false;
            compare(button.opacity, Style.opacity.disabled);

            mouseClick(button);
            compare(buttonClickedSpy.count, 0);

            button.forceActiveFocus();
            keyClick(Qt.Key_Return);
            keyClick(Qt.Key_Space);
            compare(buttonClickedSpy.count, 0);
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
        }

        function test_switch_disabled() {
            toggleSwitch.enabled = false;
            compare(toggleSwitch.opacity, Style.opacity.disabled);

            mouseClick(toggleSwitch);
            compare(toggleSwitch.checked, false);
            compare(switchToggledSpy.count, 0);

            toggleSwitch.forceActiveFocus();
            keyClick(Qt.Key_Space);
            compare(toggleSwitch.checked, false);
            compare(switchToggledSpy.count, 0);
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
            verify(slider.Accessible.focusable);
        }

        function test_slider_keyboard_navigation() {
            slider.forceActiveFocus();
            verify(slider.activeFocus);

            let initial = slider.value;
            keyClick(Qt.Key_Right);
            verify(slider.value > initial);
            compare(sliderMovedSpy.count, 1);

            let higher = slider.value;
            keyClick(Qt.Key_Left);
            verify(slider.value < higher);
            compare(sliderMovedSpy.count, 2);

            keyClick(Qt.Key_Home);
            compare(slider.value, 0.0);

            keyClick(Qt.Key_End);
            compare(slider.value, 1.0);
        }

        function test_slider_clamping() {
            slider.setValueClamped(1.5);
            compare(slider.value, 1.0);

            slider.setValueClamped(-0.5);
            compare(slider.value, 0.0);
        }

        function test_slider_disabled() {
            slider.enabled = false;
            compare(slider.opacity, Style.opacity.disabled);

            let oldVal = slider.value;
            slider.increase();
            compare(slider.value, oldVal);

            slider.forceActiveFocus();
            keyClick(Qt.Key_Right);
            compare(slider.value, oldVal);
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

            keyClick(Qt.Key_Space);
            compare(listItemClickedSpy.count, 2);
        }

        function test_listitem_selection_and_disabled() {
            listItem.selected = true;
            compare(listItem.selected, true);

            listItem.enabled = false;
            compare(listItem.opacity, Style.opacity.disabled);

            mouseClick(listItem);
            compare(listItemClickedSpy.count, 0);
        }

        function test_listitem_focus_ring() {
            compare(listItem.borderWidth, 0);
            verify(listItem.borderColor !== Style.colors.focusRing);

            listItem.forceActiveFocus();
            verify(listItem.activeFocus);
            compare(listItem.borderColor, Style.colors.focusRing);
            compare(listItem.borderWidth, 1);
        }
    }
}
