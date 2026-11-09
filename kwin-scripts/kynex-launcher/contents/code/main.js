/*
 * Kynex Shell - KWin 6 Launcher Shortcut Feeder Script
 *
 * Registers a global shortcut using KWin's registerShortcut API.
 * When triggered, notifies org.kynexos.Daemon.ShellFeed1.LauncherShortcut via callDBus.
 */

var SERVICE = "org.kynexos.Daemon";
var PATH = "/org/kynexos/Daemon/Shell";
var IFACE = "org.kynexos.Daemon.ShellFeed1";

registerShortcut(
    "kynex-launcher-shortcut",
    "Toggle Kynex Launcher",
    "Meta+Space",
    function() {
        callDBus(SERVICE, PATH, IFACE, "LauncherShortcut");
    }
);

