//! All animations and SVG parts, embedded at build time, and drawing a
//! posed animation into a vello scene.

use std::collections::HashMap;

use vello::Scene;
use vello::kurbo::{Affine, Point, Rect};
use vello::peniko::{Fill, Mix};

use crate::geometry::Edge;
use crate::palette::FrameColor;
use crate::timeline::{Exit, Pose, Timeline};

include!(concat!(env!("OUT_DIR"), "/assets.rs"));

/// Width of the cube's frame in the Figma animation frames, in SVG units.
/// The cube size setting is the on-screen width of this frame.
pub const FRAME_W: f64 = 112.8;

/// The Coral colours as they appear in the exported parts, and which
/// swatch colour replaces each.
const CORAL_LIGHT: &str = "#FF9E94";
const CORAL_DARK: &str = "#EE6F6A";
const CORAL_INSIDE: &str = "#FFF7F5";
/// Arms are drawn a touch darker than the frame.
const CORAL_ARM: &str = "#E86A66";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Kind {
    Idle,
    Done,
    NeedsYou,
}

impl Kind {
    pub fn slug(self) -> &'static str {
        match self {
            Kind::Idle => "idle",
            Kind::Done => "done",
            Kind::NeedsYou => "needs-you",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TimeOfDay {
    Morning,
    Midday,
    Evening,
}

impl TimeOfDay {
    /// As in the plan: morning before 11:00, midday 11:00 to 15:00,
    /// evening after 15:00.
    pub fn from_hour(hour: u32) -> Self {
        match hour {
            0..=10 => TimeOfDay::Morning,
            11..=14 => TimeOfDay::Midday,
            _ => TimeOfDay::Evening,
        }
    }

    pub fn slug(self) -> &'static str {
        match self {
            TimeOfDay::Morning => "morning",
            TimeOfDay::Midday => "midday",
            TimeOfDay::Evening => "evening",
        }
    }
}

fn edge_slug(edge: Edge) -> &'static str {
    match edge {
        Edge::Bottom => "bottom",
        Edge::Top => "top",
        Edge::Left => "left",
        Edge::Right => "right",
    }
}

pub fn animation_name(kind: Kind, time: TimeOfDay, edge: Edge) -> String {
    format!("{}-{}-{}", kind.slug(), time.slug(), edge_slug(edge))
}

struct Part {
    tree: vello_svg::usvg::Tree,
    size: (f64, f64),
}

pub struct Animation {
    pub timeline: Timeline,
    /// Part index for each node, for leaves.
    parts: Vec<Option<usize>>,
    pub loop_start: Option<f32>,
}

pub struct Library {
    parts: Vec<Part>,
    animations: HashMap<String, Animation>,
    /// Region of the animation frame the window shows, per edge.
    crops: HashMap<Edge, Rect>,
}

/// A part's SVG with the Coral frame colours swapped for `color`.
pub fn recolor(svg: &str, color: FrameColor) -> String {
    let s = color.swatch();
    svg.replace(CORAL_LIGHT, s.frame_light)
        .replace(CORAL_DARK, s.frame_dark)
        .replace(CORAL_ARM, s.frame_dark)
        .replace(CORAL_INSIDE, s.inside)
}

fn parse_parts(color: FrameColor) -> anyhow::Result<Vec<Part>> {
    let opt = vello_svg::usvg::Options::default();
    PARTS
        .iter()
        .map(|(name, svg)| {
            let tree = vello_svg::usvg::Tree::from_str(&recolor(svg, color), &opt)
                .map_err(|e| anyhow::anyhow!("part {name}: {e}"))?;
            let size = (tree.size().width() as f64, tree.size().height() as f64);
            Ok(Part { tree, size })
        })
        .collect()
}

impl Library {
    pub fn load(color: FrameColor) -> anyhow::Result<Self> {
        let parts = parse_parts(color)?;
        let index: HashMap<&str, usize> = PARTS
            .iter()
            .enumerate()
            .map(|(i, (n, _))| (*n, i))
            .collect();
        let mut animations = HashMap::new();
        for (name, json) in ANIMATIONS {
            let timeline: Timeline =
                serde_json::from_str(json).map_err(|e| anyhow::anyhow!("animation {name}: {e}"))?;
            let parts = timeline
                .nodes
                .iter()
                .map(|n| {
                    n.svg.as_deref().map(|h| {
                        index
                            .get(h)
                            .copied()
                            .ok_or_else(|| anyhow::anyhow!("{name}: missing part {h}"))
                    })
                })
                .map(Option::transpose)
                .collect::<anyhow::Result<Vec<_>>>()?;
            let loop_start = name
                .starts_with("needs-you")
                .then(|| timeline.loop_start())
                .flatten();
            animations.insert(
                name.to_string(),
                Animation {
                    timeline,
                    parts,
                    loop_start,
                },
            );
        }
        let mut lib = Self {
            parts,
            animations,
            crops: HashMap::new(),
        };
        for edge in Edge::ALL {
            let crop = lib.content_bounds(edge);
            lib.crops.insert(edge, crop);
        }
        Ok(lib)
    }

