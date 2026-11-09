//! Where the overlay window sits and where the cube is drawn inside it.
//!
//! The window is placed flush against one screen edge and shows part of the
//! Figma animation frame, whose border is the screen edge. The window
//! border clips the cube, which gives the "hiding behind the edge" look.

use serde::{Deserialize, Serialize};

use crate::library::FRAME_W;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Edge {
    #[default]
    Bottom,
    Top,
    Left,
    Right,
}

impl Edge {
    pub const ALL: [Edge; 4] = [Edge::Bottom, Edge::Top, Edge::Left, Edge::Right];

    pub fn label(self) -> &'static str {
        match self {
            Edge::Bottom => "Bottom",
            Edge::Top => "Top",
            Edge::Left => "Left",
            Edge::Right => "Right",
        }
    }
}

/// A rectangle in physical pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub w: u32,
    pub h: u32,
}

/// Physical pixels per SVG unit for a cube of `cube_size` logical pixels.
pub fn svg_scale(cube_size: u32, scale_factor: f64) -> f64 {
    cube_size as f64 * scale_factor / FRAME_W
}

/// Window size in physical pixels for a crop of `w` × `h` SVG units.
pub fn window_size(w: f64, h: f64, scale: f64) -> (u32, u32) {
    ((w * scale).ceil() as u32, (h * scale).ceil() as u32)
}

/// Place a window of `size` flush against `edge` of `monitor`. `along` is
/// 0.0–1.0 and says where along the edge it sits (0.5 is the middle).
pub fn window_rect(monitor: Rect, size: (u32, u32), edge: Edge, along: f32) -> Rect {
    let along = along.clamp(0.0, 1.0) as f64;
    let (w, h) = size;
    let free_x = monitor.w.saturating_sub(w) as f64;
    let free_y = monitor.h.saturating_sub(h) as f64;
    let (x, y) = match edge {
        Edge::Bottom => (
            monitor.x + (free_x * along).round() as i32,
            monitor.y + monitor.h as i32 - h as i32,
        ),
        Edge::Top => (monitor.x + (free_x * along).round() as i32, monitor.y),
        Edge::Left => (monitor.x, monitor.y + (free_y * along).round() as i32),
        Edge::Right => (
            monitor.x + monitor.w as i32 - w as i32,
            monitor.y + (free_y * along).round() as i32,
        ),
    };
    Rect { x, y, w, h }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MON: Rect = Rect {
        x: 0,
        y: 0,
        w: 3440,
        h: 1440,
    };

    #[test]
    fn window_is_flush_with_each_edge() {
        let size = (500, 400);
        let b = window_rect(MON, size, Edge::Bottom, 0.5);
        assert_eq!(b.y + b.h as i32, 1440);
        assert_eq!(b.x, (3440 - 500) / 2);
        let t = window_rect(MON, size, Edge::Top, 0.0);
        assert_eq!((t.x, t.y), (0, 0));
        let l = window_rect(MON, size, Edge::Left, 1.0);
        assert_eq!((l.x, l.y + l.h as i32), (0, 1440));
        let r = window_rect(MON, size, Edge::Right, 0.5);
        assert_eq!(r.x + r.w as i32, 3440);
    }

    #[test]
    fn window_respects_monitor_origin() {
        let second = Rect {
            x: -1920,
            y: 200,
            w: 1920,
            h: 1080,
        };
        let r = window_rect(second, (300, 200), Edge::Bottom, 0.0);
        assert_eq!((r.x, r.y), (-1920, 200 + 1080 - 200));
    }

    #[test]
    fn window_grows_with_cube_size_and_dpi() {
        let size = |cube, dpi| window_size(200.0, 100.0, svg_scale(cube, dpi));
        let (w1, h1) = size(200, 1.0);
        let (w2, _) = size(400, 1.0);
        let (w3, _) = size(200, 2.0);
        assert!(w1 > 200 && h1 > 0);
        assert!((w2 as i32 - 2 * w1 as i32).abs() <= 1);
        assert!((w3 as i32 - 2 * w1 as i32).abs() <= 1);
    }
}
