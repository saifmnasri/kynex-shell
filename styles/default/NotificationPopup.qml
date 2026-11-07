pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Layouts
import Kynex.Shell
import Kynex.Style
import Kynex.Controls

LayerSurface {
    id: root

    property NotificationModel notificationModel: null
    readonly property var model: root.notificationModel

    readonly property alias popupColumn: popupColumn
    readonly property alias popupRepeater: popupRepeater

    readonly property int popupWidth: Style.spacing.panel + Style.spacing.panel + Style.spacing.panel + Style.spacing.panel + Style.spacing.panel + Style.spacing.panel + Style.spacing.panel + Style.spacing.large + Style.spacing.small

    visible: root.notificationModel ? root.notificationModel.count > 0 : false
    width: root.popupWidth
    height: Math.max(1, popupColumn.implicitHeight)

    layer: LayerSurface.LayerTop
    anchors: LayerSurface.AnchorTop + LayerSurface.AnchorRight
    exclusiveZone: 0
    topMargin: Style.spacing.panel + Style.spacing.medium
    rightMargin: Style.spacing.medium
    bottomMargin: 0
    leftMargin: 0
    keyboardInteractivity: LayerSurface.KeyboardInteractivityNone
    scope: "notifications"

    function updateInputMask(): void {
        if (root.visible && root.height > 0) {
            root.setInputMask(0, 0, root.width, root.height);
        } else {
            root.clearInputMask();
        }
    }

    onHeightChanged: root.updateInputMask()
    onVisibleChanged: root.updateInputMask()

    Column {
        id: popupColumn
        width: root.width
        spacing: Style.spacing.small

        Repeater {
            id: popupRepeater
            model: root.notificationModel

            delegate: Rectangle {
                id: popupCard
                required property int index
                required property int id
                required property string appName
                required property bool appNameVerified
                required property string appIcon
                required property string letterBadge
                required property string summary
                required property string body
                required property string actionsJson
                required property var actionsKeys
                required property var actionsLabels
                required property bool hasDefaultAction
                required property int urgency
                required property int timeoutMs

                readonly property alias iconImg: iconImg
                readonly property alias badgeFallback: badgeFallback
                readonly property alias appNameText: appNameText
                readonly property alias dismissBtn: dismissBtn
                readonly property alias summaryText: summaryText
                readonly property alias bodyText: bodyText
                readonly property alias actionsRow: actionsRow
                readonly property alias actionRepeater: actionRepeater
                readonly property alias cardHoverArea: cardHoverArea

                width: popupColumn.width
                implicitHeight: cardLayout.implicitHeight + Style.spacing.medium + Style.spacing.medium
                color: Style.colors.surfaceRaised
                radius: Style.radius.medium
                border.width: 1
                border.color: urgency === (1 + 1) ? Style.colors.danger : Style.colors.surface

                LayoutMirroring.enabled: Qt.locale().textDirection === Qt.RightToLeft
                LayoutMirroring.childrenInherit: true

                Accessible.role: Accessible.AlertMessage
                Accessible.name: appName + ": " + summary
                Accessible.description: body

                MouseArea {
                    id: cardHoverArea
                    anchors.fill: parent
                    hoverEnabled: true

                    onEntered: {
                        if (root.model) {
                            root.model.pause(popupCard.id);
                        }
                    }

                    onExited: {
                        if (root.model) {
                            root.model.resume(popupCard.id);
                        }
                    }

                    onClicked: {
                        if (!root.model) {
                            return;
                        }
                        if (popupCard.hasDefaultAction) {
                            root.model.invokeAction(popupCard.id, "default");
                        } else {
                            root.model.dismiss(popupCard.id);
                        }
                    }
                }

                ColumnLayout {
                    id: cardLayout
                    anchors.left: parent.left
                    anchors.leftMargin: Style.spacing.medium
                    anchors.right: parent.right
                    anchors.rightMargin: Style.spacing.medium
                    anchors.top: parent.top
                    anchors.topMargin: Style.spacing.medium
                    spacing: Style.spacing.small

                    // Header row: Icon, App Name & Verification, Close button
                    RowLayout {
                        Layout.fillWidth: true
                        spacing: Style.spacing.small

                        Image {
                            id: iconImg
                            source: popupCard.appIcon.length > 0 ? ("image://kynex-icon/" + popupCard.appIcon + "?size=24&scale=1") : ""
                            visible: popupCard.appIcon.length > 0 && status === Image.Ready
                            Layout.preferredWidth: Style.spacing.large + Style.spacing.small
                            Layout.preferredHeight: Style.spacing.large + Style.spacing.small
                        }

                        Rectangle {
                            id: badgeFallback
                            visible: !iconImg.visible
                            Layout.preferredWidth: Style.spacing.large + Style.spacing.small
                            Layout.preferredHeight: Style.spacing.large + Style.spacing.small
                            radius: Style.radius.small
                            color: Style.colors.accent

                            Text {
                                anchors.centerIn: parent
                                text: popupCard.letterBadge
                                color: Style.colors.surface
                                font.family: Style.typography.family
                                font.pixelSize: Style.typography.basePx
                                font.bold: true
                            }
                        }

                        Text {
                            id: appNameText
                            Layout.fillWidth: true
                            text: popupCard.appName + (popupCard.appNameVerified ? "" : " (Unverified)")
                            textFormat: Text.PlainText
                            elide: Text.ElideRight
                            color: Style.colors.textMuted
                            font.family: Style.typography.family
                            font.pixelSize: Style.typography.basePx - Style.spacing.unit
                        }

                        KynexButton {
                            id: dismissBtn
                            text: "✕"
                            implicitWidth: Style.spacing.panel - Style.spacing.large
                            implicitHeight: Style.spacing.panel - Style.spacing.large
                            Accessible.name: "Dismiss notification from " + popupCard.appName

                            onClicked: {
                                if (root.model) {
                                    root.model.dismiss(popupCard.id);
                                }
                            }
                        }
                    }

                    // Notification summary
                    Text {
                        id: summaryText
                        Layout.fillWidth: true
                        text: popupCard.summary
                        textFormat: Text.PlainText
                        font.bold: true
                        font.family: Style.typography.family
                        font.pixelSize: Style.typography.basePx
                        color: Style.colors.text
                        elide: Text.ElideRight
                    }

                    // Notification body
                    Text {
                        id: bodyText
                        Layout.fillWidth: true
                        visible: text.length > 0
                        text: popupCard.body
                        textFormat: Text.PlainText
                        color: Style.colors.textMuted
                        font.family: Style.typography.family
                        font.pixelSize: Style.typography.basePx
                        wrapMode: Text.Wrap
                        maximumLineCount: 1 + 1 + 1
                        elide: Text.ElideRight
                    }

                    // Action buttons row (excludes "default" which is bound to body click)
                    RowLayout {
                        id: actionsRow
                        Layout.fillWidth: true
                        spacing: Style.spacing.small
                        visible: actionRepeater.count > 0

                        Repeater {
                            id: actionRepeater
                            model: popupCard.actionsLabels

                            delegate: KynexButton {
                                id: actionBtn
                                required property int index
                                required property string modelData
                                text: modelData

                                onClicked: {
                                    if (root.model) {
                                        root.model.invokeAction(popupCard.id, popupCard.actionsKeys[actionBtn.index]);
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    Component.onCompleted: {
        root.frameSwapped.connect(function() {
            if (root.visible && root.model) {
                root.model.recordFirstFrame();
            }
        });
    }
}
