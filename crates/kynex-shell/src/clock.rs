//! The live clock `QObject` exposed to QML.

use core::pin::Pin;
use cxx_qt_lib::QString;

/// CXX-Qt bridge module for the Clock `QObject`.
#[cxx_qt::bridge]
pub mod qobject {
    // SAFETY: FFI bridge declarations to Qt's QString provided by cxx-qt-lib.
    // QString is a standard Qt value type whose C++ layout and invariants are managed
    // by cxx-qt-lib and guaranteed memory-safe across the FFI boundary.
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        /// Alias to Qt QString
        type QString = cxx_qt_lib::QString;
    }

    extern "RustQt" {
        /// The Clock `QObject` exposed to QML.
        #[qobject]
        #[qml_element]
        #[qproperty(QString, time)]
        #[qproperty(QString, date)]
        type Clock = super::ClockRust;

        /// Updates the clock time and date to current system time.
        #[qinvokable]
        fn update(self: Pin<&mut Self>);
    }
}

/// The Rust backing data for the Clock `QObject`.
pub struct ClockRust {
    time: QString,
    date: QString,
}

impl Default for ClockRust {
    fn default() -> Self {
        let (time, date) = current_time_and_date();
        Self {
            time: QString::from(&time),
            date: QString::from(&date),
        }
    }
}

impl qobject::Clock {
    /// Update the time and date properties with current values.
    pub fn update(mut self: Pin<&mut Self>) {
        let (time, date) = current_time_and_date();
        let time_qstring = QString::from(&time);
        let date_qstring = QString::from(&date);
        self.as_mut().set_time(time_qstring);
        self.as_mut().set_date(date_qstring);
    }
}

fn current_time_and_date() -> (String, String) {
    let now = chrono::Local::now();
    (
        now.format("%H:%M:%S").to_string(),
        now.format("%a %b %e").to_string(),
    )
}
