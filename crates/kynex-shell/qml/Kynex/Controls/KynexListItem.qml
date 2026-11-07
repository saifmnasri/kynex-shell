import QtQuick
import Kynex.Style

FocusScope {
    id: root

    property string text: ""
    property string secondaryText: ""
    property url iconSource: ""
    property bool selected: false
    property bool spaceDown: false

    readonly property bool hovered: mouseArea.containsMouse
    readonly property bool down: mouseArea.pressed || (activeFocus && spaceDown)

    signal clicked()
    signal pressed()
    signal released()

    activeFocusOnTab: true
    Accessible.role: Accessible.ListItem
    Accessible.name: secondaryText !== "" ? (text + " - " + secondaryText) : text
    Accessible.focusable: true
    Accessible.onPressAction: root.clicked()

    LayoutMirroring.enabled: Qt.locale().textDirection === Qt.RightToLeft
    LayoutMirroring.childrenInherit: true

    implicitWidth: contentRow.implicitWidth + Style.spacing.medium + Style.spacing.medium
    implicitHeight: Math.max(Style.spacing.panel, contentRow.implicitHeight + Style.spacing.small + Style.spacing.small)
    width: implicitWidth
    height: implicitHeight

    opacity: enabled ? 1 : Style.opacity.disabled

    Rectangle {
        id: bg
        anchors.fill: parent
        radius: Style.radius.small
        color: root.selected ? Style.colors.surfaceRaised : Style.colors.surface
        border.color: root.activeFocus ? Style.colors.focusRing : (root.selected ? Style.colors.accent : Style.colors.surface)
        border.width: root.activeFocus ? 1 : (root.selected ? 1 : 0)

        Rectangle {
            id: stateOverlay
            anchors.fill: parent
            radius: bg.radius
            color: root.selected ? Style.colors.accent : Style.colors.text
            opacity: root.down ? Style.opacity.pressed : (root.hovered ? Style.opacity.hover : 0)

            Behavior on opacity {
                NumberAnimation {
                    duration: Style.motion.fastMs
                }
            }
        }
    }

    Row {
        id: contentRow
        anchors.verticalCenter: parent.verticalCenter
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.leftMargin: Style.spacing.medium
        anchors.rightMargin: Style.spacing.medium
        spacing: Style.spacing.medium

        Image {
            id: iconImage
            visible: source.toString() !== ""
            source: root.iconSource
            anchors.verticalCenter: parent.verticalCenter
            width: visible ? Style.typography.basePx + Style.spacing.unit : 0
            height: visible ? Style.typography.basePx + Style.spacing.unit : 0
            fillMode: Image.PreserveAspectFit
        }

        Column {
            anchors.verticalCenter: parent.verticalCenter
            spacing: 0

            Text {
                id: label
                visible: text !== ""
                text: root.text
                font.family: Style.typography.family
                font.pixelSize: Style.typography.basePx
                color: Style.colors.text
            }

            Text {
                id: subLabel
                visible: text !== ""
                text: root.secondaryText
                font.family: Style.typography.family
                font.pixelSize: Style.typography.basePx - Style.spacing.unit
                color: Style.colors.textMuted
            }
        }
    }

    MouseArea {
        id: mouseArea
        anchors.fill: parent
        hoverEnabled: true
        cursorShape: Qt.PointingHandCursor
        enabled: root.enabled

        onPressed: {
            root.forceActiveFocus();
            root.pressed();
        }
        onReleased: root.released()
        onClicked: root.clicked()
    }

    Keys.onReturnPressed: (event) => {
        if (root.enabled) {
            root.clicked();
            event.accepted = true;
        }
    }

    Keys.onSpacePressed: (event) => {
        if (root.enabled && !event.isAutoRepeat) {
            root.spaceDown = true;
            root.pressed();
            event.accepted = true;
        }
    }

    Keys.onReleased: (event) => {
        if (root.enabled && event.key === Qt.Key_Space && root.spaceDown) {
            root.spaceDown = false;
            root.released();
            root.clicked();
            event.accepted = true;
        }
    }
}

