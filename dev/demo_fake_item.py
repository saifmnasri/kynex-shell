#!/usr/bin/env python3
"""Stage P Interactive Demo Fake dbusmenu Item.
Exports:
- StatusNotifierItem with Menu = /MenuBar
- dbusmenu with:
  1: Preferences (icon)
  2: View Mode (submenu -> 201: Compact, 202: Comfortable)
  3: Enable Dark Theme (checkbox, toggles state on click)
  4: separator
  5: Standard Engine (radio)
  6: Turbo Engine (radio)
  7: Restricted Feature (disabled)
  8: Quit (icon + shortcut)
"""

import os
import sys
import threading
import time
import gi

gi.require_version("GLib", "2.0")
gi.require_version("Gio", "2.0")
from gi.repository import GLib, Gio

ITEM_XML = """
<node>
  <interface name="org.kde.StatusNotifierItem">
    <property name="Category" type="s" access="read"/>
    <property name="Id" type="s" access="read"/>
    <property name="Title" type="s" access="read"/>
    <property name="Status" type="s" access="read"/>
    <property name="WindowId" type="i" access="read"/>
    <property name="IconThemePath" type="s" access="read"/>
    <property name="ItemIsMenu" type="b" access="read"/>
    <property name="Menu" type="o" access="read"/>
    <property name="IconName" type="s" access="read"/>
    <property name="IconPixmap" type="a(iiay)" access="read"/>
    <property name="OverlayIconName" type="s" access="read"/>
    <property name="OverlayIconPixmap" type="a(iiay)" access="read"/>
    <property name="AttentionIconName" type="s" access="read"/>
    <property name="AttentionIconPixmap" type="a(iiay)" access="read"/>
    <property name="AttentionMovieName" type="s" access="read"/>
    <property name="ToolTip" type="(sa(iiay)ss)" access="read"/>
    <method name="ContextMenu">
      <arg name="x" type="i" direction="in"/>
      <arg name="y" type="i" direction="in"/>
    </method>
    <method name="Activate">
      <arg name="x" type="i" direction="in"/>
      <arg name="y" type="i" direction="in"/>
    </method>
  </interface>
</node>
"""

MENU_XML = """
<node>
  <interface name="com.canonical.dbusmenu">
    <property name="Version" type="u" access="read"/>
    <property name="Status" type="s" access="read"/>
    <method name="GetLayout">
      <arg name="parentId" type="i" direction="in"/>
      <arg name="recursionDepth" type="i" direction="in"/>
      <arg name="propertyNames" type="as" direction="in"/>
      <arg name="revision" type="u" direction="out"/>
      <arg name="layout" type="(ia{sv}av)" direction="out"/>
    </method>
    <method name="GetGroupProperties">
      <arg name="ids" type="ai" direction="in"/>
      <arg name="propertyNames" type="as" direction="in"/>
      <arg name="properties" type="a(ia{sv})" direction="out"/>
    </method>
    <method name="GetProperty">
      <arg name="id" type="i" direction="in"/>
      <arg name="name" type="s" direction="in"/>
      <arg name="value" type="v" direction="out"/>
    </method>
    <method name="Event">
      <arg name="id" type="i" direction="in"/>
      <arg name="eventId" type="s" direction="in"/>
      <arg name="data" type="v" direction="in"/>
      <arg name="timestamp" type="u" direction="in"/>
    </method>
    <method name="AboutToShow">
      <arg name="id" type="i" direction="in"/>
      <arg name="needUpdate" type="b" direction="out"/>
    </method>
    <signal name="LayoutUpdated">
      <arg name="revision" type="u"/>
      <arg name="parent" type="i"/>
    </signal>
  </interface>
</node>
"""


