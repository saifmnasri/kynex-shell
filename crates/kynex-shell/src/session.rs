//! Session and power management client model exposing session actions and overlay state to QML.

use core::pin::Pin;
use cxx_qt::{CxxQtType, Threading};
use cxx_qt_lib::QString;
use futures_util::StreamExt;
use kynex_ipc::session::{Session1Proxy, SESSION_PATH};
use kynex_ipc::DAEMON_DESTINATION;
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender};
use tracing::{info, warn};
use zbus::fdo::DBusProxy;

use crate::debug::get_debug_hub;
use kynex_shell_core::debug::SessionDebugState;

/// Commands dispatched from UI to background session listener.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionCommand {
    /// Order session logout.
    Logout,
    /// Order system reboot.
    Restart,
    /// Order system poweroff.
    PowerOff,
    /// Order system suspend.
    Suspend,
}

#[cxx_qt::bridge]
/// CXX-Qt bridge for the `SessionModel` `QObject`.
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        /// QString type binding for CXX-Qt.
        type QString = cxx_qt_lib::QString;
    }

    extern "RustQt" {
        /// Session lifecycle and power management overlay model.
        #[qobject]
        #[qml_element]
        #[qproperty(bool, can_logout)]
        #[qproperty(bool, can_restart)]
        #[qproperty(bool, can_power_off)]
        #[qproperty(bool, can_suspend)]
        #[qproperty(bool, action_in_progress)]
        #[qproperty(QString, error_message)]
        #[qproperty(bool, visible)]
        type SessionModel = super::SessionModelRust;

        /// Starts the background D-Bus listener connecting to `org.kynexos.Daemon`.
        #[qinvokable]
        fn init(self: Pin<&mut Self>);

        /// Opens the session and power overlay.
        #[qinvokable]
        fn open(self: Pin<&mut Self>);

        /// Dismisses the session and power overlay and resets confirmation.
        #[qinvokable]
        fn dismiss(self: Pin<&mut Self>);

        /// Requests session logout (terminates user session).
        #[qinvokable]
        #[cxx_name = "requestLogout"]
        fn request_logout(self: Pin<&mut Self>);

        /// Requests system reboot.
        #[qinvokable]
        #[cxx_name = "requestRestart"]
        fn request_restart(self: Pin<&mut Self>);

        /// Requests system poweroff.
        #[qinvokable]
        #[cxx_name = "requestPowerOff"]
        fn request_power_off(self: Pin<&mut Self>);

        /// Requests system suspend.
        #[qinvokable]
        #[cxx_name = "requestSuspend"]
        fn request_suspend(self: Pin<&mut Self>);
    }

    impl cxx_qt::Threading for SessionModel {}
}

/// Backing data for `SessionModel` `QObject`.
#[allow(clippy::struct_excessive_bools)]
pub struct SessionModelRust {
    can_logout: bool,
    can_restart: bool,
    can_power_off: bool,
    can_suspend: bool,
    action_in_progress: bool,
    error_message: QString,
    visible: bool,
    cmd_tx: Option<UnboundedSender<SessionCommand>>,
}

impl Default for SessionModelRust {
    fn default() -> Self {
        Self {
            can_logout: true,
            can_restart: false,
            can_power_off: false,
            can_suspend: false,
            action_in_progress: false,
            error_message: QString::from(""),
            visible: false,
            cmd_tx: None,
        }
    }
}

impl qobject::SessionModel {
    /// Initializes background listener and connects to `kynex-daemon`.
    pub fn init(mut self: Pin<&mut Self>) {
        let qt_thread = self.as_ref().qt_thread();
        let (cmd_tx, cmd_rx) = unbounded_channel();
        self.as_mut().rust_mut().cmd_tx = Some(cmd_tx);

        tokio::spawn(async move {
            run_session_listener(qt_thread, cmd_rx).await;
        });
    }

