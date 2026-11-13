import QtQuick
import Kynex.Shell

Item {
    id: root
    property bool callReached: false
    property double observedX: 0.0
    property double observedY: 0.0
    property string observedItemId: ""
    property bool observedVisible: false

    TrayMenuModel {
        id: menuModel
        onItem_idChanged: {
            if (menuModel.item_id.length > 0) {
                root.callReached = true;
                root.observedX = menuModel.menu_x;
                root.observedY = menuModel.menu_y;
                root.observedItemId = menuModel.item_id;
            }
        }
    }

    function triggerOpen() {
        // Production argument list from styles/default/Shell.qml:383 (15 arguments)
        menuModel.open(
            "org.kynexos.ProductionItem",
            100.0, 8.0, 32.0, 32.0,
            200.0, 300.0,
            2 | 4 | 8,
            1920.0, 48.0,
            0.0, 0.0,
            1920.0, 1080.0,
            1.0
        );
    }
}
