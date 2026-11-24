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
    readonly property alias sessionModel: sessionModel
    readonly property alias networkModel: networkModel
    readonly property alias audioModel: audioModel
    readonly property alias windowListModel: windowListModel
    readonly property alias launcherModel: launcherModel
    readonly property alias notificationModel: notificationModel
    readonly property alias trayModel: trayModel
    readonly property alias trayMenuModel: trayMenuModel
    readonly property alias taskbarList: taskbarList
    readonly property alias panelContent: panelContent
    readonly property alias launcherLoader: launcherLoader
    readonly property alias notificationLoader: notificationLoader
    readonly property alias menuOverlayLoader: menuOverlayLoader
    readonly property alias quickSettingsLoader: quickSettingsLoader
    readonly property alias sessionOverlayLoader: sessionOverlayLoader
    readonly property alias statusArea: statusArea
    readonly property alias trayArea: trayArea
    readonly property alias trayRepeater: trayRepeater
    readonly property alias networkText: networkText
    readonly property alias statusFocusScope: statusFocusScope
    readonly property alias volumeStatusIcon: volumeStatusIcon
    readonly property string networkTooltipText: (networkModel && networkModel.connected)
        ? (networkModel.wifi
            ? ("Wi-Fi: " + (networkModel.ssid && networkModel.ssid.length > 0 ? networkModel.ssid : "Connected") + " (" + (networkModel.strength || 0) + "%)")
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

    SessionModel {
        id: sessionModel
        Component.onCompleted: sessionModel.init()
    }

    property bool sessionOverlayLoaded: false

    Connections {
        target: sessionModel
        function onVisibleChanged(): void {
            if (sessionModel.visible) {
                panel.sessionOverlayLoaded = true;
            }
        }
    }

    Loader {
        id: sessionOverlayLoader
        active: panel.sessionOverlayLoaded
        source: "SessionOverlay.qml"

        Binding {
            target: sessionOverlayLoader.item
            property: "sessionModel"
            value: sessionModel
            when: sessionOverlayLoader.status === Loader.Ready
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

    AudioModel {
        id: audioModel
        Component.onCompleted: audioModel.init()
    }

    property bool menuOverlayLoaded: false
    property bool quickSettingsLoaded: false
    property bool quickSettingsOpen: false

    function toggleQuickSettings(): void {
        audioModel.toggleQuickSettings();
    }

    Connections {
        target: audioModel
        function onOpenChanged(): void {
            if (audioModel.open) {
                panel.quickSettingsLoaded = true;
            }
            panel.quickSettingsOpen = audioModel.open;
        }
    }

    onQuickSettingsOpenChanged: {
        if (panel.quickSettingsOpen) {
            panel.quickSettingsLoaded = true;
        }
        if (audioModel.open !== panel.quickSettingsOpen) {
            audioModel.open = panel.quickSettingsOpen;
        }
    }

    Loader {
        id: quickSettingsLoader
        active: panel.quickSettingsLoaded
        source: "QuickSettings.qml"

        Binding {
            target: quickSettingsLoader.item
            property: "audioModel"
            value: audioModel
        }
        Binding {
            target: quickSettingsLoader.item
            property: "notificationModel"
            value: notificationModel
        }
        Binding {
            target: quickSettingsLoader.item
            property: "networkModel"
            value: networkModel
        }
        Binding {
            target: quickSettingsLoader.item
            property: "powerModel"
            value: powerModel
        }
        Binding {
            target: quickSettingsLoader.item
            property: "sessionModel"
            value: sessionModel
        }
        Binding {
            target: quickSettingsLoader.item
            property: "isOpen"
            value: panel.quickSettingsOpen
        }
        Binding {
            target: quickSettingsLoader.item
            property: "anchorX"
            value: statusArea.x
        }
        Binding {
            target: quickSettingsLoader.item
            property: "anchorY"
            value: statusArea.y
        }
        Binding {
            target: quickSettingsLoader.item
            property: "anchorWidth"
            value: statusArea.width
        }
        Binding {
            target: quickSettingsLoader.item
            property: "anchorHeight"
            value: statusArea.height
        }
        Binding {
            target: quickSettingsLoader.item
            property: "panelAnchorsBits"
            value: panel.anchors
        }
        Binding {
            target: quickSettingsLoader.item
            property: "panelWidth"
            value: panel.width
        }
        Binding {
            target: quickSettingsLoader.item
            property: "panelHeight"
            value: panel.height
        }
    }

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
        id: panelContent
        anchors.fill: parent
        color: Style.colors.surfaceRaised
        LayoutMirroring.enabled: Qt.locale().textDirection === Qt.RightToLeft
        LayoutMirroring.childrenInherit: true

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
                visible: !Style.testMutantHiddenDelegate
                opacity: (isRunning && minimized) ? Style.opacity.disabled : 1
                width: Style.testMutantZeroWidth ? 0 : Math.min(implicitWidth, Style.spacing.panel + Style.spacing.panel + Style.spacing.panel)
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
                            var clickPos = trayBtn.mapToItem(null, mouse.x, mouse.y);
                            var gx = Math.round(clickPos.x);
                            var gy = Math.round(clickPos.y);
                            if (mouse.button === Qt.LeftButton) {
                                trayModel.activate(trayBtn.id, gx, gy,
                                                   panel.anchors,
                                                   panel.width,
                                                   panel.height,
                                                   panel.screenVirtualX,
                                                   panel.screenVirtualY,
                                                   panel.screenWidth,
                                                   panel.screenHeight,
                                                   panel.screenDevicePixelRatio);
                            } else if (mouse.button === Qt.MiddleButton) {
                                trayModel.secondaryActivate(trayBtn.id, gx, gy,
                                                            panel.anchors,
                                                            panel.width,
                                                            panel.height,
                                                            panel.screenVirtualX,
                                                            panel.screenVirtualY,
                                                            panel.screenWidth,
                                                            panel.screenHeight,
                                                            panel.screenDevicePixelRatio);
                            } else if (mouse.button === Qt.RightButton) {
                                if (trayBtn.hasMenu) {
                                    var btnPos = trayBtn.mapToItem(null, 0, 0);
                                    var bx = Math.round(btnPos.x);
                                    var by = Math.round(btnPos.y);
                                    var menuW = Style.spacing.panel + Style.spacing.panel + Style.spacing.panel + Style.spacing.panel;
                                    var menuH = Style.spacing.panel + Style.spacing.panel + Style.spacing.panel;
                                    trayMenuModel.open(trayBtn.id, bx, by, trayBtn.width, trayBtn.height,
                                                       menuW, menuH,
                                                       panel.anchors,
                                                       panel.width,
                                                       panel.height,
                                                       panel.screenVirtualX,
                                                       panel.screenVirtualY,
                                                       panel.screenWidth,
                                                       panel.screenHeight,
                                                       panel.screenDevicePixelRatio);
                                } else {
                                    trayModel.contextMenu(trayBtn.id, gx, gy,
                                                          panel.anchors,
                                                          panel.width,
                                                          panel.height,
                                                          panel.screenVirtualX,
                                                          panel.screenVirtualY,
                                                          panel.screenWidth,
                                                          panel.screenHeight,
                                                          panel.screenDevicePixelRatio);
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

        Rectangle {
            id: statusArea
            anchors.right: parent.right
            anchors.rightMargin: Style.spacing.large
            anchors.verticalCenter: parent.verticalCenter
            implicitWidth: statusRow.implicitWidth + Style.spacing.medium + Style.spacing.medium
            implicitHeight: panel.height - Style.spacing.small - Style.spacing.small
            width: implicitWidth
            height: implicitHeight
            radius: Style.radius.small
            color: statusAreaMouse.containsMouse ? Style.colors.surface : "transparent"
            border.color: statusFocusScope.activeFocus ? Style.colors.focusRing : "transparent"
            border.width: statusFocusScope.activeFocus ? 1 : 0

            Accessible.role: Accessible.Button
            Accessible.name: "Quick Settings"
            Accessible.onPressAction: panel.toggleQuickSettings()

            FocusScope {
                id: statusFocusScope
                anchors.fill: parent
                activeFocusOnTab: true

                Keys.onReturnPressed: panel.toggleQuickSettings()
                Keys.onSpacePressed: panel.toggleQuickSettings()
            }

            MouseArea {
                id: statusAreaMouse
                anchors.fill: parent
                hoverEnabled: true
                cursorShape: Qt.PointingHandCursor
                onClicked: panel.toggleQuickSettings()
            }

            Row {
                id: statusRow
                anchors.centerIn: parent
                spacing: Style.spacing.medium

                // Volume indicator
                Row {
                    id: volumeIndicator
                    visible: audioModel.present
                    spacing: Style.spacing.small
                    anchors.verticalCenter: parent.verticalCenter

                    Text {
                        id: volumeStatusIcon
                        text: audioModel.muted ? "🔇" : (audioModel.volume === 0 ? "🔈" : (audioModel.volume < Number("50") ? "🔉" : "🔊"))
                        textFormat: Text.PlainText
                        color: audioModel.muted ? Style.colors.textMuted : Style.colors.text
                        font.family: Style.typography.family
                        font.pixelSize: Style.typography.basePx
                        anchors.verticalCenter: parent.verticalCenter
                    }

                    Text {
                        text: audioModel.muted ? "Muted" : (audioModel.volume + "%")
                        textFormat: Text.PlainText
                        color: audioModel.muted ? Style.colors.textMuted : Style.colors.text
                        font.family: Style.typography.family
                        font.pixelSize: Style.typography.basePx
                        font.bold: true
                        anchors.verticalCenter: parent.verticalCenter
                    }
                }

                // Network indicator
                Row {
                    id: networkIndicator
                    visible: networkModel.present
                    spacing: Style.spacing.small
                    anchors.verticalCenter: parent.verticalCenter

                    Text {
                        text: networkModel.connected ? (networkModel.wifi ? "📶" : (networkModel.ethernet ? "🌐" : "🔗")) : "🚫"
                        textFormat: Text.PlainText
                        color: networkModel.connected ? (networkModel.weak ? Style.colors.warning : Style.colors.success) : Style.colors.textMuted
                        font.family: Style.typography.family
                        font.pixelSize: Style.typography.basePx
                        anchors.verticalCenter: parent.verticalCenter
                    }

                    Text {
                        id: networkText
                        visible: networkModel.connected
                        text: networkModel.wifi ? ((networkModel.ssid && networkModel.ssid.length > 0) ? networkModel.ssid : ((networkModel.strength || 0) + "%")) : (networkModel.ethernet ? "Ethernet" : "Connected")
                        textFormat: Text.PlainText
                        color: networkModel.weak ? Style.colors.warning : Style.colors.text
                        font.family: Style.typography.family
                        font.pixelSize: Style.typography.basePx
                        font.bold: true
                        anchors.verticalCenter: parent.verticalCenter
                    }
                }

                // Battery indicator
                Row {
                    id: batteryIndicator
                    visible: powerModel.present
                    spacing: Style.spacing.small
                    anchors.verticalCenter: parent.verticalCenter

                    Text {
                        text: powerModel.charging ? "⚡" : "🔋"
                        textFormat: Text.PlainText
                        color: powerModel.charging ? Style.colors.success : (powerModel.low ? Style.colors.warning : Style.colors.textMuted)
                        font.family: Style.typography.family
                        font.pixelSize: Style.typography.basePx
                        anchors.verticalCenter: parent.verticalCenter
                    }

                    Text {
                        text: Math.round(powerModel.percentage) + "%"
                        textFormat: Text.PlainText
                        color: powerModel.low ? Style.colors.warning : Style.colors.text
                        font.family: Style.typography.family
                        font.pixelSize: Style.typography.basePx
                        font.bold: true
                        anchors.verticalCenter: parent.verticalCenter
                    }
                }
            }
        }
    }
}
