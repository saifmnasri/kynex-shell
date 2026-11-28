//! Session lifecycle and power management state machine.
//!
//! Pure business logic governing power and session confirmation, capabilities,
//! and error states for the Kynex Shell UI overlay.

/// Power and session actions that can be requested by the user.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionAction {
    /// End the current graphical session.
    Logout,
    /// Reboot the host system.
    Restart,
    /// Power down the host system.
    PowerOff,
    /// Suspend the host system to RAM.
    Suspend,
}

/// Confirmation state for destructive actions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ConfirmationState {
    /// No confirmation pending.
    #[default]
    None,
    /// User requested restart, awaiting second confirmation.
    ConfirmRestart,
    /// User requested power off, awaiting second confirmation.
    ConfirmPowerOff,
}

/// Result of requesting a session action.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ActionRequestResult {
    /// Action requires second confirmation before execution.
    NeedsConfirmation,
    /// Action is immediately executable (e.g. Logout or Suspend).
    Execute(SessionAction),
    /// Action is disabled by system policy or lack of support.
    Disabled,
}

/// Core state machine for the session and power overlay.
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionCore {
    /// Whether user can log out.
    pub can_logout: bool,
    /// Whether user can reboot the system.
    pub can_restart: bool,
    /// Whether user can power off the system.
    pub can_power_off: bool,
    /// Whether user can suspend the system.
    pub can_suspend: bool,
    /// Current confirmation status.
    pub confirmation: ConfirmationState,
    /// Whether an action is currently being executed.
    pub action_in_progress: bool,
    /// Error message to display if the last action failed.
    pub error_message: Option<String>,
}

impl Default for SessionCore {
    fn default() -> Self {
        Self {
            can_logout: true,
            can_restart: false,
            can_power_off: false,
            can_suspend: false,
            confirmation: ConfirmationState::None,
            action_in_progress: false,
            error_message: None,
        }
    }
}

impl SessionCore {
    /// Creates a new `SessionCore` with default capabilities.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Updates capability flags queried from logind/systemd.
    #[allow(clippy::fn_params_excessive_bools)]
    pub fn update_capabilities(
        &mut self,
        can_logout: bool,
        can_restart: bool,
        can_power_off: bool,
        can_suspend: bool,
    ) {
        self.can_logout = can_logout;
        self.can_restart = can_restart;
        self.can_power_off = can_power_off;
        self.can_suspend = can_suspend;

        // If an action currently in confirmation was disabled, clear confirmation
        match self.confirmation {
            ConfirmationState::ConfirmRestart if !self.can_restart => {
                self.confirmation = ConfirmationState::None;
            }
            ConfirmationState::ConfirmPowerOff if !self.can_power_off => {
                self.confirmation = ConfirmationState::None;
            }
            _ => {}
        }
    }

    /// Initiates an action request.
    ///
    /// Destructive actions (Restart, `PowerOff`) transition to a confirmation state.
    /// Non-destructive / single-click actions (Logout, Suspend) return `Execute`.
    /// Disabled actions return `Disabled`.
    pub fn request_action(&mut self, action: SessionAction) -> ActionRequestResult {
        self.error_message = None;

        match action {
            SessionAction::Logout => {
                if self.can_logout {
                    self.action_in_progress = true;
                    ActionRequestResult::Execute(SessionAction::Logout)
                } else {
                    ActionRequestResult::Disabled
                }
            }
            SessionAction::Suspend => {
                if self.can_suspend {
                    self.action_in_progress = true;
                    ActionRequestResult::Execute(SessionAction::Suspend)
                } else {
                    ActionRequestResult::Disabled
                }
            }
            SessionAction::Restart => {
                if self.can_restart {
                    self.confirmation = ConfirmationState::ConfirmRestart;
                    ActionRequestResult::NeedsConfirmation
                } else {
                    ActionRequestResult::Disabled
                }
            }
            SessionAction::PowerOff => {
                if self.can_power_off {
                    self.confirmation = ConfirmationState::ConfirmPowerOff;
                    ActionRequestResult::NeedsConfirmation
                } else {
                    ActionRequestResult::Disabled
                }
            }
        }
    }

    /// Confirms a pending destructive action.
    ///
    /// Returns the action to execute, or `None` if no confirmation was active.
    pub fn confirm(&mut self) -> Option<SessionAction> {
        let action = match self.confirmation {
            ConfirmationState::ConfirmRestart => Some(SessionAction::Restart),
            ConfirmationState::ConfirmPowerOff => Some(SessionAction::PowerOff),
            ConfirmationState::None => None,
        };

        if action.is_some() {
            self.confirmation = ConfirmationState::None;
            self.action_in_progress = true;
        }

        action
    }

