#pragma once

#include "rust/cxx.h"
#include <cstdint>
#include <memory>
#include <string>

class QQuickWindow;
class QQuickItem;
class QQmlApplicationEngine;

/// Registers the root panel window from QQmlApplicationEngine.
void kynex_register_engine_panel(QQmlApplicationEngine &engine);

/// Traverses the panel window scene tree and returns sanitized JSON (strictly NO titles).
rust::String kynex_dump_scene_json(const QQuickWindow *window);

/// Grabs panel window to PNG in $XDG_RUNTIME_DIR/kynex/panel-<timestamp>.png (mode 0600).
rust::String kynex_grab_panel_image(QQuickWindow *window);

/// Registers the global panel window pointer for D-Bus introspection.
void kynex_set_panel_window(QQuickWindow *window);

/// Returns the registered global panel window pointer (or nullptr).
QQuickWindow *kynex_get_panel_window();

/// Dumps the current registered panel window scene as sanitized JSON.
rust::String kynex_dump_active_scene();

/// Grabs the current registered panel window to PNG.
rust::String kynex_grab_active_panel();

/// Runs the taskbar QML test harness for a given geometry and DESTDIR.
/// Returns 0 if all assertions pass, non-zero if assertions fail.
int kynex_run_taskbar_qml_harness(rust::Str destdir, int32_t width, int32_t height, double dpr);
