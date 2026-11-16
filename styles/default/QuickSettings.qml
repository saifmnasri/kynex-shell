pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Layouts
import Kynex.Shell
import Kynex.Style
import Kynex.Controls

LayerSurface {
    // qmllint disable Quick.attached-property-type
    // qmllint disable compiler
    id: root

    property AudioModel audioModel: null
    property NotificationModel notificationModel: null
    property NetworkModel networkModel: null
    property PowerModel powerModel: null

    property bool isOpen: false
    property int panelAnchorsBits: 0
    property real panelWidth: 0
    property real panelHeight: 0
    property real anchorX: 0
    property real anchorY: 0
    property real anchorWidth: 0
    property real anchorHeight: 0

    property real popupX: 0
    property real popupY: 0
    readonly property int popupWidth: Style.spacing.panel + Style.spacing.panel + Style.spacing.panel + Style.spacing.panel + Style.spacing.panel + Style.spacing.panel + Style.spacing.panel

    readonly property alias cardContainer: popupContainer
    readonly property alias cardRectangle: card
    readonly property alias volumeIconItem: volumeIcon
    readonly property alias volumeSliderItem: volumeSlider
    readonly property alias muteSwitchItem: muteSwitch
    readonly property alias deviceNameLabelItem: deviceNameLabel
    readonly property alias dndSwitchItem: dndSwitch
    readonly property alias networkSummaryItem: networkSummaryText
    readonly property alias backdropArea: backdropMouse
    readonly property alias escShortcutItem: escShortcut

    visible: root.isOpen
    width: Screen.width
    height: Screen.height

    layer: LayerSurface.LayerOverlay
    anchors: LayerSurface.AnchorTop + LayerSurface.AnchorBottom + LayerSurface.AnchorLeft + LayerSurface.AnchorRight
    exclusiveZone: 0
    leftMargin: 0
    rightMargin: 0
    topMargin: 0
    bottomMargin: 0
    keyboardInteractivity: LayerSurface.KeyboardInteractivityOnDemand
    scope: "quick-settings"

    LayoutMirroring.enabled: Qt.locale().textDirection === Qt.RightToLeft
    LayoutMirroring.childrenInherit: true

    function updatePlacement(): void {
        let expectedHeight = card.implicitHeight;

        if (root.audioModel) {
            root.popupX = root.audioModel.computePlacementX(
                root.panelAnchorsBits,
                root.panelWidth,
                root.panelHeight,
                root.anchorX,
                root.anchorY,
                root.anchorWidth,
                root.anchorHeight,
                root.popupWidth,
                expectedHeight,
                root.width,
                root.height
            );
            root.popupY = root.audioModel.computePlacementY(
                root.panelAnchorsBits,
                root.panelWidth,
                root.panelHeight,
                root.anchorX,
                root.anchorY,
                root.anchorWidth,
                root.anchorHeight,
                root.popupWidth,
                expectedHeight,
                root.width,
                root.height
            );
        } else {
            root.popupX = Math.max(0, root.width - root.popupWidth - Style.spacing.medium);
            root.popupY = Math.max(0, root.height - root.panelHeight - expectedHeight - Style.spacing.small);
        }
    }

    onIsOpenChanged: {
        if (root.isOpen) {
            root.updatePlacement();
            volumeSlider.forceActiveFocus();
            if (root.audioModel) {
                root.audioModel.recordQuickSettingsOpened();
            }
        }
    }

    function close(): void {
        root.isOpen = false;
        if (root.audioModel) {
            root.audioModel.open = false;
        }
    }

    // Dismiss on outside click
    MouseArea {
        id: backdropMouse
        anchors.fill: parent
        onClicked: root.close()
    }

    // Dismiss on Escape key
    Shortcut {
        id: escShortcut
        sequence: "Esc"
        onActivated: root.close()
    }

    Item {
        id: popupContainer
        x: root.popupX
        y: root.popupY
        width: card.width
        height: card.height

        // Swallow clicks inside the card so backdrop doesn't close it
        MouseArea {
            anchors.fill: parent
        }

        Rectangle {
            id: card
            width: root.popupWidth
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

                // Header
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

                // Separator
                Rectangle {
                    width: parent.width
                    height: 1
                    color: Style.colors.surfaceRaised
                }

                // Volume Section
                Column {
                    id: volumeSection
                    width: parent.width
                    spacing: Style.spacing.small

                    Row {
                        width: parent.width
                        spacing: Style.spacing.small

                        Text {
                            id: volumeIcon
                            text: (root.audioModel && root.audioModel.muted) ? "🔇" : ((root.audioModel && root.audioModel.volume === 0) ? "🔈" : "🔊")
                            textFormat: Text.PlainText
                            font.family: Style.typography.family
                            font.pixelSize: Style.typography.basePx
                            color: (root.audioModel && root.audioModel.present) ? Style.colors.text : Style.colors.textMuted
                            anchors.verticalCenter: parent.verticalCenter
                        }

                        Text {
                            text: "Volume"
                            textFormat: Text.PlainText
                            font.family: Style.typography.family
                            font.pixelSize: Style.typography.basePx
                            font.bold: true
                            color: (root.audioModel && root.audioModel.present) ? Style.colors.text : Style.colors.textMuted
                            anchors.verticalCenter: parent.verticalCenter
                        }

                        Item {
                            Layout.fillWidth: true
                            width: Style.spacing.medium
                            height: 1
                        }

                        Text {
                            id: volumePercentText
                            text: (root.audioModel && root.audioModel.present) ? (root.audioModel.volume + "%") : "Unavailable"
                            textFormat: Text.PlainText
                            font.family: Style.typography.family
                            font.pixelSize: Style.typography.basePx
                            color: Style.colors.textMuted
                            anchors.verticalCenter: parent.verticalCenter
                        }
                    }

                    // Volume Slider
                    KynexSlider {
                        id: volumeSlider
                        width: parent.width
                        enabled: root.audioModel ? root.audioModel.present : false
                        value: (root.audioModel && root.audioModel.present) ? (root.audioModel.volume / Number("100")) : 0
                        Accessible.name: "Master Volume Slider"

                        onMoved: function(val) {
                            if (root.audioModel && root.audioModel.present) {
                                let targetVol = Math.round(val * Number("100"));
                                root.audioModel.setVolume(targetVol);
                            }
                        }

                        KeyNavigation.tab: muteSwitch
                    }

                    // Mute Switch and Device Name
                    Row {
                        width: parent.width
                        spacing: Style.spacing.small

                        KynexSwitch {
                            id: muteSwitch
                            text: "Mute"
                            enabled: root.audioModel ? root.audioModel.present : false
                            checked: (root.audioModel && root.audioModel.present) ? root.audioModel.muted : false
                            Accessible.name: "Audio Mute Switch"

                            onToggled: function(c) {
                                if (root.audioModel && root.audioModel.present) {
                                    root.audioModel.setMuted(c);
                                }
                            }

                            KeyNavigation.tab: dndSwitch
                            KeyNavigation.backtab: volumeSlider
                        }

                        Item {
                            width: Style.spacing.medium
                            height: 1
                        }

                        Text {
                            id: deviceNameLabel
                            visible: root.audioModel && root.audioModel.present && root.audioModel.deviceName.length > 0
                            text: root.audioModel ? root.audioModel.deviceName : ""
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

                // Separator
                Rectangle {
                    width: parent.width
                    height: 1
                    color: Style.colors.surfaceRaised
                }

                // Do Not Disturb Section
                Row {
                    id: dndSection
                    width: parent.width
                    spacing: Style.spacing.small

                    KynexSwitch {
                        id: dndSwitch
                        text: "Do Not Disturb"
                        checked: root.notificationModel ? root.notificationModel.doNotDisturb : false
                        Accessible.name: "Do Not Disturb Switch"

                        onToggled: function(c) {
                            if (root.notificationModel) {
                                root.notificationModel.setDoNotDisturb(c);
                            }
                        }

                        KeyNavigation.tab: volumeSlider
                        KeyNavigation.backtab: muteSwitch
                    }
                }

                // Separator
                Rectangle {
                    width: parent.width
                    height: 1
                    color: Style.colors.surfaceRaised
                }

                // Read-only Status Rows (Network & Battery)
                Column {
                    id: statusSection
                    width: parent.width
                    spacing: Style.spacing.small

                    // Network Row
                    Row {
                        id: networkStatusRow
                        width: parent.width
                        spacing: Style.spacing.small

                        Text {
                            text: (root.networkModel && root.networkModel.connected) ? ((root.networkModel && root.networkModel.wifi) ? "📶" : ((root.networkModel && root.networkModel.ethernet) ? "🌐" : "🔗")) : "🚫"
                            textFormat: Text.PlainText
                            font.family: Style.typography.family
                            font.pixelSize: Style.typography.basePx
                            color: (root.networkModel && root.networkModel.connected) ? ((root.networkModel && root.networkModel.weak) ? Style.colors.warning : Style.colors.success) : Style.colors.textMuted
                            anchors.verticalCenter: parent.verticalCenter
                        }

                        Text {
                            id: networkSummaryText
                            text: {
                                if (!root.networkModel || !root.networkModel.present) {
                                    return "Network Unavailable";
                                }
                                if (!root.networkModel.connected) {
                                    return "Disconnected";
                                }
                                if (root.networkModel.wifi) {
                                    return root.networkModel.ssid.length > 0 ? (root.networkModel.ssid + " (" + root.networkModel.strength + "%)") : (root.networkModel.strength + "%");
                                }
                                if (root.networkModel.ethernet) {
                                    return "Ethernet Connected";
                                }
                                return "Connected";
                            }
                            textFormat: Text.PlainText
                            font.family: Style.typography.family
                            font.pixelSize: Style.typography.basePx
                            color: (root.networkModel && root.networkModel.weak) ? Style.colors.warning : Style.colors.text
                            anchors.verticalCenter: parent.verticalCenter
                        }
                    }

                    // Battery Row
                    Row {
                        id: batteryStatusRow
                        width: parent.width
                        spacing: Style.spacing.small

                        Text {
                            text: (root.powerModel && root.powerModel.charging) ? "⚡" : "🔋"
                            textFormat: Text.PlainText
                            font.family: Style.typography.family
                            font.pixelSize: Style.typography.basePx
                            color: (root.powerModel && root.powerModel.charging) ? Style.colors.success : ((root.powerModel && root.powerModel.low) ? Style.colors.warning : Style.colors.textMuted)
                            anchors.verticalCenter: parent.verticalCenter
                        }

                        Text {
                            id: batterySummaryText
                            text: {
                                if (!root.powerModel || !root.powerModel.present) {
                                    return "No Battery Detected";
                                }
                                let pct = Math.round(root.powerModel.percentage);
                                return (root.powerModel.charging ? "Charging: " : "Battery: ") + pct + "%";
                            }
                            textFormat: Text.PlainText
                            font.family: Style.typography.family
                            font.pixelSize: Style.typography.basePx
                            color: (root.powerModel && root.powerModel.low) ? Style.colors.warning : Style.colors.text
                            anchors.verticalCenter: parent.verticalCenter
                        }
                    }
                }
            }
        }
    }
}
