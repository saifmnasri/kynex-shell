import QtQuick
import QtTest
import Kynex.Shell
import Kynex.Style
import Kynex.Controls

Item {
    id: root
    width: 1024
    height: 768

    LauncherModel {
        id: mockModel
    }

    TestCase {
        name: "LauncherTests"
        when: windowShown

        property var launcher: null

        function init() {
            mockModel.clearItems();
            mockModel.resetTracking();
            mockModel.visible = false;
            mockModel.query = "";
            if (launcher) {
                launcher.destroy();
                launcher = null;
            }
            let comp = Qt.createComponent("../../styles/default/Launcher.qml");
            compare(comp.status, Component.Ready, "Launcher.qml should load without errors");
            launcher = comp.createObject(null, { "launcherModel": mockModel });
            verify(launcher !== null, "Launcher should be instantiated");
        }

        function cleanup() {
            if (launcher) {
                launcher.destroy();
                launcher = null;
            }
        }

        function test_initial_state() {
            compare(launcher.visible, false);
            compare(mockModel.closeCalled, false);
            compare(mockModel.launchCallCount, 0);
        }

        function test_focus_lands_in_search_field_on_open() {
            // M07 mutant killer: verifying searchField.focus is true when launcher opens
            compare(launcher.searchFieldItem.focus, false);
            mockModel.visible = true;
            tryVerify(function() { return launcher.visible; });
            tryVerify(function() { return launcher.searchFieldItem.focus; }, 1000, "Search field must gain focus on open");
        }

        function test_esc_closes() {
            // M05 mutant killer: verify Shortcut is "Esc" and Escape closes launcher
            compare(launcher.dismissShortcut.sequence, "Esc", "Dismiss shortcut must be Esc");

            mockModel.visible = true;
            tryVerify(function() { return launcher.visible; });
            tryVerify(function() { return launcher.searchFieldItem.focus; });

            LayoutHelper.sendKey(launcher.searchFieldItem, Qt.Key_Escape);
            tryVerify(function() { return mockModel.closeCalled; }, 1000, "Esc must call model.close()");
            compare(launcher.visible, false);

            // Also verify the Shortcut component directly calls close
            mockModel.resetTracking();
            mockModel.visible = true;
            launcher.dismissShortcut.activated();
            compare(mockModel.closeCalled, true, "Shortcut activation must call model.close()");
        }

        function test_backdrop_click_closes() {
            // M06 mutant killer: verify backdrop click invokes model.close()
            mockModel.visible = true;
            tryVerify(function() { return launcher.visible; });

            LayoutHelper.sendMouseClick(launcher.backdropArea, 10, 10);
            tryVerify(function() { return mockModel.closeCalled; }, 1000, "Backdrop click must call model.close()");
            compare(launcher.visible, false);
        }

        function test_null_model_listview_hidden() {
            // Kills mutant: visible: root.launcherModel ? (root.launcherModel.count > 0) : true
            let comp = Qt.createComponent("../../styles/default/Launcher.qml");
            let nullLauncher = comp.createObject(null, { "launcherModel": null });
            verify(nullLauncher !== null);
            compare(nullLauncher.listView.visible, false, "ListView must be hidden when launcherModel is null");
            nullLauncher.destroy();
        }

        function test_results_list_visibility() {
            // Kills Launcher.qml:146 visibility mutants
            mockModel.visible = true;
            tryVerify(function() { return launcher.visible; });

            compare(mockModel.count, 0);
            compare(launcher.listView.visible, false, "ListView must be hidden when count is 0");

            mockModel.addItem("app-1", "App 1", "", "", "1");
            compare(mockModel.count, 1);
            tryVerify(function() { return launcher.listView.visible; }, 1000, "ListView must be visible when count > 0");

            mockModel.clearItems();
            compare(mockModel.count, 0);
            tryVerify(function() { return !launcher.listView.visible; }, 1000, "ListView must hide when items cleared");
        }

        function test_placeholder_text_states() {
            // Kills Launcher.qml:184 text formatting mutants
            mockModel.visible = true;
            tryVerify(function() { return launcher.visible; });

            compare(launcher.searchFieldItem.text, "");
            compare(launcher.placeholderTextItem.text, "Type to search applications");

            launcher.searchFieldItem.text = "nonexistent";
            compare(launcher.placeholderTextItem.text, "No applications found");

            launcher.searchFieldItem.text = "";
            compare(launcher.placeholderTextItem.text, "Type to search applications");
        }

        function test_delegate_item_selection_and_secondary_text() {
            // Kills Launcher.qml:164 & 166 delegate mutants
            mockModel.addItem("app-browser", "Browser", "Web Browser", "Explore web", "B");
            mockModel.addItem("app-term", "Terminal", "", "CLI terminal emulator", "T");
            mockModel.selectedIndex = 0;
            mockModel.visible = true;

            tryVerify(function() { return launcher.visible; });
            tryVerify(function() {
                let children = launcher.listView.contentItem.children;
                return children.length >= 2;
            }, 1000, "Delegates must be instantiated");

            let delegates = [];
            for (let i = 0; i < launcher.listView.contentItem.children.length; ++i) {
                let child = launcher.listView.contentItem.children[i];
                if (child.hasOwnProperty("selected") && child.hasOwnProperty("secondaryText")) {
                    delegates.push(child);
                }
            }
            compare(delegates.length, 2, "Expected 2 KynexListItem delegates");

            // Delegate 0 has non-empty genericName
            compare(delegates[0].secondaryText, "Web Browser");
            compare(delegates[0].selected, true, "Item 0 must be selected");
            compare(launcher.listView.currentIndex, 0, "ListView currentIndex must track selectedIndex 0");

            // Delegate 1 has empty genericName, so secondaryText is comment
            compare(delegates[1].secondaryText, "CLI tool" !== "" ? delegates[1].secondaryText : "");
            compare(delegates[1].secondaryText, "CLI terminal emulator");
            compare(delegates[1].selected, false, "Item 1 must not be selected");

            // Change selected index to 1
            mockModel.selectedIndex = 1;
            tryVerify(function() { return launcher.listView.currentIndex === 1; }, 1000, "ListView currentIndex must track selectedIndex 1");
            tryVerify(function() { return delegates[1].selected; });
            compare(delegates[0].selected, false);

            // Click item 0 to trigger launch(0)
            delegates[0].clicked();
            tryVerify(function() { return mockModel.launchCallCount === 1; });
            compare(mockModel.lastLaunchedIndex, 0);
        }

        function test_enter_launches_selected_bounds() {
            // Case A: Empty list
            mockModel.visible = true;
            tryVerify(function() { return launcher.visible; });
            tryVerify(function() { return launcher.searchFieldItem.focus; });

            compare(mockModel.count, 0);
            compare(mockModel.selectedIndex, -1);
            LayoutHelper.sendKey(launcher.searchFieldItem, Qt.Key_Return);
            wait(50);
            compare(mockModel.launchCallCount, 0, "Enter on empty list must not launch anything");

            // Case B: Populate items and select last index
            mockModel.addItem("app-term", "Terminal", "Terminal Emulator", "Run CLI commands", "T");
            mockModel.addItem("app-edit", "Editor", "Text Editor", "Edit plain text", "E");
            mockModel.addItem("app-web", "Browser", "Web Browser", "Explore the web", "B");
            compare(mockModel.count, 3);

            mockModel.selectedIndex = 2; // last index
            compare(mockModel.selectedIndex, 2);

            launcher.launching = false;
            LayoutHelper.sendKey(launcher.searchFieldItem, Qt.Key_Return);
            tryVerify(function() { return mockModel.launchCallCount === 1; }, 1000, "Enter on last index must launch once");
            compare(mockModel.lastLaunchedIndex, 2, "Must launch index 2");
        }

        function test_up_down_wrap() {
            mockModel.addItem("app-1", "App 1", "", "", "1");
            mockModel.addItem("app-2", "App 2", "", "", "2");
            mockModel.addItem("app-3", "App 3", "", "", "3");
            compare(mockModel.count, 3);

            mockModel.visible = true;
            mockModel.selectedIndex = 0;
            tryVerify(function() { return launcher.visible; });
            tryVerify(function() { return launcher.searchFieldItem.focus; });

            // Press Up from index 0 -> wraps to last index (2)
            LayoutHelper.sendKey(launcher.searchFieldItem, Qt.Key_Up);
            compare(mockModel.selectedIndex, 2, "Up from first index must wrap to last index");
            compare(launcher.listView.currentIndex, 2, "ListView currentIndex must follow selection");

            // Press Down from index 2 -> wraps back to first index (0)
            LayoutHelper.sendKey(launcher.searchFieldItem, Qt.Key_Down);
            compare(mockModel.selectedIndex, 0, "Down from last index must wrap to first index");

            // Press Down to index 1
            LayoutHelper.sendKey(launcher.searchFieldItem, Qt.Key_Down);
            compare(mockModel.selectedIndex, 1, "Down from index 0 must advance to index 1");
        }

        function test_no_double_launch_on_repeated_enter() {
            mockModel.addItem("app-1", "App 1", "", "", "1");
            mockModel.addItem("app-2", "App 2", "", "", "2");
            mockModel.selectedIndex = 0;
            mockModel.visible = true;
            tryVerify(function() { return launcher.visible; });
            tryVerify(function() { return launcher.searchFieldItem.focus; });

            // Send multiple rapid Enter key presses
            LayoutHelper.sendKey(launcher.searchFieldItem, Qt.Key_Return);
            LayoutHelper.sendKey(launcher.searchFieldItem, Qt.Key_Return);
            LayoutHelper.sendKey(launcher.searchFieldItem, Qt.Key_Enter);
            wait(50);

            compare(mockModel.launchCallCount, 1, "Repeated Enter must not trigger double launch");
            compare(mockModel.lastLaunchedIndex, 0);
            compare(launcher.launching, true);
        }

        function test_ctrl_p_pin_toggle() {
            mockModel.addItem("app-1", "App 1", "", "", "1", "icon-1", false);
            mockModel.selectedIndex = 0;
            mockModel.visible = true;
            tryVerify(function() { return launcher.visible; });
            tryVerify(function() { return launcher.searchFieldItem.focus; });

            // Verify delegate has correct iconSource from iconName
            let children = launcher.listView.contentItem.children;
            let foundIcon = false;
            for (let i = 0; i < children.length; ++i) {
                if (children[i].hasOwnProperty("iconSource")) {
                    compare(children[i].iconSource, "image://kynex-icon/icon-1?size=24&scale=1");
                    foundIcon = true;
                    break;
                }
            }
            verify(foundIcon, "Delegate must have iconSource bound from iconName");

            // Non-matching keys must NOT toggle pinned state
            LayoutHelper.sendKey(launcher.searchFieldItem, Qt.Key_O, Qt.ControlModifier);
            LayoutHelper.sendKey(launcher.searchFieldItem, Qt.Key_P, Qt.NoModifier);
            wait(20);
            let stillUnpinned = false;
            children = launcher.listView.contentItem.children;
            for (let i = 0; i < children.length; ++i) {
                if (children[i].hasOwnProperty("pinned")) {
                    stillUnpinned = (children[i].pinned === false);
                    break;
                }
            }
            verify(stillUnpinned, "Non-Ctrl+P key must not toggle pin");

            // Send Ctrl+P to toggle pin
            LayoutHelper.sendKey(launcher.searchFieldItem, Qt.Key_P, Qt.ControlModifier);
            tryVerify(function() {
                let items = launcher.listView.contentItem.children;
                for (let i = 0; i < items.length; ++i) {
                    if (items[i].hasOwnProperty("pinned")) {
                        return items[i].pinned === true;
                    }
                }
                return false;
            }, 1000, "Ctrl+P must toggle pinned status on selected item");
        }
    }
}
