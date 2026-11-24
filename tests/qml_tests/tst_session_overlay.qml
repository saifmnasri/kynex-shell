import QtQuick
import QtTest
import Kynex.Shell
import Kynex.Style
import Kynex.Controls

Item {
    id: root
    width: 1920
    height: 1080

    SessionModel {
        id: mockSession
    }

    TestCase {
        name: "SessionOverlayTests"
        when: windowShown

        property var overlay: null

        function init() {
            mockSession.canLogout = true;
            mockSession.canRestart = true;
            mockSession.canPowerOff = true;
            mockSession.canSuspend = true;
            mockSession.confirmingRestart = false;
            mockSession.confirmingPowerOff = false;
            mockSession.actionInProgress = false;
            mockSession.errorMessage = "";
            mockSession.visible = false;

            if (overlay) {
                overlay.destroy();
                overlay = null;
            }

            let comp = Qt.createComponent("../../styles/default/SessionOverlay.qml");
            compare(comp.status, Component.Ready, "SessionOverlay.qml must load cleanly without errors");
            overlay = comp.createObject(null, {
                "sessionModel": mockSession,
                "width": 1920,
                "height": 1080
            });
            verify(overlay !== null, "SessionOverlay should be instantiated");
        }

        function cleanup() {
            if (overlay) {
                overlay.destroy();
                overlay = null;
            }
        }

        function test_initial_state() {
            compare(overlay.visible, false);
            compare(overlay.actionsSectionItem.visible, true);
            compare(overlay.confirmationSectionItem.visible, false);
            compare(overlay.progressSectionItem.visible, false);
            compare(overlay.errorBannerItem.visible, false);
            compare(overlay.scope, "session-overlay");
            compare(overlay.layer, LayerSurface.LayerOverlay);
        }

        function test_open_and_focus() {
            mockSession.open();
            tryVerify(function() { return overlay.visible; });
            tryVerify(function() { return overlay.logoutButtonItem.activeFocus; });

            // Accessible names check
            compare(overlay.titleTextItem.Accessible.role, Accessible.Heading);
            compare(overlay.titleTextItem.Accessible.name, "Session & Power");
            compare(overlay.logoutButtonItem.Accessible.name, "Log out Button");
            compare(overlay.restartButtonItem.Accessible.name, "Restart Button");
            compare(overlay.powerOffButtonItem.Accessible.name, "Shut down Button");
            compare(overlay.suspendButtonItem.Accessible.name, "Suspend Button");
            compare(overlay.lockButtonItem.Accessible.name, "Lock Screen Unavailable Button");
            compare(overlay.dismissButtonItem.Accessible.name, "Dismiss Session Overlay Button");
        }

        function test_esc_closes_overlay() {
            mockSession.open();
            tryVerify(function() { return overlay.visible; });
            overlay.escShortcutItem.activated();
            tryVerify(function() { return !overlay.visible; });
            compare(mockSession.visible, false);
        }

        function test_backdrop_click_closes_overlay() {
            mockSession.open();
            tryVerify(function() { return overlay.visible; });
            overlay.backdropMouseItem.clicked(null);
            tryVerify(function() { return !overlay.visible; });
            compare(mockSession.visible, false);
        }

        function test_destructive_restart_flow() {
            mockSession.open();
            tryVerify(function() { return overlay.visible; });

            // Click Restart
            overlay.restartButtonItem.click();
            tryVerify(function() { return mockSession.confirmingRestart; });
            tryVerify(function() { return overlay.confirmationSectionItem.visible; });
            tryVerify(function() { return !overlay.actionsSectionItem.visible; });
            compare(overlay.titleTextItem.text, "Restart Computer");
            tryVerify(function() { return overlay.confirmButtonItem.activeFocus; });
            compare(overlay.confirmButtonItem.Accessible.name, "Restart Confirm Button");
            compare(overlay.cancelConfirmButtonItem.Accessible.name, "Cancel Confirmation Button");

            // Test Cancellation
            overlay.cancelConfirmButtonItem.click();
            tryVerify(function() { return !mockSession.confirmingRestart; });
            tryVerify(function() { return overlay.actionsSectionItem.visible; });
            compare(mockSession.visible, true);

            // Click Restart again and confirm
            overlay.restartButtonItem.click();
            tryVerify(function() { return mockSession.confirmingRestart; });
            overlay.confirmButtonItem.click();
            tryVerify(function() { return !overlay.visible; });
            compare(mockSession.visible, false);
        }

        function test_destructive_power_off_flow_and_enter_key() {
            mockSession.open();
            tryVerify(function() { return overlay.visible; });

            // Click Shut Down
            overlay.powerOffButtonItem.click();
            tryVerify(function() { return mockSession.confirmingPowerOff; });
            compare(overlay.titleTextItem.text, "Shut Down Computer");
            tryVerify(function() { return overlay.confirmButtonItem.activeFocus; });
            compare(overlay.confirmButtonItem.Accessible.name, "Shut Down Confirm Button");

            // Test Esc cancels confirmation back to menu
            overlay.escShortcutItem.activated();
            tryVerify(function() { return !mockSession.confirmingPowerOff; });
            compare(mockSession.visible, true);

            // Click Shut Down again and use Enter shortcut
            overlay.powerOffButtonItem.click();
            tryVerify(function() { return mockSession.confirmingPowerOff; });
            overlay.enterShortcutItem.activated();
            tryVerify(function() { return !overlay.visible; });
            compare(mockSession.visible, false);
        }

        function test_non_destructive_logout_and_suspend() {
            // Logout executes in 1 click
            mockSession.open();
            tryVerify(function() { return overlay.visible; });
            overlay.logoutButtonItem.click();
            tryVerify(function() { return !overlay.visible; });
            compare(mockSession.visible, false);

            // Suspend executes in 1 click
            mockSession.open();
            tryVerify(function() { return overlay.visible; });
            overlay.suspendButtonItem.click();
            tryVerify(function() { return !overlay.visible; });
            compare(mockSession.visible, false);
        }

        function test_disabled_items_and_tooltips() {
            mockSession.canLogout = false;
            mockSession.canRestart = false;
            mockSession.canPowerOff = false;
            mockSession.canSuspend = false;
            mockSession.open();
            tryVerify(function() { return overlay.visible; });

            compare(overlay.logoutButtonItem.enabled, false);
            compare(overlay.restartButtonItem.enabled, false);
            compare(overlay.powerOffButtonItem.enabled, false);
            compare(overlay.suspendButtonItem.enabled, false);
            compare(overlay.lockButtonItem.enabled, false);

            // Clicking disabled items has no effect
            overlay.logoutButtonItem.click();
            compare(mockSession.visible, true);
            overlay.restartButtonItem.click();
            compare(mockSession.confirmingRestart, false);
            overlay.powerOffButtonItem.click();
            compare(mockSession.confirmingPowerOff, false);
            overlay.suspendButtonItem.click();
            compare(mockSession.visible, true);

            // Tooltips
            compare(overlay.lockTooltipItem.text, "Screen locking is not implemented yet");
            compare(overlay.restartTooltipItem.text, "Restart is not authorized by system policy");
            compare(overlay.powerOffTooltipItem.text, "Shut down is not authorized by system policy");
            compare(overlay.suspendTooltipItem.text, "Suspend is not authorized by system policy");
            compare(overlay.logoutTooltipItem.text, "Logout is unavailable");

            // Tooltips hidden when unhovered
            compare(overlay.logoutTooltipItem.visible, false);
            compare(overlay.restartTooltipItem.visible, false);
            compare(overlay.powerOffTooltipItem.visible, false);
            compare(overlay.suspendTooltipItem.visible, false);

            // Hover state activates tooltips for disabled items
            LayoutHelper.sendMouseMove(overlay.logoutHoverAreaItem, 10, 10);
            tryVerify(function() { return overlay.logoutTooltipItem.visible; });
            LayoutHelper.sendMouseLeave(overlay.logoutHoverAreaItem);
            tryVerify(function() { return !overlay.logoutTooltipItem.visible; });

            LayoutHelper.sendMouseMove(overlay.restartHoverAreaItem, 10, 10);
            tryVerify(function() { return overlay.restartTooltipItem.visible; });
            LayoutHelper.sendMouseLeave(overlay.restartHoverAreaItem);
            tryVerify(function() { return !overlay.restartTooltipItem.visible; });

            LayoutHelper.sendMouseMove(overlay.powerOffHoverAreaItem, 10, 10);
            tryVerify(function() { return overlay.powerOffTooltipItem.visible; });
            LayoutHelper.sendMouseLeave(overlay.powerOffHoverAreaItem);
            tryVerify(function() { return !overlay.powerOffTooltipItem.visible; });

            LayoutHelper.sendMouseMove(overlay.suspendHoverAreaItem, 10, 10);
            tryVerify(function() { return overlay.suspendTooltipItem.visible; });
            LayoutHelper.sendMouseLeave(overlay.suspendHoverAreaItem);
            tryVerify(function() { return !overlay.suspendTooltipItem.visible; });

            LayoutHelper.sendMouseMove(overlay.lockHoverAreaItem, 10, 10);
            tryVerify(function() { return overlay.lockTooltipItem.visible; });
            LayoutHelper.sendMouseLeave(overlay.lockHoverAreaItem);
            tryVerify(function() { return !overlay.lockTooltipItem.visible; });
        }

        function test_rtl_layout_mirroring() {
            LayoutHelper.setRtl(true);
            let comp = Qt.createComponent("../../styles/default/SessionOverlay.qml");
            compare(comp.status, Component.Ready, "SessionOverlay.qml should load in RTL");
            let rtlOverlay = comp.createObject(null, {
                "sessionModel": mockSession,
                "width": 1920,
                "height": 1080
            });
            verify(rtlOverlay !== null, "RTL SessionOverlay should be instantiated");
            compare(rtlOverlay.LayoutMirroring.enabled, true, "In RTL LayoutMirroring must be true");
            compare(rtlOverlay.LayoutMirroring.childrenInherit, true, "LayoutMirroring childrenInherit must be true in RTL");
            rtlOverlay.destroy();
            LayoutHelper.setRtl(false);
        }

        function test_error_banner() {
            mockSession.open();
            mockSession.errorMessage = "Failed to suspend system: permission denied";
            tryVerify(function() { return overlay.errorBannerItem.visible; });
        }
    }
}
