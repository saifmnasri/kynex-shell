import QtQuick
import Kynex.Style

FocusScope {
    id: root

    property bool checked: false
    property string text: ""
    property bool spaceDown: false

    readonly property bool hovered: mouseArea.containsMouse
    readonly property bool down: root.enabled && (mouseArea.pressed || (activeFocus && spaceDown))
    readonly property color borderColor: track.border.color
    readonly property int borderWidth: track.border.width
    readonly property bool labelVisible: label.visible
    readonly property int cursorShape: mouseArea.cursorShape

    signal toggled(bool checked)

    function toggle(): void {
        if (root.enabled) {
            root.checked = !root.checked;
            root.toggled(root.checked);
        }
    }

    activeFocusOnTab: true
    Accessible.role: Accessible.CheckBox
    Accessible.name: text
    Accessible.checked: checked
    Accessible.focusable: true
    Accessible.onPressAction: root.toggle()

    LayoutMirroring.enabled: Qt.locale().textDirection === Qt.RightToLeft
    LayoutMirroring.childrenInherit: true

    implicitWidth: switchRow.implicitWidth
    implicitHeight: Math.max(track.implicitHeight, label.implicitHeight)
    width: implicitWidth
    height: implicitHeight

    opacity: enabled ? 1 : Style.opacity.disabled

    Row {
        id: switchRow
        anchors.verticalCenter: parent.verticalCenter
        spacing: Style.spacing.small

        Rectangle {
            id: track
            implicitWidth: Style.spacing.panel
            implicitHeight: Style.spacing.medium + Style.spacing.medium
            radius: Style.spacing.medium
            color: root.checked ? Style.colors.accent : Style.colors.surfaceRaised
            border.color: root.activeFocus ? Style.colors.focusRing : Style.colors.surfaceRaised
            border.width: root.activeFocus ? 1 : 0

            Behavior on color {
                ColorAnimation {
                    duration: Style.motion.fastMs
                }
            }

            Rectangle {
                id: overlay
                anchors.fill: parent
                radius: track.radius
                color: root.checked ? Style.colors.surface : Style.colors.text
                opacity: root.down ? Style.opacity.pressed : (root.hovered ? Style.opacity.hover : 0)

                Behavior on opacity {
                    NumberAnimation {
                        duration: Style.motion.fastMs
                    }
                }
            }

            Rectangle {
                id: thumb
                width: track.height - 1 - 1
                height: track.height - 1 - 1
                radius: Style.radius.medium
                y: 1
                x: root.checked ? (track.width - width - 1) : 1
                color: root.checked ? Style.colors.surface : Style.colors.text

                Behavior on x {
                    NumberAnimation {
                        duration: Style.motion.fastMs
                    }
                }
            }
        }

        Text {
            id: label
            visible: text !== ""
            text: root.text
            font.family: Style.typography.family
            font.pixelSize: Style.typography.basePx
            color: Style.colors.text
            anchors.verticalCenter: parent.verticalCenter
        }
    }

    MouseArea {
        id: mouseArea
        anchors.fill: parent
        hoverEnabled: true
        cursorShape: root.enabled ? Qt.PointingHandCursor : Qt.ArrowCursor

        onPressed: {
            root.forceActiveFocus();
        }
        onClicked: {
            root.toggle();
        }
    }

    Keys.onReturnPressed: (event) => {
        root.toggle();
        event.accepted = true;
    }

    Keys.onEnterPressed: (event) => {
        root.toggle();
        event.accepted = true;
    }

    Keys.onSpacePressed: (event) => {
        if (!event.isAutoRepeat) {
            root.spaceDown = true;
            event.accepted = true;
        }
    }

    Keys.onReleased: (event) => {
        if (event.key === Qt.Key_Space && root.spaceDown) {
            root.spaceDown = false;
            root.toggle();
            event.accepted = true;
        }
    }
}
