//! The favourites: which effects an app lists as rows in its work view without being asked (the "standard" ones). The catalogue says
//! which effects are standard by default; the user may choose his own. The choice is `favourites.json`, in the folder of `effects.json`,
//! so it survives an update of ffmpeg and its catalogue (an install writes those two files only) and is shared by the apps of the machine:
//! each app keeps its own list under its own id and leaves the others alone.
//!
//! ⚠ SOURCE OF TRUTH: the shared ffmpeg-effects folder (`rust/engine`). A copy in an app is never edited there.

use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    sync::RwLock,
};

/// The name of the file, beside `effects.json`.
pub const FILE: &str = "favourites.json";

#[derive(Serialize, Deserialize, Default)]
struct Stored {
    #[serde(default)]
    schema: u32,
    /// The keys (an effect's id, or `try:` and a filter's name) each app lists, by app id.
    #[serde(default)]
    apps: BTreeMap<String, Vec<String>>,
}

fn read(dir: &Path) -> Stored {
    fs::read_to_string(dir.join(FILE)).ok().and_then(|text| serde_json::from_str(&text).ok()).unwrap_or_default()
}

/// The favourites of `app`, or `None` when the user has not chosen (the app then lists the catalogue's standard effects).
pub fn load(dir: &Path, app: &str) -> Option<Vec<String>> {
    read(dir).apps.remove(app)
}

/// Keep the favourites of `app`; `None` takes its choice back, so it lists the catalogue's standard effects again. The other apps' lists
/// are written back as they were.
pub fn save(dir: &Path, app: &str, keys: Option<&[String]>) -> Result<(), String> {
    let mut stored = read(dir);
    stored.schema = 1;
    match keys {
        Some(keys) => stored.apps.insert(app.to_owned(), keys.to_vec()),
        None => stored.apps.remove(app),
    };
    let path = dir.join(FILE);
    if stored.apps.is_empty() {
        return match fs::remove_file(&path) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(format!("{}: {e}", path.display())),
            _ => Ok(()),
        };
    }
    let text = serde_json::to_string_pretty(&stored).map_err(|e| e.to_string())?;
    let part = dir.join(format!("{FILE}.part"));
    fs::write(&part, text).map_err(|e| format!("{}: {e}", part.display()))?;
    fs::rename(&part, &path).map_err(|e| format!("{}: {e}", path.display()))
}

/// What one app holds of its favourites while it runs: where the file is (the folder of the installed catalogue) and the list itself.
/// An app has one, as a `static`.
pub struct Choice {
    app: &'static str,
    dir: RwLock<Option<PathBuf>>,
    keys: RwLock<Option<Vec<String>>>,
}

impl Choice {
    pub const fn new(app: &'static str) -> Choice {
        Choice { app, dir: RwLock::new(None), keys: RwLock::new(None) }
    }

    /// Read the favourites from the folder of the installed catalogue (`None`: no catalogue is installed, so there is nowhere to keep them
    /// and the user has no choice). Called whenever the catalogue is loaded.
    pub fn load(&self, dir: Option<&Path>) {
        *self.keys.write().unwrap() = dir.and_then(|d| load(d, self.app));
        *self.dir.write().unwrap() = dir.map(Path::to_path_buf);
    }

    /// The user's list, or `None` when he has not chosen.
    pub fn get(&self) -> Option<Vec<String>> {
        self.keys.read().unwrap().clone()
    }

    /// Choose a list (`None` takes the choice back). Kept in the file first: what the app holds is what the file holds.
    pub fn set(&self, keys: Option<Vec<String>>) -> Result<(), String> {
        let dir = self.dir.read().unwrap().clone().ok_or_else(|| "no effects catalogue is installed".to_owned())?;
        save(&dir, self.app, keys.as_deref())?;
        *self.keys.write().unwrap() = keys;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("yaiol-fav-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn keys(list: &[&str]) -> Vec<String> {
        list.iter().map(|k| (*k).to_owned()).collect()
    }

    #[test]
    fn no_file_is_no_choice() {
        let dir = scratch("none");
        assert_eq!(load(&dir, "ffmpeg-audio-studio"), None);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn a_list_is_kept_and_the_other_apps_list_is_left_alone() {
        let dir = scratch("two");
        save(&dir, "ffmpeg-audio-studio", Some(&keys(&["try:flanger", "equalizer"]))).unwrap();
        save(&dir, "ffmpeg-video-studio", Some(&keys(&["try:vignette"]))).unwrap();
        assert_eq!(load(&dir, "ffmpeg-audio-studio"), Some(keys(&["try:flanger", "equalizer"])));
        assert_eq!(load(&dir, "ffmpeg-video-studio"), Some(keys(&["try:vignette"])));
        save(&dir, "ffmpeg-audio-studio", None).unwrap();
        assert_eq!(load(&dir, "ffmpeg-audio-studio"), None, "taking the choice back");
        assert_eq!(load(&dir, "ffmpeg-video-studio"), Some(keys(&["try:vignette"])), "the other app keeps its list");
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn the_last_choice_taken_back_leaves_no_file() {
        let dir = scratch("empty");
        save(&dir, "ffmpeg-audio-studio", Some(&keys(&["x"]))).unwrap();
        save(&dir, "ffmpeg-audio-studio", None).unwrap();
        assert!(!dir.join(FILE).exists());
        save(&dir, "ffmpeg-audio-studio", None).unwrap();
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn an_app_holds_what_the_file_holds() {
        let dir = scratch("choice");
        let choice = Choice::new("ffmpeg-audio-studio");
        assert!(choice.set(Some(keys(&["x"]))).is_err(), "nowhere to keep it before a catalogue is loaded");
        choice.load(Some(&dir));
        assert_eq!(choice.get(), None);
        choice.set(Some(keys(&["x", "try:flanger"]))).unwrap();
        let again = Choice::new("ffmpeg-audio-studio");
        again.load(Some(&dir));
        assert_eq!(again.get(), Some(keys(&["x", "try:flanger"])));
        again.set(None).unwrap();
        assert_eq!(again.get(), None);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn a_file_that_cannot_be_read_is_no_choice_and_is_replaced_by_the_next_save() {
        let dir = scratch("broken");
        fs::write(dir.join(FILE), "{ not json").unwrap();
        assert_eq!(load(&dir, "ffmpeg-audio-studio"), None);
        save(&dir, "ffmpeg-audio-studio", Some(&keys(&["x"]))).unwrap();
        assert_eq!(load(&dir, "ffmpeg-audio-studio"), Some(keys(&["x"])));
        let _ = fs::remove_dir_all(dir);
    }
}
