import QtQuick
import Kynex.Controls
import Kynex.Style

Rectangle {
    id: root
    width: Style.spacing.panel + Style.spacing.panel + Style.spacing.panel + Style.spacing.panel + Style.spacing.panel + Style.spacing.panel + Style.spacing.panel + Style.spacing.panel
    height: Style.spacing.panel + Style.spacing.panel + Style.spacing.panel + Style.spacing.panel + Style.spacing.panel + Style.spacing.panel + Style.spacing.panel + Style.spacing.panel + Style.spacing.panel + Style.spacing.panel + Style.spacing.panel + Style.spacing.panel
    color: Style.colors.surface

    Column {
        anchors.fill: parent
        anchors.margins: Style.spacing.medium
        spacing: Style.spacing.medium

        Text {
            text: "Kynex Base Controls Gallery"
            font.family: Style.typography.family
            font.pixelSize: Style.typography.basePx
            color: Style.colors.text
        }

        Row {
            spacing: Style.spacing.small
            KynexButton {
                text: "Standard"
            }
            KynexButton {
                text: "Primary"
                primary: true
            }
            KynexButton {
                id: focusedBtn
                text: "Focused"
                focus: true
                Component.onCompleted: forceActiveFocus()
            }
            KynexButton {
                text: "Disabled"
                enabled: false
            }
        }

        Row {
            spacing: Style.spacing.medium
            KynexSwitch {
                text: "Switch Off"
                checked: false
            }
            KynexSwitch {
                text: "Switch On"
                checked: true
            }
            KynexSwitch {
                text: "Disabled"
                enabled: false
                checked: true
            }
        }

        Column {
            spacing: Style.spacing.small
            width: parent.width - Style.spacing.medium - Style.spacing.medium

            KynexSlider {
                value: 0
            }
            KynexSlider {
                value: 1 / (1 + 1)
            }
            KynexSlider {
                value: 1
            }
            KynexSlider {
                value: 1 / (1 + 1)
                enabled: false
            }
        }

        KynexTooltip {
            text: "KynexTooltip preview overlay"
            visible: true
        }

        Column {
            spacing: Style.spacing.small
            width: parent.width - Style.spacing.medium - Style.spacing.medium

            KynexListItem {
                text: "List Item Normal"
                secondaryText: "Detailed subtitle text"
            }
            KynexListItem {
                text: "List Item Selected"
                secondaryText: "Selected active item"
                selected: true
            }
            KynexListItem {
                text: "List Item Disabled"
                secondaryText: "Inactive item"
                enabled: false
            }
        }

        Text {
            text: "Taskbar Buttons (Active, Inactive, Minimized, Elided)"
            font.family: Style.typography.family
            font.pixelSize: Style.typography.basePx
            color: Style.colors.text
        }

        Row {
            spacing: Style.spacing.small
            KynexButton {
                text: "Active Window"
                primary: true
                width: Style.spacing.panel + Style.spacing.panel + Style.spacing.panel
                height: Style.spacing.panel - Style.spacing.small
            }
            KynexButton {
                text: "Inactive Window"
                primary: false
                width: Style.spacing.panel + Style.spacing.panel + Style.spacing.panel
                height: Style.spacing.panel - Style.spacing.small
            }
            KynexButton {
                text: "Minimized Window"
                primary: false
                opacity: Style.opacity.disabled
                width: Style.spacing.panel + Style.spacing.panel + Style.spacing.panel
                height: Style.spacing.panel - Style.spacing.small
            }
            KynexButton {
                text: "Very Long Window Title That Must Be Elided Cleanly"
                primary: false
                width: Style.spacing.panel + Style.spacing.panel + Style.spacing.panel
                height: Style.spacing.panel - Style.spacing.small
            }
        }
    }
}

