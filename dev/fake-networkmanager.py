#!/usr/bin/env python3
"""Dev-only fake NetworkManager service for nested session testing and live demos.

Not shipped with production releases.
Claims `org.freedesktop.NetworkManager` on the session bus and serves:
- `/org/freedesktop/NetworkManager` (org.freedesktop.NetworkManager)
- `/org/freedesktop/NetworkManager/ActiveConnection/1` (org.freedesktop.NetworkManager.Connection.Active)
- `/org/freedesktop/NetworkManager/AccessPoint/1` (org.freedesktop.NetworkManager.AccessPoint)
with properties that can be updated live via D-Bus (`busctl`).
"""

import sys
import warnings
warnings.filterwarnings("ignore", category=DeprecationWarning)
import gi
from gi.repository import Gio, GLib

NM_INTROSPECTION = """
<node>
  <interface name='org.freedesktop.NetworkManager'>
    <property name='State' type='u' access='readwrite'/>
    <property name='PrimaryConnection' type='o' access='readwrite'/>
    <property name='Devices' type='ao' access='readwrite'/>
  </interface>
</node>
"""

CONN_INTROSPECTION = """
<node>
  <interface name='org.freedesktop.NetworkManager.Connection.Active'>
    <property name='Type' type='s' access='readwrite'/>
    <property name='State' type='u' access='readwrite'/>
    <property name='SpecificObject' type='o' access='readwrite'/>
  </interface>
</node>
"""

AP_INTROSPECTION = """
<node>
  <interface name='org.freedesktop.NetworkManager.AccessPoint'>
    <property name='Strength' type='y' access='readwrite'/>
    <property name='Ssid' type='ay' access='readwrite'/>
  </interface>
</node>
"""


class FakeNetworkManagerService:
    def __init__(self):
        self.nm_node = Gio.DBusNodeInfo.new_for_xml(NM_INTROSPECTION)
        self.conn_node = Gio.DBusNodeInfo.new_for_xml(CONN_INTROSPECTION)
        self.ap_node = Gio.DBusNodeInfo.new_for_xml(AP_INTROSPECTION)

        self.nm_props = {
            "State": GLib.Variant("u", 70),
            "PrimaryConnection": GLib.Variant("o", "/org/freedesktop/NetworkManager/ActiveConnection/1"),
            "Devices": GLib.Variant("ao", ["/org/freedesktop/NetworkManager/Devices/1"]),
        }

        self.conn_props = {
            "Type": GLib.Variant("s", "802-11-wireless"),
            "State": GLib.Variant("u", 2),
            "SpecificObject": GLib.Variant("o", "/org/freedesktop/NetworkManager/AccessPoint/1"),
        }

        self.ap_props = {
            "Strength": GLib.Variant("y", 85),
            "Ssid": GLib.Variant("ay", list(b"KynexNet")),
        }

        self.conn = None

    def on_bus_acquired(self, conn, name):
        self.conn = conn

        # 1. Register NM Root
        conn.register_object(
            object_path="/org/freedesktop/NetworkManager",
            interface_info=self.nm_node.interfaces[0],
            method_call_closure=self.on_method_call,
            get_property_closure=lambda c, s, p, i, prop: self.nm_props.get(prop),
            set_property_closure=lambda c, s, p, i, prop, val: self.set_prop(self.nm_props, "/org/freedesktop/NetworkManager", "org.freedesktop.NetworkManager", prop, val),
        )

        # 2. Register ActiveConnection
        conn.register_object(
            object_path="/org/freedesktop/NetworkManager/ActiveConnection/1",
            interface_info=self.conn_node.interfaces[0],
            method_call_closure=self.on_method_call,
            get_property_closure=lambda c, s, p, i, prop: self.conn_props.get(prop),
            set_property_closure=lambda c, s, p, i, prop, val: self.set_prop(self.conn_props, "/org/freedesktop/NetworkManager/ActiveConnection/1", "org.freedesktop.NetworkManager.Connection.Active", prop, val),
        )

        # 3. Register AccessPoint
        conn.register_object(
            object_path="/org/freedesktop/NetworkManager/AccessPoint/1",
            interface_info=self.ap_node.interfaces[0],
            method_call_closure=self.on_method_call,
            get_property_closure=lambda c, s, p, i, prop: self.ap_props.get(prop),
            set_property_closure=lambda c, s, p, i, prop, val: self.set_prop(self.ap_props, "/org/freedesktop/NetworkManager/AccessPoint/1", "org.freedesktop.NetworkManager.AccessPoint", prop, val),
        )

        print("[fake-nm] Registered NetworkManager objects on session bus", flush=True)

    def on_name_acquired(self, conn, name):
        print(f"[fake-nm] Claimed name '{name}'", flush=True)

    def on_name_lost(self, conn, name):
        print(f"[fake-nm] Lost name '{name}', exiting", flush=True)
        sys.exit(0)

    def on_method_call(self, conn, sender, path, iface, method, params, invocation):
        invocation.return_error_literal(Gio.dbus_error_quark(), Gio.DBusError.UNKNOWN_METHOD, f"Unknown method: {method}")

    def set_prop(self, prop_dict, path, iface, prop_name, value):
        if prop_name in prop_dict:
            prop_dict[prop_name] = value
            print(f"[fake-nm] Property updated on {path}: {prop_name} = {value.unpack()}", flush=True)

            val_wrapped = GLib.Variant("v", value) if value.get_type_string() != "v" else value
            changed_props = GLib.Variant("a{sv}", {prop_name: val_wrapped})
            invalidated = GLib.Variant("as", [])
            body = GLib.Variant.new_tuple(GLib.Variant("s", iface), changed_props, invalidated)

            if self.conn:
                self.conn.emit_signal(
                    None,
                    path,
                    "org.freedesktop.DBus.Properties",
                    "PropertiesChanged",
                    body,
                )
            return True
        return False


def main():
    service = FakeNetworkManagerService()
    _owner_id = Gio.bus_own_name(
        Gio.BusType.SESSION,
        "org.freedesktop.NetworkManager",
        Gio.BusNameOwnerFlags.NONE,
        service.on_bus_acquired,
        service.on_name_acquired,
        service.on_name_lost,
    )

    loop = GLib.MainLoop()
    try:
        loop.run()
    except KeyboardInterrupt:
        pass


if __name__ == "__main__":
    main()