    /// Opens the session overlay.
    pub fn open(mut self: Pin<&mut Self>) {
        self.as_mut().set_visible(true);
        self.as_mut().set_error_message(QString::from(""));
        self.as_mut().set_action_in_progress(false);
    }

    /// Dismisses the session overlay and resets all confirmation states.
    pub fn dismiss(mut self: Pin<&mut Self>) {
        self.as_mut().set_visible(false);
        self.as_mut().set_action_in_progress(false);
        self.as_mut().set_error_message(QString::from(""));
    }

    /// Requests session logout.
    pub fn request_logout(mut self: Pin<&mut Self>) {
        if !self.rust().can_logout {
            return;
        }
        self.as_mut().set_error_message(QString::from(""));
        self.as_mut().set_action_in_progress(true);
        if let Some(ref tx) = self.rust().cmd_tx {
            let _ = tx.send(SessionCommand::Logout);
        }
    }

    /// Requests machine restart.
    pub fn request_restart(mut self: Pin<&mut Self>) {
        if !self.rust().can_restart {
            return;
        }
        self.as_mut().set_error_message(QString::from(""));
        self.as_mut().set_action_in_progress(true);
        if let Some(ref tx) = self.rust().cmd_tx {
            let _ = tx.send(SessionCommand::Restart);
        }
    }

    /// Requests machine poweroff.
    pub fn request_power_off(mut self: Pin<&mut Self>) {
        if !self.rust().can_power_off {
            return;
        }
        self.as_mut().set_error_message(QString::from(""));
        self.as_mut().set_action_in_progress(true);
        if let Some(ref tx) = self.rust().cmd_tx {
            let _ = tx.send(SessionCommand::PowerOff);
        }
    }

    /// Requests system suspend.
    pub fn request_suspend(mut self: Pin<&mut Self>) {
        if !self.rust().can_suspend {
            return;
        }
        self.as_mut().set_error_message(QString::from(""));
        self.as_mut().set_action_in_progress(true);
        if let Some(ref tx) = self.rust().cmd_tx {
            let _ = tx.send(SessionCommand::Suspend);
        }
    }
}

