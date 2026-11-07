import QtQuick
import Kynex.Style

FocusScope {
    id: root

    property string text: ""
    property string secondaryText: ""
    property url iconSource: ""
    property string letterBadge: ""
    property bool pinned: false
    property bool selected: false
    property bool spaceDown: false

    readonly property bool hovered: mouseArea.containsMouse
    readonly property bool down: root.enabled && (mouseArea.pressed || (activeFocus && spaceDown))
    readonly property color borderColor: bg.border.color
    readonly property int borderWidth: bg.border.width
    readonly property bool labelVisible: label.visible
    readonly property bool secondaryLabelVisible: subLabel.visible
    readonly property int textFormat: label.textFormat
    readonly property int secondaryTextFormat: subLabel.textFormat
    readonly property int badgeTextFormat: badgeText.textFormat
    readonly property int cursorShape: mouseArea.cursorShape
    readonly property color backgroundColor: bg.color
    readonly property color textColor: label.color
    readonly property real iconX: iconImage.x
    readonly property real labelX: textColumn.x
    readonly property bool iconVisible: iconImage.visible
    readonly property real iconWidth: iconImage.width
    readonly property real iconHeight: iconImage.height
    readonly property bool badgeVisible: badgeRect.visible

    signal clicked()
    signal pressed()
    signal released()

    function click(): void {
        if (root.enabled) {
            root.clicked();
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
    Accessible.role: Accessible.ListItem
    Accessible.name: secondaryText !== "" ? (text + " - " + secondaryText) : text
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
        anchors.right: pinBadge.visible ? pinBadge.left : parent.right
        anchors.leftMargin: Style.spacing.medium
        anchors.rightMargin: Style.spacing.medium
        spacing: Style.spacing.medium

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
            width: Style.typography.basePx + Style.spacing.unit + Style.spacing.unit
            height: Style.typography.basePx + Style.spacing.unit + Style.spacing.unit
            radius: Style.radius.small
            color: root.selected ? Style.colors.accent : Style.colors.surfaceRaised
            anchors.verticalCenter: parent.verticalCenter

            Text {
                id: badgeText
                anchors.centerIn: parent
                text: root.letterBadge
                textFormat: Text.PlainText
                font.family: Style.typography.family
                font.pixelSize: Style.typography.basePx - Style.spacing.unit
                font.bold: true
                color: root.selected ? Style.colors.surface : Style.colors.text
            }
        }

        Column {
            id: textColumn
            anchors.verticalCenter: parent.verticalCenter
            spacing: 0

            Text {
                id: label
                visible: text !== ""
                text: root.text
                textFormat: Text.PlainText
                font.family: Style.typography.family
                font.pixelSize: Style.typography.basePx
                color: Style.colors.text
            }

            Text {
                id: subLabel
                visible: text !== ""
                text: root.secondaryText
                textFormat: Text.PlainText
                font.family: Style.typography.family
                font.pixelSize: Style.typography.basePx - Style.spacing.unit
                color: Style.colors.textMuted
            }
        }
    }

    Rectangle {
        id: pinBadge
        visible: root.pinned
        anchors.right: parent.right
        anchors.rightMargin: Style.spacing.medium
        anchors.verticalCenter: parent.verticalCenter
        width: pinText.implicitWidth + Style.spacing.unit + Style.spacing.unit
        height: pinText.implicitHeight + Style.spacing.unit
        radius: Style.radius.small
        color: root.selected ? Style.colors.accent : Style.colors.surfaceRaised

        Text {
            id: pinText
            anchors.centerIn: parent
            text: "📌"
            font.family: Style.typography.family
            font.pixelSize: Style.typography.basePx - Style.spacing.unit
        }
    }

    MouseArea {
        id: mouseArea
        anchors.fill: parent
        hoverEnabled: true
        cursorShape: root.enabled ? Qt.PointingHandCursor : Qt.ArrowCursor

        onPressed: {
            root.forceActiveFocus();
            root.press();
        }
        onReleased: root.release()
        onClicked: root.click()
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

