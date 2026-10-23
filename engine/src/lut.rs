//! A colour LUT: a 3D lookup table (`.cube`) applied to the picture. A named entry the user keeps in
//! Settings and picks in the window, apart from the preset - the same look goes with any codec.
//!
//! The table is applied to the picture as it is read, BEFORE it is resized, so the colour maths runs on
//! the source's own pixels. A copied video stream cannot be touched, so a LUT is ignored with it.

use serde::{Deserialize, Serialize};
use std::path::Path;

/// Id of the window-only choice that applies no LUT. Not an entry of Settings; the window names it.
pub const NONE_ID: &str = "none";

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Lut {
    pub id: String,
    pub name: String,
    /// The `.cube` file, as the user typed or picked it.
    pub path: String,
}

impl Lut {
    /// The window-only "(None)".
    pub fn none() -> Lut {
        Lut { id: NONE_ID.to_owned(), ..Lut::default() }
    }

    pub fn new(name: &str) -> Lut {
        Lut { id: crate::preset::new_id(), name: name.to_owned(), path: String::new() }
    }

    pub fn is_none(&self) -> bool {
        self.id == NONE_ID
    }

    /// The same entry with its path trimmed (a path pasted with a space or a quote around it is common).
    pub fn normalized(mut self) -> Lut {
        self.path = self.path.trim().trim_matches('"').trim().to_owned();
        self
    }

    /// Whether the entry points at a file that is there. An entry with no path is not usable either.
    pub fn file_exists(&self) -> bool {
        !self.path.is_empty() && Path::new(&self.path).is_file()
    }

    /// The ffmpeg video filter that applies the table, or None when no LUT is chosen.
    ///
    /// A path is escaped for ffmpeg's two levels of filter parsing: slashes forward (a backslash is the
    /// escape character), the drive colon escaped (`D\:/luts/x.cube`), the whole thing quoted, and a
    /// quote inside it closed, escaped and reopened.
    pub fn filter(&self) -> Option<String> {
        if self.is_none() || self.path.is_empty() {
            return None;
        }
        let escaped = self.path.replace('\\', "/").replace(':', "\\:").replace('\'', "'\\''");
        Some(format!("lut3d=file='{escaped}'"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lut(path: &str) -> Lut {
        Lut { path: path.to_owned(), ..Lut::new("t") }
    }

    #[test]
    fn no_lut_adds_no_filter() {
        assert_eq!(Lut::none().filter(), None);
        assert_eq!(lut("").filter(), None);
    }

    #[test]
    fn a_windows_path_is_escaped_for_ffmpeg() {
        assert_eq!(lut("D:\\Luts\\Rec 709\\hlg.cube").filter().unwrap(), "lut3d=file='D\\:/Luts/Rec 709/hlg.cube'");
    }

    #[test]
    fn a_quote_in_a_path_cannot_break_out() {
        assert_eq!(lut("/x/it's.cube").filter().unwrap(), "lut3d=file='/x/it'\\''s.cube'");
    }

    #[test]
    fn a_pasted_path_is_cleaned() {
        assert_eq!(lut("  \"D:\\a.cube\" ").normalized().path, "D:\\a.cube");
    }

    #[test]
    fn a_missing_file_is_not_usable() {
        assert!(!lut("").file_exists());
        assert!(!lut("Z:\\definitely\\not\\here.cube").file_exists());
    }
}
