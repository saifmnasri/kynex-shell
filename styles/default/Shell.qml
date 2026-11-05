import QtQuick
import Kynex.Shell
import Kynex.Style

LayerSurface {
    id: panel
    visible: true
    width: Screen.width
    height: Style.spacing.panel

    layer: LayerSurface.LayerTop
    anchors: LayerSurface.AnchorBottom + LayerSurface.AnchorLeft + LayerSurface.AnchorRight
    exclusiveZone: Style.spacing.panel
    leftMargin: 0
    rightMargin: 0
    topMargin: 0
    bottomMargin: 0
    keyboardInteractivity: LayerSurface.KeyboardInteractivityNone
    scope: "panel"

    Component.onCompleted: {
        console.log("Kynex Shell panel initialized successfully");
    }

    Clock {
        id: clock
        Component.onCompleted: clock.init()
    }

    PowerModel {
        id: powerModel
        Component.onCompleted: powerModel.init()
    }

    Rectangle {
        anchors.fill: parent
        color: Style.colors.surfaceRaised

        Row {
            anchors.centerIn: parent
            spacing: Style.spacing.medium

            Text {
                text: clock.date
                color: Style.colors.textMuted
                font.family: Style.typography.family
                font.pixelSize: Style.typography.basePx - 1
                anchors.verticalCenter: parent.verticalCenter
            }

            Text {
                text: clock.time
                color: Style.colors.text
                font.family: Style.typography.family
                font.pixelSize: Style.typography.basePx + 1
                font.bold: true
                anchors.verticalCenter: parent.verticalCenter
            }
        }

        Row {
            id: batteryIndicator
            visible: powerModel.present
            anchors.right: parent.right
            anchors.rightMargin: Style.spacing.large
            anchors.verticalCenter: parent.verticalCenter
            spacing: Style.spacing.small

            Text {
                text: powerModel.charging ? "⚡" : "🔋"
                color: powerModel.charging ? Style.colors.success : (powerModel.low ? Style.colors.warning : Style.colors.textMuted)
                font.family: Style.typography.family
                font.pixelSize: Style.typography.basePx
                anchors.verticalCenter: parent.verticalCenter
            }

            Text {
                text: Math.round(powerModel.percentage) + "%"
                color: powerModel.low ? Style.colors.warning : Style.colors.text
                font.family: Style.typography.family
                font.pixelSize: Style.typography.basePx
                font.bold: true
                anchors.verticalCenter: parent.verticalCenter
            }
        }
    }
}
