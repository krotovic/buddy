//! The still idle cube, used for the tray icon.
//!
//! The SVG in `assets/figma` is exported from the Figma design file and
//! stays untouched. The frame colour is swapped in the SVG text before it
//! is parsed, so one file covers all eight colours.

// Only the tray (Windows and macOS) uses it.
#![cfg_attr(not(any(target_os = "windows", target_os = "macos")), allow(dead_code))]

use crate::palette::FrameColor;

/// Coral idle cube, exported from Figma node 5:55 ("Coral – idle").
const IDLE_SVG: &str = include_str!("../assets/figma/coral-idle.svg");

/// The Coral swatch as it appears in the exported files.
const CORAL_LIGHT: &str = "#FF9E94";
const CORAL_DARK: &str = "#EE6F6A";
const CORAL_INSIDE: &str = "#FFF7F5";

/// Bounds of the cube body (the frame) inside the artboard, in SVG units.
pub const BODY_LEFT: f32 = 61.92;
pub const BODY_TOP: f32 = 47.52;
pub const BODY_RIGHT: f32 = 197.28;
pub const BODY_BOTTOM: f32 = 174.24;
pub const BODY_W: f32 = BODY_RIGHT - BODY_LEFT;

/// The idle cube in the given frame colour, without the floor shadow
/// (the cube peeks from a screen edge, so there is no floor).
pub fn idle_svg(color: FrameColor) -> String {
    let swatch = color.swatch();
    IDLE_SVG
        .lines()
        .filter(|line| !line.contains("id=\"shadow\""))
        .collect::<Vec<_>>()
        .join("\n")
        .replace(CORAL_LIGHT, swatch.frame_light)
        .replace(CORAL_DARK, swatch.frame_dark)
        .replace(CORAL_INSIDE, swatch.inside)
}

/// Rasterise the idle cube into straight (non-premultiplied) RGBA pixels,
/// cropped to the cube body. Used for the tray icon.
pub fn icon_rgba(color: FrameColor, size: u32) -> anyhow::Result<Vec<u8>> {
    // resvg has its own usvg version, so parse again with it.
    let opt = resvg::usvg::Options::default();
    let tree = resvg::usvg::Tree::from_str(&idle_svg(color), &opt)?;
    let mut pixmap = resvg::tiny_skia::Pixmap::new(size, size)
        .ok_or_else(|| anyhow::anyhow!("bad icon size"))?;
    let body_h = BODY_BOTTOM - BODY_TOP;
    let scale = size as f32 / BODY_W.max(body_h);
    let dx = -BODY_LEFT * scale + (size as f32 - BODY_W * scale) / 2.0;
    let dy = -BODY_TOP * scale + (size as f32 - body_h * scale) / 2.0;
    let transform = resvg::tiny_skia::Transform::from_row(scale, 0.0, 0.0, scale, dx, dy);
    resvg::render(&tree, transform, &mut pixmap.as_mut());
    Ok(pixmap
        .pixels()
        .iter()
        .flat_map(|p| {
            let c = p.demultiply();
            [c.red(), c.green(), c.blue(), c.alpha()]
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_colour_parses() {
        for color in FrameColor::ALL {
            let svg = idle_svg(color);
            let tree = resvg::usvg::Tree::from_str(&svg, &Default::default()).unwrap();
            assert_eq!(tree.size().width(), 260.0);
            assert!(svg.contains(color.swatch().frame_dark));
        }
    }

    #[test]
    fn non_coral_colours_replace_coral() {
        let svg = idle_svg(FrameColor::Mint);
        assert!(!svg.contains(CORAL_LIGHT));
        assert!(!svg.contains(CORAL_DARK));
        assert!(!svg.contains(CORAL_INSIDE));
    }

    #[test]
    fn shadow_is_removed() {
        assert!(IDLE_SVG.contains("id=\"shadow\""));
        assert!(!idle_svg(FrameColor::Coral).contains("id=\"shadow\""));
    }

    #[test]
    fn icon_has_opaque_centre_and_clear_corner() {
        let size = 32;
        let px = icon_rgba(FrameColor::Coral, size).unwrap();
        assert_eq!(px.len(), (size * size * 4) as usize);
        let at = |x: u32, y: u32| px[((y * size + x) * 4 + 3) as usize];
        assert_eq!(at(16, 16), 255);
        assert_eq!(at(0, 0), 0);
    }
}
