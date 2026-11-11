# ADR-014: System Tray Screen Coordinate Projection, Wayland Client Limitations, and Deferred D-Bus Menu Architecture

## Status
Accepted (Stage O2 / Tray Remediation)

## Context
Under the StatusNotifierItem (SNI) specification, user interaction methods forward 2D screen coordinates to registered applications:
- `Activate(x: i32, y: i32)`
- `SecondaryActivate(x: i32, y: i32)`
- `ContextMenu(x: i32, y: i32)`

In X11 environments, window positions and root screen coordinates exist in a shared, unprivileged global coordinate space. In contrast, Wayland strictly isolates client coordinate spaces:
1. `kynex-shell` renders panel surfaces using the Wayland `zwlr_layer_shell_v1` protocol, anchored to compositor output edges (top, bottom, left, or right).
2. Inside QML, standard methods such as `item.mapToItem(null, x, y)` project coordinates solely relative to the Wayland layer surface window (`[0, 0]` at the panel's top-left), not compositor screen coordinates.
3. Multi-monitor layouts, display scaling, and secondary displays with negative compositor origins (e.g. monitors placed to the left or above the primary display) require deterministic translation to global compositor coordinates.
4. Furthermore, because Wayland compositors prohibit unprivileged clients from arbitrarily positioning top-level windows at explicit global screen coordinates without subsurface parentage or specific layer shell protocols, many Wayland applications (both Qt and GTK based) either ignore the passed `(x, y)` coordinates or open popup menus in default fallback locations.

## Decisions

### 1. Pure Projection Function in `kynex-shell-core`
All coordinate translation logic is encapsulated in a pure, UI-free, testable Rust module in `kynex-shell-core::coordinates`:
```rust
pub fn map_to_screen_coordinates(
    anchors: SurfaceAnchors,
    panel: PanelSize,
    output: OutputGeometry,
    local_point: Point,
) -> (i32, i32)
```
- **Inputs**: Edge anchoring configuration (`top`, `bottom`, `left`, `right`), panel width/height, multi-monitor output origin (`x`, `y`), dimensions, display scale, and local window point.
- **Outputs**: Integer compositor global screen coordinates `(screen_x, screen_y)`.
- **Coverage**: Verified by deterministic unit tests covering single-monitor configurations, top/bottom/left/right anchoring, secondary displays with negative origins, and scaled outputs.

### 2. Interaction Coordinate Forwarding
In `styles/default/Shell.qml`, mouse interaction handlers project delegate-relative click points to the root panel window using `mapToItem(null, mouse.x, mouse.y)`, and pass these window coordinates to `TrayModel`. In `kynex-shell`, `TrayModel` applies `map_to_screen_coordinates` to project the coordinates to screen space before invoking `Tray1.Activate`, `Tray1.SecondaryActivate`, and `Tray1.ContextMenu` over D-Bus IPC.

### 3. Wayland Client Limitations & Next Slice (`com.canonical.dbusmenu`)
We explicitly document that on Wayland:
- Many external applications cannot position native context menus adjacent to the panel icon via `ContextMenu(x, y)` due to Wayland security boundaries restricting arbitrary window placement.
- In conformance with owner decision D-048, full client menu rendering via host-side parsing of `com.canonical.dbusmenu` (and `org.freedesktop.DBusMenu`) is out of scope for Stage O/O2 and will be implemented in the next dedicated menu slice.
- For Stage O/O2, Kynex Shell accurately calculates and forwards the correct global screen coordinates, fulfilling the SNI specification contract while maintaining a secure sandbox.