/// Background asynchronous listener connecting to `org.kynexos.Daemon.Session1`.
#[allow(clippy::too_many_lines)]
async fn run_session_listener(
    qt_thread: cxx_qt::CxxQtThread<qobject::SessionModel>,
    mut cmd_rx: UnboundedReceiver<SessionCommand>,
) {
    let session_conn = match zbus::connection::Builder::session().build().await {
        Ok(c) => c,
        Err(err) => {
            warn!("SessionModel failed to connect to session bus: {err}");
            return;
        }
    };

    let dbus_proxy = match DBusProxy::new(&session_conn).await {
        Ok(p) => p,
        Err(err) => {
            warn!("SessionModel failed to create DBusProxy: {err}");
            return;
        }
    };

    let mut name_owner_stream = dbus_proxy
        .receive_name_owner_changed_with_args(&[(0, DAEMON_DESTINATION)])
        .await
        .ok();

    let mut proxy = Session1Proxy::builder(&session_conn)
        .destination(DAEMON_DESTINATION)
        .path(SESSION_PATH)
        .cache_properties(zbus::proxy::CacheProperties::No)
        .build()
        .await
        .ok();

    if let Some(ref p) = proxy {
        let can_l = p.can_logout().await.unwrap_or(true);
        let can_r = p.can_restart().await.unwrap_or(false);
        let can_p = p.can_power_off().await.unwrap_or(false);
        let can_s = p.can_suspend().await.unwrap_or(false);

        let _ = qt_thread.queue(move |mut qobj| {
            qobj.as_mut().set_can_logout(can_l);
            qobj.as_mut().set_can_restart(can_r);
            qobj.as_mut().set_can_power_off(can_p);
            qobj.as_mut().set_can_suspend(can_s);
        });

        get_debug_hub()
            .update_model_readiness(|m| m.session = true)
            .await;
        get_debug_hub()
            .update_session(SessionDebugState {
                can_logout: can_l,
                can_restart: can_r,
                can_power_off: can_p,
                can_suspend: can_s,
                overlay_open: false,
                confirmation: "none".to_string(),
            })
            .await;
        info!("SessionModel connected to daemon: logout={can_l}, restart={can_r}, poweroff={can_p}, suspend={can_s}");
    }

    loop {
        tokio::select! {
            // UI command received
            Some(cmd) = cmd_rx.recv() => {
                if let Some(ref p) = proxy {
                    let res = match cmd {
                        SessionCommand::Logout => p.logout().await,
                        SessionCommand::Restart => p.restart().await,
                        SessionCommand::PowerOff => p.power_off().await,
                        SessionCommand::Suspend => p.suspend().await,
                    };

                    if let Err(e) = res {
                        warn!("Session action {cmd:?} failed: {e}");
                        let clean_err = e.to_string();
                        let q_err = QString::from(&clean_err);
                        let _ = qt_thread.queue(move |mut qobj| {
                            qobj.as_mut().set_action_in_progress(false);
                            qobj.as_mut().set_error_message(q_err);
                        });
                    } else {
                        let _ = qt_thread.queue(move |mut qobj| {
                            qobj.as_mut().set_action_in_progress(false);
                        });
                    }
                } else {
                    let _ = qt_thread.queue(|mut qobj| {
                        qobj.as_mut().set_action_in_progress(false);
                        qobj.as_mut()
                            .set_error_message(QString::from("Daemon unavailable"));
                    });
                }
            }

            // Daemon appeared or disappeared
            Some(name_event) = async {
                if let Some(ref mut s) = name_owner_stream {
                    s.next().await
                } else {
                    std::future::pending().await
                }
            } => {
                if let Ok(args) = name_event.args() {
                    let new_owner = args.new_owner();
                    if new_owner.as_ref().is_none_or(|o| o.as_str().is_empty()) {
                        info!("kynex-daemon disconnected; resetting session capabilities");
                        proxy = None;
                        let _ = qt_thread.queue(|mut qobj| {
                            qobj.as_mut().set_can_logout(true);
                            qobj.as_mut().set_can_restart(false);
                            qobj.as_mut().set_can_power_off(false);
                            qobj.as_mut().set_can_suspend(false);
                        });
                    } else {
                        info!("kynex-daemon connected; querying Session1 capabilities");
                        if let Ok(new_proxy) = Session1Proxy::builder(&session_conn)
                            .destination(DAEMON_DESTINATION)
                            .path(SESSION_PATH)
                            .cache_properties(zbus::proxy::CacheProperties::No)
                            .build()
                            .await
                        {
                            let can_l = new_proxy.can_logout().await.unwrap_or(true);
                            let can_r = new_proxy.can_restart().await.unwrap_or(false);
                            let can_p = new_proxy.can_power_off().await.unwrap_or(false);
                            let can_s = new_proxy.can_suspend().await.unwrap_or(false);

                            proxy = Some(new_proxy);

                            let _ = qt_thread.queue(move |mut qobj| {
                                qobj.as_mut().set_can_logout(can_l);
                                qobj.as_mut().set_can_restart(can_r);
                                qobj.as_mut().set_can_power_off(can_p);
                                qobj.as_mut().set_can_suspend(can_s);
                            });

                            get_debug_hub()
                                .update_model_readiness(|m| m.session = true)
                                .await;
                            get_debug_hub()
                                .update_session(SessionDebugState {
                                    can_logout: can_l,
                                    can_restart: can_r,
                                    can_power_off: can_p,
                                    can_suspend: can_s,
                                    overlay_open: false,
                                    confirmation: "none".to_string(),
                                })
                                .await;
                        }
                    }
                }
            }
        }
    }
}
