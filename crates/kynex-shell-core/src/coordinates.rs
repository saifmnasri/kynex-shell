//! Screen coordinate mapping for Wayland `LayerShell` surface windows.
//!
//! Provides pure, deterministic projection from `LayerShell` surface-local window coordinates
//! to compositor global screen coordinates, taking into account surface edge anchoring,
//! panel dimensions, multi-monitor output geometry (including negative origins), and output scale.

/// Surface edge anchors for a `LayerShell` window.
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SurfaceAnchors {
    /// Anchored to top edge.
    pub top: bool,
    /// Anchored to bottom edge.
    pub bottom: bool,
    /// Anchored to left edge.
    pub left: bool,
    /// Anchored to right edge.
    pub right: bool,
}

impl SurfaceAnchors {
    /// Top horizontal panel (anchored top, left, right).
    #[must_use]
    pub fn top_panel() -> Self {
        Self {
            top: true,
            bottom: false,
            left: true,
            right: true,
        }
    }

    /// Bottom horizontal panel (anchored bottom, left, right).
    #[must_use]
    pub fn bottom_panel() -> Self {
        Self {
            top: false,
            bottom: true,
            left: true,
            right: true,
        }
    }

    /// Left vertical panel (anchored left, top, bottom).
    #[must_use]
    pub fn left_panel() -> Self {
        Self {
            top: true,
            bottom: true,
            left: true,
            right: false,
        }
    }

    /// Right vertical panel (anchored right, top, bottom).
    #[must_use]
    pub fn right_panel() -> Self {
        Self {
            top: true,
            bottom: true,
            left: false,
            right: true,
        }
    }

    /// Converts bit flags (matching `LayerShellQt::Window::Anchor`) to `SurfaceAnchors`.
    /// `AnchorTop = 1`, `AnchorBottom = 2`, `AnchorLeft = 4`, `AnchorRight = 8`.
    #[must_use]
    pub fn from_bits(bits: u32) -> Self {
        Self {
            top: (bits & 1) != 0,
            bottom: (bits & 2) != 0,
            left: (bits & 4) != 0,
            right: (bits & 8) != 0,
        }
    }

    /// Converts `SurfaceAnchors` to bit flags matching `LayerShellQt::Window::Anchor`.
    #[must_use]
    pub fn to_bits(self) -> u32 {
        u32::from(self.top)
            | (u32::from(self.bottom) << 1)
            | (u32::from(self.left) << 2)
            | (u32::from(self.right) << 3)
    }
}

/// Dimensions of the panel surface window.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct PanelSize {
    /// Width in logical pixels.
    pub width: f64,
    /// Height in logical pixels.
    pub height: f64,
}

/// Geometry and scaling configuration of a physical display output.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct OutputGeometry {
    /// Compositor X origin (can be negative for monitors positioned to the left).
    pub x: f64,
    /// Compositor Y origin (can be negative for monitors positioned above).
    pub y: f64,
    /// Width in logical pixels.
    pub width: f64,
    /// Height in logical pixels.
    pub height: f64,
    /// Display scaling factor (e.g., 1.0, 1.5, 2.0).
    pub scale: f64,
}

/// 2D point coordinate.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Point {
    /// X coordinate.
    pub x: f64,
    /// Y coordinate.
    pub y: f64,
}

impl Point {
    /// Creates a new 2D point.
    #[must_use]
    pub fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }
}

/// Maps a surface-local window point to compositor global screen coordinates.
///
/// # Arguments
/// - `anchors`: `LayerShell` edge anchoring configuration.
/// - `panel`: Panel surface width and height.
/// - `output`: Target display output geometry and scale.
/// - `local_point`: Coordinates relative to the panel window top-left.
///
/// # Returns
/// `(screen_x, screen_y)` in global screen coordinates.
#[must_use]
pub fn map_to_screen_coordinates(
    anchors: SurfaceAnchors,
    panel: PanelSize,
    output: OutputGeometry,
    local_point: Point,
) -> (i32, i32) {
    // Calculate panel origin relative to output origin (in logical pixels)
    let panel_rel_x = if anchors.left && !anchors.right {
        0.0
    } else if anchors.right && !anchors.left {
        (output.width - panel.width).max(0.0)
    } else {
        // Full width or centered
        0.0
    };

    let panel_rel_y = if anchors.top && !anchors.bottom {
        0.0
    } else if anchors.bottom && !anchors.top {
        (output.height - panel.height).max(0.0)
    } else {
        // Full height or centered
        0.0
    };

    // Global compositor logical coordinates
    let logical_screen_x = output.x + panel_rel_x + local_point.x;
    let logical_screen_y = output.y + panel_rel_y + local_point.y;

    #[allow(clippy::cast_possible_truncation)]
    (
        logical_screen_x.round() as i32,
        logical_screen_y.round() as i32,
    )
}

