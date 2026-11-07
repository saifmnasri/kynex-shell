import QtQuick
import Kynex.Style

FocusScope {
    id: root

    property real value: 0
    property real stepSize: 1 / Style.radius.large

    readonly property bool hovered: mouseArea.containsMouse
    readonly property bool down: mouseArea.pressed
    readonly property color borderColor: handle.border.color
    readonly property int borderWidth: handle.border.width

    signal moved(real value)

    function setValueClamped(v: real): void {
        let clamped = Math.max(0, Math.min(1, v));
        if (root.value !== clamped) {
            root.value = clamped;
            root.moved(clamped);
        }
    }

    function increase(): void {
        if (root.enabled) {
            root.setValueClamped(root.value + root.stepSize);
        }
    }

    function decrease(): void {
        if (root.enabled) {
            root.setValueClamped(root.value - root.stepSize);
        }
    }

    activeFocusOnTab: true
    Accessible.role: Accessible.Slider
    Accessible.name: "Slider"
    Accessible.focusable: true
    Accessible.onIncreaseAction: root.increase()
    Accessible.onDecreaseAction: root.decrease()

    LayoutMirroring.enabled: Qt.locale().textDirection === Qt.RightToLeft
    LayoutMirroring.childrenInherit: true

    implicitWidth: Style.spacing.panel + Style.spacing.panel + Style.spacing.panel
    implicitHeight: Style.spacing.large + Style.spacing.small
    width: implicitWidth
    height: implicitHeight

    opacity: enabled ? 1 : Style.opacity.disabled

    Rectangle {
        id: track
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.verticalCenter: parent.verticalCenter
        height: Style.spacing.unit
        radius: Style.radius.small
        color: Style.colors.surfaceRaised

        Rectangle {
            id: filledTrack
            anchors.left: parent.left
            anchors.top: parent.top
            anchors.bottom: parent.bottom
            width: Math.max(0, Math.min(track.width, root.value * track.width))
            radius: track.radius
            color: Style.colors.accent
        }
    }

    Rectangle {
        id: handle
        width: Style.spacing.large
        height: Style.spacing.large
        radius: Style.radius.medium
        anchors.verticalCenter: track.verticalCenter
        x: Math.max(0, Math.min(track.width - width, root.value * (track.width - width)))
        color: Style.colors.surfaceRaised
        border.color: root.activeFocus ? Style.colors.focusRing : Style.colors.accent
        border.width: 1

        Rectangle {
            id: handleOverlay
            anchors.fill: parent
            radius: handle.radius
            color: Style.colors.text
            opacity: root.down ? Style.opacity.pressed : (root.hovered ? Style.opacity.hover : 0)

            Behavior on opacity {
                NumberAnimation {
                    duration: Style.motion.fastMs
                }
            }
        }
    }

    MouseArea {
        id: mouseArea
        anchors.fill: parent
        hoverEnabled: true
        cursorShape: Qt.PointingHandCursor
        enabled: root.enabled

        function updateFromMouse(mouseX: real): void {
            let available = track.width - handle.width;
            if (available > 0) {
                let frac = mouseX / track.width;
                if (root.LayoutMirroring.enabled) {
                    frac = 1 - frac;
                }
                root.setValueClamped(frac);
            }
        }

        onPressed: (mouse) => {
            root.forceActiveFocus();
            updateFromMouse(mouse.x);
        }

        onPositionChanged: (mouse) => {
            if (pressed) {
                updateFromMouse(mouse.x);
            }
        }
    }

    Keys.onLeftPressed: (event) => {
        if (root.enabled) {
            if (root.LayoutMirroring.enabled) {
                root.increase();
            } else {
                root.decrease();
            }
            event.accepted = true;
        }
    }

    Keys.onRightPressed: (event) => {
        if (root.enabled) {
            if (root.LayoutMirroring.enabled) {
                root.decrease();
            } else {
                root.increase();
            }
            event.accepted = true;
        }
    }

    Keys.onDownPressed: (event) => {
        if (root.enabled) {
            root.decrease();
            event.accepted = true;
        }
    }

    Keys.onUpPressed: (event) => {
        if (root.enabled) {
            root.increase();
            event.accepted = true;
        }
    }

    Keys.onPressed: (event) => {
        if (!root.enabled) {
            return;
        }
        if (event.key === Qt.Key_Home) {
            root.setValueClamped(0);
            event.accepted = true;
        } else if (event.key === Qt.Key_End) {
            root.setValueClamped(1);
            event.accepted = true;
        }
    }
}
