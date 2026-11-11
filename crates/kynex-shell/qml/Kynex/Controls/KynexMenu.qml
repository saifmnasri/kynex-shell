pragma ComponentBehavior: Bound
import QtQuick
import Kynex.Style

FocusScope {
    id: root

    property var model: null
    property int selectedIndex: -1
    property real minimumMenuWidth: Style.spacing.panel + Style.spacing.panel + Style.spacing.panel + Style.spacing.panel
    readonly property alias count: repeater.count
    function itemAt(idx: int): Item { return repeater.itemAt(idx); }

    signal itemTriggered(int id)
    signal submenuOpened(int id)
    signal submenuClosed()

    Accessible.role: Accessible.PopupMenu
    Accessible.name: "Menu"

    LayoutMirroring.enabled: Qt.locale().textDirection === Qt.RightToLeft
    LayoutMirroring.childrenInherit: true

    implicitWidth: Math.max(minimumMenuWidth, menuColumn.implicitWidth + Style.spacing.small + Style.spacing.small)
    implicitHeight: menuColumn.implicitHeight + Style.spacing.small + Style.spacing.small
    width: implicitWidth
    height: implicitHeight

    opacity: enabled ? 1 : Style.opacity.disabled

    Rectangle {
        id: bg
        anchors.fill: parent
        radius: Style.radius.medium
        color: Style.colors.surfaceRaised
        border.color: Style.colors.surface
        border.width: 1

        Column {
            id: menuColumn
            anchors.fill: parent
            anchors.margins: Style.spacing.small
            spacing: 0

            Repeater {
                id: repeater
                model: root.model

                delegate: KynexMenuItem {
                    id: itemDelegate
                    required property int index
                    required property int id
                    required property string label
                    required property string shortcut
                    required property int kind
                    required property int toggle
                    required property int toggleState
                    required property int disposition
                    required property bool hasSubmenu
                    required property string iconName
                    required property bool enabled

                    width: menuColumn.width
                    text: itemDelegate.label
                    shortcutText: itemDelegate.shortcut
                    kind: itemDelegate.kind
                    toggleType: itemDelegate.toggle
                    toggleState: itemDelegate.toggleState
                    disposition: itemDelegate.disposition
                    hasSubmenu: itemDelegate.hasSubmenu
                    iconSource: itemDelegate.iconName.length > 0 ? ("image://kynex-icon/" + itemDelegate.iconName) : ""
                    selected: itemDelegate.index === root.selectedIndex

                    onClicked: {
                        if (itemDelegate.hasSubmenu) {
                            root.submenuOpened(itemDelegate.id);
                        } else {
                            root.itemTriggered(itemDelegate.id);
                        }
                    }
                }
            }
        }
    }
}
