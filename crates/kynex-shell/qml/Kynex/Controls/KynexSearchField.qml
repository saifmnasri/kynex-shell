import QtQuick
import Kynex.Style

FocusScope {
    id: root

    property string text: ""
    property string placeholderText: "Search applications..."
    property bool readOnly: false

    readonly property bool hasText: root.text.length > 0
    readonly property color borderColor: bg.border.color
    readonly property int borderWidth: bg.border.width
    readonly property color backgroundColor: bg.color
    readonly property color textColor: textInput.color
    readonly property bool clearButtonVisible: clearBtn.visible
    readonly property real inputX: textInput.x
    readonly property bool clearButtonHovered: clearMouse.containsMouse
    readonly property bool inputEnabled: textInput.enabled
    readonly property var clearAccessibleRole: clearMouse.Accessible.role
    readonly property string clearAccessibleName: clearMouse.Accessible.name
    readonly property Item clearButtonItem: clearBtn
    readonly property Item clearMouseItem: clearMouse
    readonly property bool inputActiveFocus: textInput.activeFocus

    signal accepted()
    signal cleared()
    signal textEdited()

    function clear(): void {
        if (root.enabled) {
            textInput.text = "";
            root.text = "";
            root.cleared();
            root.textEdited();
        }
    }

    function selectAll(): void {
        textInput.selectAll();
    }

    function forceFieldFocus(): void {
        textInput.forceActiveFocus();
    }

    activeFocusOnTab: true
    Accessible.role: Accessible.EditableText
    Accessible.name: root.placeholderText
    Accessible.focusable: true
    Accessible.description: root.text

    LayoutMirroring.enabled: Qt.locale().textDirection === Qt.RightToLeft
    LayoutMirroring.childrenInherit: true

    implicitWidth: Style.spacing.panel + Style.spacing.panel + Style.spacing.panel + Style.spacing.panel
    implicitHeight: Style.spacing.panel - Style.spacing.small
    width: implicitWidth
    height: implicitHeight

    opacity: enabled ? 1 : Style.opacity.disabled

    Rectangle {
        id: bg
        anchors.fill: parent
        radius: Style.radius.small
        color: Style.colors.surface
        border.color: root.activeFocus ? Style.colors.focusRing : Style.colors.surfaceRaised
        border.width: root.activeFocus ? 1 : 0
    }

    Row {
        id: layoutRow
        anchors.fill: parent
        anchors.leftMargin: Style.spacing.medium
        anchors.rightMargin: Style.spacing.medium
        spacing: Style.spacing.small

        Text {
            id: searchIcon
            anchors.verticalCenter: parent.verticalCenter
            text: "🔍"
            color: Style.colors.textMuted
            font.family: Style.typography.family
            font.pixelSize: Style.typography.basePx
        }

        Item {
            id: inputContainer
            anchors.verticalCenter: parent.verticalCenter
            width: layoutRow.width - searchIcon.width - (clearBtn.visible ? clearBtn.width : 0) - (layoutRow.spacing + layoutRow.spacing)
            height: parent.height

            TextInput {
                id: textInput
                anchors.left: parent.left
                anchors.right: parent.right
                anchors.verticalCenter: parent.verticalCenter
                text: root.text
                readOnly: root.readOnly
                font.family: Style.typography.family
                font.pixelSize: Style.typography.basePx
                color: Style.colors.text
                selectByMouse: true
                selectionColor: Style.colors.accent
                selectedTextColor: Style.colors.surface
                clip: true
                horizontalAlignment: TextInput.AlignLeft

                onTextEdited: {
                    root.text = textInput.text;
                    root.textEdited();
                }

                onAccepted: {
                    root.accepted();
                }
            }

            Text {
                id: placeholder
                anchors.left: parent.left
                anchors.right: parent.right
                anchors.verticalCenter: parent.verticalCenter
                visible: !root.hasText
                text: root.placeholderText
                color: Style.colors.textMuted
                font.family: Style.typography.family
                font.pixelSize: Style.typography.basePx
                elide: Text.ElideRight
                horizontalAlignment: Text.AlignLeft
            }
        }

        Item {
            id: clearBtn
            anchors.verticalCenter: parent.verticalCenter
            width: Style.typography.basePx + Style.spacing.small
            height: Style.typography.basePx + Style.spacing.small
            visible: root.hasText && root.enabled

            Rectangle {
                id: clearBg
                anchors.fill: parent
                radius: Style.radius.small
                color: Style.colors.surfaceRaised
                opacity: clearMouse.pressed ? Style.opacity.pressed : (clearMouse.containsMouse ? Style.opacity.hover : 0)
            }

            Text {
                anchors.centerIn: parent
                text: "✕"
                color: clearMouse.containsMouse ? Style.colors.text : Style.colors.textMuted
                font.family: Style.typography.family
                font.pixelSize: Style.typography.basePx - Style.spacing.unit
            }

            MouseArea {
                id: clearMouse
                anchors.fill: parent
                hoverEnabled: true
                cursorShape: Qt.PointingHandCursor
                Accessible.role: Accessible.Button
                Accessible.name: "Clear search"
                onClicked: {
                    root.clear();
                    textInput.forceActiveFocus();
                }
            }
        }
    }

    MouseArea {
        anchors.fill: parent
        z: -1
        onClicked: {
            textInput.forceActiveFocus();
        }
    }

    onActiveFocusChanged: {
        if (activeFocus) {
            textInput.forceActiveFocus();
        }
    }
}
