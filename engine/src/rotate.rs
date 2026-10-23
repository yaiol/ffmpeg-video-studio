//! Rotation: turn or flip the picture. A fixed set of eight choices rather than a list the user edits - there
//! is nothing to name or tune - chosen in the window apart from the preset, like the size: the same
//! turn goes with any codec.
//!
//! The filter runs after the colour LUT and before the size, so a size limit applies to the picture as it
//! is delivered (a portrait box on a turned picture). A copied video stream cannot be turned: ignored.

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Rotation {
    #[default]
    None,
    /// 90 degrees clockwise.
    Cw,
    /// 90 degrees counter-clockwise.
    Ccw,
    /// 90 degrees clockwise, then flipped vertically (ffmpeg's `transpose=3`).
    CwFlip,
    /// 90 degrees counter-clockwise, then flipped vertically (ffmpeg's `transpose=0`).
    CcwFlip,
    /// A half turn.
    Half,
    /// Mirrored left to right.
    FlipH,
    /// Mirrored top to bottom.
    FlipV,
}

impl Rotation {
    /// Every choice, in the order the window lists them.
    pub const ALL: [Rotation; 8] =
        [Rotation::None, Rotation::Cw, Rotation::Ccw, Rotation::CwFlip, Rotation::CcwFlip, Rotation::Half, Rotation::FlipH, Rotation::FlipV];

    /// A stable text for the window's drop-down values.
    pub fn id(self) -> &'static str {
        match self {
            Rotation::None => "none",
            Rotation::Cw => "cw",
            Rotation::Ccw => "ccw",
            Rotation::CwFlip => "cwflip",
            Rotation::CcwFlip => "ccwflip",
            Rotation::Half => "half",
            Rotation::FlipH => "fliph",
            Rotation::FlipV => "flipv",
        }
    }

    pub fn from_id(id: &str) -> Option<Rotation> {
        Rotation::ALL.into_iter().find(|r| r.id() == id)
    }

    /// The ffmpeg video filter, or None when the picture stays as it is.
    pub fn filter(self) -> Option<&'static str> {
        match self {
            Rotation::None => None,
            Rotation::Cw => Some("transpose=1"),
            Rotation::Ccw => Some("transpose=2"),
            Rotation::CwFlip => Some("transpose=3"),
            Rotation::CcwFlip => Some("transpose=0"),
            Rotation::Half => Some("hflip,vflip"),
            Rotation::FlipH => Some("hflip"),
            Rotation::FlipV => Some("vflip"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_four_transposes_are_the_ones_ffmpeg_documents() {
        // 0 = counter-clockwise and vertical flip, 1 = clockwise, 2 = counter-clockwise, 3 = clockwise and vertical flip.
        assert_eq!(Rotation::CcwFlip.filter(), Some("transpose=0"));
        assert_eq!(Rotation::Cw.filter(), Some("transpose=1"));
        assert_eq!(Rotation::Ccw.filter(), Some("transpose=2"));
        assert_eq!(Rotation::CwFlip.filter(), Some("transpose=3"));
    }

    #[test]
    fn nothing_adds_no_filter_and_every_id_round_trips() {
        assert_eq!(Rotation::None.filter(), None);
        for r in Rotation::ALL {
            assert_eq!(Rotation::from_id(r.id()), Some(r));
        }
        assert_eq!(Rotation::from_id("sideways"), None);
    }
}
