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

    readonly property alias powerModel: powerModel
    readonly property alias networkModel: networkModel
    readonly property alias windowListModel: windowListModel
    readonly property alias launcherModel: launcherModel
    readonly property alias notificationModel: notificationModel
    readonly property alias taskbarList: taskbarList
    readonly property alias launcherLoader: launcherLoader
    readonly property alias notificationLoader: notificationLoader
    readonly property alias networkText: networkText
    readonly property string networkTooltipText: networkModel.connected
        ? (networkModel.wifi
            ? ("Wi-Fi: " + (networkModel.ssid.length > 0 ? networkModel.ssid : "Connected") + " (" + networkModel.strength + "%)")
            : (networkModel.ethernet ? "Ethernet Connection" : "Connected"))
        : "Network Disconnected"

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

    WindowListModel {
        id: windowListModel
        Component.onCompleted: windowListModel.init()
    }

    LauncherModel {
        id: launcherModel
        Component.onCompleted: launcherModel.init()
    }

    property bool launcherLoaded: false

    Connections {
        target: launcherModel
        function onVisibleChanged(): void {
            if (launcherModel.visible) {
                panel.launcherLoaded = true;
            }
        }
    }

    Loader {
        id: launcherLoader
        active: panel.launcherLoaded
        source: "Launcher.qml"

        Binding {
            target: launcherLoader.item
            property: "launcherModel"
            value: launcherModel
            when: launcherLoader.status === Loader.Ready
        }
    }

    NotificationModel {
        id: notificationModel
        Component.onCompleted: notificationModel.init()
    }

    property bool notificationPopupLoaded: false

    Connections {
        target: notificationModel
        function onCountChanged(): void {
            if (notificationModel.count > 0) {
                panel.notificationPopupLoaded = true;
            }
        }
    }

    Loader {
        id: notificationLoader
        active: panel.notificationPopupLoaded
        source: "NotificationPopup.qml"

        Binding {
            target: notificationLoader.item
            property: "notificationModel"
            value: notificationModel
            when: notificationLoader.status === Loader.Ready
        }
    }

    Rectangle {
        anchors.fill: parent
        color: Style.colors.surfaceRaised

        ListView {
            id: taskbarList
            anchors.left: parent.left
            anchors.leftMargin: Style.spacing.medium
            anchors.right: clockRow.left
            anchors.rightMargin: Style.spacing.medium
            anchors.verticalCenter: parent.verticalCenter
            height: Style.spacing.panel - Style.spacing.small
            orientation: ListView.Horizontal
            spacing: Style.spacing.small
            clip: true
            interactive: false
            model: windowListModel
            visible: windowListModel.count > 0

            delegate: KynexButton {
                id: taskbarBtn
                required property int index
                required property string id
                required property string title
                required property string appId
                required property string iconName
                required property string letterBadge
                required property bool active
                required property bool minimized
                required property bool isPinned
                required property bool isRunning
                required windowCount

                text: title.length > 0 ? title : (appId.length > 0 ? appId : "Window")
                letterBadge: letterBadge
                iconSource: iconName.length > 0 ? ("image://kynex-icon/" + iconName + "?size=24&scale=1") : ""
                primary: active
                showRunningIndicator: isRunning
                opacity: (isRunning && minimized) ? Style.opacity.disabled : 1
                width: Math.min(implicitWidth, Style.spacing.panel + Style.spacing.panel + Style.spacing.panel)
                height: Style.spacing.panel - Style.spacing.small

                Accessible.name: (title.length > 0 ? title : "App") + (active ? " (Active)" : (isRunning ? " (Running)" : " (Pinned)")) + (windowCount > 1 ? (" - " + windowCount + " windows") : "")

                onClicked: {
                    windowListModel.handleClick(index);
                }

                onMiddleClicked: {
                    windowListModel.handleMiddleClick(index);
                }

                KynexTooltip {
                    id: taskbarTooltip
                    text: taskbarBtn.text
                    visible: taskbarBtn.hovered
                    anchors.bottom: taskbarBtn.top
                    anchors.bottomMargin: Style.spacing.small
                    anchors.horizontalCenter: taskbarBtn.horizontalCenter
                }
            }
        }

        Row {
            id: clockRow
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
                        id: networkText
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
                                text: panel.networkTooltipText
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
