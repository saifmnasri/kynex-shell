pragma ComponentBehavior: Bound
import QtQuick
import Kynex.Shell
import Kynex.Controls

LayerSurface {
    // qmllint disable Quick.attached-property-type
    id: root

    property TrayMenuModel menuModel: null
    readonly property var model: root.menuModel
    readonly property alias menuContainer: menuContainer
    readonly property alias kynexMenu: kynexMenu
    readonly property alias backdropMouse: backdropMouse
    readonly property alias escShortcut: escShortcut
    readonly property alias leftShortcut: leftShortcut
    readonly property alias rightShortcut: rightShortcut

    visible: root.menuModel ? root.menuModel.visible : false
    width: Screen.width
    height: Screen.height

    layer: LayerSurface.LayerOverlay
    anchors: LayerSurface.AnchorTop + LayerSurface.AnchorBottom + LayerSurface.AnchorLeft + LayerSurface.AnchorRight
    exclusiveZone: 0
    leftMargin: 0
    rightMargin: 0
    topMargin: 0
    bottomMargin: 0
    keyboardInteractivity: LayerSurface.KeyboardInteractivityOnDemand
    scope: "menu-overlay"

    LayoutMirroring.enabled: Qt.locale().textDirection === Qt.RightToLeft
    LayoutMirroring.childrenInherit: true

    // Click outside menu dismisses menu
    MouseArea {
        id: backdropMouse
        anchors.fill: parent
        onClicked: {
            if (root.model) {
                root.model.close();
            }
        }
    }

    // Dismiss or close submenu on Escape
    Shortcut {
        id: escShortcut
        sequence: "Esc"
        onActivated: {
            if (root.model) {
                root.model.closeSubmenu();
            }
        }
    }

    Shortcut {
        id: upShortcut
        sequence: "Up"
        onActivated: {
            if (root.model) {
                root.model.selectPrev();
            }
        }
    }

    Shortcut {
        id: downShortcut
        sequence: "Down"
        onActivated: {
            if (root.model) {
                root.model.selectNext();
            }
        }
    }

    Shortcut {
        id: leftShortcut
        sequence: "Left"
        onActivated: {
            if (root.model) {
                if (root.LayoutMirroring.enabled) {
                    root.model.openSubmenu();
                } else {
                    root.model.closeSubmenu();
                }
            }
        }
    }

    Shortcut {
        id: rightShortcut
        sequence: "Right"
        onActivated: {
            if (root.model) {
                if (root.LayoutMirroring.enabled) {
                    root.model.closeSubmenu();
                } else {
                    root.model.openSubmenu();
                }
            }
        }
    }

    Shortcut {
        id: homeShortcut
        sequence: "Home"
        onActivated: {
            if (root.model) {
                root.model.selectFirst();
            }
        }
    }

    Shortcut {
        id: endShortcut
        sequence: "End"
        onActivated: {
            if (root.model) {
                root.model.selectLast();
            }
        }
    }

    Shortcut {
        id: enterShortcut
        sequence: "Return"
        onActivated: {
            if (root.model) {
                root.model.activateSelected();
            }
        }
    }

    Shortcut {
        id: spaceShortcut
        sequence: "Space"
        onActivated: {
            if (root.model) {
                root.model.activateSelected();
            }
        }
    }

    Item {
        id: menuContainer
        x: root.menuModel ? root.menuModel.menuX : 0
        y: root.menuModel ? root.menuModel.menuY : 0
        width: kynexMenu.width
        height: kynexMenu.height

        // Consume clicks inside the menu card so backdrop doesn't trigger
        MouseArea {
            anchors.fill: parent
        }

        KynexMenu {
            id: kynexMenu
            model: root.menuModel
            selectedIndex: root.menuModel ? root.menuModel.selectedIndex : -1

            onItemTriggered: function(id) {
                if (root.model) {
                    root.model.activateItem(id);
                }
            }

            onSubmenuOpened: function(id) {
                if (root.model) {
                    root.model.openSubmenu();
                }
            }

            onSubmenuClosed: function() {
                if (root.model) {
                    root.model.closeSubmenu();
                }
            }
        }
    }
}
