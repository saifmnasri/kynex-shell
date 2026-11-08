/*
 * Kynex Shell - KWin 6 Window Tracking Feeder Script
 *
 * Tracks normal windows and feeds updates to org.kynexos.Daemon.WindowsFeed1
 * via callDBus. Skips special windows and windows marked skipTaskbar.
 * Coalesces rapid updates using a QTimer debounce mechanism.
 */

var SERVICE = "org.kynexos.Daemon";
var PATH = "/org/kynexos/Daemon/Windows";
var IFACE = "org.kynexos.Daemon.WindowsFeed1";

var trackedWindows = {};
var pendingUpserts = {};
var lastSent = {};
var lastActiveId = null;
var timerRunning = false;
var flushTimer = new QTimer();
flushTimer.interval = 50;
flushTimer.singleShot = true;

function isValidWindow(window) {
    if (!window) {
        return false;
    }
    // Only track normal windows that should appear on taskbar
    if (!window.normalWindow || window.skipTaskbar) {
        return false;
    }
    return true;
}

function flushPendingUpserts() {
    timerRunning = false;
    for (var id in pendingUpserts) {
        if (Object.prototype.hasOwnProperty.call(pendingUpserts, id)) {
            var item = pendingUpserts[id];
            var last = lastSent[id];
            if (!last || last.title !== item.title || last.appId !== item.appId ||
                last.minimized !== item.minimized || last.active !== item.active) {
                lastSent[id] = {
                    title: item.title,
                    appId: item.appId,
                    minimized: item.minimized,
                    active: item.active
                };
                callDBus(
                    SERVICE,
                    PATH,
                    IFACE,
                    "Upsert",
                    item.id,
                    item.title,
                    item.appId,
                    item.minimized,
                    item.active
                );
            }
        }
    }
    pendingUpserts = {};
}

function scheduleUpsert(window) {
    if (!isValidWindow(window)) {
        return;
    }
    var id = String(window.internalId);
    var title = String(window.caption || "");
    var appId = String(window.desktopFileName || window.resourceClass || "");
    var minimized = Boolean(window.minimized);
    var active = Boolean(window.active);

    var last = lastSent[id];
    if (last && last.title === title && last.appId === appId &&
        last.minimized === minimized && last.active === active) {
        if (!pendingUpserts[id]) {
            return;
        }
    }

    pendingUpserts[id] = {
        id: id,
        title: title,
        appId: appId,
        minimized: minimized,
        active: active
    };

    if (!timerRunning) {
        timerRunning = true;
        flushTimer.start();
    }
}

function trackWindow(window) {
    if (!isValidWindow(window)) {
        return;
    }
    var id = String(window.internalId);
    if (trackedWindows[id]) {
        return;
    }
    trackedWindows[id] = true;

    // Connect window change signals
    window.captionChanged.connect(function() {
        scheduleUpsert(window);
    });
    window.minimizedChanged.connect(function() {
        scheduleUpsert(window);
    });
    window.activeChanged.connect(function() {
        scheduleUpsert(window);
    });
    if (typeof window.desktopFileNameChanged !== "undefined") {
        window.desktopFileNameChanged.connect(function() {
            scheduleUpsert(window);
        });
    }

    scheduleUpsert(window);
}

function untrackWindow(window) {
    if (!window) {
        return;
    }
    var id = String(window.internalId);
    if (trackedWindows[id]) {
        delete trackedWindows[id];
        delete pendingUpserts[id];
        delete lastSent[id];
        if (lastActiveId === id) {
            lastActiveId = null;
        }
        callDBus(SERVICE, PATH, IFACE, "Remove", id);
    }
}

flushTimer.timeout.connect(flushPendingUpserts);

// Initial synchronization: send Reset to clear any stale state
lastSent = {};
lastActiveId = null;
callDBus(SERVICE, PATH, IFACE, "Reset");

// Track all existing windows
var windows = workspace.windowList();
for (var i = 0; i < windows.length; ++i) {
    trackWindow(windows[i]);
}

// Workspace lifecycle signals
workspace.windowAdded.connect(function(window) {
    trackWindow(window);
});

workspace.windowRemoved.connect(function(window) {
    untrackWindow(window);
});

workspace.windowActivated.connect(function(window) {
    var newActiveId = (window && isValidWindow(window)) ? String(window.internalId) : "";
    if (newActiveId !== lastActiveId) {
        lastActiveId = newActiveId;
        callDBus(SERVICE, PATH, IFACE, "SetActive", newActiveId);
    }
});

