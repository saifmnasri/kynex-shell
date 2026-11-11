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

    property real screenVirtualX: Screen.virtualX
    property real screenVirtualY: Screen.virtualY
    property real screenWidth: Screen.width
    property real screenHeight: Screen.height
    property real screenDevicePixelRatio: Screen.devicePixelRatio

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
    readonly property alias trayModel: trayModel
    readonly property alias trayMenuModel: trayMenuModel
    readonly property alias taskbarList: taskbarList
    readonly property alias launcherLoader: launcherLoader
    readonly property alias notificationLoader: notificationLoader
    readonly property alias menuOverlayLoader: menuOverlayLoader
    readonly property alias trayArea: trayArea
    readonly property alias trayRepeater: trayRepeater
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

    TrayModel {
        id: trayModel
        Component.onCompleted: trayModel.init()
    }

    TrayMenuModel {
        id: trayMenuModel
        Component.onCompleted: trayMenuModel.init()
    }

    property bool menuOverlayLoaded: false

    Connections {
        target: trayMenuModel
        function onVisibleChanged(): void {
            if (trayMenuModel.visible) {
                panel.menuOverlayLoaded = true;
            }
        }
    }

    Loader {
        id: menuOverlayLoader
        active: panel.menuOverlayLoaded
        source: "MenuOverlay.qml"

        Binding {
            target: menuOverlayLoader.item
            property: "menuModel"
            value: trayMenuModel
            when: menuOverlayLoader.status === Loader.Ready
        }
    }

    Connections {
        target: panel
        function onFrameSwapped(): void {
            trayModel.recordFirstFrame();
            trayMenuModel.recordFirstFrame();
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
            id: trayArea
            anchors.right: statusArea.left
            anchors.rightMargin: Style.spacing.medium
            anchors.verticalCenter: parent.verticalCenter
            spacing: Style.spacing.small
            visible: trayModel.count > 0

            Repeater {
                id: trayRepeater
                model: trayModel

                delegate: Rectangle {
                    id: trayBtn
                    required property int index
                    required property string id
                    required property string title
                    required property string status
                    required property string iconName
                    required property bool hasPixmap
                    required property string pixmapId
                    required property string tooltipTitle
                    required property string tooltipText
                    required property bool isAttention
                    required property string letterBadge
                    required property bool isMenu
                    required property int windowId
                    required property string menuPath
                    required property bool hasMenu

                    readonly property alias mouseArea: trayMouseArea
                    readonly property alias iconImage: trayIconImage
                    readonly property alias badgeText: trayBadgeText
                    readonly property alias attentionIndicator: attentionDot
                    readonly property alias tooltipLoader: trayTooltipLoader

                    width: Style.spacing.panel - Style.spacing.small
                    height: Style.spacing.panel - Style.spacing.small
                    radius: Style.radius.small
                    color: trayMouseArea.pressed
                        ? Style.colors.surfaceRaised
                        : (trayMouseArea.containsMouse ? Style.colors.surface : "transparent")
                    border.color: trayBtn.isAttention ? Style.colors.warning : "transparent"
                    border.width: trayBtn.isAttention ? 1 : 0

                    Accessible.role: Accessible.Button
                    Accessible.name: trayBtn.title.length > 0 ? trayBtn.title : trayBtn.id
                    Accessible.focusable: true

                    Image {
                        id: trayIconImage
                        anchors.centerIn: parent
                        width: Style.spacing.large
                        height: Style.spacing.large
                        source: trayBtn.hasPixmap
                            ? ("image://kynex-tray/" + trayBtn.pixmapId)
                            : (trayBtn.iconName.length > 0 ? ("image://kynex-icon/" + trayBtn.iconName + "?size=24&scale=1") : "")
                        fillMode: Image.PreserveAspectFit
                        visible: status === Image.Ready && source.toString().length > 0
                        asynchronous: false
                    }

                    Text {
                        id: trayBadgeText
                        anchors.centerIn: parent
                        text: trayBtn.letterBadge
                        font.family: Style.typography.family
                        font.pixelSize: Style.typography.basePx
                        font.bold: true
                        color: Style.colors.text
                        textFormat: Text.PlainText
                        visible: !trayIconImage.visible
                    }

                    Rectangle {
                        id: attentionDot
                        visible: trayBtn.isAttention
                        width: Style.spacing.small
                        height: Style.spacing.small
                        radius: Style.radius.small
                        color: Style.colors.warning
                        anchors.top: parent.top
                        anchors.topMargin: Style.spacing.unit
                        anchors.right: parent.right
                        anchors.rightMargin: Style.spacing.unit
                    }

                    MouseArea {
                        id: trayMouseArea
                        anchors.fill: parent
                        hoverEnabled: true
                        acceptedButtons: Qt.LeftButton | Qt.MiddleButton | Qt.RightButton

                        onClicked: function(mouse) {
                            var globalPos = trayBtn.mapToItem(null, mouse.x, mouse.y);
                            var gx = Math.round(globalPos.x);
                            var gy = Math.round(globalPos.y);
                            if (mouse.button === Qt.LeftButton) {
                                trayModel.activate(trayBtn.id, gx, gy,
                                                   panel.screenVirtualX,
                                                   panel.screenVirtualY,
                                                   panel.screenWidth,
                                                   panel.screenHeight,
                                                   panel.screenDevicePixelRatio,
                                                   panel.height);
                            } else if (mouse.button === Qt.MiddleButton) {
                                trayModel.secondaryActivate(trayBtn.id, gx, gy,
                                                            panel.screenVirtualX,
                                                            panel.screenVirtualY,
                                                            panel.screenWidth,
                                                            panel.screenHeight,
                                                            panel.screenDevicePixelRatio,
                                                            panel.height);
                            } else if (mouse.button === Qt.RightButton) {
                                if (trayBtn.hasMenu) {
                                    var menuW = Style.spacing.panel + Style.spacing.panel + Style.spacing.panel + Style.spacing.panel;
                                    var menuH = Style.spacing.panel + Style.spacing.panel + Style.spacing.panel;
                                    trayMenuModel.open(trayBtn.id, gx, gy, trayBtn.width, trayBtn.height,
                                                       menuW, menuH,
                                                       panel.screenVirtualX,
                                                       panel.screenVirtualY,
                                                       panel.screenWidth,
                                                       panel.screenHeight);
                                } else {
                                    trayModel.contextMenu(trayBtn.id, gx, gy,
                                                          panel.screenVirtualX,
                                                          panel.screenVirtualY,
                                                          panel.screenWidth,
                                                          panel.screenHeight,
                                                          panel.screenDevicePixelRatio,
                                                          panel.height);
                                }
                            }
                        }

                        onWheel: function(wheel) {
                            var delta = wheel.angleDelta.y !== 0 ? wheel.angleDelta.y : wheel.angleDelta.x;
                            var orient = wheel.angleDelta.y !== 0 ? "vertical" : "horizontal";
                            trayModel.scroll(trayBtn.id, delta, orient);
                        }

                        Loader {
                            id: trayTooltipLoader
                            active: trayMouseArea.containsMouse
                            sourceComponent: Component {
                                KynexTooltip {
                                    text: {
                                        if (trayBtn.tooltipTitle.length > 0 && trayBtn.tooltipText.length > 0) {
                                            return trayBtn.tooltipTitle + "\n" + trayBtn.tooltipText;
                                        } else if (trayBtn.tooltipTitle.length > 0) {
                                            return trayBtn.tooltipTitle;
                                        } else if (trayBtn.tooltipText.length > 0) {
                                            return trayBtn.tooltipText;
                                        } else {
                                            return trayBtn.title.length > 0 ? trayBtn.title : trayBtn.id;
                                        }
                                    }
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
