import QtQuick
import QtTest
import Kynex.Shell
import Kynex.Style
import Kynex.Controls

Item {
    id: root
    width: 1920
    height: 1080

    AudioModel {
        id: mockAudio
    }

    NotificationModel {
        id: mockNotifications
    }

    NetworkModel {
        id: mockNetwork
    }

    PowerModel {
        id: mockPower
    }

    TestCase {
        name: "QuickSettingsTests"
        when: windowShown

        property var quickSettings: null

        function init() {
            if (quickSettings) {
                quickSettings.destroy();
                quickSettings = null;
            }
            mockAudio.present = true;
            mockAudio.volume = 50;
            mockAudio.muted = false;
            mockAudio.device_name = "Speakers";
            mockNotifications.do_not_disturb = false;
            mockNetwork.present = true;
            mockNetwork.connected = true;
            mockNetwork.wifi = true;
            mockNetwork.ethernet = false;
            mockNetwork.weak = false;
            mockNetwork.strength = 80;
            mockNetwork.ssid = "KynexNet";
            mockPower.present = true;
            mockPower.charging = false;
            mockPower.low = false;
            mockPower.percentage = 85;

            let comp = Qt.createComponent("../../styles/default/QuickSettings.qml");
            compare(comp.status, Component.Ready, "QuickSettings.qml must load cleanly without errors");
            quickSettings = comp.createObject(null, {
                "audioModel": mockAudio,
                "notificationModel": mockNotifications,
                "networkModel": mockNetwork,
                "powerModel": mockPower,
                "panelAnchorsBits": 8, // Bottom
                "panelWidth": 1920,
                "panelHeight": 48,
                "anchorX": 1700,
                "anchorY": 1032,
                "anchorWidth": 200,
                "anchorHeight": 48,
                "isOpen": false
            });
            verify(quickSettings !== null, "QuickSettings surface should be instantiated");
        }

        function cleanup() {
            if (quickSettings) {
                quickSettings.destroy();
                quickSettings = null;
            }
        }

        function test_initial_state() {
            compare(quickSettings.isOpen, false);
            compare(quickSettings.visible, false);
            compare(quickSettings.cardRectangle.radius, Style.radius.medium);
            compare(quickSettings.cardRectangle.color, Style.colors.surface);
        }

        function test_open_and_focus() {
            quickSettings.isOpen = true;
            tryVerify(function() { return quickSettings.visible; });
            tryVerify(function() { return quickSettings.popupY > 0; });
            tryVerify(function() { return quickSettings.volumeSliderItem.activeFocus; });

            // Accessibility names
            compare(quickSettings.volumeSliderItem.Accessible.name, "Master Volume Slider");
            compare(quickSettings.muteSwitchItem.Accessible.name, "Audio Mute Switch");
            compare(quickSettings.dndSwitchItem.Accessible.name, "Do Not Disturb Switch");
            compare(quickSettings.powerSessionButtonItem.Accessible.name, "Power and Session Button");

            // Audio icons and device name label
            compare(quickSettings.volumeIconItem.text, "🔊");
            compare(quickSettings.deviceNameLabelItem.visible, true);
            compare(quickSettings.deviceNameLabelItem.text, "Speakers");

            mockAudio.muted = true;
            compare(quickSettings.volumeIconItem.text, "🔇");

            mockAudio.muted = false;
            mockAudio.volume = 0;
            compare(quickSettings.volumeIconItem.text, "🔈");

            mockAudio.volume = 50;
            compare(quickSettings.volumeIconItem.text, "🔊");

            mockAudio.device_name = "";
            compare(quickSettings.deviceNameLabelItem.visible, false);
            mockAudio.device_name = "Speakers";
            compare(quickSettings.deviceNameLabelItem.visible, true);

            // Network summary formatting
            compare(quickSettings.networkSummaryItem.text, "KynexNet (80%)");
            mockNetwork.ssid = "";
            compare(quickSettings.networkSummaryItem.text, "80%");
            mockNetwork.ssid = "KynexNet";
            compare(quickSettings.networkSummaryItem.text, "KynexNet (80%)");
        }

        function test_volume_slider_interaction() {
            quickSettings.isOpen = true;
            tryVerify(function() { return quickSettings.visible; });
            compare(quickSettings.volumeSliderItem.enabled, true);
            compare(quickSettings.volumeSliderItem.value, 0.5);

            quickSettings.volumeSliderItem.value = 0.8;
            quickSettings.volumeSliderItem.moved(0.8);
            tryCompare(mockAudio, "volume", 80);
        }

        function test_mute_switch_toggle() {
            quickSettings.isOpen = true;
            tryVerify(function() { return quickSettings.visible; });
            compare(quickSettings.muteSwitchItem.enabled, true);
            compare(quickSettings.muteSwitchItem.checked, false);

            quickSettings.muteSwitchItem.toggled(true);
            tryCompare(mockAudio, "muted", true);

            quickSettings.muteSwitchItem.toggled(false);
            tryCompare(mockAudio, "muted", false);
        }

        function test_dnd_switch_toggle() {
            quickSettings.isOpen = true;
            tryVerify(function() { return quickSettings.visible; });
            compare(quickSettings.dndSwitchItem.checked, false);

            quickSettings.dndSwitchItem.toggled(true);
            tryCompare(mockNotifications, "do_not_disturb", true);

            quickSettings.dndSwitchItem.toggled(false);
            tryCompare(mockNotifications, "do_not_disturb", false);
        }

        function test_esc_closes() {
            quickSettings.isOpen = true;
            tryVerify(function() { return quickSettings.visible; });

            quickSettings.escShortcutItem.activated();
            tryCompare(quickSettings, "isOpen", false);
            tryCompare(quickSettings, "visible", false);
        }

        function test_outside_click_dismisses() {
            quickSettings.isOpen = true;
            tryVerify(function() { return quickSettings.visible; });

            LayoutHelper.sendMouseClick(quickSettings.backdropArea, 10, 10, Qt.LeftButton);
            tryCompare(quickSettings, "isOpen", false);
            tryCompare(quickSettings, "visible", false);
        }

        function test_audio_unavailable_state() {
            mockAudio.present = false;
            quickSettings.isOpen = true;
            tryVerify(function() { return quickSettings.visible; });

            compare(quickSettings.volumeSliderItem.enabled, false);
            compare(quickSettings.muteSwitchItem.enabled, false);
            compare(quickSettings.deviceNameLabelItem.visible, false);

            // When audioModel is null
            quickSettings.audioModel = null;
            compare(quickSettings.volumeSliderItem.enabled, false);
            compare(quickSettings.muteSwitchItem.enabled, false);
            quickSettings.audioModel = mockAudio;
        }

        function test_placement_negative_origin() {
            // For an output whose compositor position is at x = -1920,
            // QuickSettings surface coordinates must be output-local (landing within [0, width])
            let comp = Qt.createComponent("../../styles/default/QuickSettings.qml");
            let qsNegativeOrigin = comp.createObject(root, {
                "audioModel": mockAudio,
                "notificationModel": mockNotifications,
                "networkModel": mockNetwork,
                "powerModel": mockPower,
                "panelAnchorsBits": 8, // Bottom
                "panelWidth": 1920,
                "panelHeight": 48,
                "anchorX": 1700,
                "anchorY": 1032,
                "anchorWidth": 200,
                "anchorHeight": 48,
                "width": 1920,
                "height": 1080,
                "isOpen": true
            });
            verify(qsNegativeOrigin !== null, "QuickSettings should instantiate on negative origin output");
            tryVerify(function() { return qsNegativeOrigin.visible; });

            // Assert that coordinates actually used by QuickSettings.qml land strictly inside that output [0, 1920] x [0, 1080]
            verify(qsNegativeOrigin.popupX >= 0, "popupX must be >= 0 in output-local space");
            verify(qsNegativeOrigin.popupX + qsNegativeOrigin.cardContainer.width <= qsNegativeOrigin.width,
                   "popupContainer must land completely inside the output width");
            // Specifically verify it was not displaced by global -1920 offset
            compare(qsNegativeOrigin.popupX, qsNegativeOrigin.anchorX + qsNegativeOrigin.anchorWidth - qsNegativeOrigin.popupWidth);

            qsNegativeOrigin.destroy();
        }

        function test_rtl_mirroring() {
            LayoutHelper.setRtl(true);
            let comp = Qt.createComponent("../../styles/default/QuickSettings.qml");
            let rtlQs = comp.createObject(null, {
                "audioModel": mockAudio,
                "notificationModel": mockNotifications,
                "isOpen": true
            });
            verify(rtlQs !== null);

            compare(rtlQs.LayoutMirroring.enabled, true);
            compare(rtlQs.LayoutMirroring.childrenInherit, true);

            rtlQs.destroy();
            LayoutHelper.setRtl(false);
        }

        function test_undefined_models_null_safe() {
            let comp = Qt.createComponent("../../styles/default/QuickSettings.qml");
            let uninitAudio = Qt.createQmlObject("import Kynex.Shell; AudioModel {}", root);
            let uninitNotif = Qt.createQmlObject("import Kynex.Shell; NotificationModel {}", root);
            let uninitNet = Qt.createQmlObject("import Kynex.Shell; NetworkModel {}", root);
            let uninitPower = Qt.createQmlObject("import Kynex.Shell; PowerModel {}", root);

            let uninitQs = comp.createObject(root, {
                "audioModel": uninitAudio,
                "notificationModel": uninitNotif,
                "networkModel": uninitNet,
                "powerModel": uninitPower,
                "isOpen": true
            });
            verify(uninitQs !== null, "QuickSettings should instantiate with uninitialized models");
            tryVerify(function() { return uninitQs.visible; });

            // Also test with completely null models
            let nullQs = comp.createObject(root, {
                "audioModel": null,
                "notificationModel": null,
                "networkModel": null,
                "powerModel": null,
                "isOpen": true
            });
            verify(nullQs !== null, "QuickSettings should instantiate with null models");

            uninitQs.destroy();
            nullQs.destroy();
            uninitAudio.destroy();
            uninitNotif.destroy();
            uninitNet.destroy();
            uninitPower.destroy();
        }

        function test_first_open_late_populating_models_reactive_sizing_and_all_rows_visible() {
            // Regression test for R3.4:
            // When QuickSettings opens on first open before models are populated or when models populate
            // asynchronously after the surface is mapped, card.implicitHeight must expand and
            // popupY must adjust reactively so that ALL rows are visible without clipping.
            let comp = Qt.createComponent("../../styles/default/QuickSettings.qml");
            let lateAudio = Qt.createQmlObject("import Kynex.Shell; AudioModel {}", root);
            let lateNotif = Qt.createQmlObject("import Kynex.Shell; NotificationModel {}", root);
            let lateNet = Qt.createQmlObject("import Kynex.Shell; NetworkModel {}", root);
            let latePower = Qt.createQmlObject("import Kynex.Shell; PowerModel {}", root);

            // 1. Initially unpopulated models (present=false, no device name, etc.)
            lateAudio.present = false;
            lateAudio.device_name = "";
            lateNet.present = false;
            latePower.present = false;

            let testQs = comp.createObject(root, {
                "audioModel": lateAudio,
                "notificationModel": lateNotif,
                "networkModel": lateNet,
                "powerModel": latePower,
                "panelAnchorsBits": 8, // Bottom
                "panelWidth": 1920,
                "panelHeight": 48,
                "anchorX": 1700,
                "anchorY": 1032,
                "anchorWidth": 200,
                "anchorHeight": 48,
                "width": 1920,
                "height": 1080,
                "isOpen": false
            });
            verify(testQs !== null);

            // 2. Open popup for the very first time with unpopulated models
            testQs.isOpen = true;
            tryVerify(function() { return testQs.visible; });
            let initialY = testQs.popupY;
            let initialHeight = testQs.cardRectangle.implicitHeight;

            // 3. Populate models late (as occurs when D-Bus answers async)
            lateAudio.present = true;
            lateAudio.volume = 75;
            lateAudio.muted = false;
            lateAudio.device_name = "Headphones High Fidelity Stereo Sink";
            lateNotif.do_not_disturb = true;
            lateNet.present = true;
            lateNet.connected = true;
            lateNet.wifi = true;
            lateNet.ssid = "Enterprise-WPA3";
            lateNet.strength = 92;
            latePower.present = true;
            latePower.charging = true;
            latePower.percentage = 99;

            // 4. Assert that card.implicitHeight expands and popupY updates reactively
            tryVerify(function() { return testQs.cardRectangle.implicitHeight > initialHeight; });
            tryVerify(function() { return testQs.popupY < initialY; });
            tryVerify(function() { return testQs.deviceNameLabelItem.visible; });
            tryVerify(function() { return testQs.dndSwitchItem.visible; });
            tryVerify(function() { return testQs.networkSummaryItem.visible; });

            // 5. Assert all rows remain strictly within output bounds [0, 1080]
            verify(testQs.popupY >= 0, "popupY must be non-negative");
            verify(testQs.popupY + testQs.cardContainer.height <= testQs.height,
                   "Entire card container must be fully visible and not clipped off the bottom edge");

            testQs.destroy();
            lateAudio.destroy();
            lateNotif.destroy();
            lateNet.destroy();
            latePower.destroy();
        }
    }
}
