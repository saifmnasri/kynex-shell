import QtQuick
import org.kynexos.core

LayerSurface {
    id: panel
    visible: true
    width: Screen.width
    height: 48

    layer: LayerSurface.LayerTop
    anchors: LayerSurface.AnchorBottom | LayerSurface.AnchorLeft | LayerSurface.AnchorRight
    exclusiveZone: 48
    leftMargin: 0
    rightMargin: 0
    topMargin: 0
    bottomMargin: 0
    keyboardInteractivity: LayerSurface.KeyboardInteractivityNone
    scope: "panel"

    Clock {
        id: clock
    }

    Timer {
        interval: 1000
        running: true
        repeat: true
        onTriggered: clock.update()
    }

    Rectangle {
        anchors.fill: parent
        color: "#1d2029"

        Row {
            anchors.centerIn: parent
            spacing: 12

            Text {
                text: clock.date
                color: "#9aa3b2"
                font.pixelSize: 13
                anchors.verticalCenter: parent.verticalCenter
            }

            Text {
                text: clock.time
                color: "#eceff4"
                font.pixelSize: 15
                font.bold: true
                anchors.verticalCenter: parent.verticalCenter
            }
        }
    }
}