class DemoDbusmenuItem:
    def __init__(self, item_id="demo-app-1", title="Kynex Demo App"):
        self.item_id = item_id
        self.title = title
        self.menu_path = "/MenuBar"
        self.revision = 1
        self.checkbox_state = 1
        self.radio_state = 1
        self.events_received = []

    def make_tree(self):
        # Submenu
        sub1 = GLib.Variant(
            "(ia{sv}av)",
            (
                201,
                {
                    "label": GLib.Variant("s", "Compact View"),
                    "enabled": GLib.Variant("b", True),
                },
                [],
            ),
        )
        sub2 = GLib.Variant(
            "(ia{sv}av)",
            (
                202,
                {
                    "label": GLib.Variant("s", "Comfortable View"),
                    "enabled": GLib.Variant("b", True),
                },
                [],
            ),
        )
        c_submenu = GLib.Variant(
            "(ia{sv}av)",
            (
                2,
                {
                    "label": GLib.Variant("s", "View Mode"),
                    "children-display": GLib.Variant("s", "submenu"),
                },
                [GLib.Variant("v", sub1), GLib.Variant("v", sub2)],
            ),
        )

        c_pref = GLib.Variant(
            "(ia{sv}av)",
            (
                1,
                {
                    "label": GLib.Variant("s", "_Preferences..."),
                    "icon-name": GLib.Variant("s", "preferences-system"),
                },
                [],
            ),
        )
        c_check = GLib.Variant(
            "(ia{sv}av)",
            (
                3,
                {
                    "label": GLib.Variant("s", "Enable Dark Theme"),
                    "toggle-type": GLib.Variant("s", "checkmark"),
                    "toggle-state": GLib.Variant("i", self.checkbox_state),
                },
                [],
            ),
        )
        c_sep = GLib.Variant(
            "(ia{sv}av)",
            (4, {"type": GLib.Variant("s", "separator")}, []),
        )
        c_radio1 = GLib.Variant(
            "(ia{sv}av)",
            (
                5,
                {
                    "label": GLib.Variant("s", "Standard Engine"),
                    "toggle-type": GLib.Variant("s", "radio"),
                    "toggle-state": GLib.Variant("i", self.radio_state),
                },
                [],
            ),
        )
        c_radio2 = GLib.Variant(
            "(ia{sv}av)",
            (
                6,
                {
                    "label": GLib.Variant("s", "Turbo Engine"),
                    "toggle-type": GLib.Variant("s", "radio"),
                    "toggle-state": GLib.Variant(
                        "i", 0 if self.radio_state == 1 else 1
                    ),
                },
                [],
            ),
        )
        c_dis = GLib.Variant(
            "(ia{sv}av)",
            (
                7,
                {
                    "label": GLib.Variant("s", "Restricted Feature"),
                    "enabled": GLib.Variant("b", False),
                },
                [],
            ),
        )
        c_quit = GLib.Variant(
            "(ia{sv}av)",
            (
                8,
                {
                    "label": GLib.Variant("s", "_Quit"),
                    "icon-name": GLib.Variant("s", "application-exit"),
                    "shortcut": GLib.Variant("s", "Ctrl+Q"),
                },
                [],
            ),
        )

        children = [
            GLib.Variant("v", c_pref),
            GLib.Variant("v", c_submenu),
            GLib.Variant("v", c_check),
            GLib.Variant("v", c_sep),
            GLib.Variant("v", c_radio1),
            GLib.Variant("v", c_radio2),
            GLib.Variant("v", c_dis),
            GLib.Variant("v", c_quit),
        ]
        return (0, {}, children)

    def export(self, conn, path="/StatusNotifierItem"):
        item_node = Gio.DBusNodeInfo.new_for_xml(ITEM_XML)
        item_iface = item_node.interfaces[0]

        def get_item_prop(connection, sender, opath, oiface, prop):
            if prop == "Category":
                return GLib.Variant("s", "ApplicationStatus")
            elif prop == "Id":
                return GLib.Variant("s", self.item_id)
            elif prop == "Title":
                return GLib.Variant("s", self.title)
            elif prop == "Status":
                return GLib.Variant("s", "Active")
            elif prop == "WindowId":
                return GLib.Variant("i", 0)
            elif prop == "IconThemePath":
                return GLib.Variant("s", "")
            elif prop == "ItemIsMenu":
                return GLib.Variant("b", True)
            elif prop == "Menu":
                return GLib.Variant("o", self.menu_path)
            elif prop == "IconName":
                return GLib.Variant("s", "applications-system")
            elif prop == "IconPixmap":
                return GLib.Variant("a(iiay)", [])
            elif prop == "OverlayIconName":
                return GLib.Variant("s", "")
            elif prop == "OverlayIconPixmap":
                return GLib.Variant("a(iiay)", [])
            elif prop == "AttentionIconName":
                return GLib.Variant("s", "")
            elif prop == "AttentionIconPixmap":
                return GLib.Variant("a(iiay)", [])
            elif prop == "AttentionMovieName":
                return GLib.Variant("s", "")
            elif prop == "ToolTip":
                return GLib.Variant("(sa(iiay)ss)", ("", [], self.title, "dbusmenu active"))
            return None

        conn.register_object(
            path,
            item_iface,
            None,
            get_item_prop,
            None,
        )

        menu_node = Gio.DBusNodeInfo.new_for_xml(MENU_XML)
        menu_iface = menu_node.interfaces[0]

        def menu_method_call(
            connection,
            sender,
            opath,
            oiface,
            method_name,
            parameters,
            invocation,
        ):
            if method_name == "GetLayout":
                tree = self.make_tree()
                ret = GLib.Variant("(u(ia{sv}av))", (self.revision, tree))
                invocation.return_value(ret)
            elif method_name == "AboutToShow":
                item_id = parameters[0]
                print(f"[FakeItem] AboutToShow received for id={item_id}", flush=True)
                invocation.return_value(GLib.Variant("(b)", (False,)))
            elif method_name == "Event":
                item_id, event_id, data, ts = parameters
                print(f"[FakeItem] Event received: id={item_id}, event='{event_id}'", flush=True)
                self.events_received.append((item_id, event_id))
                if item_id == 3 and event_id == "clicked":
                    self.checkbox_state = 0 if self.checkbox_state == 1 else 1
                    self.revision += 1
                    print(f"[FakeItem] Checkbox toggled to: {self.checkbox_state}", flush=True)
                elif item_id in (5, 6) and event_id == "clicked":
                    self.radio_state = 1 if item_id == 5 else 0
                    self.revision += 1
                    print(f"[FakeItem] Radio state updated to: {self.radio_state}", flush=True)
                invocation.return_value(None)
            else:
                invocation.return_value(None)

        def menu_get_prop(connection, sender, opath, oiface, prop):
            if prop == "Version":
                return GLib.Variant("u", 3)
            elif prop == "Status":
                return GLib.Variant("s", "normal")
            return None

        conn.register_object(
            self.menu_path,
            menu_iface,
            menu_method_call,
            menu_get_prop,
            None,
        )


