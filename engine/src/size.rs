//! A size: the largest picture a run may produce. A named entry the user keeps in Settings and picks in
//! the window, apart from the preset - the same picture size goes with any codec, so it is not a
//! property of one.
//!
//! The picture is shrunk to fit inside the box, keeping its shape, and never enlarged. A side left
//! empty is not limited.

use serde::{Deserialize, Serialize};

/// Id of the window-only choice that leaves the size alone. Not an entry of Settings; the window names it.
pub const ORIGINAL_ID: &str = "original";

/// The smallest and largest side a size can name.
pub const MIN_SIDE: u32 = 64;
pub const MAX_SIDE: u32 = 8192;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Size {
    pub id: String,
    pub name: String,
    pub max_width: Option<u32>,
    pub max_height: Option<u32>,
}

impl Default for Size {
    fn default() -> Self {
        Size { id: String::new(), name: String::new(), max_width: None, max_height: None }
    }
}

impl Size {
    /// The window-only "(Original)": no limit at all.
    pub fn original() -> Size {
        Size { id: ORIGINAL_ID.to_owned(), ..Size::default() }
    }

    pub fn new(name: &str) -> Size {
        Size { id: crate::preset::new_id(), name: name.to_owned(), ..Size::default() }
    }

    pub fn is_original(&self) -> bool {
        self.id == ORIGINAL_ID
    }

    /// The same size with each side inside what a picture can have (an empty or zero side is no limit).
    pub fn normalized(mut self) -> Size {
        let fix = |v: Option<u32>| v.filter(|n| *n > 0).map(|n| n.clamp(MIN_SIDE, MAX_SIDE));
        self.max_width = fix(self.max_width);
        self.max_height = fix(self.max_height);
        self
    }

    /// The ffmpeg video filter that applies the size, or None when it limits nothing. One expression
    /// does all three jobs - shrink to fit, never enlarge (`min` against the source's own side), keep
    /// both sides even - so it needs no knowledge of the source's dimensions.
    pub fn scale_filter(&self) -> Option<String> {
        let side = |max: Option<u32>, own: &str| match max {
            Some(n) => format!("'min({own},{n})'"),
            None => own.to_owned(),
        };
        if self.max_width.is_none() && self.max_height.is_none() {
            return None;
        }
        Some(format!(
            "scale=w={}:h={}:force_original_aspect_ratio=decrease:force_divisible_by=2",
            side(self.max_width, "iw"),
            side(self.max_height, "ih")
        ))
    }
}

/// What a fresh install starts with, with stable ids so a saved selection survives. Heights only: the
/// width follows from the picture's shape.
pub fn defaults() -> Vec<Size> {
    [("4K (2160p)", 2160), ("1440p", 1440), ("1080p", 1080), ("720p", 720), ("480p", 480), ("360p", 360)]
        .into_iter()
        .map(|(name, h)| Size { id: format!("default-{h}p"), name: name.to_owned(), max_width: None, max_height: Some(h) })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_original_limits_nothing() {
        assert_eq!(Size::original().scale_filter(), None);
        assert!(Size::original().is_original());
    }

    #[test]
    fn a_height_only_leaves_the_width_to_the_shape() {
        let s = Size { max_height: Some(720), ..Size::new("t") };
        assert_eq!(
            s.scale_filter().unwrap(),
            "scale=w=iw:h='min(ih,720)':force_original_aspect_ratio=decrease:force_divisible_by=2"
        );
    }

    #[test]
    fn both_sides_make_a_box() {
        let s = Size { max_width: Some(1280), max_height: Some(720), ..Size::new("t") };
        let f = s.scale_filter().unwrap();
        assert!(f.contains("w='min(iw,1280)'") && f.contains("h='min(ih,720)'"), "{f}");
    }

    #[test]
    fn sides_are_kept_inside_what_a_picture_can_have() {
        let s = Size { max_width: Some(0), max_height: Some(7), ..Size::new("t") }.normalized();
        assert_eq!((s.max_width, s.max_height), (None, Some(MIN_SIDE)));
        let s = Size { max_width: Some(99_999), ..Size::new("t") }.normalized();
        assert_eq!(s.max_width, Some(MAX_SIDE));
    }

    #[test]
    fn the_seeded_sizes_are_normal_and_unique() {
        let all = defaults();
        for s in &all {
            assert_eq!(s.clone(), s.clone().normalized(), "{}", s.name);
        }
        assert_eq!(all.iter().map(|s| &s.id).collect::<std::collections::HashSet<_>>().len(), all.len());
    }
}
