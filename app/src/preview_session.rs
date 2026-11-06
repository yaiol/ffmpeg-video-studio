//! What the work view tells its preview pane: the file, the size cap, the effects, the cut, and the marks the user
//! sets in the pane, which the work view turns into the panel's cut boxes.

use engine::{EffectsEntry, Size, ffmpeg_effects::Values};
use std::path::PathBuf;

/// What the user set with the buttons of the preview, for the main window to write into the cut boxes.
#[derive(Clone, Debug, PartialEq)]
pub enum Mark {
    /// The cut starts here (a time as `engine::preview::clock` writes it).
    Start(String),
    /// The cut ends here.
    End(String),
    /// Keep the whole file.
    Clear,
}

/// What the user did with a widget drawn over the picture: the effect it belongs to and the values of its controls now (every one, the
/// way an effect form writes them); none = back to the defaults.
#[derive(Clone, Debug, PartialEq)]
pub struct WidgetEdit {
    pub key: String,
    pub values: Option<Values>,
    /// The effect stays in the list even at its defaults (the drawer chose it); otherwise it leaves the list while every value is neutral.
    pub keep: bool,
}

/// What stands in the place of the file list, under the picture: the files, or one of the drawers (one at a time).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum Drawer {
    #[default]
    Files,
    Colour,
    Curves,
    Tone,
    Compressor,
    Channels,
    Level,
}

/// What the user did with the texts drawn over the picture: a new one (its values), one changed (its number among the texts, its values now),
/// or one taken away.
#[derive(Clone, Debug, PartialEq)]
pub enum TextEdit {
    Add(Values),
    Set(usize, Values),
    Remove(usize),
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct State {
    /// The file shown: the first selected one in the work view.
    pub source: Option<PathBuf>,
    pub size: Size,
    pub effects: EffectsEntry,
    /// The two boxes of the panel's cut, as typed.
    pub cut_start: String,
    pub cut_end: String,
    pub lang: String,
    pub theme: String,
}
