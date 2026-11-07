import QtQuick
import Kynex.Style

FocusScope {
    id: root

    property string text: ""
    property url iconSource: ""
    property bool primary: false
    property bool spaceDown: false

    readonly property bool hovered: mouseArea.containsMouse
    readonly property bool down: mouseArea.pressed || (activeFocus && spaceDown)
    readonly property color borderColor: bg.border.color
    readonly property int borderWidth: bg.border.width
    readonly property bool labelVisible: label.visible

    signal clicked()
    signal middleClicked()
    signal pressed()
    signal released()

    activeFocusOnTab: true
    Accessible.role: Accessible.Button
    Accessible.name: text
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
        color: root.primary ? Style.colors.accent : Style.colors.surfaceRaised
        border.color: root.activeFocus ? Style.colors.focusRing : Style.colors.surfaceRaised
        border.width: root.activeFocus ? 1 : 0

        Rectangle {
            id: stateOverlay
            anchors.fill: parent
            radius: bg.radius
            color: root.primary ? Style.colors.surface : Style.colors.text
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
        anchors.centerIn: parent
        spacing: Style.spacing.small

        Image {
            id: iconImage
            visible: source.toString() !== ""
            source: root.iconSource
            anchors.verticalCenter: parent.verticalCenter
            width: visible ? Style.typography.basePx + Style.spacing.unit : 0
            height: visible ? Style.typography.basePx + Style.spacing.unit : 0
            fillMode: Image.PreserveAspectFit
        }

        Text {
            id: label
            visible: text !== ""
            text: root.text
            elide: Text.ElideRight
            width: root.width > (Style.spacing.medium + Style.spacing.medium) ? Math.min(implicitWidth, root.width - Style.spacing.medium - Style.spacing.medium - (iconImage.visible ? iconImage.width + Style.spacing.small : 0)) : implicitWidth
            font.family: Style.typography.family
            font.pixelSize: Style.typography.basePx
            color: root.primary ? Style.colors.surface : Style.colors.text
            anchors.verticalCenter: parent.verticalCenter
        }
    }

    MouseArea {
        id: mouseArea
        anchors.fill: parent
        hoverEnabled: true
        cursorShape: Qt.PointingHandCursor
        acceptedButtons: Qt.LeftButton | Qt.MiddleButton
        enabled: root.enabled

        onPressed: (mouse) => {
            root.forceActiveFocus();
            if (mouse.button === Qt.LeftButton) {
                root.pressed();
            }
        }
        onReleased: (mouse) => {
            if (mouse.button === Qt.LeftButton) {
                root.released();
            }
        }
        onClicked: (mouse) => {
            if (mouse.button === Qt.MiddleButton) {
                root.middleClicked();
            } else {
                root.clicked();
            }
        }
    }

    Keys.onReturnPressed: (event) => {
        if (root.enabled) {
            root.clicked();
            event.accepted = true;
        }
    }

    Keys.onEnterPressed: (event) => {
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