def main():
    if len(sys.argv) < 2:
        print("Usage: demo_fake_item.py <DBUS_ADDRESS>")
        sys.exit(1)

    bus_addr = sys.argv[1]
    conn = Gio.DBusConnection.new_for_address_sync(
        bus_addr,
        Gio.DBusConnectionFlags.AUTHENTICATION_CLIENT
        | Gio.DBusConnectionFlags.MESSAGE_BUS_CONNECTION,
        None,
        None,
    )

    item = DemoDbusmenuItem()
    item.export(conn, "/StatusNotifierItem")

    loop = GLib.MainLoop()
    t_loop = threading.Thread(target=loop.run, daemon=True)
    t_loop.start()

    watcher = Gio.DBusProxy.new_sync(
        conn,
        Gio.DBusProxyFlags.NONE,
        None,
        "org.kde.StatusNotifierWatcher",
        "/StatusNotifierWatcher",
        "org.kde.StatusNotifierWatcher",
        None,
    )
    watcher.call_sync(
        "RegisterStatusNotifierItem",
        GLib.Variant("(s)", ("/StatusNotifierItem",)),
        Gio.DBusCallFlags.NONE,
        -1,
        None,
    )
    print("Registered FakeStatusNotifierItem with /MenuBar", flush=True)

    try:
        while True:
            time.sleep(1)
    except KeyboardInterrupt:
        pass


if __name__ == "__main__":
    main()

