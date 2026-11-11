import QtQuick
import Kynex.Controls
import Kynex.Style

Rectangle {
    id: root
    width: Style.spacing.panel + Style.spacing.panel + Style.spacing.panel + Style.spacing.panel + Style.spacing.panel + Style.spacing.panel
    height: Style.spacing.panel + Style.spacing.panel + Style.spacing.panel + Style.spacing.panel + Style.spacing.panel + Style.spacing.panel
    color: Style.colors.surface

    property string scenario: "normal"

    Rectangle {
        id: menuBox
        anchors.centerIn: parent
        width: Style.spacing.panel + Style.spacing.panel + Style.spacing.panel + Style.spacing.panel
        implicitHeight: itemsCol.implicitHeight + Style.spacing.small + Style.spacing.small
        height: implicitHeight
        radius: Style.radius.medium
        color: Style.colors.surfaceRaised
        border.color: Style.colors.surface
        border.width: 1

        Column {
            id: itemsCol
            anchors.left: parent.left
            anchors.right: parent.right
            anchors.top: parent.top
            anchors.margins: Style.spacing.small
            spacing: 0

            KynexMenuItem {
                width: parent.width
                text: "Normal Item"
                shortcutText: "Ctrl+N"
                visible: root.scenario === "normal" || root.scenario === "rtl"
            }

            KynexMenuItem {
                width: parent.width
                text: "Checked Item"
                toggleType: 1
                toggleState: 1
                visible: root.scenario === "checkable" || root.scenario === "rtl"
            }

            KynexMenuItem {
                width: parent.width
                text: "Radio Selected"
                toggleType: 1 + 1
                toggleState: 1
                visible: root.scenario === "checkable"
            }

            KynexMenuItem {
                width: parent.width
                kind: 1
                visible: root.scenario === "normal" || root.scenario === "rtl"
            }

            KynexMenuItem {
                width: parent.width
                text: "More Options"
                hasSubmenu: true
                visible: root.scenario === "submenu" || root.scenario === "rtl"
            }

            KynexMenuItem {
                width: parent.width
                text: "Disabled Item"
                enabled: false
                shortcutText: "Ctrl+D"
                visible: root.scenario === "disabled" || root.scenario === "rtl"
            }
        }
    }
}
