import QtQuick
import QtTest
import Kynex.Shell
import Kynex.Style
import Kynex.Controls

Item {
    id: root
    width: 1024
    height: 768

    TestCase {
        name: "ShellTests"
        when: windowShown

        property var shell: null

        function init() {
            if (shell) {
                shell.destroy();
                shell = null;
            }
            let comp = Qt.createComponent("../../styles/default/Shell.qml");
            compare(comp.status, Component.Ready, "Shell.qml must load cleanly without errors");
            shell = comp.createObject(null, { width: 1920, height: 48 });
            verify(shell !== null, "Shell panel should be instantiated");
            shell.show();
            shell.contentItem.width = shell.width;
            shell.contentItem.height = shell.height;
            shell.windowListModel.clear();
            if (shell.trayModel) {
                shell.trayModel.clear();
            }
        }

        function cleanup() {
            if (shell) {
                shell.destroy();
                shell = null;
            }
        }

        function test_initial_state() {
            verify(shell.visible, "Shell should be visible");
            compare(shell.windowListModel.count, 0);
            compare(shell.taskbarList.visible, false, "Taskbar should be hidden when window count is 0");
        }

        function test_taskbar_window_count_badge_and_delegates() {
            // Add Item 0: Active window with windowCount = 1
            shell.windowListModel.addItem(
                "win1",
                "Firefox",
                "org.mozilla.firefox",
                false, // minimized
                true,  // active
                "firefox",
                "F",
                true,  // isPinned
                true,  // isRunning
                1      // windowCount = 1
            );

            // Add Item 1: Minimized window with windowCount = 3
            shell.windowListModel.addItem(
                "win2",
                "",
                "org.gnome.Terminal",
                true,  // minimized
                false, // active
                "utilities-terminal",
                "T",
                false, // isPinned
                true,  // isRunning
                3      // windowCount = 3
            );

            // Add Item 2: Unnamed app falling back to "Window"
            shell.windowListModel.addItem(
                "win3",
                "",
                "",
                false,
                false,
                "",
                "",
                false,
                false,
                0
            );

            compare(shell.windowListModel.count, 3);
            compare(shell.taskbarList.visible, true, "Taskbar list should be visible when count > 0");

            shell.taskbarList.forceLayout();
            tryVerify(function() { return shell.taskbarList.count === 3 && shell.taskbarList.itemAtIndex(0) !== null; });

            // Delegate 0 (windowCount = 1)
            let btn0 = shell.taskbarList.itemAtIndex(0);
            verify(btn0 !== null, "Delegate 0 must be created");
            compare(btn0.text, "Firefox");
            compare(btn0.iconSource.toString(), "image://kynex-icon/firefox?size=24&scale=1");
            compare(btn0.opacity, 1.0);
            compare(btn0.Accessible.name, "Firefox (Active)");
            verify(!btn0.countBadgeVisible, "Count badge must be hidden when windowCount <= 1");

            // Delegate 1 (windowCount = 3, minimized)
            let btn1 = shell.taskbarList.itemAtIndex(1);
            verify(btn1 !== null, "Delegate 1 must be created");
            compare(btn1.text, "org.gnome.Terminal");
            compare(btn1.opacity, Style.opacity.disabled, "Minimized running app should have disabled opacity");
            compare(btn1.Accessible.name, "App (Running) - 3 windows");
            verify(btn1.countBadgeVisible, "Count badge must be visible when windowCount > 1");
            compare(btn1.countBadgeText, "3", "Count badge text must equal windowCount string");

            // Delegate 2 (fallback text "Window")
            let btn2 = shell.taskbarList.itemAtIndex(2);
            verify(btn2 !== null, "Delegate 2 must be created");
            compare(btn2.text, "Window");
            compare(btn2.iconSource.toString(), "");
        }

        function test_taskbar_keyboard_activation() {
            shell.windowListModel.addItem(
                "win1",
                "Firefox",
                "org.mozilla.firefox",
                false,
                true,
                "firefox",
                "F",
                true,
                true,
                1
            );
            shell.taskbarList.forceLayout();
            tryVerify(function() { return shell.taskbarList.count === 1 && shell.taskbarList.itemAtIndex(0) !== null; });

            let btn0 = shell.taskbarList.itemAtIndex(0);
            verify(btn0 !== null);
            btn0.forceActiveFocus();

            LayoutHelper.sendKey(btn0, Qt.Key_Return);
            compare(shell.windowListModel.clickCount, 1);
            compare(shell.windowListModel.lastClickedIndex, 0);

            LayoutHelper.sendKey(btn0, Qt.Key_Enter);
            compare(shell.windowListModel.clickCount, 2);
            compare(shell.windowListModel.lastClickedIndex, 0);

            LayoutHelper.sendKey(btn0, Qt.Key_Space);
            compare(shell.windowListModel.clickCount, 3);
            compare(shell.windowListModel.lastClickedIndex, 0);
        }

        function test_network_indicator_states() {
            // Wi-Fi with SSID
            shell.networkModel.wifi = true;
            shell.networkModel.ssid = "KynexMesh";
            shell.networkModel.strength = 85;
            shell.networkModel.connected = true;

            compare(shell.networkText.text, "KynexMesh");
            compare(shell.networkTooltipText, "Wi-Fi: KynexMesh (85%)");

            // Wi-Fi without SSID
            shell.networkModel.ssid = "";
            compare(shell.networkText.text, "85%");
            compare(shell.networkTooltipText, "Wi-Fi: Connected (85%)");

            // Ethernet
            shell.networkModel.wifi = false;
            shell.networkModel.ethernet = true;
            compare(shell.networkText.text, "Ethernet");
            compare(shell.networkTooltipText, "Ethernet Connection");
        }

        function test_launcher_loader_ready_binding() {
            compare(shell.launcherLoaded, false);
            shell.launcherModel.visible = true;
            compare(shell.launcherLoaded, true);

            tryVerify(function() {
                return shell.launcherLoader.status === Loader.Ready;
            });

            verify(shell.launcherLoader.item !== null);
            compare(shell.launcherLoader.item.launcherModel, shell.launcherModel);
        }

        function test_notification_loader_and_model() {
            compare(shell.notificationPopupLoaded, false);
            compare(shell.notificationModel.count, 0);

            shell.notificationModel.addNotification(1, "TestApp", "Test Summary", "Test Body", 1);
            compare(shell.notificationModel.count, 1);
            compare(shell.notificationPopupLoaded, true);

            tryVerify(function() {
                return shell.notificationLoader.status === Loader.Ready;
            });
            verify(shell.notificationLoader.item !== null);
            compare(shell.notificationLoader.item.notificationModel, shell.notificationModel);
        }

        function test_tray_initial_state() {
            compare(shell.trayModel.count, 0);
            compare(shell.trayArea.visible, false, "Tray area should be hidden when count is 0");
        }

        function test_tray_items_presentation() {
            compare(shell.trayModel.count, 0);
            compare(shell.trayArea.visible, false);

            // Item 1: Normal active item
            shell.trayModel.addItem(
                "item1",
                "Wi-Fi",
                "Active",
                "nm-device-wireless",
                false,
                "",
                "Network",
                "Connected to Wi-Fi",
                false,
                "W",
                false,
                0
            );

            // Item 2: Needs attention with pixmap
            shell.trayModel.addItem(
                "item2",
                "Chat",
                "NeedsAttention",
                "",
                true,
                "item2",
                "Messages",
                "3 new alerts",
                true,
                "C",
                false,
                0
            );

            // Item 3: Badge fallback
            shell.trayModel.addItem(
                "item3",
                "",
                "Active",
                "",
                false,
                "",
                "",
                "",
                false,
                "M",
                false,
                0
            );

            compare(shell.trayModel.count, 3);
            compare(shell.trayArea.visible, true, "Tray area should be visible when count > 0");
            compare(shell.trayRepeater.count, 3);

            let btn0 = shell.trayRepeater.itemAt(0);
            verify(btn0 !== null, "Delegate 0 must be created");
            compare(btn0.id, "item1");
            compare(btn0.title, "Wi-Fi");
            compare(btn0.border.width, 0);
            compare(btn0.Accessible.role, Accessible.Button);
            compare(btn0.Accessible.focusable, true);
            compare(btn0.Accessible.name, "Wi-Fi");
            compare(btn0.iconImage.source.toString(), "image://kynex-icon/nm-device-wireless?size=24&scale=1");
            tryVerify(function() { return btn0.iconImage.status === Image.Ready && !btn0.badgeText.visible; });
            compare(btn0.badgeText.visible, false);
            LayoutHelper.sendMouseMove(btn0.mouseArea, btn0.width / 2, btn0.height / 2);
            tryVerify(function() { return btn0.tooltipLoader.item !== null; });
            compare(btn0.tooltipLoader.item.text, "Network\nConnected to Wi-Fi");

            let btn1 = shell.trayRepeater.itemAt(1);
            verify(btn1 !== null, "Delegate 1 must be created");
            compare(btn1.id, "item2");
            compare(btn1.isAttention, true);
            compare(btn1.border.color, Style.colors.warning);
            compare(btn1.border.width, 1);
            compare(btn1.Accessible.name, "Chat");
            compare(btn1.iconImage.source.toString(), "image://kynex-tray/item2");
            compare(btn1.attentionIndicator.visible, true);

            let btn2 = shell.trayRepeater.itemAt(2);
            verify(btn2 !== null, "Delegate 2 must be created");
            compare(btn2.id, "item3");
            compare(btn2.letterBadge, "M");
            compare(btn2.Accessible.name, "item3");
            compare(btn2.iconImage.source.toString(), "");
            compare(btn2.badgeText.visible, true);
            compare(btn2.badgeText.text, "M");
            LayoutHelper.sendMouseMove(btn2.mouseArea, btn2.width / 2, btn2.height / 2);
            tryVerify(function() { return btn2.tooltipLoader.item !== null; });
            compare(btn2.tooltipLoader.item.text, "item3");
        }

        function test_tray_tooltip_branches() {
            // Title only in tooltip
            shell.trayModel.addItem("tip1", "App1", "Active", "", false, "", "OnlyTitle", "", false, "1", false, 0);
            // Text only in tooltip
            shell.trayModel.addItem("tip2", "App2", "Active", "", false, "", "", "OnlyText", false, "2", false, 0);
            // Title in app only
            shell.trayModel.addItem("tip3", "AppTitleOnly", "Active", "", false, "", "", "", false, "3", false, 0);

            compare(shell.trayRepeater.count, 3);
            let b0 = shell.trayRepeater.itemAt(0);
            let b1 = shell.trayRepeater.itemAt(1);
            let b2 = shell.trayRepeater.itemAt(2);

            LayoutHelper.sendMouseMove(b0.mouseArea, b0.width / 2, b0.height / 2);
            tryVerify(function() { return b0.tooltipLoader.item !== null; });
            compare(b0.tooltipLoader.item.text, "OnlyTitle");

            LayoutHelper.sendMouseMove(b1.mouseArea, b1.width / 2, b1.height / 2);
            tryVerify(function() { return b1.tooltipLoader.item !== null; });
            compare(b1.tooltipLoader.item.text, "OnlyText");

            LayoutHelper.sendMouseMove(b2.mouseArea, b2.width / 2, b2.height / 2);
            tryVerify(function() { return b2.tooltipLoader.item !== null; });
            compare(b2.tooltipLoader.item.text, "AppTitleOnly");
        }

        function test_tray_click_and_scroll_interactions() {
            shell.trayModel.addItem(
                "test-tray-id",
                "Test App",
                "Active",
                "test-icon",
                false,
                "",
                "Test Headline",
                "Test Description",
                false,
                "T",
                false,
                0
            );

            compare(shell.trayRepeater.count, 1);
            let btn = shell.trayRepeater.itemAt(0);
            verify(btn !== null);
            verify(btn.mouseArea !== null);
            compare(btn.badgeText.textFormat, Text.PlainText);

            // Initially no actions recorded
            compare(shell.trayModel.lastActivatedId, "");
            compare(shell.trayModel.lastSecondaryActivatedId, "");
            compare(shell.trayModel.lastContextMenuId, "");

            let clickX = btn.width / 2;
            let clickY = btn.height / 2;
            let expectedPos = btn.mapToItem(null, clickX, clickY);
            let expectedX = Math.round(expectedPos.x);
            let expectedY = Math.round(expectedPos.y);

            // Left click -> activate
            LayoutHelper.sendMouseClick(btn.mouseArea, clickX, clickY, Qt.LeftButton);
            compare(shell.trayModel.lastActivatedId, "test-tray-id");
            compare(shell.trayModel.lastActivatedX, expectedX);
            compare(shell.trayModel.lastActivatedY, expectedY);
            compare(shell.trayModel.lastSecondaryActivatedId, "");
            compare(shell.trayModel.lastContextMenuId, "");

            // Middle click -> secondaryActivate
            LayoutHelper.sendMouseClick(btn.mouseArea, clickX, clickY, Qt.MiddleButton);
            compare(shell.trayModel.lastSecondaryActivatedId, "test-tray-id");
            compare(shell.trayModel.lastActivatedX, expectedX);
            compare(shell.trayModel.lastActivatedY, expectedY);
            compare(shell.trayModel.lastContextMenuId, "");

            // Right click -> contextMenu
            LayoutHelper.sendMouseClick(btn.mouseArea, clickX, clickY, Qt.RightButton);
            compare(shell.trayModel.lastContextMenuId, "test-tray-id");
            compare(shell.trayModel.lastContextMenuX, expectedX);
            compare(shell.trayModel.lastContextMenuY, expectedY);

            // Wheel scroll
            LayoutHelper.sendWheel(btn.mouseArea, btn.width / 2, btn.height / 2, 120, 0);
            compare(shell.trayModel.lastScrolledId, "test-tray-id");
            compare(shell.trayModel.lastScrollDelta, 120);
            compare(shell.trayModel.lastScrollOrientation, "vertical");
        }

        function test_tray_secondary_screen_negative_origin() {
            // Mock a secondary display positioned to the left of primary with negative origin (-1920, 0)
            shell.screenVirtualX = -1920;
            shell.screenVirtualY = 0;
            shell.screenWidth = 1920;
            shell.screenHeight = 1080;
            shell.screenDevicePixelRatio = 1.0;
            shell.height = 48;

            shell.trayModel.addItem(
                "negative-origin-item",
                "Secondary Display App",
                "Active",
                "system-run",
                false,
                "",
                "Title",
                "Text",
                false,
                "S",
                false,
                0
            );

            verify(shell.trayRepeater.count > 0, "Tray repeater should instantiate delegate");
            let btn = shell.trayRepeater.itemAt(0);
            verify(btn !== null, "Tray button delegate must exist");

            let clickX = btn.width / 2;
            let clickY = btn.height / 2;
            let expectedPos = btn.mapToItem(null, clickX, clickY);
            let expectedX = Math.round(expectedPos.x);
            let expectedY = Math.round(expectedPos.y);

            // Left click -> activate: asserts negative origin & panel geometry reached model
            LayoutHelper.sendMouseClick(btn.mouseArea, clickX, clickY, Qt.LeftButton);
            compare(shell.trayModel.lastActivatedId, "negative-origin-item");
            compare(shell.trayModel.lastActivatedX, expectedX);
            compare(shell.trayModel.lastActivatedY, expectedY);
            compare(shell.trayModel.lastScreenX, -1920);
            compare(shell.trayModel.lastScreenY, 0);
            compare(shell.trayModel.lastScreenWidth, 1920);
            compare(shell.trayModel.lastScreenHeight, 1080);
            compare(shell.trayModel.lastDevicePixelRatio, 1.0);
            compare(shell.trayModel.lastPanelHeight, 48);

            // Middle click -> secondaryActivate: asserts negative origin reached model
            LayoutHelper.sendMouseClick(btn.mouseArea, clickX, clickY, Qt.MiddleButton);
            compare(shell.trayModel.lastSecondaryActivatedId, "negative-origin-item");
            compare(shell.trayModel.lastScreenX, -1920);
            compare(shell.trayModel.lastScreenY, 0);
            compare(shell.trayModel.lastPanelHeight, 48);

            // Right click -> contextMenu: asserts negative origin reached model
            LayoutHelper.sendMouseClick(btn.mouseArea, clickX, clickY, Qt.RightButton);
            compare(shell.trayModel.lastContextMenuId, "negative-origin-item");
            compare(shell.trayModel.lastScreenX, -1920);
            compare(shell.trayModel.lastScreenY, 0);
            compare(shell.trayModel.lastPanelHeight, 48);
        }
    }
}
