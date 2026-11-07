import QtQuick
import Kynex.Style

FocusScope {
    id: root

    property string text: ""
    property url iconSource: ""
    property string letterBadge: ""
    property bool primary: false
    property bool isRunning: false
    property int windowCount: 0
    property bool showRunningIndicator: false
    property bool spaceDown: false

    readonly property bool hovered: mouseArea.containsMouse
    readonly property bool down: root.enabled && (mouseArea.pressed || (activeFocus && spaceDown))
    readonly property color borderColor: bg.border.color
    readonly property int borderWidth: bg.border.width
    readonly property bool labelVisible: label.visible
    readonly property int cursorShape: mouseArea.cursorShape
    readonly property color backgroundColor: bg.color
    readonly property color textColor: label.color
    readonly property real iconX: iconImage.x
    readonly property real labelX: label.x
    readonly property bool iconVisible: iconImage.visible
    readonly property real iconWidth: iconImage.width
    readonly property real iconHeight: iconImage.height
    readonly property bool badgeVisible: badgeRect.visible

    signal clicked()
    signal middleClicked()
    signal pressed()
    signal released()

    function click(): void {
        if (root.enabled) {
            root.clicked();
        }
    }

    function middleClick(): void {
        if (root.enabled) {
            root.middleClicked();
        }
    }

    function press(): void {
        if (root.enabled) {
            root.pressed();
        }
    }

    function release(): void {
        if (root.enabled) {
            root.released();
        }
    }

    activeFocusOnTab: true
    Accessible.role: Accessible.Button
    Accessible.name: text
    Accessible.focusable: true
    Accessible.onPressAction: root.click()

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
            visible: source.toString() !== "" && status === Image.Ready
            source: root.iconSource
            anchors.verticalCenter: parent.verticalCenter
            width: (source.toString() !== "") ? Style.typography.basePx + Style.spacing.unit : 0
            height: (source.toString() !== "") ? Style.typography.basePx + Style.spacing.unit : 0
            fillMode: Image.PreserveAspectFit
            asynchronous: true
        }

        Rectangle {
            id: badgeRect
            visible: root.letterBadge.length > 0 && (!iconImage.visible || iconImage.status === Image.Error || iconImage.status === Image.Null)
            width: Style.typography.basePx + Style.spacing.unit
            height: Style.typography.basePx + Style.spacing.unit
            radius: Style.radius.small
            color: root.primary ? Style.colors.surface : Style.colors.surfaceRaised
            anchors.verticalCenter: parent.verticalCenter

            Text {
                id: badgeText
                anchors.centerIn: parent
                text: root.letterBadge
                textFormat: Text.PlainText
                font.family: Style.typography.family
                font.pixelSize: Style.typography.basePx - Style.spacing.unit
                font.bold: true
                color: root.primary ? Style.colors.accent : Style.colors.text
            }
        }

        Text {
            id: label
            visible: text !== ""
            text: root.text
            font.family: Style.typography.family
            font.pixelSize: Style.typography.basePx
            color: root.primary ? Style.colors.surface : Style.colors.text
            anchors.verticalCenter: parent.verticalCenter
        }
    }

    Rectangle {
        id: runningIndicator
        visible: root.showRunningIndicator
        anchors.bottom: parent.bottom
        anchors.bottomMargin: Style.spacing.unit / (1 + 1)
        anchors.horizontalCenter: parent.horizontalCenter
        width: root.primary ? Style.spacing.large : Style.spacing.small
        height: Style.spacing.unit / (1 + 1)
        radius: Style.radius.small
        color: root.primary ? Style.colors.surface : Style.colors.accent
    }

    Rectangle {
        id: countBadge
        visible: root.windowCount > 1
        anchors.top: parent.top
        anchors.topMargin: Style.spacing.unit / (1 + 1)
        anchors.right: parent.right
        anchors.rightMargin: Style.spacing.unit / (1 + 1)
        width: countText.implicitWidth + Style.spacing.unit + Style.spacing.unit
        height: countText.implicitHeight + Style.spacing.unit
        radius: Style.radius.small
        color: root.primary ? Style.colors.surface : Style.colors.surfaceRaised

        Text {
            id: countText
            anchors.centerIn: parent
            text: root.windowCount.toString()
            textFormat: Text.PlainText
            font.family: Style.typography.family
            font.pixelSize: Style.typography.basePx - Style.spacing.unit
            font.bold: true
            color: root.primary ? Style.colors.accent : Style.colors.text
        }
    }

    MouseArea {
        id: mouseArea
        anchors.fill: parent
        hoverEnabled: true
        cursorShape: root.enabled ? Qt.PointingHandCursor : Qt.ArrowCursor
        acceptedButtons: Qt.LeftButton | Qt.MiddleButton

        onPressed: (mouse) => {
            root.forceActiveFocus();
            if (mouse.button === Qt.LeftButton) {
                root.press();
            }
        }
        onReleased: (mouse) => {
            if (mouse.button === Qt.LeftButton) {
                root.release();
            }
        }
        onClicked: (mouse) => {
            if (mouse.button === Qt.MiddleButton) {
                root.middleClick();
            } else {
                root.click();
            }
        }
    }

    Keys.onReturnPressed: (event) => {
        root.click();
        event.accepted = true;
    }

    Keys.onEnterPressed: (event) => {
        root.click();
        event.accepted = true;
    }

    Keys.onSpacePressed: (event) => {
        if (!event.isAutoRepeat) {
            root.spaceDown = true;
            root.press();
            event.accepted = true;
        }
    }

    Keys.onReleased: (event) => {
        if (event.key === Qt.Key_Space && root.spaceDown) {
            root.spaceDown = false;
            root.release();
            root.click();
            event.accepted = true;
        }
    }
}
