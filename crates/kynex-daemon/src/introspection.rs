//! Clean non-recursive D-Bus object introspection for intermediate nodes.
//!
//! Upstream systemd `busctl` (`src/busctl/busctl-introspect.c`) contains a parser bug:
//! when parsing nested sub-nodes (`n_depth > 0`) in an introspect XML tree, it fails to free
//! `argument_direction` on method args, causing subsequent signal args to inherit `direction="in"`
//! and logging warnings ("Unexpected signal <arg> direction value 'in'").
//!
//! To adhere to the D-Bus specification and provide clean `busctl --user tree` output,
//! intermediate nodes must return clean non-recursive XML listing only their immediate child
//! `<node name="..."/>` tags without inlining child interfaces.

use zbus::names::InterfaceName;
use zbus::{interface, Connection};

/// Non-recursive introspection interface server returning precomputed XML.
#[derive(Debug, Clone)]
pub struct NonRecursiveIntrospect {
    xml: String,
}

impl NonRecursiveIntrospect {
    /// Creates a new `NonRecursiveIntrospect` with given static XML.
    #[must_use]
    pub fn new(xml: impl Into<String>) -> Self {
        Self { xml: xml.into() }
    }
}

#[interface(name = "org.freedesktop.DBus.Introspectable")]
impl NonRecursiveIntrospect {
    /// Returns the static non-recursive XML representation of the node.
    fn introspect(&self) -> String {
        self.xml.clone()
    }
}

/// Registers non-recursive `Introspectable` handlers on all intermediate object paths.
///
/// # Errors
///
/// Returns a `zbus::Error` if interface parsing fails or if registration fails.
pub async fn setup_clean_introspection(conn: &Connection) -> zbus::Result<()> {
    let iface_name = InterfaceName::try_from("org.freedesktop.DBus.Introspectable")?;

    let xml_root = r#"<!DOCTYPE node PUBLIC "-//freedesktop//DTD D-BUS Object Introspection 1.0//EN"
 "http://www.freedesktop.org/standards/dbus/1.0/introspect.dtd">
<node>
  <node name="org"/>
  <node name="StatusNotifierWatcher"/>
</node>"#;

    let xml_org = r#"<!DOCTYPE node PUBLIC "-//freedesktop//DTD D-BUS Object Introspection 1.0//EN"
 "http://www.freedesktop.org/standards/dbus/1.0/introspect.dtd">
<node>
  <node name="freedesktop"/>
  <node name="kynexos"/>
</node>"#;

    let xml_org_freedesktop = r#"<!DOCTYPE node PUBLIC "-//freedesktop//DTD D-BUS Object Introspection 1.0//EN"
 "http://www.freedesktop.org/standards/dbus/1.0/introspect.dtd">
<node>
  <node name="Notifications"/>
</node>"#;

    let xml_org_kynexos = r#"<!DOCTYPE node PUBLIC "-//freedesktop//DTD D-BUS Object Introspection 1.0//EN"
 "http://www.freedesktop.org/standards/dbus/1.0/introspect.dtd">
<node>
  <node name="Daemon"/>
</node>"#;

    let xml_daemon = r#"<!DOCTYPE node PUBLIC "-//freedesktop//DTD D-BUS Object Introspection 1.0//EN"
 "http://www.freedesktop.org/standards/dbus/1.0/introspect.dtd">
<node>
  <interface name="org.freedesktop.DBus.Introspectable">
    <method name="Introspect">
      <arg name="xml_data" type="s" direction="out"/>
    </method>
  </interface>
  <interface name="org.freedesktop.DBus.Peer">
    <method name="Ping"/>
    <method name="GetMachineId">
      <arg name="machine_uuid" type="s" direction="out"/>
    </method>
  </interface>
  <interface name="org.kynexos.Daemon.Core1">
    <method name="Ping">
      <arg type="s" direction="out"/>
    </method>
    <method name="Version">
      <arg type="s" direction="out"/>
    </method>
    <method name="Status">
      <arg type="(sta(s))" direction="out"/>
    </method>
  </interface>
  <interface name="org.kynexos.Daemon.Power1">
    <property name="Present" type="b" access="read"/>
    <property name="Percentage" type="d" access="read"/>
    <property name="State" type="u" access="read"/>
    <property name="Low" type="b" access="read"/>
  </interface>
  <interface name="org.kynexos.Daemon.Network1">
    <property name="Present" type="b" access="read"/>
    <property name="State" type="u" access="read"/>
    <property name="Kind" type="u" access="read"/>
    <property name="SignalStrength" type="u" access="read"/>
    <property name="Ssid" type="s" access="read"/>
    <property name="Weak" type="b" access="read"/>
  </interface>
  <node name="Apps"/>
  <node name="Audio"/>
  <node name="Notifications"/>
  <node name="Pins"/>
  <node name="Shell"/>
  <node name="Tray"/>
  <node name="Windows"/>
</node>"#;

    let intermediate_nodes = [
        ("/", xml_root),
        ("/org", xml_org),
        ("/org/freedesktop", xml_org_freedesktop),
        ("/org/kynexos", xml_org_kynexos),
        ("/org/kynexos/Daemon", xml_daemon),
    ];

    let obj_server = conn.object_server();
    for (path, xml) in intermediate_nodes {
        let _ = obj_server.remove_named(path, iface_name.clone()).await;
        obj_server
            .at(path, NonRecursiveIntrospect::new(xml))
            .await?;
    }

    Ok(())
}
