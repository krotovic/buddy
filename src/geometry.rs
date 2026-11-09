//! Where the overlay window sits and where the cube is drawn inside it.
//!
//! The window is placed flush against one screen edge. The cube slides
//! inside the window, and the window border (which lies on the screen edge)
//! clips it, which gives the "hiding behind the edge" look.

use serde::{Deserialize, Serialize};

use crate::cube::{ARTBOARD_H, ARTBOARD_W, BODY_BOTTOM, BODY_LEFT, BODY_RIGHT, BODY_TOP, BODY_W};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
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

/// How much of the frame stays visible while resting, in SVG units.
pub const SLIVER: f32 = 8.0;

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
    cube_size as f64 * scale_factor / BODY_W as f64
}

/// Window size in physical pixels: the whole Figma artboard (about 1.9× the
/// cube), so waving arms and effects fit later.
pub fn window_size(cube_size: u32, scale_factor: f64) -> (u32, u32) {
    let s = svg_scale(cube_size, scale_factor);
    (
        (ARTBOARD_W as f64 * s).ceil() as u32,
        (ARTBOARD_H as f64 * s).ceil() as u32,
    )
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

/// Offset of the cube inside the artboard, in SVG units, for a reveal from
/// 0.0 (only a sliver of the frame shows) to 1.0 (fully out). Values above
/// 1.0 are allowed for overshoot.
pub fn cube_offset(edge: Edge, reveal: f32) -> (f32, f32) {
    // Gap between the cube and the screen edge when fully out: the same gap
    // as below the cube in the Figma artboard.
    let gap = ARTBOARD_H - BODY_BOTTOM;
    let hidden = 1.0 - reveal;
    match edge {
        Edge::Bottom => (0.0, hidden * (ARTBOARD_H - SLIVER - BODY_TOP)),
        Edge::Top => {
            let out = gap - BODY_TOP;
            let hide = SLIVER - BODY_BOTTOM;
            (0.0, out + hidden * (hide - out))
        }
        Edge::Left => {
            let out = gap - BODY_LEFT;
            let hide = SLIVER - BODY_RIGHT;
            (out + hidden * (hide - out), 0.0)
        }
        Edge::Right => {
            let out = (ARTBOARD_W - gap) - BODY_RIGHT;
            let hide = (ARTBOARD_W - SLIVER) - BODY_LEFT;
            (out + hidden * (hide - out), 0.0)
        }
    }
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

    fn visible_body(edge: Edge, reveal: f32) -> (f32, f32, f32, f32) {
        let (dx, dy) = cube_offset(edge, reveal);
        (
            (BODY_LEFT + dx).max(0.0),
            (BODY_TOP + dy).max(0.0),
            (BODY_RIGHT + dx).min(ARTBOARD_W),
            (BODY_BOTTOM + dy).min(ARTBOARD_H),
        )
    }

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
    fn hidden_cube_shows_only_a_sliver() {
        for edge in Edge::ALL {
            let (l, t, r, b) = visible_body(edge, 0.0);
            let visible = match edge {
                Edge::Bottom | Edge::Top => b - t,
                Edge::Left | Edge::Right => r - l,
            };
            assert!((visible - SLIVER).abs() < 0.01, "{edge:?}: {visible}");
        }
    }

    #[test]
    fn revealed_cube_is_fully_inside_the_window() {
        for edge in Edge::ALL {
            let (dx, dy) = cube_offset(edge, 1.0);
            assert!(
                BODY_LEFT + dx >= 0.0 && BODY_RIGHT + dx <= ARTBOARD_W,
                "{edge:?}"
            );
            assert!(
                BODY_TOP + dy >= 0.0 && BODY_BOTTOM + dy <= ARTBOARD_H,
                "{edge:?}"
            );
        }
    }

    #[test]
    fn revealed_cube_keeps_the_figma_gap_to_the_edge() {
        let gap = ARTBOARD_H - BODY_BOTTOM;
        let (_, dy) = cube_offset(Edge::Top, 1.0);
        assert!((BODY_TOP + dy - gap).abs() < 0.01);
        let (dx, _) = cube_offset(Edge::Left, 1.0);
        assert!((BODY_LEFT + dx - gap).abs() < 0.01);
        let (dx, _) = cube_offset(Edge::Right, 1.0);
        assert!((ARTBOARD_W - (BODY_RIGHT + dx) - gap).abs() < 0.01);
    }

    #[test]
    fn window_grows_with_cube_size_and_dpi() {
        let (w1, h1) = window_size(200, 1.0);
        let (w2, _) = window_size(400, 1.0);
        let (w3, _) = window_size(200, 2.0);
        assert!(w1 > 200 && h1 > 0);
        assert!((w2 as i32 - 2 * w1 as i32).abs() <= 1);
        assert!((w3 as i32 - 2 * w1 as i32).abs() <= 1);
    }
}
