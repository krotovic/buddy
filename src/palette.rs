//! Frame colours, taken from the Figma page "Framed cube – colours".

use serde::{Deserialize, Serialize};

/// One of the eight frame colours. Coral is the reference colour.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FrameColor {
    Peach,
    #[default]
    Coral,
    Butter,
    Mint,
    Sky,
    Lavender,
    Bubblegum,
    Graphite,
}

/// The colours that change with the frame colour. Everything else
/// (eyes, blush, highlight) is shared by all colours.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Swatch {
    /// Top of the frame gradient (stop at 55%).
    pub frame_light: &'static str,
    /// Bottom of the frame gradient (stop at 100%).
    pub frame_dark: &'static str,
    /// The tinted off-white inside of the cube.
    pub inside: &'static str,
}

impl FrameColor {
    pub const ALL: [FrameColor; 8] = [
        FrameColor::Peach,
        FrameColor::Coral,
        FrameColor::Butter,
        FrameColor::Mint,
        FrameColor::Sky,
        FrameColor::Lavender,
        FrameColor::Bubblegum,
        FrameColor::Graphite,
    ];

    pub fn label(self) -> &'static str {
        match self {
            FrameColor::Peach => "Peach",
            FrameColor::Coral => "Coral",
            FrameColor::Butter => "Butter",
            FrameColor::Mint => "Mint",
            FrameColor::Sky => "Sky",
            FrameColor::Lavender => "Lavender",
            FrameColor::Bubblegum => "Bubblegum",
            FrameColor::Graphite => "Graphite",
        }
    }

    pub fn swatch(self) -> Swatch {
        let (frame_light, frame_dark, inside) = match self {
            FrameColor::Peach => ("#FFC0A3", "#F29C80", "#FFF8F2"),
            FrameColor::Coral => ("#FF9E94", "#EE6F6A", "#FFF7F5"),
            FrameColor::Butter => ("#FFE08A", "#F5BF4F", "#FFFBF0"),
            FrameColor::Mint => ("#A8EBC7", "#6FCF9E", "#F6FFFA"),
            FrameColor::Sky => ("#A8D8FF", "#6FB3EE", "#F5FAFF"),
            FrameColor::Lavender => ("#CDBDFF", "#A18BF0", "#FAF7FF"),
            FrameColor::Bubblegum => ("#FFB8DA", "#F587BB", "#FFF6FA"),
            FrameColor::Graphite => ("#6E6A7C", "#47435A", "#F4F2F7"),
        };
        Swatch {
            frame_light,
            frame_dark,
            inside,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn coral_is_default() {
        assert_eq!(FrameColor::default(), FrameColor::Coral);
    }

    #[test]
    fn all_colours_are_distinct() {
        for (i, a) in FrameColor::ALL.iter().enumerate() {
            for b in &FrameColor::ALL[i + 1..] {
                assert_ne!(a.swatch(), b.swatch(), "{a:?} vs {b:?}");
            }
        }
    }

    #[test]
    fn serialises_lowercase() {
        let s = serde_json::to_string(&FrameColor::Bubblegum).unwrap();
        assert_eq!(s, "\"bubblegum\"");
    }
}
