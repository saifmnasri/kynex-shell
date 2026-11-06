import QtQuick
import Kynex.Style

Item {
    id: root

    property string text: ""
    property int delay: Style.motion.slowMs

    readonly property color backgroundColor: bg.color
    readonly property color textColor: tooltipLabel.color

    Accessible.role: Accessible.ToolTip
    Accessible.name: text

    LayoutMirroring.enabled: Qt.locale().textDirection === Qt.RightToLeft
    LayoutMirroring.childrenInherit: true

    implicitWidth: tooltipLabel.implicitWidth + Style.spacing.small + Style.spacing.small
    implicitHeight: tooltipLabel.implicitHeight + Style.spacing.unit + Style.spacing.unit
    width: implicitWidth
    height: implicitHeight

    opacity: visible ? Style.opacity.focus : 0
    Behavior on opacity {
        NumberAnimation {
            duration: Style.motion.fastMs
        }
    }

    Rectangle {
        id: bg
        anchors.fill: parent
        radius: Style.radius.small
        color: Style.colors.surfaceRaised
        border.color: Style.colors.surface
        border.width: 1

        Text {
            id: tooltipLabel
            anchors.centerIn: parent
            text: root.text
            font.family: Style.typography.family
            font.pixelSize: Style.typography.basePx
            color: Style.colors.text
        }
    }
}

