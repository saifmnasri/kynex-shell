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
    }
}
