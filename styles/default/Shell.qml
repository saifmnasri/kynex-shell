pragma ComponentBehavior: Bound
import QtQuick
import Kynex.Shell
import Kynex.Style
import Kynex.Controls

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

    NetworkModel {
        id: networkModel
        Component.onCompleted: networkModel.init()
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
            id: statusArea
            anchors.right: parent.right
            anchors.rightMargin: Style.spacing.large
            anchors.verticalCenter: parent.verticalCenter
            spacing: Style.spacing.medium

            Item {
                id: networkIndicator
                visible: networkModel.present
                width: networkRow.implicitWidth
                height: networkRow.implicitHeight
                anchors.verticalCenter: parent.verticalCenter

                Row {
                    id: networkRow
                    spacing: Style.spacing.small
                    anchors.verticalCenter: parent.verticalCenter

                    Text {
                        text: networkModel.connected ? (networkModel.wifi ? "📶" : (networkModel.ethernet ? "🌐" : "🔗")) : "🚫"
                        color: networkModel.connected ? (networkModel.weak ? Style.colors.warning : Style.colors.success) : Style.colors.textMuted
                        font.family: Style.typography.family
                        font.pixelSize: Style.typography.basePx
                        anchors.verticalCenter: parent.verticalCenter
                    }

                    Text {
                        visible: networkModel.connected
                        text: networkModel.wifi ? (networkModel.ssid.length > 0 ? networkModel.ssid : networkModel.strength + "%") : (networkModel.ethernet ? "Ethernet" : "Connected")
                        color: networkModel.weak ? Style.colors.warning : Style.colors.text
                        font.family: Style.typography.family
                        font.pixelSize: Style.typography.basePx
                        font.bold: true
                        anchors.verticalCenter: parent.verticalCenter
                    }
                }

                MouseArea {
                    id: networkHover
                    anchors.fill: parent
                    hoverEnabled: true

                    Loader {
                        id: networkTooltipLoader
                        active: networkHover.containsMouse
                        sourceComponent: Component {
                            KynexTooltip {
                                text: networkModel.connected
                                    ? (networkModel.wifi
                                        ? ("Wi-Fi: " + (networkModel.ssid.length > 0 ? networkModel.ssid : "Connected") + " (" + networkModel.strength + "%)")
                                        : (networkModel.ethernet ? "Ethernet Connection" : "Connected"))
                                    : "Network Disconnected"
                                visible: true
                                anchors.bottom: parent.top
                                anchors.bottomMargin: Style.spacing.small
                                anchors.horizontalCenter: parent.horizontalCenter
                            }
                        }
                    }
                }
            }

            Item {
                id: batteryIndicator
                visible: powerModel.present
                width: batteryRow.implicitWidth
                height: batteryRow.implicitHeight
                anchors.verticalCenter: parent.verticalCenter

                Row {
                    id: batteryRow
                    spacing: Style.spacing.small
                    anchors.verticalCenter: parent.verticalCenter

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

                MouseArea {
                    id: batteryHover
                    anchors.fill: parent
                    hoverEnabled: true

                    Loader {
                        id: batteryTooltipLoader
                        active: batteryHover.containsMouse
                        sourceComponent: Component {
                            KynexTooltip {
                                text: (powerModel.charging ? "Battery Charging: " : "Battery: ") + Math.round(powerModel.percentage) + "%"
                                visible: true
                                anchors.bottom: parent.top
                                anchors.bottomMargin: Style.spacing.small
                                anchors.horizontalCenter: parent.horizontalCenter
                            }
                        }
                    }
                }
            }
        }
    }
}
