import QtQuick
import Kynex.Controls
import Kynex.Style

Rectangle {
    id: root
    width: Style.spacing.panel + Style.spacing.panel + Style.spacing.panel + Style.spacing.panel + Style.spacing.panel + Style.spacing.panel + Style.spacing.panel + Style.spacing.panel + Style.spacing.panel + Style.spacing.panel + Style.spacing.panel + Style.spacing.panel + Style.spacing.panel + Style.spacing.panel + Style.spacing.panel + Style.spacing.panel
    height: Style.spacing.panel + Style.spacing.panel + Style.spacing.panel + Style.spacing.panel + Style.spacing.panel + Style.spacing.panel + Style.spacing.panel + Style.spacing.panel + Style.spacing.panel + Style.spacing.panel
    color: Style.colors.surface

    property string scenario: "results"

    ListModel {
        id: sampleModel
        ListElement {
            name: "Terminal"
            genericName: "Command Line"
            comment: "Terminal emulator"
            letterBadge: "T"
        }
        ListElement {
            name: "Text Editor"
            genericName: "Editor"
            comment: "Edit text files"
            letterBadge: "T"
        }
    }

    Rectangle {
        id: card
        anchors.centerIn: parent
        width: Style.spacing.panel + Style.spacing.panel + Style.spacing.panel + Style.spacing.panel + Style.spacing.panel + Style.spacing.panel + Style.spacing.panel + Style.spacing.panel + Style.spacing.panel + Style.spacing.panel
        implicitHeight: cardContent.implicitHeight + Style.spacing.medium + Style.spacing.medium
        height: implicitHeight
        radius: Style.radius.medium
        color: Style.colors.surfaceRaised
        border.color: Style.colors.surface
        border.width: 1

        Column {
            id: cardContent
            anchors.left: parent.left
            anchors.right: parent.right
            anchors.top: parent.top
            anchors.margins: Style.spacing.medium
            spacing: Style.spacing.medium

            KynexSearchField {
                id: searchField
                anchors.left: parent.left
                anchors.right: parent.right
                placeholderText: "Search applications..."
                text: (root.scenario === "empty") ? "" : ((root.scenario === "no-results") ? "nonexistent app" : "term")
            }

            Item {
                id: listContainer
                anchors.left: parent.left
                anchors.right: parent.right
                height: resultsList.visible ? resultsList.height : emptyPlaceholder.height

                ListView {
                    id: resultsList
                    anchors.left: parent.left
                    anchors.right: parent.right
                    anchors.top: parent.top
                    visible: root.scenario === "results" || root.scenario === "rtl"
                    model: sampleModel
                    currentIndex: 0
                    clip: true
                    spacing: Style.spacing.small
                    implicitHeight: contentHeight
                    height: Math.min(contentHeight, Style.spacing.panel + Style.spacing.panel + Style.spacing.panel + Style.spacing.panel + Style.spacing.panel + Style.spacing.panel)

                    delegate: KynexListItem {
                        required property int index
                        required property string name
                        required property string genericName
                        required property string comment
                        required property string letterBadge

                        width: resultsList.width
                        text: name
                        secondaryText: genericName.length > 0 ? genericName : comment
                        letterBadge: letterBadge
                        selected: index === 0
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
}