    /// Cancels any active confirmation prompt.
    pub fn cancel_confirmation(&mut self) {
        self.confirmation = ConfirmationState::None;
    }

    /// Sets an error message from a failed action and resets execution state.
    pub fn set_error(&mut self, error: impl Into<String>) {
        self.action_in_progress = false;
        self.confirmation = ConfirmationState::None;
        self.error_message = Some(error.into());
    }

    /// Clears any error message.
    pub fn clear_error(&mut self) {
        self.error_message = None;
    }

    /// Resets overlay state when dismissed.
    pub fn dismiss(&mut self) {
        self.confirmation = ConfirmationState::None;
        self.action_in_progress = false;
        self.error_message = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_initial_state() {
        let core = SessionCore::new();
        assert!(core.can_logout);
        assert!(!core.can_restart);
        assert!(!core.can_power_off);
        assert!(!core.can_suspend);
        assert_eq!(core.confirmation, ConfirmationState::None);
        assert!(!core.action_in_progress);
        assert_eq!(core.error_message, None);
    }

    #[test]
    fn test_capabilities_update() {
        let mut core = SessionCore::new();
        core.update_capabilities(true, true, true, true);
        assert!(core.can_logout);
        assert!(core.can_restart);
        assert!(core.can_power_off);
        assert!(core.can_suspend);
    }

    #[test]
    fn test_single_click_actions() {
        let mut core = SessionCore::new();
        core.update_capabilities(true, true, true, true);

        // Logout
        let res = core.request_action(SessionAction::Logout);
        assert_eq!(res, ActionRequestResult::Execute(SessionAction::Logout));
        assert!(core.action_in_progress);
        assert_eq!(core.confirmation, ConfirmationState::None);

        core.dismiss();

        // Suspend
        let res = core.request_action(SessionAction::Suspend);
        assert_eq!(res, ActionRequestResult::Execute(SessionAction::Suspend));
        assert!(core.action_in_progress);
        assert_eq!(core.confirmation, ConfirmationState::None);
    }

    #[test]
    fn test_destructive_actions_require_confirmation() {
        let mut core = SessionCore::new();
        core.update_capabilities(true, true, true, true);

        // Restart
        let res = core.request_action(SessionAction::Restart);
        assert_eq!(res, ActionRequestResult::NeedsConfirmation);
        assert_eq!(core.confirmation, ConfirmationState::ConfirmRestart);
        assert!(!core.action_in_progress);

        // Confirm
        let action = core.confirm();
        assert_eq!(action, Some(SessionAction::Restart));
        assert_eq!(core.confirmation, ConfirmationState::None);
        assert!(core.action_in_progress);

        core.dismiss();

        // PowerOff
        let res = core.request_action(SessionAction::PowerOff);
        assert_eq!(res, ActionRequestResult::NeedsConfirmation);
        assert_eq!(core.confirmation, ConfirmationState::ConfirmPowerOff);

        // Cancel
        core.cancel_confirmation();
        assert_eq!(core.confirmation, ConfirmationState::None);
        assert!(!core.action_in_progress);
        assert_eq!(core.confirm(), None);
    }

    #[test]
    fn test_disabled_actions_rejected() {
        let mut core = SessionCore::new();
        core.update_capabilities(false, false, false, false);

        assert_eq!(
            core.request_action(SessionAction::Logout),
            ActionRequestResult::Disabled
        );
        assert_eq!(
            core.request_action(SessionAction::Restart),
            ActionRequestResult::Disabled
        );
        assert_eq!(
            core.request_action(SessionAction::PowerOff),
            ActionRequestResult::Disabled
        );
        assert_eq!(
            core.request_action(SessionAction::Suspend),
            ActionRequestResult::Disabled
        );
        assert!(!core.action_in_progress);
        assert_eq!(core.confirmation, ConfirmationState::None);
    }

    #[test]
    fn test_error_handling() {
        let mut core = SessionCore::new();
        core.update_capabilities(true, true, true, true);
        core.request_action(SessionAction::Restart);
        core.confirm();
        assert!(core.action_in_progress);

        core.set_error("Action failed due to inhibitor");
        assert!(!core.action_in_progress);
        assert_eq!(
            core.error_message,
            Some("Action failed due to inhibitor".to_string())
        );

        core.clear_error();
        assert_eq!(core.error_message, None);
    }
}
