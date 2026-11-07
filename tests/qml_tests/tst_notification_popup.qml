import QtQuick
import QtTest
import Kynex.Shell
import Kynex.Style
import Kynex.Controls

Item {
    id: root
    width: 1024
    height: 768

    NotificationModel {
        id: mockModel
    }

    TestCase {
        name: "NotificationPopupTests"
        when: windowShown

        property var popup: null

        function init() {
            mockModel.clear();
            if (popup) {
                popup.destroy();
                popup = null;
            }
            let comp = Qt.createComponent("../../styles/default/NotificationPopup.qml");
            compare(comp.status, Component.Ready, "NotificationPopup.qml should load cleanly");
            popup = comp.createObject(null, { "notificationModel": mockModel });
            verify(popup !== null, "NotificationPopup should be instantiated");
        }

        function cleanup() {
            if (popup) {
                popup.destroy();
                popup = null;
            }
        }

        function test_null_model_initial_state() {
            let comp = Qt.createComponent("../../styles/default/NotificationPopup.qml");
            let popupNull = comp.createObject(null, { "notificationModel": null });
            verify(popupNull !== null);
            compare(popupNull.visible, false, "Popup without a notificationModel must be hidden");
            popupNull.destroy();
        }

        function test_initial_state() {
            compare(popup.visible, false, "Popup should be hidden when count is 0");
            compare(popup.width, popup.popupWidth, "Popup width should match token computed width");
            compare(popup.keyboardInteractivity, LayerSurface.KeyboardInteractivityNone, "Must never steal keyboard focus");
            compare(popup.hasInputMask, false, "Input mask must be cleared when popup is hidden");
        }

        function test_visibility_and_mask() {
            compare(popup.visible, false);
            compare(popup.hasInputMask, false);

            mockModel.addNotification(1, "App1", "Summary 1", "Body 1", 1);
            compare(mockModel.count, 1);
            compare(popup.visible, true, "Popup should become visible when notifications exist");
            tryVerify(function() { return popup.hasInputMask; }, 1000, "Input mask must be active when visible");
            compare(popup.hasInputMask, true);

            mockModel.clear();
            compare(mockModel.count, 0);
            compare(popup.visible, false, "Popup should hide when notifications are cleared");
            tryVerify(function() { return !popup.hasInputMask; }, 1000, "Input mask must be cleared when hidden");
            compare(popup.hasInputMask, false);
        }

        function test_urgency_border_and_styling() {
            // Urgency 1 (Normal) -> border.color == Style.colors.surface
            mockModel.addNotification(1, "NormalApp", "Normal", "Normal Body", 1);
            tryVerify(function() { return popup.popupRepeater.count === 1; });
            let card1 = popup.popupRepeater.itemAt(0);
            verify(card1 !== null);
            compare(card1.border.color, Style.colors.surface, "Normal urgency must have surface border");

            // Urgency 2 (Critical) -> border.color == Style.colors.danger
            mockModel.clear();
            mockModel.addNotification(2, "CriticalApp", "Critical", "Critical Body", 2);
            tryVerify(function() { return popup.popupRepeater.count === 1; });
            let card2 = popup.popupRepeater.itemAt(0);
            verify(card2 !== null);
            compare(card2.border.color, Style.colors.danger, "Critical urgency must have danger border");
        }

        function test_accessibility_and_layout_mirroring() {
            mockModel.addNotification(10, "TestApp", "Test Summary", "Test Body", 1);
            tryVerify(function() { return popup.popupRepeater.count === 1; });
            let card = popup.popupRepeater.itemAt(0);
            verify(card !== null);

            compare(card.Accessible.role, Accessible.AlertMessage, "Card Accessible role must be AlertMessage");
            compare(card.Accessible.name, "TestApp: Test Summary", "Card Accessible name must match appName: summary");
            compare(card.Accessible.description, "Test Body", "Card Accessible description must match body");
            compare(card.dismissBtn.Accessible.name, "Dismiss notification from TestApp");
            compare(card.LayoutMirroring.childrenInherit, true);

            // RTL mirroring mutant killer: instantiate in RTL locale
            LayoutHelper.setRtl(true);
            let rtlComp = Qt.createComponent("../../styles/default/NotificationPopup.qml");
            let rtlPopup = rtlComp.createObject(null, { "notificationModel": mockModel });
            tryVerify(function() { return rtlPopup.popupRepeater.count === 1; });
            let rtlCard = rtlPopup.popupRepeater.itemAt(0);
            verify(rtlCard !== null);
            compare(rtlCard.LayoutMirroring.enabled, true, "Card must enable layout mirroring in RTL");
            rtlPopup.destroy();
            LayoutHelper.setRtl(false);
        }

        function test_icon_and_letter_badge() {
            // Without icon -> badge visible, icon hidden
            mockModel.addNotificationFull(20, "KynexMail", true, "", "Subject", "Content", 1, [], [], false);
            tryVerify(function() { return popup.popupRepeater.count === 1; });
            let card = popup.popupRepeater.itemAt(0);
            verify(card !== null);
            compare(card.iconImg.visible, false);
            compare(card.badgeFallback.visible, true);

            // With icon -> icon source set, iconImg visible, badge hidden
            mockModel.clear();
            mockModel.addNotificationFull(21, "KynexMail", true, "mail-client", "Subject", "Content", 1, [], [], false);
            tryVerify(function() { return popup.popupRepeater.count === 1; });
            let cardWithIcon = popup.popupRepeater.itemAt(0);
            verify(cardWithIcon !== null);
            compare(cardWithIcon.iconImg.source.toString(), "image://kynex-icon/mail-client?size=24&scale=1");
            tryVerify(function() {
                return cardWithIcon.iconImg.visible && cardWithIcon.iconImg.status === Image.Ready;
            }, 1000, "Icon image must be visible when status is Image.Ready");
            compare(cardWithIcon.iconImg.visible, true);
            compare(cardWithIcon.badgeFallback.visible, false);
        }

        function test_body_visibility_and_elision() {
            // Non-empty body
            mockModel.addNotification(30, "App", "Summary", "Some body text", 1);
            tryVerify(function() { return popup.popupRepeater.count === 1; });
            let card = popup.popupRepeater.itemAt(0);
            verify(card !== null);
            compare(card.bodyText.visible, true);

            // Empty body
            mockModel.clear();
            mockModel.addNotification(31, "App", "Summary", "", 1);
            tryVerify(function() { return popup.popupRepeater.count === 1; });
            let emptyBodyCard = popup.popupRepeater.itemAt(0);
            verify(emptyBodyCard !== null);
            compare(emptyBodyCard.bodyText.visible, false);
        }

        function test_action_buttons_and_interaction() {
            // With actions
            mockModel.addNotificationFull(40, "App", true, "", "Title", "Msg", 1, ["reply", "archive"], ["Reply", "Archive"], false);
            tryVerify(function() { return popup.popupRepeater.count === 1; });
            let card = popup.popupRepeater.itemAt(0);
            verify(card !== null);
            compare(card.actionsRow.visible, true);
            compare(card.actionRepeater.count, 2);

            let btn0 = card.actionRepeater.itemAt(0);
            verify(btn0 !== null);
            compare(btn0.text, "Reply");
            compare(btn0.Accessible.name, "Reply");

            btn0.clicked();
            compare(mockModel.lastInvokedId, 40);
            compare(mockModel.lastInvokedKey, "reply");

            // Without actions
            mockModel.clear();
            mockModel.addNotification(41, "App", "Title", "Msg", 1);
            tryVerify(function() { return popup.popupRepeater.count === 1; });
            let noActionCard = popup.popupRepeater.itemAt(0);
            verify(noActionCard !== null);
            compare(noActionCard.actionsRow.visible, false);
        }

        function test_dismiss_and_hover_tracking() {
            mockModel.addNotification(50, "TrackApp", "Summary", "Body", 1);
            tryVerify(function() { return popup.popupRepeater.count === 1; });
            let card = popup.popupRepeater.itemAt(0);
            verify(card !== null);

            card.cardHoverArea.entered();
            compare(mockModel.lastPausedId, 50);

            card.cardHoverArea.exited();
            compare(mockModel.lastResumedId, 50);

            card.dismissBtn.clicked();
            compare(mockModel.lastDismissedId, 50);
            compare(mockModel.count, 0);
        }
    }
}
