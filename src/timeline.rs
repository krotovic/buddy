//! Figma motion timelines, as exported by tools/figma-export.js, and how to
//! evaluate them at a point in time.
//!
//! An animation is a flat, paint-ordered list of nodes. Each node knows its
//! parent, its centre at rest (the pivot for rotation and scale), and its
//! keyframe tracks. Leaves carry an SVG part drawn with its top-left corner
//! at (x, y). All coordinates are in the animation frame, whose borders are
//! the screen edges.
//!
//! Figma semantics, as checked against the design file:
//! - the easing stored on a keyframe shapes the segment that ends at it;
//! - HOLD keeps the previous value and jumps when the keyframe is reached;
//! - translation is an offset from the rest position;
//! - rotation is in degrees, counter-clockwise on screen;
//! - before the first keyframe the first value holds, after the last the
//!   last value holds.

use serde::Deserialize;
use vello::kurbo::Affine;

#[derive(Debug, Clone, Deserialize)]
pub struct Timeline {
    pub name: String,
    /// Figma node id of the animation frame, for finding it in the file.
    #[allow(dead_code)]
    pub figma_node: String,
    pub width: f64,
    pub height: f64,
    /// Seconds.
    pub duration: f32,
    pub nodes: Vec<Node>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Node {
    /// Layer name in Figma.
    #[serde(rename = "n")]
    #[cfg_attr(not(test), allow(dead_code))]
    pub name: String,
    /// Index of the parent node, -1 for a top-level node.
    #[serde(rename = "p")]
    pub parent: i32,
    #[serde(default)]
    pub cx: f64,
    #[serde(default)]
    pub cy: f64,
    #[serde(rename = "t", default)]
    pub tracks: Tracks,
    /// Opacity at rest when there is no opacity track.
    #[serde(rename = "o", default = "one")]
    pub opacity: f32,
    /// Hash of the SVG part, for leaves.
    pub svg: Option<String>,
    #[serde(default)]
    pub x: f64,
    #[serde(default)]
    pub y: f64,
}

fn one() -> f32 {
    1.0
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct Tracks {
    pub tx: Option<Track>,
    pub ty: Option<Track>,
    pub rot: Option<Track>,
    /// Uniform scale.
    pub s: Option<Track>,
    pub sx: Option<Track>,
    pub sy: Option<Track>,
    pub o: Option<Track>,
}

pub type Track = Vec<Key>;

/// One keyframe: time in seconds, value, and the easing of the segment
/// that ends here.
#[derive(Debug, Clone, Deserialize)]
pub struct Key(pub f32, pub f32, pub Easing);

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum Easing {
    /// "h": hold.
    Hold(#[allow(dead_code)] String),
    /// x1, y1, x2, y2 of a CSS-style cubic bezier.
    Bezier([f32; 4]),
    Spring {
        spring: f32,
    },
}

impl Easing {
    /// Eased progress for linear progress `u` in 0.0–1.0.
    pub fn apply(&self, u: f32) -> f32 {
        let u = u.clamp(0.0, 1.0);
        match self {
            Easing::Hold(_) => {
                if u >= 1.0 {
                    1.0
                } else {
                    0.0
                }
            }
            Easing::Bezier([x1, y1, x2, y2]) => cubic_bezier(*x1, *y1, *x2, *y2, u),
            // No spring easing is used in the design yet; a damped
            // oscillation that settles at 1 stands in for it.
            Easing::Spring { spring } => {
                let bounce = spring.clamp(0.0, 0.9);
                let decay = 6.0 * (1.0 - bounce) + 2.0;
                1.0 - (-decay * u).exp() * (std::f32::consts::PI * 2.5 * u).cos() * (1.0 - u)
            }
        }
    }
}

/// y of the cubic bezier from (0,0) to (1,1) with control points (x1,y1)
/// and (x2,y2), at the point whose x is `x`.
pub fn cubic_bezier(x1: f32, y1: f32, x2: f32, y2: f32, x: f32) -> f32 {
    let bez = |a: f32, b: f32, s: f32| {
        let m = 1.0 - s;
        3.0 * m * m * s * a + 3.0 * m * s * s * b + s * s * s
    };
    let dbez = |a: f32, b: f32, s: f32| {
        let m = 1.0 - s;
        3.0 * m * m * a + 6.0 * m * s * (b - a) + 3.0 * s * s * (1.0 - b)
    };
    // Newton first, bisection if it does not converge (flat spots).
    let mut s = x;
    for _ in 0..8 {
        let err = bez(x1, x2, s) - x;
        if err.abs() < 1e-5 {
            return bez(y1, y2, s);
        }
        let d = dbez(x1, x2, s);
        if d.abs() < 1e-6 {
            break;
        }
        s = (s - err / d).clamp(0.0, 1.0);
    }
    let (mut lo, mut hi) = (0.0f32, 1.0f32);
    s = x;
    for _ in 0..30 {
        let v = bez(x1, x2, s);
        if (v - x).abs() < 1e-5 {
            break;
        }
        if v < x {
            lo = s;
        } else {
            hi = s;
        }
        s = (lo + hi) / 2.0;
    }
    bez(y1, y2, s)
}

/// Value of a track at time `t`.
pub fn sample(track: &[Key], t: f32) -> f32 {
    let Some(first) = track.first() else {
        return 0.0;
    };
    if t <= first.0 {
        return first.1;
    }
    for w in track.windows(2) {
        let (a, b) = (&w[0], &w[1]);
        if t < b.0 {
            let span = b.0 - a.0;
            if span <= 0.0 {
                return b.1;
            }
            let e = b.2.apply((t - a.0) / span);
            return a.1 + (b.1 - a.1) * e;
        }
    }
    track.last().map(|k| k.1).unwrap_or(0.0)
}

fn sample_or(track: &Option<Track>, t: f32, rest: f32) -> f32 {
    track.as_deref().map(|k| sample(k, t)).unwrap_or(rest)
}

/// Where every node is at one moment: its transform into frame coordinates
/// and its opacity (including its parents').
#[derive(Debug, Clone, Default)]
pub struct Pose {
    pub transform: Vec<Affine>,
    pub alpha: Vec<f32>,
}

/// Overrides for leaving a looping animation early: top-level nodes that
/// move are pulled back to where they start, by `progress` 0.0–1.0, and
/// top-level nodes that do not move take their starting state at the end.
#[derive(Debug, Clone, Copy)]
pub struct Exit {
    pub progress: f32,
}

impl Timeline {
    /// Evaluate the timeline at `t` seconds.
    pub fn pose(&self, t: f32) -> Pose {
        self.pose_with(t, None)
    }

    pub fn pose_with(&self, t: f32, exit: Option<Exit>) -> Pose {
        let n = self.nodes.len();
        let mut pose = Pose {
            transform: Vec::with_capacity(n),
            alpha: Vec::with_capacity(n),
        };
        for node in &self.nodes {
            let tr = &node.tracks;
            let mut tx = sample_or(&tr.tx, t, 0.0) as f64;
            let mut ty = sample_or(&tr.ty, t, 0.0) as f64;
            let mut alpha = sample_or(&tr.o, t, node.opacity);
            if let (Some(exit), true) = (exit, node.parent < 0) {
                let moves = tr.tx.is_some() || tr.ty.is_some();
                if moves {
                    let k = crate::timeline::ease_in(exit.progress) as f64;
                    tx += (sample_or(&tr.tx, 0.0, 0.0) as f64 - tx) * k;
                    ty += (sample_or(&tr.ty, 0.0, 0.0) as f64 - ty) * k;
                } else if exit.progress >= 0.85 {
                    alpha = sample_or(&tr.o, 0.0, node.opacity);
                }
            }
            let rot = sample_or(&tr.rot, t, 0.0) as f64;
            let s = sample_or(&tr.s, t, 1.0) as f64;
            let sx = s * sample_or(&tr.sx, t, 1.0) as f64;
            let sy = s * sample_or(&tr.sy, t, 1.0) as f64;
            let c = (node.cx, node.cy);
            let local = Affine::translate((tx, ty))
                * Affine::translate(c)
                * Affine::rotate(-rot.to_radians())
                * Affine::scale_non_uniform(sx, sy)
                * Affine::translate((-c.0, -c.1));
            let (parent_t, parent_a) = match usize::try_from(node.parent) {
                Ok(p) if p < pose.transform.len() => (pose.transform[p], pose.alpha[p]),
                _ => (Affine::IDENTITY, 1.0),
            };
            pose.transform.push(parent_t * local);
            pose.alpha.push(parent_a * alpha.clamp(0.0, 1.0));
        }
        pose
    }

    /// For looping animations (Needs you): the time the loop goes back to.
    /// It is where the main moving node first settles on the value it ends
    /// on, so playing from there to the end and jumping back is seamless.
    pub fn loop_start(&self) -> Option<f32> {
        let track = self
            .nodes
            .iter()
            .filter(|n| n.parent < 0)
            .find_map(|n| n.tracks.ty.as_ref().or(n.tracks.tx.as_ref()))?;
        let last = track.last()?;
        let mut i = track.len() - 1;
        // Walk back over keyframes with the final value...
        while i > 0 && (track[i - 1].1 - last.1).abs() < 0.01 {
            i -= 1;
        }
        // ...to the last stretch before it that differs, then back over the
        // earlier stretch that holds the same value as the end.
        while i > 0 && (track[i - 1].1 - last.1).abs() >= 0.01 {
            i -= 1;
        }
        while i > 0 && (track[i - 1].1 - last.1).abs() < 0.01 {
            i -= 1;
        }
        (i > 0 && track[i].0 < self.duration).then_some(track[i].0)
    }
}

pub fn ease_in(u: f32) -> f32 {
    let u = u.clamp(0.0, 1.0);
    u * u * u
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(t: f32, v: f32, e: Easing) -> Key {
        Key(t, v, e)
    }
    fn linear() -> Easing {
        Easing::Bezier([0.0, 0.0, 1.0, 1.0])
    }

    #[test]
    fn bezier_end_points_and_linear() {
        for &(x1, y1, x2, y2) in &[(0.42, 0.0, 0.58, 1.0), (0.45, 1.45, 0.8, 1.0)] {
            assert!(cubic_bezier(x1, y1, x2, y2, 0.0).abs() < 1e-4);
            assert!((cubic_bezier(x1, y1, x2, y2, 1.0) - 1.0).abs() < 1e-4);
        }
        assert!((cubic_bezier(0.0, 0.0, 1.0, 1.0, 0.3) - 0.3).abs() < 1e-3);
        // ease-in-out is symmetric
        assert!((cubic_bezier(0.42, 0.0, 0.58, 1.0, 0.5) - 0.5).abs() < 1e-3);
    }

    #[test]
    fn back_easing_overshoots() {
        let peak = (0..100)
            .map(|i| cubic_bezier(0.45, 1.45, 0.8, 1.0, i as f32 / 100.0))
            .fold(0.0f32, f32::max);
        assert!(peak > 1.05, "{peak}");
    }

    #[test]
    fn easing_belongs_to_the_segment_ending_at_the_key() {
        let track = vec![
            key(0.0, 0.0, Easing::Hold("h".into())),
            key(1.0, 10.0, linear()),
            key(2.0, 20.0, Easing::Hold("h".into())),
        ];
        assert_eq!(sample(&track, -1.0), 0.0);
        assert!((sample(&track, 0.5) - 5.0).abs() < 1e-3);
        // second segment holds 10 until it reaches 2.0
        assert_eq!(sample(&track, 1.99), 10.0);
        assert_eq!(sample(&track, 2.0), 20.0);
        assert_eq!(sample(&track, 9.0), 20.0);
    }

    fn timeline(name: &str) -> Timeline {
        let (_, json) = crate::library::ANIMATIONS
            .iter()
            .find(|(n, _)| *n == name)
            .unwrap();
        serde_json::from_str(json).unwrap()
    }

    #[test]
    fn all_animations_parse() {
        for (name, json) in crate::library::ANIMATIONS {
            let tl: Timeline = serde_json::from_str(json).unwrap_or_else(|e| panic!("{name}: {e}"));
            assert!(tl.duration > 1.0, "{name}");
            assert!(tl.nodes.iter().all(|n| n.parent < tl.nodes.len() as i32));
            for n in &tl.nodes {
                let t = &n.tracks;
                for tr in [&t.tx, &t.ty, &t.rot, &t.s, &t.sx, &t.sy, &t.o]
                    .into_iter()
                    .flatten()
                {
                    assert!(tr.windows(2).all(|w| w[0].0 <= w[1].0), "{name}");
                }
            }
        }
        assert_eq!(crate::library::ANIMATIONS.len(), 36);
    }

    #[test]
    fn idle_starts_and_ends_hidden_and_is_out_in_between() {
        let tl = timeline("idle-midday-bottom");
        let cube = tl.nodes.iter().position(|n| n.name == "Cube").unwrap();
        let y = |t: f32| tl.pose(t).transform[cube].translation().y;
        assert!(y(0.0) > 80.0);
        assert!(y(3.0).abs() < 0.5);
        assert!(y(tl.duration) > 80.0);
    }

    #[test]
    fn faces_swap_with_hold_keyframes() {
        let tl = timeline("idle-midday-bottom");
        let idx = |name: &str| tl.nodes.iter().position(|n| n.name == name).unwrap();
        let (idle, look) = (idx("Face – idle"), idx("Face – lookL"));
        let p = tl.pose(1.0);
        assert_eq!((p.alpha[idle], p.alpha[look]), (1.0, 0.0));
        let p = tl.pose(2.0);
        assert_eq!((p.alpha[idle], p.alpha[look]), (0.0, 1.0));
        // children inherit the face's opacity
        assert_eq!(p.alpha[look + 1], 1.0);
        assert_eq!(p.alpha[idle + 1], 0.0);
    }

    #[test]
    fn arms_pivot_on_their_centre() {
        let tl = timeline("done-midday-bottom");
        let arm = tl.nodes.iter().position(|n| n.name == "Arm right").unwrap();
        let node = &tl.nodes[arm];
        let p = tl.pose(2.42); // rotated 22°, scale 1
        let centre = p.transform[arm] * vello::kurbo::Point::new(node.cx, node.cy);
        let cube = p.transform[0] * vello::kurbo::Point::new(node.cx, node.cy);
        assert!((centre - cube).hypot() < 0.01);
    }

    #[test]
    fn needs_you_loops_back_to_the_half_visible_wave() {
        assert_eq!(timeline("needs-you-midday-bottom").loop_start(), Some(4.0));
        assert_eq!(timeline("needs-you-morning-left").loop_start(), Some(5.4));
        assert_eq!(timeline("needs-you-evening-top").loop_start(), Some(7.2));
        for name in ["needs-you-midday-right", "needs-you-evening-bottom"] {
            let tl = timeline(name);
            let ls = tl.loop_start().unwrap();
            let (a, b) = (tl.pose(ls), tl.pose(tl.duration));
            for i in 0..tl.nodes.len() {
                assert!((a.alpha[i] - b.alpha[i]).abs() < 1e-3, "{name} {i}");
                let d = a.transform[i].translation() - b.transform[i].translation();
                assert!(d.hypot() < 0.01, "{name} {i}");
            }
        }
    }

    #[test]
    fn exit_hides_the_cube() {
        let tl = timeline("needs-you-midday-bottom");
        let p = tl.pose_with(6.0, Some(Exit { progress: 1.0 }));
        assert!(p.transform[0].translation().y > 80.0);
        let hand = tl
            .nodes
            .iter()
            .position(|n| n.name == "Hand holding edge")
            .unwrap();
        assert_eq!(p.alpha[hand], 0.0);
    }
}

/// Where in the timeline playback is, `elapsed` seconds after it started.
/// A looping animation plays once to the end, then repeats from
/// `loop_start`.
pub fn playhead(elapsed: f32, duration: f32, loop_start: Option<f32>) -> f32 {
    match loop_start {
        Some(ls) if elapsed >= duration && duration - ls > 0.05 => {
            ls + (elapsed - duration) % (duration - ls)
        }
        _ => elapsed,
    }
}

#[cfg(test)]
mod playhead_tests {
    use super::playhead;

    #[test]
    fn plays_through_then_loops() {
        assert_eq!(playhead(3.0, 8.0, Some(4.0)), 3.0);
        assert_eq!(playhead(8.0, 8.0, Some(4.0)), 4.0);
        assert!((playhead(13.0, 8.0, Some(4.0)) - 5.0).abs() < 1e-4);
        assert_eq!(playhead(9.0, 8.0, None), 9.0);
    }
}
