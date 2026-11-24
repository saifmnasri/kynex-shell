import QtQuick
import Kynex.Controls
import Kynex.Style

Rectangle {
    id: root
    property string scenario: "2-windows"

    // Geometry selection based on scenario
    width: {
        if (scenario === "1920" || scenario === "geom-1920") return 1920;
        if (scenario === "2520" || scenario === "geom-2520") return 2520;
        if (scenario === "3840" || scenario === "geom-3840") return 3840;
        return 1280;
    }
    height: Style.spacing.panel
    color: Style.colors.surfaceRaised

    LayoutMirroring.enabled: scenario === "rtl"
    LayoutMirroring.childrenInherit: true

    // Model for mock taskbar windows
    ListModel {
        id: mockWindowsModel

        Component.onCompleted: {
            populate();
        }

        function populate() {
            clear();
            if (root.scenario === "0" || root.scenario === "0-windows") {
                return;
            }

            if (root.scenario === "1" || root.scenario === "1-window") {
                append({
                    "id": "win1",
                    "title": "Calculator",
                    "appId": "org.gnome.Calculator",
                    "iconName": "calc",
                    "letterBadge": "C",
                    "active": true,
                    "minimized": false,
                    "isPinned": false,
                    "isRunning": true,
                    "windowCount": 1
                });
                return;
            }

            if (root.scenario === "minimized") {
                append({
                    "id": "win1",
                    "title": "Calculator",
                    "appId": "org.gnome.Calculator",
                    "iconName": "calc",
                    "letterBadge": "C",
                    "active": false,
                    "minimized": true,
                    "isPinned": false,
                    "isRunning": true,
                    "windowCount": 1
                });
                append({
                    "id": "win2",
                    "title": "Terminal",
                    "appId": "org.gnome.Terminal",
                    "iconName": "term",
                    "letterBadge": "T",
                    "active": false,
                    "minimized": true,
                    "isPinned": false,
                    "isRunning": true,
                    "windowCount": 1
                });
                return;
            }

            if (root.scenario === "8" || root.scenario === "8-windows") {
                let titles = [
                    "Calculator",
                    "Terminal",
                    "Text Editor - Extremely Long Document Title That Tests Proper Right Elision Without Clock Collision",
                    "Web Browser",
                    "Files",
                    "Settings",
                    "System Monitor",
                    "Music"
                ];
                let badges = ["C", "T", "E", "B", "F", "S", "M", "P"];
                for (let i = 0; i < titles.length; ++i) {
                    append({
                        "id": "win" + (i + 1),
                        "title": titles[i],
                        "appId": "app" + (i + 1),
                        "iconName": "",
                        "letterBadge": badges[i],
                        "active": (i === 1), // Terminal active
                        "minimized": (i === 3 || i === 6), // Browser and Monitor minimized
                        "isPinned": (i < 2),
                        "isRunning": true,
                        "windowCount": (i === 1 ? 2 : 1)
                    });
                }
                return;
            }

            // Default: 2 windows (1 active, 1 minimized)
            append({
                "id": "win1",
                "title": "Calculator",
                "appId": "org.gnome.Calculator",
                "iconName": "calc",
                "letterBadge": "C",
                "active": true,
                "minimized": false,
                "isPinned": false,
                "isRunning": true,
                "windowCount": 1
            });
            append({
                "id": "win2",
                "title": "Terminal",
                "appId": "org.gnome.Terminal",
                "iconName": "term",
                "letterBadge": "T",
                "active": false,
                "minimized": true,
                "isPinned": false,
                "isRunning": true,
                "windowCount": 1
            });
        }
    }

    onScenarioChanged: {
        mockWindowsModel.populate();
    }

    // 1. Taskbar buttons area (Left anchored, ends before centered clock)
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
        model: mockWindowsModel
        visible: count > 0

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
            required property int windowCount

            text: title.length > 0 ? title : (appId.length > 0 ? appId : "Window")
            letterBadge: letterBadge
            iconSource: iconName.length > 0 ? ("image://kynex-icon/" + iconName + "?size=24&scale=1") : ""
            primary: active
            showRunningIndicator: isRunning
            windowCount: windowCount
            opacity: (isRunning && minimized) ? Style.opacity.disabled : 1
            width: Math.min(implicitWidth, Style.spacing.panel * 3)
            height: Style.spacing.panel - Style.spacing.small

            Accessible.name: (title.length > 0 ? title : "App") + (active ? " (Active)" : (isRunning ? " (Running)" : " (Pinned)"))

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

    // 2. Centered Clock
    Row {
        id: clockRow
        anchors.centerIn: parent
        spacing: Style.spacing.small

        Text {
            id: timeText
            text: "12:00"
            textFormat: Text.PlainText
            font.family: Style.typography.family
            font.pixelSize: Style.typography.basePx
            font.bold: true
            color: Style.colors.text
            anchors.verticalCenter: parent.verticalCenter
        }

        Text {
            id: dateText
            text: "Fri Nov 20"
            textFormat: Text.PlainText
            font.family: Style.typography.family
            font.pixelSize: Style.typography.basePx - Style.spacing.unit
            color: Style.colors.textMuted
            anchors.verticalCenter: parent.verticalCenter
        }
    }

    // 3. Status Area (Right anchored, opening Quick Settings)
    Row {
        id: statusArea
        anchors.right: parent.right
        anchors.rightMargin: Style.spacing.medium
        anchors.verticalCenter: parent.verticalCenter
        spacing: Style.spacing.small

        Rectangle {
            id: statusFocusScope
            width: statusContent.implicitWidth + Style.spacing.medium + Style.spacing.medium
            height: Style.spacing.panel - Style.spacing.small
            radius: Style.radius.small
            color: Style.colors.surfaceRaised

            Row {
                id: statusContent
                anchors.centerIn: parent
                spacing: Style.spacing.small

                // Volume Status
                Row {
                    spacing: Style.spacing.unit
                    anchors.verticalCenter: parent.verticalCenter
                    Text {
                        text: "🔊"
                        font.pixelSize: Style.typography.basePx
                        anchors.verticalCenter: parent.verticalCenter
                    }
                    Text {
                        text: "50%"
                        textFormat: Text.PlainText
                        font.family: Style.typography.family
                        font.pixelSize: Style.typography.basePx - Style.spacing.unit
                        color: Style.colors.text
                        anchors.verticalCenter: parent.verticalCenter
                    }
                }

                // Network Status
                Row {
                    spacing: Style.spacing.unit
                    anchors.verticalCenter: parent.verticalCenter
                    Text {
                        text: "📶"
                        font.pixelSize: Style.typography.basePx
                        anchors.verticalCenter: parent.verticalCenter
                    }
                    Text {
                        text: "Wi-Fi"
                        textFormat: Text.PlainText
                        font.family: Style.typography.family
                        font.pixelSize: Style.typography.basePx - Style.spacing.unit
                        color: Style.colors.text
                        anchors.verticalCenter: parent.verticalCenter
                    }
                }

                // Battery Status
                Row {
                    spacing: Style.spacing.unit
                    anchors.verticalCenter: parent.verticalCenter
                    Text {
                        text: "🔋"
                        font.pixelSize: Style.typography.basePx
                        anchors.verticalCenter: parent.verticalCenter
                    }
                    Text {
                        text: "85%"
                        textFormat: Text.PlainText
                        font.family: Style.typography.family
                        font.pixelSize: Style.typography.basePx - Style.spacing.unit
                        color: Style.colors.text
                        anchors.verticalCenter: parent.verticalCenter
                    }
                }
            }
        }
    }
}