/// Maps a surface-local window point to physical device pixel coordinates if scaled coordinates are needed.
#[must_use]
pub fn map_to_physical_screen_coordinates(
    anchors: SurfaceAnchors,
    panel: PanelSize,
    output: OutputGeometry,
    local_point: Point,
) -> (i32, i32) {
    let scale = if output.scale > 0.0 {
        output.scale
    } else {
        1.0
    };
    let (lx, ly) = map_to_screen_coordinates(anchors, panel, output, local_point);
    #[allow(clippy::cast_possible_truncation)]
    (
        (f64::from(lx) * scale).round() as i32,
        (f64::from(ly) * scale).round() as i32,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_single_monitor_top_anchoring() {
        let output = OutputGeometry {
            x: 0.0,
            y: 0.0,
            width: 1920.0,
            height: 1080.0,
            scale: 1.0,
        };
        let panel = PanelSize {
            width: 1920.0,
            height: 40.0,
        };
        let anchors = SurfaceAnchors::top_panel();
        let local = Point::new(1850.0, 20.0);

        let (sx, sy) = map_to_screen_coordinates(anchors, panel, output, local);
        assert_eq!(sx, 1850);
        assert_eq!(sy, 20);
    }

    #[test]
    fn test_single_monitor_bottom_anchoring() {
        let output = OutputGeometry {
            x: 0.0,
            y: 0.0,
            width: 1920.0,
            height: 1080.0,
            scale: 1.0,
        };
        let panel = PanelSize {
            width: 1920.0,
            height: 40.0,
        };
        let anchors = SurfaceAnchors::bottom_panel();
        let local = Point::new(1850.0, 20.0);

        let (sx, sy) = map_to_screen_coordinates(anchors, panel, output, local);
        assert_eq!(sx, 1850);
        assert_eq!(sy, 1060); // 1080 - 40 + 20 = 1060
    }

    #[test]
    fn test_single_monitor_left_anchoring() {
        let output = OutputGeometry {
            x: 0.0,
            y: 0.0,
            width: 1920.0,
            height: 1080.0,
            scale: 1.0,
        };
        let panel = PanelSize {
            width: 48.0,
            height: 1080.0,
        };
        let anchors = SurfaceAnchors::left_panel();
        let local = Point::new(24.0, 500.0);

        let (sx, sy) = map_to_screen_coordinates(anchors, panel, output, local);
        assert_eq!(sx, 24);
        assert_eq!(sy, 500);
    }

    #[test]
    fn test_single_monitor_right_anchoring() {
        let output = OutputGeometry {
            x: 0.0,
            y: 0.0,
            width: 1920.0,
            height: 1080.0,
            scale: 1.0,
        };
        let panel = PanelSize {
            width: 48.0,
            height: 1080.0,
        };
        let anchors = SurfaceAnchors::right_panel();
        let local = Point::new(24.0, 500.0);

        let (sx, sy) = map_to_screen_coordinates(anchors, panel, output, local);
        assert_eq!(sx, 1896); // 1920 - 48 + 24 = 1896
        assert_eq!(sy, 500);
    }

    #[test]
    fn test_second_monitor_at_negative_x() {
        // Left monitor positioned at x = -1920
        let output = OutputGeometry {
            x: -1920.0,
            y: 0.0,
            width: 1920.0,
            height: 1080.0,
            scale: 1.0,
        };
        let panel = PanelSize {
            width: 1920.0,
            height: 40.0,
        };
        let anchors = SurfaceAnchors::top_panel();
        let local = Point::new(100.0, 20.0);

        let (sx, sy) = map_to_screen_coordinates(anchors, panel, output, local);
        assert_eq!(sx, -1820); // -1920 + 100 = -1820
        assert_eq!(sy, 20);
    }

    #[test]
    fn test_scaled_output() {
        let output = OutputGeometry {
            x: 0.0,
            y: 0.0,
            width: 1920.0,
            height: 1080.0,
            scale: 2.0,
        };
        let panel = PanelSize {
            width: 1920.0,
            height: 40.0,
        };
        let anchors = SurfaceAnchors::top_panel();
        let local = Point::new(100.0, 20.0);

        let (lx, ly) = map_to_screen_coordinates(anchors, panel, output, local);
        assert_eq!(lx, 100);
        assert_eq!(ly, 20);

        let (px, py) = map_to_physical_screen_coordinates(anchors, panel, output, local);
        assert_eq!(px, 200); // 100 * 2.0 = 200
        assert_eq!(py, 40); // 20 * 2.0 = 40
    }
}
