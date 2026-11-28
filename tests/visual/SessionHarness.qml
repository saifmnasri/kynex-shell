import QtQuick
import Kynex.Controls
import Kynex.Style

Rectangle {
    id: root
    width: Style.spacing.panel + Style.spacing.panel + Style.spacing.panel + Style.spacing.panel + Style.spacing.panel + Style.spacing.panel + Style.spacing.panel + Style.spacing.panel + Style.spacing.panel
    height: Style.spacing.panel + Style.spacing.panel + Style.spacing.panel + Style.spacing.panel + Style.spacing.panel + Style.spacing.panel + Style.spacing.panel + Style.spacing.panel + Style.spacing.panel
    color: Style.colors.surface

    property string scenario: "normal"

    readonly property bool isConfirmation: root.scenario === "confirmation"
    readonly property bool isDisabledScenario: root.scenario === "disabled"
    readonly property bool isRtl: root.scenario === "rtl"

    readonly property bool canLogoutVal: !root.isDisabledScenario && !root.isConfirmation
    readonly property bool canRestartVal: !root.isDisabledScenario && !root.isConfirmation
    readonly property bool canPowerOffVal: !root.isDisabledScenario && !root.isConfirmation
    readonly property bool canSuspendVal: !root.isDisabledScenario && !root.isConfirmation

    LayoutMirroring.enabled: root.isRtl
    LayoutMirroring.childrenInherit: true

    Rectangle {
        id: card
        anchors.centerIn: parent
        width: Style.spacing.panel + Style.spacing.panel + Style.spacing.panel + Style.spacing.panel + Style.spacing.panel + Style.spacing.panel + Style.spacing.panel
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

            // Header Title
            Text {
                id: titleText
                text: root.isConfirmation ? "Restart Computer" : "Session & Power"
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

            // Confirmation Section
            Column {
                id: confirmationSection
                visible: root.isConfirmation
                width: parent.width
                spacing: Style.spacing.medium

                Text {
                    text: "Are you sure you want to restart your computer? Unsaved changes will be lost."
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
                        text: "Restart"
                        primary: true
                        width: (parent.width - Style.spacing.medium) / Number("2")
                        Accessible.name: "Restart Confirm Button"
                    }

                    KynexButton {
                        text: "Cancel"
                        width: (parent.width - Style.spacing.medium) / Number("2")
                        Accessible.name: "Cancel Confirmation Button"
                    }
                }
            }

            // Normal & Disabled Menu Section
            Column {
                id: actionsSection
                visible: !root.isConfirmation
                width: parent.width
                spacing: Style.spacing.small

                Item {
                    width: parent.width
                    height: logoutBtn.height

                    KynexButton {
                        id: logoutBtn
                        text: "Log out"
                        width: parent.width
                        enabled: root.canLogoutVal
                        Accessible.name: "Log out Button"
                    }

                    KynexTooltip {
                        text: "Logout is unavailable"
                        visible: root.isDisabledScenario
                        anchors.top: logoutBtn.bottom
                        anchors.horizontalCenter: logoutBtn.horizontalCenter
                    }
                }

                Item {
                    width: parent.width
                    height: restartBtn.height

                    KynexButton {
                        id: restartBtn
                        text: "Restart…"
                        width: parent.width
                        enabled: root.canRestartVal
                        Accessible.name: "Restart Button"
                    }

                    KynexTooltip {
                        text: "Restart is not authorized by system policy"
                        visible: root.isDisabledScenario
                        anchors.top: restartBtn.bottom
                        anchors.horizontalCenter: restartBtn.horizontalCenter
                    }
                }

                Item {
                    width: parent.width
                    height: powerOffBtn.height

                    KynexButton {
                        id: powerOffBtn
                        text: "Shut down…"
                        width: parent.width
                        enabled: root.canPowerOffVal
                        Accessible.name: "Shut down Button"
                    }

                    KynexTooltip {
                        text: "Shut down is not authorized by system policy"
                        visible: root.isDisabledScenario
                        anchors.top: powerOffBtn.bottom
                        anchors.horizontalCenter: powerOffBtn.horizontalCenter
                    }
                }

                Item {
                    width: parent.width
                    height: suspendBtn.height

                    KynexButton {
                        id: suspendBtn
                        text: "Suspend"
                        width: parent.width
                        enabled: root.canSuspendVal
                        Accessible.name: "Suspend Button"
                    }

                    KynexTooltip {
                        text: "Suspend is not authorized by system policy"
                        visible: root.isDisabledScenario
                        anchors.top: suspendBtn.bottom
                        anchors.horizontalCenter: suspendBtn.horizontalCenter
                    }
                }

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

                    KynexTooltip {
                        text: "Screen locking is not implemented yet"
                        visible: root.isDisabledScenario
                        anchors.top: lockBtn.bottom
                        anchors.horizontalCenter: lockBtn.horizontalCenter
                    }
                }

                Rectangle {
                    width: parent.width
                    height: 1
                    color: Style.colors.surfaceRaised
                }

                KynexButton {
                    text: "Cancel"
                    width: parent.width
                    Accessible.name: "Dismiss Session Overlay Button"
                }
            }
        }
    }
}
