import QtQuick
import Kynex.Style

FocusScope {
    id: root

    property string text: ""
    property string shortcutText: ""
    property int kind: 0
    property int toggleType: 0
    property int toggleState: 0
    property int disposition: 0
    property bool hasSubmenu: false
    property url iconSource: ""
    property bool selected: false
    property bool spaceDown: false

    readonly property bool isSeparator: kind === 1
    readonly property bool hovered: mouseArea.containsMouse
    readonly property bool down: root.enabled && !root.isSeparator && (mouseArea.pressed || (activeFocus && spaceDown))
    readonly property int cursorShape: mouseArea.cursorShape
    readonly property alias iconVisible: iconImage.visible
    readonly property alias borderColor: bg.border.color
    readonly property alias borderWidth: bg.border.width
    readonly property color textColor: {
        if (disposition === 1) {
            return Style.colors.textMuted;
        } else if (disposition === (1 + 1)) {
            return Style.colors.warning;
        } else if (disposition === (1 + 1 + 1)) {
            return Style.colors.danger;
        }
        return Style.colors.text;
    }
    readonly property string checkmarkText: {
        if (toggleType === 1) {
            return toggleState === 1 ? "✓" : "";
        } else if (toggleType === (1 + 1)) {
            return toggleState === 1 ? "●" : "○";
        }
        return "";
    }
    readonly property string submenuArrowText: LayoutMirroring.enabled ? "◀" : "▶"
    readonly property real labelX: labelText.x

    signal clicked()
    signal pressed()
    signal released()

    function click(): void {
        if (root.enabled && !root.isSeparator) {
            root.clicked();
        }
    }

    function press(): void {
        if (root.enabled && !root.isSeparator) {
            root.pressed();
        }
    }

    function release(): void {
        if (root.enabled && !root.isSeparator) {
            root.released();
        }
    }

    activeFocusOnTab: !isSeparator
    Accessible.role: isSeparator ? Accessible.Separator : Accessible.MenuItem
    Accessible.name: text
    Accessible.focusable: enabled && !isSeparator
    Accessible.onPressAction: root.click()

    LayoutMirroring.enabled: Qt.locale().textDirection === Qt.RightToLeft
    LayoutMirroring.childrenInherit: true

    implicitWidth: isSeparator ? Style.spacing.panel : (Style.spacing.panel + Style.spacing.panel + Style.spacing.panel + Style.spacing.panel)
    implicitHeight: isSeparator ? 1 : Math.max(Style.spacing.panel - Style.spacing.medium, Style.typography.basePx + Style.spacing.medium + Style.spacing.unit)
    width: implicitWidth
    height: implicitHeight

    opacity: enabled ? 1 : Style.opacity.disabled

    // Background highlight for normal items
    Rectangle {
        id: bg
        visible: !root.isSeparator
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

    // Separator line
    Rectangle {
        id: sepLine
        visible: root.isSeparator
        anchors.verticalCenter: parent.verticalCenter
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.leftMargin: Style.spacing.small
        anchors.rightMargin: Style.spacing.small
        height: 1
        color: Style.colors.surface
    }

    // Interactive item content area
    Item {
        id: contentArea
        visible: !root.isSeparator
        anchors.fill: parent
        anchors.leftMargin: Style.spacing.medium
        anchors.rightMargin: Style.spacing.medium

        // Toggle mark (checkmark or radio bullet)
        Text {
            id: markText
            visible: root.toggleType !== 0
            width: visible ? Style.typography.basePx : 0
            text: root.checkmarkText
            textFormat: Text.PlainText
            font.family: Style.typography.family
            font.pixelSize: Style.typography.basePx
            color: root.textColor
            anchors.left: parent.left
            anchors.verticalCenter: parent.verticalCenter
        }

        // Optional icon image
        Image {
            id: iconImage
            visible: source.toString() !== "" && status === Image.Ready
            source: root.iconSource
            width: visible ? Style.typography.basePx : 0
            height: visible ? Style.typography.basePx : 0
            fillMode: Image.PreserveAspectFit
            asynchronous: true
            anchors.left: markText.visible ? markText.right : parent.left
            anchors.leftMargin: markText.visible ? Style.spacing.small : 0
            anchors.verticalCenter: parent.verticalCenter
        }

        // Submenu indicator arrow (anchored to trailing edge)
        Text {
            id: arrowText
            visible: root.hasSubmenu
            text: root.submenuArrowText
            textFormat: Text.PlainText
            font.family: Style.typography.family
            font.pixelSize: Style.typography.basePx
            color: Style.colors.textMuted
            anchors.right: parent.right
            anchors.verticalCenter: parent.verticalCenter
        }

        // Shortcut text (anchored to left of arrow or trailing edge)
        Text {
            id: shortcutLabel
            visible: root.shortcutText !== ""
            text: root.shortcutText
            textFormat: Text.PlainText
            font.family: Style.typography.family
            font.pixelSize: Style.typography.basePx - Style.spacing.unit
            color: Style.colors.textMuted
            anchors.right: arrowText.visible ? arrowText.left : parent.right
            anchors.rightMargin: arrowText.visible ? Style.spacing.small : 0
            anchors.verticalCenter: parent.verticalCenter
        }

        // Main text label (fills between leading icon/mark and trailing shortcut/arrow)
        Text {
            id: labelText
            text: root.text
            textFormat: Text.PlainText
            font.family: Style.typography.family
            font.pixelSize: Style.typography.basePx
            color: root.textColor
            anchors.left: iconImage.visible ? iconImage.right : (markText.visible ? markText.right : parent.left)
            anchors.leftMargin: (iconImage.visible || markText.visible) ? Style.spacing.small : 0
            anchors.right: shortcutLabel.visible ? shortcutLabel.left : (arrowText.visible ? arrowText.left : parent.right)
            anchors.rightMargin: (shortcutLabel.visible || arrowText.visible) ? Style.spacing.small : 0
            anchors.verticalCenter: parent.verticalCenter
            elide: Text.ElideRight
        }
    }

    MouseArea {
        id: mouseArea
        anchors.fill: parent
        hoverEnabled: !root.isSeparator
        cursorShape: (root.enabled && !root.isSeparator) ? Qt.PointingHandCursor : Qt.ArrowCursor
        onClicked: {
            root.click();
        }
    }
}
