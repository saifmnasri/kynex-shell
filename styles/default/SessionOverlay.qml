pragma ComponentBehavior: Bound
import QtQuick
import Kynex.Shell
import Kynex.Style
import Kynex.Controls

LayerSurface {
    // qmllint disable Quick.attached-property-type
    // qmllint disable compiler
    id: root

    property SessionModel sessionModel: null

    readonly property alias titleTextItem: titleText
    readonly property alias cardContainerItem: cardContainer
    readonly property alias actionsSectionItem: actionsSection
    readonly property alias confirmationSectionItem: confirmationSection
    readonly property alias progressSectionItem: progressSection
    readonly property alias confirmButtonItem: confirmButton
    readonly property alias cancelConfirmButtonItem: cancelConfirmButton
    readonly property alias confirmationPromptItem: confirmationPrompt
    readonly property alias logoutButtonItem: logoutBtn
    readonly property alias restartButtonItem: restartBtn
    readonly property alias powerOffButtonItem: powerOffBtn
    readonly property alias suspendButtonItem: suspendBtn
    readonly property alias lockButtonItem: lockBtn
    readonly property alias dismissButtonItem: dismissBtn
    readonly property alias escShortcutItem: escShortcut
    readonly property alias returnShortcutItem: returnShortcut
    readonly property alias enterShortcutItem: enterShortcut
    readonly property alias logoutTooltipItem: logoutTooltip
    readonly property alias restartTooltipItem: restartTooltip
    readonly property alias powerOffTooltipItem: powerOffTooltip
    readonly property alias suspendTooltipItem: suspendTooltip
    readonly property alias lockTooltipItem: lockTooltip
    readonly property alias logoutHoverAreaItem: logoutHoverArea
    readonly property alias restartHoverAreaItem: restartHoverArea
    readonly property alias powerOffHoverAreaItem: powerOffHoverArea
    readonly property alias suspendHoverAreaItem: suspendHoverArea
    readonly property alias lockHoverAreaItem: lockHoverArea
    readonly property alias errorBannerItem: errorBanner
    readonly property alias backdropMouseItem: backdropMouse

    visible: Boolean(root.sessionModel && root.sessionModel.visible)
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
    scope: "session-overlay"

    LayoutMirroring.enabled: Qt.locale().textDirection === Qt.RightToLeft
    LayoutMirroring.childrenInherit: true

    // Click outside overlay dismisses it
    MouseArea {
        id: backdropMouse
        anchors.fill: parent
        onClicked: {
            if (root.sessionModel) {
                root.sessionModel.dismiss();
            }
        }
    }

    // Dismiss or cancel on Escape
    Shortcut {
        id: escShortcut
        sequence: "Esc"
        onActivated: {
            if (root.sessionModel) {
                if (root.sessionModel.confirmingRestart || root.sessionModel.confirmingPowerOff) {
                    root.sessionModel.cancelConfirmation();
                } else {
                    root.sessionModel.dismiss();
                }
            }
        }
    }

    // Confirm on Return or Enter
    Shortcut {
        id: returnShortcut
        sequence: "Return"
        onActivated: {
            if (root.sessionModel && (root.sessionModel.confirmingRestart || root.sessionModel.confirmingPowerOff)) {
                root.sessionModel.confirmAction();
            }
        }
    }

    Shortcut {
        id: enterShortcut
        sequence: "Enter"
        onActivated: {
            if (root.sessionModel && (root.sessionModel.confirmingRestart || root.sessionModel.confirmingPowerOff)) {
                root.sessionModel.confirmAction();
            }
        }
    }

    Connections {
        target: root
        function onVisibleChanged(): void {
            if (root.visible) {
                if (root.sessionModel && (root.sessionModel.confirmingRestart || root.sessionModel.confirmingPowerOff)) {
                    confirmButton.forceActiveFocus();
                } else {
                    logoutBtn.forceActiveFocus();
                }
            }
        }
    }

    Connections {
        target: root.sessionModel
        function onConfirmingRestartChanged(): void {
            if (root.sessionModel && root.sessionModel.confirmingRestart) {
                confirmButton.forceActiveFocus();
            }
        }
        function onConfirmingPowerOffChanged(): void {
            if (root.sessionModel && root.sessionModel.confirmingPowerOff) {
                confirmButton.forceActiveFocus();
            }
        }
    }

    // Centered Card Container
    Rectangle {
        id: cardContainer
        anchors.centerIn: parent
        width: Style.spacing.panel + Style.spacing.panel + Style.spacing.panel + Style.spacing.panel + Style.spacing.panel + Style.spacing.panel + Style.spacing.panel
        implicitHeight: cardContent.implicitHeight + Style.spacing.large + Style.spacing.large
        radius: Style.radius.medium
        color: Style.colors.surface
        border.color: Style.colors.surfaceRaised
        border.width: 1

        // Consume clicks inside the card so backdrop doesn't trigger
        MouseArea {
            anchors.fill: parent
            onClicked: {}
        }

        Column {
            id: cardContent
            anchors.left: parent.left
            anchors.right: parent.right
            anchors.top: parent.top
            anchors.margins: Style.spacing.large
            spacing: Style.spacing.medium

            // Header Title
            Text {
                id: titleText
                text: {
                    if (root.sessionModel && root.sessionModel.confirmingRestart) {
                        return "Restart Computer";
                    }
                    if (root.sessionModel && root.sessionModel.confirmingPowerOff) {
                        return "Shut Down Computer";
                    }
                    if (root.sessionModel && root.sessionModel.actionInProgress) {
                        return "Please Wait";
                    }
                    return "Session & Power";
                }
                textFormat: Text.PlainText
                font.family: Style.typography.family
                font.pixelSize: Style.typography.basePx + Style.spacing.small
                font.weight: Font.Bold
                color: Style.colors.text
                width: parent.width
                horizontalAlignment: Text.AlignHCenter
                Accessible.role: Accessible.Heading
                Accessible.name: text
            }

            // In Progress State
            Column {
                id: progressSection
                visible: Boolean(root.sessionModel && root.sessionModel.actionInProgress)
                width: parent.width
                spacing: Style.spacing.medium

                Text {
                    text: "Performing requested session action…"
                    textFormat: Text.PlainText
                    font.family: Style.typography.family
                    font.pixelSize: Style.typography.basePx
                    color: Style.colors.textMuted
                    horizontalAlignment: Text.AlignHCenter
                    width: parent.width
                }
            }

            // Confirmation Dialog State
            Column {
                id: confirmationSection
                visible: Boolean(root.sessionModel && !root.sessionModel.actionInProgress && (root.sessionModel.confirmingRestart || root.sessionModel.confirmingPowerOff))
                width: parent.width
                spacing: Style.spacing.medium

                Text {
                    id: confirmationPrompt
                    text: (root.sessionModel && root.sessionModel.confirmingRestart)
                        ? "Are you sure you want to restart your computer? Unsaved changes will be lost."
                        : "Are you sure you want to shut down your computer? Unsaved changes will be lost."
                    textFormat: Text.PlainText
                    font.family: Style.typography.family
                    font.pixelSize: Style.typography.basePx
                    color: Style.colors.text
                    wrapMode: Text.Wrap
                    width: parent.width
                    horizontalAlignment: Text.AlignHCenter
                }

                Row {
                    width: parent.width
                    spacing: Style.spacing.medium

                    KynexButton {
                        id: confirmButton
                        text: (root.sessionModel && root.sessionModel.confirmingRestart) ? "Restart" : "Shut Down"
                        primary: true
                        width: (parent.width - Style.spacing.medium) / Number("2")
                        Accessible.name: text + " Confirm Button"
                        onClicked: {
                            if (root.sessionModel) {
                                root.sessionModel.confirmAction();
                            }
                        }
                    }

                    KynexButton {
                        id: cancelConfirmButton
                        text: "Cancel"
                        width: (parent.width - Style.spacing.medium) / Number("2")
                        Accessible.name: "Cancel Confirmation Button"
                        onClicked: {
                            if (root.sessionModel) {
                                root.sessionModel.cancelConfirmation();
                            }
                        }
                    }
                }
            }

            // Normal Menu State
            Column {
                id: actionsSection
                visible: Boolean(!root.sessionModel || (!root.sessionModel.actionInProgress && !root.sessionModel.confirmingRestart && !root.sessionModel.confirmingPowerOff))
                width: parent.width
                spacing: Style.spacing.small

                // Log out
                Item {
                    width: parent.width
                    height: logoutBtn.height

                    KynexButton {
                        id: logoutBtn
                        text: "Log out"
                        width: parent.width
                        enabled: Boolean(root.sessionModel && root.sessionModel.canLogout)
                        Accessible.name: "Log out Button"
                        onClicked: {
                            if (root.sessionModel) {
                                root.sessionModel.requestLogout();
                            }
                        }
                    }

                    MouseArea {
                        id: logoutHoverArea
                        anchors.fill: parent
                        hoverEnabled: true
                        acceptedButtons: Qt.NoButton
                    }

                    KynexTooltip {
                        id: logoutTooltip
                        text: "Logout is unavailable"
                        visible: logoutHoverArea.containsMouse && !logoutBtn.enabled
                        anchors.top: logoutBtn.bottom
                        anchors.horizontalCenter: logoutBtn.horizontalCenter
                    }
                }

                // Restart…
                Item {
                    width: parent.width
                    height: restartBtn.height

                    KynexButton {
                        id: restartBtn
                        text: "Restart…"
                        width: parent.width
                        enabled: Boolean(root.sessionModel && root.sessionModel.canRestart)
                        Accessible.name: "Restart Button"
                        onClicked: {
                            if (root.sessionModel) {
                                root.sessionModel.requestRestart();
                            }
                        }
                    }

                    MouseArea {
                        id: restartHoverArea
                        anchors.fill: parent
                        hoverEnabled: true
                        acceptedButtons: Qt.NoButton
                    }

                    KynexTooltip {
                        id: restartTooltip
                        text: "Restart is not authorized by system policy"
                        visible: restartHoverArea.containsMouse && !restartBtn.enabled
                        anchors.top: restartBtn.bottom
                        anchors.horizontalCenter: restartBtn.horizontalCenter
                    }
                }

                // Shut down…
                Item {
                    width: parent.width
                    height: powerOffBtn.height

                    KynexButton {
                        id: powerOffBtn
                        text: "Shut down…"
                        width: parent.width
                        enabled: Boolean(root.sessionModel && root.sessionModel.canPowerOff)
                        Accessible.name: "Shut down Button"
                        onClicked: {
                            if (root.sessionModel) {
                                root.sessionModel.requestPowerOff();
                            }
                        }
                    }

                    MouseArea {
                        id: powerOffHoverArea
                        anchors.fill: parent
                        hoverEnabled: true
                        acceptedButtons: Qt.NoButton
                    }

                    KynexTooltip {
                        id: powerOffTooltip
                        text: "Shut down is not authorized by system policy"
                        visible: powerOffHoverArea.containsMouse && !powerOffBtn.enabled
                        anchors.top: powerOffBtn.bottom
                        anchors.horizontalCenter: powerOffBtn.horizontalCenter
                    }
                }

                // Suspend
                Item {
                    width: parent.width
                    height: suspendBtn.height

                    KynexButton {
                        id: suspendBtn
                        text: "Suspend"
                        width: parent.width
                        enabled: Boolean(root.sessionModel && root.sessionModel.canSuspend)
                        Accessible.name: "Suspend Button"
                        onClicked: {
                            if (root.sessionModel) {
                                root.sessionModel.requestSuspend();
                            }
                        }
                    }

                    MouseArea {
                        id: suspendHoverArea
                        anchors.fill: parent
                        hoverEnabled: true
                        acceptedButtons: Qt.NoButton
                    }

                    KynexTooltip {
                        id: suspendTooltip
                        text: "Suspend is not authorized by system policy"
                        visible: suspendHoverArea.containsMouse && !suspendBtn.enabled
                        anchors.top: suspendBtn.bottom
                        anchors.horizontalCenter: suspendBtn.horizontalCenter
                    }
                }

                // Lock row (not available yet)
                Item {
                    width: parent.width
                    height: lockBtn.height

                    KynexButton {
                        id: lockBtn
                        text: "Lock (not available yet)"
                        width: parent.width
                        enabled: false
                        Accessible.name: "Lock Screen Unavailable Button"
                    }

                    MouseArea {
                        id: lockHoverArea
                        anchors.fill: parent
                        hoverEnabled: true
                        acceptedButtons: Qt.NoButton
                    }

                    KynexTooltip {
                        id: lockTooltip
                        text: "Screen locking is not implemented yet"
                        visible: lockHoverArea.containsMouse
                        anchors.top: lockBtn.bottom
                        anchors.horizontalCenter: lockBtn.horizontalCenter
                    }
                }

                // Separator
                Rectangle {
                    width: parent.width
                    height: 1
                    color: Style.colors.surfaceRaised
                }

                // Dismiss / Cancel Button
                KynexButton {
                    id: dismissBtn
                    text: "Cancel"
                    width: parent.width
                    Accessible.name: "Dismiss Session Overlay Button"
                    onClicked: {
                        if (root.sessionModel) {
                            root.sessionModel.dismiss();
                        }
                    }
                }
            }

            // Error Banner
            Rectangle {
                id: errorBanner
                visible: Boolean(root.sessionModel && root.sessionModel.errorMessage && root.sessionModel.errorMessage.length > 0)
                width: parent.width
                implicitHeight: errorText.implicitHeight + Style.spacing.medium
                radius: Style.radius.small
                color: Style.colors.surfaceRaised
                border.color: Style.colors.danger
                border.width: 1

                Text {
                    id: errorText
                    anchors.fill: parent
                    anchors.margins: Style.spacing.small
                    text: (root.sessionModel && root.sessionModel.errorMessage) ? root.sessionModel.errorMessage : ""
                    textFormat: Text.PlainText
                    font.family: Style.typography.family
                    font.pixelSize: Style.typography.basePx
                    color: Style.colors.danger
                    wrapMode: Text.Wrap
                    horizontalAlignment: Text.AlignHCenter
                }
            }
        }
    }
}