    /// Swap the frame colour of every part.
    pub fn set_color(&mut self, color: FrameColor) -> anyhow::Result<()> {
        self.parts = parse_parts(color)?;
        Ok(())
    }

    pub fn get(&self, kind: Kind, time: TimeOfDay, edge: Edge) -> Option<&Animation> {
        self.animations.get(&animation_name(kind, time, edge))
    }

    /// The part of the animation frame the window shows for `edge`, in SVG
    /// units: everything any animation for that edge draws, within the
    /// frame. The side on the screen edge is always the frame border.
    pub fn crop(&self, edge: Edge) -> Rect {
        self.crops[&edge]
    }

    fn content_bounds(&self, edge: Edge) -> Rect {
        let mut bounds: Option<Rect> = None;
        let mut frame = Rect::ZERO;
        let suffix = format!("-{}", edge_slug(edge));
        for (_, anim) in self.animations.iter().filter(|(n, _)| n.ends_with(&suffix)) {
            frame = Rect::new(0.0, 0.0, anim.timeline.width, anim.timeline.height);
            let steps = (anim.timeline.duration * 20.0).ceil() as usize;
            for i in 0..=steps {
                let t = anim.timeline.duration * i as f32 / steps as f32;
                let pose = anim.timeline.pose(t);
                for (j, part) in anim.parts.iter().enumerate() {
                    let (Some(p), true) = (part, pose.alpha[j] > 0.0) else {
                        continue;
                    };
                    let node = &anim.timeline.nodes[j];
                    let (w, h) = self.parts[*p].size;
                    let m = pose.transform[j] * Affine::translate((node.x, node.y));
                    for c in [(0.0, 0.0), (w, 0.0), (0.0, h), (w, h)] {
                        let pt = m * Point::new(c.0, c.1);
                        let r = Rect::from_points(pt, pt);
                        bounds = Some(bounds.map_or(r, |b| b.union(r)));
                    }
                }
            }
        }
        let b = bounds.unwrap_or(frame).inflate(2.0, 2.0).intersect(frame);
        // Snap to whole units so the window size does not jitter.
        Rect::new(b.x0.floor(), b.y0.floor(), b.x1.ceil(), b.y1.ceil())
    }

