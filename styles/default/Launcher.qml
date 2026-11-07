pragma ComponentBehavior: Bound
import QtQuick
import Kynex.Shell
import Kynex.Style
import Kynex.Controls

LayerSurface {
    id: root

    property LauncherModel launcherModel: null
    readonly property var model: root.launcherModel
    property bool launching: false

    readonly property alias searchFieldItem: searchField
    readonly property alias backdropArea: backdropMouse
    readonly property alias listView: resultsList
    readonly property alias dismissShortcut: escShortcut
    readonly property alias placeholderTextItem: emptyPlaceholderText

    function triggerLaunchSelected(): void {
        if (!root.launching && root.model) {
            root.launching = true;
            root.model.launchSelected();
        }
    }

    function triggerLaunch(index: int): void {
        if (!root.launching && root.model) {
            root.launching = true;
            root.model.launch(index);
        }
    }

    visible: root.launcherModel ? root.launcherModel.visible : false
    width: Screen.width
    height: Screen.height

    layer: LayerSurface.LayerOverlay
    anchors: LayerSurface.AnchorTop + LayerSurface.AnchorBottom + LayerSurface.AnchorLeft + LayerSurface.AnchorRight
    exclusiveZone: 0
    leftMargin: 0
    rightMargin: 0
    topMargin: 0
    bottomMargin: 0
    keyboardInteractivity: LayerSurface.KeyboardInteractivityExclusive
    scope: "launcher"

    // Click outside central card dismisses launcher
    MouseArea {
        id: backdropMouse
        anchors.fill: parent
        onClicked: {
            if (root.model) {
                root.model.close();
            }
        }
    }

    // Dismiss on Escape
    Shortcut {
        id: escShortcut
        sequence: "Esc"
        onActivated: {
            if (root.model) {
                root.model.close();
            }
        }
    }

    // Toggle pin on Ctrl+P
    Shortcut {
        id: pinShortcut
        sequence: "Ctrl+P"
        onActivated: {
            if (root.model) {
                root.model.togglePinSelected();
            }
        }
    }

    Rectangle {
        id: card
        anchors.centerIn: parent
        width: Math.min(parent.width - Style.spacing.panel - Style.spacing.panel, Style.spacing.panel + Style.spacing.panel + Style.spacing.panel + Style.spacing.panel + Style.spacing.panel + Style.spacing.panel + Style.spacing.panel + Style.spacing.panel + Style.spacing.panel + Style.spacing.panel)
        implicitHeight: cardContent.implicitHeight + Style.spacing.medium + Style.spacing.medium
        height: implicitHeight
        radius: Style.radius.medium
        color: Style.colors.surfaceRaised
        border.color: Style.colors.surface
        border.width: 1

        // Consume clicks inside card
        MouseArea {
            anchors.fill: parent
        }

        Column {
            id: cardContent
            anchors.fill: parent
            anchors.margins: Style.spacing.medium
            spacing: Style.spacing.medium

            KynexSearchField {
                id: searchField
                width: parent.width
                Keys.priority: Keys.BeforeItem
                text: root.launcherModel ? root.launcherModel.query : ""
                placeholderText: "Search applications..."

                onTextEdited: {
                    if (root.model) {
                        root.model.setSearchQuery(searchField.text);
                    }
                }

                Keys.onUpPressed: (event) => {
                    if (root.model) {
                        root.model.selectPrev();
                    }
                    event.accepted = true;
                }

                Keys.onDownPressed: (event) => {
                    if (root.model) {
                        root.model.selectNext();
                    }
                    event.accepted = true;
                }

                Keys.onReturnPressed: (event) => {
                    root.triggerLaunchSelected();
                    event.accepted = true;
                }

                Keys.onEnterPressed: (event) => {
                    root.triggerLaunchSelected();
                    event.accepted = true;
                }

                Keys.onEscapePressed: (event) => {
                    if (root.model) {
                        root.model.close();
                    }
                    event.accepted = true;
                }

                Keys.onPressed: (event) => {
                    if (event.key === Qt.Key_P && (event.modifiers & Qt.ControlModifier)) {
                        if (root.model) {
                            root.model.togglePinSelected();
                        }
                        event.accepted = true;
                    }
                }
            }

            Item {
                id: listContainer
                width: parent.width
                height: resultsList.visible ? resultsList.height : emptyPlaceholder.height

                ListView {
                    id: resultsList
                    anchors.left: parent.left
                    anchors.right: parent.right
                    anchors.top: parent.top
                    visible: root.launcherModel ? (root.launcherModel.count > 0) : false
                    model: root.launcherModel
                    currentIndex: (root.launcherModel && root.launcherModel.selectedIndex !== undefined) ? root.launcherModel.selectedIndex : 0
                    clip: true
                    spacing: Style.spacing.small
                    implicitHeight: contentHeight
                    height: Math.min(contentHeight, Style.spacing.panel + Style.spacing.panel + Style.spacing.panel + Style.spacing.panel + Style.spacing.panel + Style.spacing.panel)

                    delegate: KynexListItem {
                        required property int index
                        required property string id
                        required property string name
                        required property string genericName
                        required property string comment
                        required property string iconName
                        required property string letterBadge
                        required property bool isPinned

                        width: resultsList.width
                        text: name
                        secondaryText: genericName.length > 0 ? genericName : comment
                        letterBadge: letterBadge
                        iconSource: iconName.length > 0 ? ("image://kynex-icon/" + iconName + "?size=24&scale=1") : ""
                        pinned: isPinned
                        selected: root.launcherModel ? (root.launcherModel.selectedIndex === index) : false

                        onClicked: {
                            root.triggerLaunch(index);
                        }
                    }
                }

                Item {
                    id: emptyPlaceholder
                    anchors.left: parent.left
                    anchors.right: parent.right
                    anchors.top: parent.top
                    visible: !resultsList.visible
                    height: Style.spacing.panel + Style.spacing.panel

                    Text {
                        id: emptyPlaceholderText
                        anchors.centerIn: parent
                        text: (searchField.text.length > 0) ? "No applications found" : "Type to search applications"
                        textFormat: Text.PlainText
                        font.family: Style.typography.family
                        font.pixelSize: Style.typography.basePx
                        color: Style.colors.textMuted
                    }
                }
            }
        }
    }

    Connections {
        target: root
        function onVisibleChanged(): void {
            if (root.visible) {
                root.launching = false;
                searchField.forceFieldFocus();
            }
        }
    }

    Component.onCompleted: {
        if (root.visible) {
            root.launching = false;
            searchField.forceFieldFocus();
        }
        root.frameSwapped.connect(function() {
            if (root.visible && root.model) {
                root.model.recordFirstFrame();
            }
        });
    }
}
