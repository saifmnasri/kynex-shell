#!/usr/bin/env python3
"""Dev-only fake UPower service for nested session testing and live demos.

Not shipped with production releases.
Claims `org.freedesktop.UPower` on the session bus and serves
`/org/freedesktop/UPower/devices/DisplayDevice` with properties that can be
updated live via D-Bus (`busctl`) or standard Properties.Set calls.
"""

import sys
import warnings
warnings.filterwarnings("ignore", category=DeprecationWarning)
import gi
from gi.repository import Gio, GLib

INTROSPECTION_XML = """
<node>
  <interface name='org.freedesktop.UPower.Device'>
    <property name='IsPresent' type='b' access='readwrite'/>
    <property name='Percentage' type='d' access='readwrite'/>
    <property name='State' type='u' access='readwrite'/>
    <property name='TimeToEmpty' type='x' access='readwrite'/>
    <property name='TimeToFull' type='x' access='readwrite'/>
  </interface>
</node>
"""

class FakeUPowerService:
    def __init__(self):
        self.node_info = Gio.DBusNodeInfo.new_for_xml(INTROSPECTION_XML)
        self.interface_info = self.node_info.interfaces[0]
        self.props = {
            "IsPresent": GLib.Variant("b", True),
            "Percentage": GLib.Variant("d", 85.0),
            "State": GLib.Variant("u", 2),
            "TimeToEmpty": GLib.Variant("x", 7200),
            "TimeToFull": GLib.Variant("x", 0),
        }
        self.conn = None
        self.reg_id = None

    def on_bus_acquired(self, conn, name):
        self.conn = conn
        self.reg_id = conn.register_object(
            object_path="/org/freedesktop/UPower/devices/DisplayDevice",
            interface_info=self.interface_info,
            method_call_closure=self.on_method_call,
            get_property_closure=self.on_get_property,
            set_property_closure=self.on_set_property,
        )
        print(f"[fake-upower] Registered /org/freedesktop/UPower/devices/DisplayDevice on session bus", flush=True)

    def on_name_acquired(self, conn, name):
        print(f"[fake-upower] Claimed name '{name}'", flush=True)

    def on_name_lost(self, conn, name):
        print(f"[fake-upower] Lost name '{name}', exiting", flush=True)
        sys.exit(0)

    def on_method_call(self, conn, sender, path, iface, method, params, invocation):
        invocation.return_error_literal(Gio.dbus_error_quark(), Gio.DBusError.UNKNOWN_METHOD, f"Unknown method: {method}")

    def on_get_property(self, conn, sender, path, iface, prop_name):
        return self.props.get(prop_name)

    def on_set_property(self, conn, sender, path, iface, prop_name, value):
        if prop_name in self.props:
            old_val = self.props[prop_name]
            self.props[prop_name] = value
            print(f"[fake-upower] Property updated: {prop_name} = {value.unpack()}", flush=True)

            # Emit PropertiesChanged signal
            val_wrapped = GLib.Variant("v", value) if value.get_type_string() != "v" else value
            changed_props = GLib.Variant("a{sv}", {prop_name: val_wrapped})
            invalidated = GLib.Variant("as", [])
            iface_var = GLib.Variant("s", "org.freedesktop.UPower.Device")
            body = GLib.Variant.new_tuple(iface_var, changed_props, invalidated)

            conn.emit_signal(
                None,
                "/org/freedesktop/UPower/devices/DisplayDevice",
                "org.freedesktop.DBus.Properties",
                "PropertiesChanged",
                body,
            )
            return True
        return False

def main():
    service = FakeUPowerService()
    owner_id = Gio.bus_own_name(
        Gio.BusType.SESSION,
        "org.freedesktop.UPower",
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
    finally:
        Gio.bus_unown_name(owner_id)

if __name__ == "__main__":
    main()