    /// Draw `anim` posed at `t` seconds. `view` maps frame units to window
    /// pixels.
    pub fn draw(
        &self,
        scene: &mut Scene,
        anim: &Animation,
        t: f32,
        exit: Option<Exit>,
        view: Affine,
    ) {
        let pose: Pose = anim.timeline.pose_with(t, exit);
        for (j, part) in anim.parts.iter().enumerate() {
            let Some(p) = part else { continue };
            let alpha = pose.alpha[j];
            if alpha <= 0.002 {
                continue;
            }
            let node = &anim.timeline.nodes[j];
            let part = &self.parts[*p];
            let m = view * pose.transform[j] * Affine::translate((node.x, node.y));
            if alpha < 0.998 {
                let clip = Rect::new(0.0, 0.0, part.size.0, part.size.1);
                scene.push_layer(Fill::NonZero, Mix::Normal, alpha, m, &clip);
                vello_svg::append_tree_with_transform(scene, &part.tree, m);
                scene.pop_layer();
            } else {
                vello_svg::append_tree_with_transform(scene, &part.tree, m);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_animation_exists_for_every_edge_and_time() {
        let lib = Library::load(FrameColor::Coral).unwrap();
        assert_eq!(lib.parts.len(), PARTS.len());
        for kind in [Kind::Idle, Kind::Done, Kind::NeedsYou] {
            for time in [TimeOfDay::Morning, TimeOfDay::Midday, TimeOfDay::Evening] {
                for edge in Edge::ALL {
                    let a = lib
                        .get(kind, time, edge)
                        .unwrap_or_else(|| panic!("{}", animation_name(kind, time, edge)));
                    assert_eq!(a.loop_start.is_some(), kind == Kind::NeedsYou);
                }
            }
        }
    }

    #[test]
    fn crops_touch_the_screen_edge_and_hold_the_cube() {
        let lib = Library::load(FrameColor::Coral).unwrap();
        let b = lib.crop(Edge::Bottom);
        assert_eq!(b.y1, 300.0);
        let t = lib.crop(Edge::Top);
        assert_eq!(t.y0, 0.0);
        let l = lib.crop(Edge::Left);
        assert_eq!(l.x0, 0.0);
        let r = lib.crop(Edge::Right);
        assert_eq!(r.x1, 480.0);
        // The cube's frame is 112.8 wide; the crop must be larger, and much
        // smaller than the whole frame across the edge.
        assert!(b.width() > FRAME_W && b.height() < 200.0, "{b:?}");
        assert!(l.height() > FRAME_W && l.width() < 200.0, "{l:?}");
    }

    #[test]
    fn every_colour_recolours_every_part() {
        for color in FrameColor::ALL {
            let parts = parse_parts(color).unwrap();
            assert_eq!(parts.len(), PARTS.len());
        }
        let mint = recolor(CORAL_ARM, FrameColor::Mint);
        assert_eq!(mint, FrameColor::Mint.swatch().frame_dark);
    }

    /// Renders contact sheets of every animation to target/frames/ with
    /// resvg, to eyeball the player without a GPU:
    /// `cargo test contact_sheets -- --ignored`
    #[test]
    #[ignore]
    fn contact_sheets() {
        let lib = Library::load(FrameColor::Coral).unwrap();
        let out = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("target/frames");
        std::fs::create_dir_all(&out).unwrap();
        let svg_of: HashMap<&str, &str> = PARTS.iter().copied().collect();
        let mut names: Vec<_> = lib.animations.keys().cloned().collect();
        names.sort();
        let cols = 8;
        for name in names {
            let anim = &lib.animations[&name];
            let tl = &anim.timeline;
            let edge = Edge::ALL
                .into_iter()
                .find(|e| name.ends_with(edge_slug(*e)))
                .unwrap();
            let crop = lib.crop(edge);
            let (cw, ch) = (crop.width(), crop.height());
            let mut body = String::new();
            for i in 0..cols {
                let t = tl.duration * i as f32 / cols as f32;
                let pose = tl.pose(t);
                let ox = i as f64 * (cw + 10.0);
                body += &format!(
                    "<rect x='{ox}' y='0' width='{cw}' height='{ch}' fill='#3a3f4b'/>\
                     <svg x='{ox}' y='0' width='{cw}' height='{ch}' viewBox='{} {} {cw} {ch}'>",
                    crop.x0, crop.y0
                );
                for (j, part) in anim.parts.iter().enumerate() {
                    if part.is_none() || pose.alpha[j] <= 0.0 {
                        continue;
                    }
                    let node = &tl.nodes[j];
                    let m = (pose.transform[j] * Affine::translate((node.x, node.y))).as_coeffs();
                    let inner = svg_of[node.svg.as_deref().unwrap()];
                    body += &format!(
                        "<g opacity='{}' transform='matrix({} {} {} {} {} {})'>{inner}</g>",
                        pose.alpha[j], m[0], m[1], m[2], m[3], m[4], m[5]
                    );
                }
                body += "</svg>";
            }
            let w = cols as f64 * (cw + 10.0);
            let doc = format!(
                "<svg xmlns='http://www.w3.org/2000/svg' width='{w}' height='{ch}'>{body}</svg>"
            );
            let tree = resvg::usvg::Tree::from_str(&doc, &resvg::usvg::Options::default())
                .unwrap_or_else(|e| panic!("{name}: {e}"));
            let mut pixmap = resvg::tiny_skia::Pixmap::new(w as u32, ch as u32).unwrap();
            resvg::render(&tree, Default::default(), &mut pixmap.as_mut());
            pixmap.save_png(out.join(format!("{name}.png"))).unwrap();
        }
    }

    #[test]
    fn time_of_day_boundaries() {
        assert_eq!(TimeOfDay::from_hour(3), TimeOfDay::Morning);
        assert_eq!(TimeOfDay::from_hour(10), TimeOfDay::Morning);
        assert_eq!(TimeOfDay::from_hour(11), TimeOfDay::Midday);
        assert_eq!(TimeOfDay::from_hour(15), TimeOfDay::Evening);
    }
}
