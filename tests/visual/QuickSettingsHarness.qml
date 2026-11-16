import QtQuick
import Kynex.Controls
import Kynex.Style

Rectangle {
    id: root
    width: Style.spacing.panel + Style.spacing.panel + Style.spacing.panel + Style.spacing.panel + Style.spacing.panel + Style.spacing.panel + Style.spacing.panel + Style.spacing.panel + Style.spacing.panel
    height: Style.spacing.panel + Style.spacing.panel + Style.spacing.panel + Style.spacing.panel + Style.spacing.panel + Style.spacing.panel + Style.spacing.panel + Style.spacing.panel
    color: Style.colors.surface

    property string scenario: "normal"

    readonly property bool isMuted: root.scenario === "muted"
    readonly property bool isDnd: root.scenario === "dnd"
    readonly property bool hasAudio: root.scenario !== "no-audio"
    readonly property real volumeVal: root.hasAudio ? (isMuted ? 0 : (1 / (1 + 1))) : 0

    LayoutMirroring.enabled: root.scenario === "rtl"
    LayoutMirroring.childrenInherit: true

    Rectangle {
        id: card
        anchors.centerIn: parent
        width: Style.spacing.panel + Style.spacing.panel + Style.spacing.panel + Style.spacing.panel + Style.spacing.panel + Style.spacing.panel + Style.spacing.panel
        implicitHeight: cardColumn.implicitHeight + Style.spacing.large + Style.spacing.large
        height: implicitHeight
        radius: Style.radius.medium
        color: Style.colors.surface
        border.color: Style.colors.surfaceRaised
        border.width: 1

        Column {
            id: cardColumn
            anchors.left: parent.left
            anchors.right: parent.right
            anchors.top: parent.top
            anchors.margins: Style.spacing.large
            spacing: Style.spacing.medium

            Row {
                id: headerRow
                width: parent.width

                Text {
                    text: "Quick Settings"
                    textFormat: Text.PlainText
                    font.family: Style.typography.family
                    font.pixelSize: Style.typography.basePx + Style.spacing.unit
                    font.bold: true
                    color: Style.colors.text
                }
            }

            Rectangle {
                width: parent.width
                height: 1
                color: Style.colors.surfaceRaised
            }

            Column {
                id: volumeSection
                width: parent.width
                spacing: Style.spacing.small

                Row {
                    width: parent.width
                    spacing: Style.spacing.small

                    Text {
                        text: root.hasAudio ? (root.isMuted ? "🔇" : "🔊") : "🔈"
                        textFormat: Text.PlainText
                        font.family: Style.typography.family
                        font.pixelSize: Style.typography.basePx
                        color: root.hasAudio ? Style.colors.text : Style.colors.textMuted
                        anchors.verticalCenter: parent.verticalCenter
                    }

                    Text {
                        text: "Volume"
                        textFormat: Text.PlainText
                        font.family: Style.typography.family
                        font.pixelSize: Style.typography.basePx
                        font.bold: true
                        color: root.hasAudio ? Style.colors.text : Style.colors.textMuted
                        anchors.verticalCenter: parent.verticalCenter
                    }

                    Item {
                        width: Style.spacing.medium
                        height: 1
                    }

                    Text {
                        id: volumePercentText
                        text: root.hasAudio ? (root.isMuted ? "Muted" : "50%") : "Unavailable"
                        textFormat: Text.PlainText
                        font.family: Style.typography.family
                        font.pixelSize: Style.typography.basePx
                        color: Style.colors.textMuted
                        anchors.verticalCenter: parent.verticalCenter
                    }
                }

                KynexSlider {
                    id: volumeSlider
                    width: parent.width
                    enabled: root.hasAudio
                    value: root.volumeVal
                    Accessible.name: "Master Volume Slider"
                }

                Row {
                    width: parent.width
                    spacing: Style.spacing.small

                    KynexSwitch {
                        id: muteSwitch
                        text: "Mute"
                        enabled: root.hasAudio
                        checked: root.isMuted
                        Accessible.name: "Audio Mute Switch"
                    }

                    Item {
                        width: Style.spacing.medium
                        height: 1
                    }

                    Text {
                        id: deviceNameLabel
                        visible: root.hasAudio
                        text: "Default Output"
                        textFormat: Text.PlainText
                        elide: Text.ElideRight
                        width: parent.width - muteSwitch.width - Style.spacing.medium
                        font.family: Style.typography.family
                        font.pixelSize: Style.typography.basePx - Style.spacing.unit
                        color: Style.colors.textMuted
                        anchors.verticalCenter: parent.verticalCenter
                    }
                }
            }

            Rectangle {
                width: parent.width
                height: 1
                color: Style.colors.surfaceRaised
            }

            Row {
                id: dndSection
                width: parent.width
                spacing: Style.spacing.small

                KynexSwitch {
                    id: dndSwitch
                    text: "Do Not Disturb"
                    checked: root.isDnd
                    Accessible.name: "Do Not Disturb Switch"
                }
            }

            Rectangle {
                width: parent.width
                height: 1
                color: Style.colors.surfaceRaised
            }

            Column {
                id: statusSection
                width: parent.width
                spacing: Style.spacing.small

                Row {
                    id: networkStatusRow
                    width: parent.width
                    spacing: Style.spacing.small

                    Text {
                        text: "📶"
                        textFormat: Text.PlainText
                        font.family: Style.typography.family
                        font.pixelSize: Style.typography.basePx
                        color: Style.colors.success
                        anchors.verticalCenter: parent.verticalCenter
                    }

                    Text {
                        text: "KynexSecure (85%)"
                        textFormat: Text.PlainText
                        font.family: Style.typography.family
                        font.pixelSize: Style.typography.basePx
                        color: Style.colors.text
                        anchors.verticalCenter: parent.verticalCenter
                    }
                }

                Row {
                    id: batteryStatusRow
                    width: parent.width
                    spacing: Style.spacing.small

                    Text {
                        text: "🔋"
                        textFormat: Text.PlainText
                        font.family: Style.typography.family
                        font.pixelSize: Style.typography.basePx
                        color: Style.colors.success
                        anchors.verticalCenter: parent.verticalCenter
                    }

                    Text {
                        text: "Battery: 85%"
                        textFormat: Text.PlainText
                        font.family: Style.typography.family
                        font.pixelSize: Style.typography.basePx
                        color: Style.colors.text
                        anchors.verticalCenter: parent.verticalCenter
                    }
                }
            }
        }
    }
}
