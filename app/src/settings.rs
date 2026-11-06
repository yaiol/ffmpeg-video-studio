//! Persisted user settings: language and theme, in `settings.json` under the OS app-data folder.
//!
//! The folder mirrors the electron convention exactly: `…\AppData\Roaming\yaiol\<Product Name>`,
//! with a ` (Dev)` suffix on a debug build so a dev run and an installed copy never share state.
//! The file name is plain (`settings.json`) — the folder is already per-app, so there is no
//! storage-prefix to namespace it.

use engine::{AudioEntry, Destination, EffectsEntry, MetadataCleanup, Overwrite, Preset, Size, audio, destination::SOURCE_ID, effects, metadata, preset::Encoder, size::ORIGINAL_ID};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
};

/// The choices of ONE view's options panel. The batch view's are the flat fields of `Settings` (the file format of
/// every earlier version); the work view has its own set, so changing one view never changes the other.
#[derive(Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Panel {
    /// False until it has been given its first values (copied from the batch view's, once, when an older settings file
    /// is read); `Settings::repaired` does that.
    pub initialised: bool,
    pub preset_id: String,
    pub size_id: String,
    pub audio_id: String,
    pub metadata_id: String,
    pub destination_id: String,
    pub overwrite: Overwrite,
    pub parallel: u32,
}

impl Panel {
    fn seeded(initialised: bool) -> Panel {
        Panel {
            initialised,
            preset_id: engine::preset::defaults()[0].id.clone(),
            size_id: ORIGINAL_ID.to_owned(),
            audio_id: audio::ORIGINAL_ID.to_owned(),
            metadata_id: metadata::STANDARD_ID.to_owned(),
            destination_id: SOURCE_ID.to_owned(),
            overwrite: Overwrite::Skip,
            parallel: 1,
        }
    }
}

impl Default for Panel {
    fn default() -> Panel {
        Panel::seeded(false)
    }
}

#[derive(Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub lang: String,
    pub theme: String,
    /// The user's presets; a fresh install gets the seeded ones (stable ids `default-...`).
    pub presets: Vec<Preset>,
    /// Which engine encodes H.264 and H.265 for every preset that does not ask for software.
    pub encoder: Encoder,
    /// The selected preset.
    pub preset_id: String,
    /// The user's sizes (the largest picture a run may produce). "(Original)" is the window's own
    /// choice and is never stored.
    pub sizes: Vec<Size>,
    /// The selected size; `original` when none.
    pub size_id: String,
    /// The user's audio entries (what happens to the sound). "(Original)" is the window's own choice.
    pub audios: Vec<AudioEntry>,
    /// The selected audio entry; `original` when none.
    pub audio_id: String,
    /// The user's effects entries (a turn, a watermark, text). "(None)" is the window's own choice and is
    /// never stored; a fresh install gets one entry for each way of turning or flipping the picture.
    pub effects: Vec<EffectsEntry>,
    /// The selected entry; `none` when none.
    pub effects_id: String,
    /// The user's audio effects entries (what is done to the sound: an equaliser, a compressor, a noise gate). Their own list, beside the
    /// picture's; "(None)" is the window's own choice and is never stored, and a fresh install has none.
    pub audio_effects: Vec<EffectsEntry>,
    /// The selected audio effects entry; `none` when none.
    pub audio_effects_id: String,
    /// The user's metadata settings (which tags and chapters are left out). Never zero, like presets: a fresh install
    /// has one that keeps everything.
    pub metadata: Vec<MetadataCleanup>,
    /// The selected one.
    pub metadata_id: String,
    /// The user's destinations (a name and a folder). "Source Folder", beside each file, is the window's own
    /// choice and is never stored.
    pub destinations: Vec<Destination>,
    /// The selected destination; `source` when none.
    pub destination_id: String,
    pub overwrite: Overwrite,
    /// Conversions at once. A video encoder already uses every core, so the default is one.
    pub parallel: u32,
    /// Show the operating system's notice when a run ends.
    pub notify: bool,
    /// The work view's own panel choices.
    pub work: Panel,
}

impl Default for Settings {
    fn default() -> Self {
        let presets = engine::preset::defaults();
        Self {
            lang: "en".to_owned(),
            theme: "light".to_owned(),
           
            preset_id: presets[0].id.clone(),
            presets,
            encoder: Encoder::Software,
            sizes: engine::size::defaults(),
            size_id: ORIGINAL_ID.to_owned(),
            audios: audio::defaults(),
            audio_id: audio::ORIGINAL_ID.to_owned(),
            effects: effects::defaults(),
            effects_id: effects::NONE_ID.to_owned(),
            audio_effects: vec![EffectsEntry::standard_sound(&crate::i18n::t("en", "lblMainEffectsNone"))],
            audio_effects_id: effects::SOUND_STANDARD_ID.to_owned(),
            metadata: metadata::defaults(),
            metadata_id: metadata::STANDARD_ID.to_owned(),
            destinations: Vec::new(),
            destination_id: SOURCE_ID.to_owned(),
            overwrite: Overwrite::Skip,
            parallel: 1,
            notify: true,
            work: Panel::seeded(true),
        }
    }
}

/// The most conversions at once the window offers: the machine's physical cores, at most four (a
/// second video encoder only competes with the first for the same cores).
pub fn max_parallel() -> u32 {
    (num_cpus::get_physical() as u32).clamp(1, 4)
}

impl Settings {
    /// Never zero presets, a dangling selection falls back, every preset is what its container holds.
    pub fn repaired(mut self) -> Settings {
        // A file from before the engine was a setting: the six graphics-card presets are gone (the engine is chosen in Settings), and any
        // other preset that named a card's encoder is that codec's family.
        self.presets.retain(|p| !engine::preset::RETIRED_HARDWARE_IDS.contains(&p.id.as_str()));
        self.presets = self.presets.into_iter().map(Preset::into_family).map(Preset::normalized).collect();
        if self.presets.is_empty() {
            self.presets = engine::preset::defaults();
        }
        if !self.presets.iter().any(|p| p.id == self.preset_id) {
            self.preset_id = self.presets[0].id.clone();
        }
        self.sizes = self.sizes.into_iter().map(Size::normalized).collect();
        if self.size_id != ORIGINAL_ID && !self.sizes.iter().any(|z| z.id == self.size_id) {
            self.size_id = ORIGINAL_ID.to_owned();
        }
        self.audios = self.audios.into_iter().map(AudioEntry::normalized).collect();
        if self.audio_id != audio::ORIGINAL_ID && !self.audios.iter().any(|a| a.id == self.audio_id) {
            self.audio_id = audio::ORIGINAL_ID.to_owned();
        }
        self.effects = self.effects.into_iter().map(EffectsEntry::normalized).collect();
        if self.effects_id != effects::NONE_ID && !self.effects.iter().any(|e| e.id == self.effects_id) {
            self.effects_id = effects::NONE_ID.to_owned();
        }
        self.audio_effects = self.audio_effects.into_iter().map(EffectsEntry::normalized).collect();
        // Never zero sound entries, as FFmpeg Audio Studio: an ordinary entry with no effect in it is the one a fresh install starts with.
        if self.audio_effects.is_empty() {
            self.audio_effects.push(EffectsEntry::standard_sound(&crate::i18n::t(&self.lang, "lblMainEffectsNone")));
        }
        if self.audio_effects_id != effects::NONE_ID && !self.audio_effects.iter().any(|e| e.id == self.audio_effects_id) {
            self.audio_effects_id = effects::NONE_ID.to_owned();
        }
        // The metadata entries keep what the user typed (the list is tidied when a job runs). Never zero.
        if self.metadata.is_empty() {
            self.metadata = metadata::defaults();
        }
        if !self.metadata.iter().any(|m| m.id == self.metadata_id) {
            self.metadata_id = self.metadata[0].id.clone();
        }
        self.destinations = self.destinations.into_iter().map(Destination::normalized).collect();
        if self.destination_id != SOURCE_ID && !self.destinations.iter().any(|d| d.id == self.destination_id) {
            self.destination_id = SOURCE_ID.to_owned();
        }
        self.parallel = self.parallel.clamp(1, max_parallel());
        // The work view's panel: copied from the batch view's the first time (an older file has none), then its own.
        if !self.work.initialised {
            self.work = Panel { initialised: true, ..self.panel() };
        }
        let mut w = self.work.clone();
        if !self.presets.iter().any(|p| p.id == w.preset_id) {
            w.preset_id = self.presets[0].id.clone();
        }
        if w.size_id != ORIGINAL_ID && !self.sizes.iter().any(|z| z.id == w.size_id) {
            w.size_id = ORIGINAL_ID.to_owned();
        }
        if w.audio_id != audio::ORIGINAL_ID && !self.audios.iter().any(|a| a.id == w.audio_id) {
            w.audio_id = audio::ORIGINAL_ID.to_owned();
        }
        if !self.metadata.iter().any(|m| m.id == w.metadata_id) {
            w.metadata_id = self.metadata[0].id.clone();
        }
        if w.destination_id != SOURCE_ID && !self.destinations.iter().any(|d| d.id == w.destination_id) {
            w.destination_id = SOURCE_ID.to_owned();
        }
        w.parallel = w.parallel.clamp(1, max_parallel());
        self.work = w;
        self
    }

    /// The batch view's panel choices.
    pub fn panel(&self) -> Panel {
        Panel {
            initialised: true,
            preset_id: self.preset_id.clone(),
            size_id: self.size_id.clone(),
            audio_id: self.audio_id.clone(),
            metadata_id: self.metadata_id.clone(),
            destination_id: self.destination_id.clone(),
            overwrite: self.overwrite,
            parallel: self.parallel,
        }
    }

    /// The id of the destination that writes into `dir`: one already saved for it, or a new entry named after the folder.
    pub fn destination_for_folder(&mut self, dir: &Path) -> String {
        if let Some(d) = self.destinations.iter().find(|d| d.writes_to(dir)) {
            return d.id.clone();
        }
        let d = Destination::for_folder(dir);
        let id = d.id.clone();
        self.destinations.push(d);
        id
    }

    /// A destination by id; beside each file when there is none of that id.
    pub fn destination_by(&self, id: &str) -> Destination {
        self.destinations.iter().find(|d| d.id == id).cloned().unwrap_or_else(Destination::source)
    }

    /// A metadata setting by id; the first when there is none of that id.
    pub fn metadata_by(&self, id: &str) -> MetadataCleanup {
        self.metadata.iter().find(|m| m.id == id).or(self.metadata.first()).cloned().unwrap_or_default()
    }

    /// An effects entry by id; nothing when there is none of that id.
    pub fn effects_by(&self, id: &str) -> EffectsEntry {
        self.effects.iter().find(|e| e.id == id).cloned().unwrap_or_else(EffectsEntry::none)
    }

    /// An audio effects entry by id; nothing when there is none of that id.
    /// A settings file from before 2026-11-01 holds a level change, a loudness switch and a channel count on its audio entries; they are sound
    /// effects now, so each entry that had one gets an entry of the audio effects list (named after it) holding the same filters in the same
    /// order, and the one of the selected audio entry is selected. The entries do not write the fields back, so this happens once.
    fn take_over_sound_settings(&mut self, text: &str) {
        let Ok(json) = serde_json::from_str::<serde_json::Value>(text) else { return };
        for old in json.get("audios").and_then(|a| a.as_array()).into_iter().flatten() {
            let gain = old.get("gain_db").and_then(|g| g.as_i64()).unwrap_or(0);
            let loud = old.get("loudness").and_then(|l| l.as_bool()).unwrap_or(false);
            let way = match old.get("channels").and_then(|c| c.as_u64()) {
                Some(1) => Some("mono"),
                Some(2) => Some("stereo"),
                _ => None,
            };
            if gain == 0 && !loud && way.is_none() {
                continue;
            }
            let name = old.get("name").and_then(|n| n.as_str()).unwrap_or_default();
            let mut entry = EffectsEntry::new(name);
            if gain != 0 {
                entry.items.push(effects::Item::with("try:volume", &[("volume", &format!("{gain}dB"))]));
            }
            if loud {
                entry.items.push(effects::Item::with("try:loudnorm", &[("I", "-16"), ("TP", "-1.5"), ("LRA", "11")]));
            }
            if let Some(way) = way {
                entry.items.push(effects::Item::with("channels", &[("mode", way)]));
            }
            if old.get("id").and_then(|i| i.as_str()) == Some(self.audio_id.as_str()) && (self.audio_effects_id == effects::NONE_ID || self.audio_effects_id == effects::SOUND_STANDARD_ID) {
                self.audio_effects_id = entry.id.clone();
            }
            self.audio_effects.push(entry);
        }
    }

    pub fn audio_effects_by(&self, id: &str) -> EffectsEntry {
        self.audio_effects.iter().find(|e| e.id == id).cloned().unwrap_or_else(EffectsEntry::none)
    }

    /// The list of effects entries of a kind: the picture's, or the sound's (`sound`). The shared effects editor asks for the one it edits.
    pub fn effects_list(&self, sound: bool) -> &Vec<EffectsEntry> {
        if sound { &self.audio_effects } else { &self.effects }
    }

    pub fn effects_list_mut(&mut self, sound: bool) -> &mut Vec<EffectsEntry> {
        if sound { &mut self.audio_effects } else { &mut self.effects }
    }

    /// The id of the selected entry of that list.
    pub fn effects_choice(&self, sound: bool) -> &str {
        if sound { &self.audio_effects_id } else { &self.effects_id }
    }

    pub fn effects_choice_mut(&mut self, sound: bool) -> &mut String {
        if sound { &mut self.audio_effects_id } else { &mut self.effects_id }
    }

    /// An audio entry by id; the original rule when there is none of that id.
    pub fn audio_by(&self, id: &str) -> AudioEntry {
        self.audios.iter().find(|a| a.id == id).cloned().unwrap_or_else(AudioEntry::original)
    }

    pub fn size_by(&self, id: &str) -> Size {
        self.sizes.iter().find(|z| z.id == id).cloned().unwrap_or_else(Size::original)
    }

    /// A preset by id; the first one when there is none of that id.
    pub fn preset_by(&self, id: &str) -> Preset {
        self.presets.iter().find(|p| p.id == id).unwrap_or(&self.presets[0]).clone()
    }

    /// The preset a job runs: `preset_by`, with the engine Settings names where the preset leaves it to Settings.
    pub fn run_preset(&self, id: &str) -> Preset {
        engine::hardware::resolve(&self.preset_by(id), self.encoder)
    }

    pub fn selected_preset(&self) -> Preset {
        self.preset_by(&self.preset_id)
    }

    /// The selected audio entry's id, or the original when the selected preset's container cannot hold it.
    pub fn effective_audio_id(&self) -> String {
        self.effective_audio_id_of(&self.panel())
    }

    /// The same, for the choices of the panel `p` (the batch view's or the work view's).
    pub fn effective_audio_id_of(&self, p: &Panel) -> String {
        let preset = self.preset_by(&p.preset_id);
        let holds = self.audios.iter().any(|a| a.id == p.audio_id && a.available_for(&preset));
        if holds { p.audio_id.clone() } else { audio::ORIGINAL_ID.to_owned() }
    }

    /// Everything a run is made of, as the panel shows it now (the cut is the window's own, not saved).
    /// The effects a file is converted with: the work view's own list when it has one (picture and sound effects together), else the saved
    /// picture entry it names followed by the saved sound entry it names.
    pub fn effects_of(&self, run: &RunChoice) -> EffectsEntry {
        if let Some(own) = &run.effects_inline {
            return own.clone();
        }
        let mut all = self.effects_by(&run.effects_id);
        all.items.extend(self.audio_effects_by(&run.audio_effects_id).items);
        all
    }

    pub fn choice(&self, trim_start: &str, trim_end: &str) -> RunChoice {
        let mut run = self.choice_of(&self.panel(), trim_start, trim_end);
        run.effects_id = self.effects_id.clone();
        run.audio_effects_id = self.audio_effects_id.clone();
        run
    }

    /// Everything a run is made of, from the choices of the panel `p`.
    pub fn choice_of(&self, p: &Panel, trim_start: &str, trim_end: &str) -> RunChoice {
        RunChoice {
            preset_id: p.preset_id.clone(),
            size_id: p.size_id.clone(),
            audio_id: self.effective_audio_id_of(p),
            effects_id: effects::NONE_ID.to_owned(),
            audio_effects_id: effects::NONE_ID.to_owned(),
            effects_inline: None,
            metadata_id: p.metadata_id.clone(),
            trim_start: trim_start.to_owned(),
            trim_end: trim_end.to_owned(),
            destination_id: p.destination_id.clone(),
            overwrite: p.overwrite,
        }
    }
}

/// The complete run settings of ONE file: the choices of the panel at the moment they were applied to it.
/// Entries are kept by id, so editing a preset in Settings changes every file that uses it.
#[derive(Clone, PartialEq, Debug)]
pub struct RunChoice {
    pub preset_id: String,
    pub size_id: String,
    pub audio_id: String,
    pub effects_id: String,
    /// The saved entry of sound effects the file names; unused when `effects_inline` is there (it holds both kinds).
    pub audio_effects_id: String,
    /// The work view's own list of effects, when the file was applied from there: it is used instead of the saved entry `effects_id`.
    pub effects_inline: Option<EffectsEntry>,
    pub metadata_id: String,
    pub trim_start: String,
    pub trim_end: String,
    pub destination_id: String,
    pub overwrite: Overwrite,
}

/// The folder every yaiol app keeps its one ffmpeg and its effects catalogue in: shared by the apps and by their debug and release builds
/// alike, and per user, so installing or replacing it needs no administrator rights. A subfolder per schema of the pair, so an app
/// of another age never overwrites what this one reads.
pub fn ffmpeg_dir() -> Option<PathBuf> {
    Some(dirs::data_local_dir()?.join("yaiol").join("Common Files").join("bin").join(engine::download::SCHEMA.to_string()))
}

fn path() -> Option<PathBuf> {
    let folder = if cfg!(debug_assertions) {
        format!("{} (Dev)", crate::APP_NAME)
    } else {
        crate::APP_NAME.to_owned()
    };
    Some(dirs::data_dir()?.join("yaiol").join(folder).join("settings.json"))
}

/// The folder the log file lives in, beside the settings.
pub fn log_dir() -> Option<PathBuf> {
    Some(path()?.parent()?.join("logs"))
}

pub fn load() -> Settings {
    path().map(|p| load_from(&p)).unwrap_or_default()
}

pub fn save(settings: &Settings) {
    if let Some(p) = path() {
        save_to(&p, settings);
    }
}

fn load_from(path: &Path) -> Settings {
    let read = match fs::read_to_string(path) {
        Ok(text) => serde_json::from_str::<Settings>(&text).map_err(|e| format!("cannot read {}: {e}", path.display())),
        // No file yet is the first launch, not a problem.
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Err(String::new()),
        Err(e) => Err(format!("cannot read {}: {e}", path.display())),
    };
    if let Err(why) = &read {
        if !why.is_empty() {
            engine::log::error("settings", &format!("{why} - the defaults are used"));
        }
    }
    let mut settings = read.unwrap_or_default();
    if let Ok(text) = fs::read_to_string(path) {
        settings.take_over_sound_settings(&text);
    }
    settings.repaired()
}

fn save_to(path: &Path, settings: &Settings) {
    if let Some(dir) = path.parent() {
        let _ = fs::create_dir_all(dir);
    }
    if let Ok(text) = serde_json::to_string_pretty(settings) {
        let _ = fs::write(path, text);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("yaiol-settings-test-{}-{name}", std::process::id()))
    }

    #[test]
    fn a_file_with_a_saved_picture_entry_and_a_saved_sound_entry_is_converted_with_both() {
        let mut cfg = Settings::default();
        let mut picture = EffectsEntry::new("picture");
        picture.items.push(engine::effects::Item::new("flip"));
        let mut sound = EffectsEntry::new("sound");
        sound.items.push(engine::effects::Item::new("try:acontrast"));
        cfg.effects = vec![picture.clone()];
        cfg.audio_effects = vec![sound.clone()];
        let mut run = cfg.choice_of(&cfg.work, "", "");
        run.effects_id = picture.id.clone();
        run.audio_effects_id = sound.id.clone();
        let keys: Vec<String> = cfg.effects_of(&run).items.into_iter().map(|i| i.key).collect();
        assert_eq!(keys, vec!["flip".to_owned(), "try:acontrast".to_owned()]);
        // a dangling selection of the sound list falls back to none
        cfg.audio_effects_id = "gone".to_owned();
        assert_eq!(cfg.repaired().audio_effects_id, effects::NONE_ID);
    }

    #[test]
    fn the_level_loudness_and_channels_of_an_old_audio_entry_become_a_sound_effects_entry() {
        let old = r#"{"audios":[{"id":"a1","name":"Loud AAC","mode":"aac","kbps":192,"gain_db":6,"loudness":true,"channels":2},{"id":"a2","name":"Plain","mode":"aac","kbps":128,"gain_db":0,"loudness":false}],"audio_id":"a1"}"#;
        let mut cfg: Settings = serde_json::from_str(old).unwrap();
        cfg.take_over_sound_settings(old);
        // the standard entry is there from the start; the taken-over one follows it
        assert_eq!(cfg.audio_effects.len(), 2);
        assert_eq!(cfg.audio_effects[1].name, "Loud AAC");
        let keys: Vec<&str> = cfg.audio_effects[1].items.iter().map(|i| i.key.as_str()).collect();
        assert_eq!(keys, ["try:volume", "try:loudnorm", "channels"]);
        assert_eq!(cfg.audio_effects_id, cfg.audio_effects[1].id);
        // written back, the entry no longer holds the old fields, so a second read finds nothing to take over
        let saved = serde_json::to_string(&cfg).unwrap();
        let mut again: Settings = serde_json::from_str(&saved).unwrap();
        let before = again.audio_effects.len();
        again.take_over_sound_settings(&saved);
        assert_eq!(again.audio_effects.len(), before);
    }

    #[test]
    fn round_trips() {
        let p = temp("roundtrip").join("settings.json");
        let s = Settings { lang: "ar".to_owned(), theme: "dark".to_owned(), ..Settings::default() };
        save_to(&p, &s);
        assert!(load_from(&p) == s);
        let _ = fs::remove_dir_all(p.parent().unwrap());
    }

    #[test]
    fn missing_or_broken_file_gives_defaults() {
        assert!(load_from(&temp("missing")) == Settings::default());
        let p = temp("broken");
        fs::write(&p, "{ not json").unwrap();
        assert!(load_from(&p) == Settings::default());
        let _ = fs::remove_file(p);
    }

    #[test]
    fn a_dangling_selection_and_an_empty_list_are_repaired() {
        let p = temp("repair");
        fs::write(&p, r#"{ "presets": [], "preset_id": "gone", "parallel": 99 }"#).unwrap();
        let s = load_from(&p);
        assert!(!s.presets.is_empty());
        assert_eq!(s.preset_id, s.presets[0].id);
        assert!(s.parallel <= max_parallel());
        let _ = fs::remove_file(p);
    }

    #[test]
    fn a_file_from_before_the_engine_was_a_setting_loses_the_card_presets_and_reads_a_card_codec_as_its_family() {
        let p = temp("engine");
        fs::write(
            &p,
            r#"{ "presets": [
                { "id": "default-h264-nvenc", "name": "H.264 NVIDIA graphics card (MKV)", "video": "h264nvenc" },
                { "id": "mine", "name": "Mine", "video": "h265amd" },
                { "id": "default-h264-hq", "name": "H.264 high quality (MKV)", "video": "h264" } ],
               "preset_id": "default-h264-nvenc" }"#,
        )
        .unwrap();
        let s = load_from(&p);
        let ids: Vec<&str> = s.presets.iter().map(|p| p.id.as_str()).collect();
        assert_eq!(ids, ["mine", "default-h264-hq"]);
        assert_eq!(s.presets[0].video, engine::preset::VideoCodec::H265);
        assert_eq!(s.presets[0].encoder, engine::preset::PresetEncoder::Default);
        assert_eq!(s.encoder, Encoder::Software);
        assert_eq!(s.preset_id, "mine");
        let _ = fs::remove_file(p);
    }

    #[test]
    fn a_vanished_size_falls_back_to_the_original() {
        let p = temp("size");
        fs::write(&p, r#"{ "size_id": "gone", "sizes": [] }"#).unwrap();
        let s = load_from(&p);
        assert_eq!(s.size_id, ORIGINAL_ID);
        assert!(s.size_by(&s.size_id).is_original());
        let _ = fs::remove_file(p);
    }

    #[test]
    fn a_file_from_before_sizes_gets_the_seeded_ones() {
        let p = temp("presizes");
        fs::write(&p, r#"{ "theme": "dark" }"#).unwrap();
        let s = load_from(&p);
        assert!(!s.sizes.is_empty());
        assert_eq!(s.size_id, ORIGINAL_ID);
        let _ = fs::remove_file(p);
    }

    #[test]
    fn a_vanished_audio_entry_falls_back_to_the_original_and_old_files_get_the_seeded_ones() {
        let p = temp("audio");
        fs::write(&p, r#"{ "audio_id": "gone", "audios": [] }"#).unwrap();
        let s = load_from(&p);
        assert_eq!(s.audio_id, audio::ORIGINAL_ID);
        assert!(s.audio_by(&s.audio_id).is_original());
        fs::write(&p, r#"{ "theme": "dark" }"#).unwrap();
        assert!(!load_from(&p).audios.is_empty());
        let _ = fs::remove_file(p);
    }

    #[test]
    fn a_pasted_lut_path_in_an_effects_entry_is_cleaned_and_the_old_lut_fields_are_ignored() {
        engine::effects::set_catalog_text(include_str!("../../engine/testdata/effects.json")).unwrap();
        let p = temp("lut");
        fs::write(
            &p,
            r#"{ "lut_id": "gone", "luts": [ { "id": "x" } ],
                 "effects": [ { "id": "a", "name": "A", "items": [ { "key": "try:lut3d", "values": { "file": " \"D:\\hlg.cube\" " } } ] } ] }"#,
        )
        .unwrap();
        let s = load_from(&p);
        assert_eq!(s.effects_by("a").items[0].values["file"], "D:\\hlg.cube");
        let _ = fs::remove_file(p);
    }

    #[test]
    fn a_vanished_effects_entry_falls_back_to_none_and_old_files_get_the_seeded_ones() {
        let p = temp("effects");
        fs::write(&p, r#"{ "effects_id": "gone", "effects": [] }"#).unwrap();
        let s = load_from(&p);
        assert_eq!(s.effects_id, effects::NONE_ID);
        assert!(s.effects_by(&s.effects_id).is_none());
        // A file from the time the window had a rotation of its own simply loses that field.
        fs::write(&p, r#"{ "rotation": "cw", "theme": "dark" }"#).unwrap();
        let s = load_from(&p);
        assert_eq!(s.theme, "dark");
        assert_eq!(s.effects.len(), 7);
        let _ = fs::remove_file(p);
    }

    #[test]
    fn a_fresh_install_keeps_all_metadata_and_there_is_never_none() {
        let s = Settings::default();
        assert_eq!(s.metadata.len(), 1);
        assert!(s.metadata_by(&s.metadata_id).removes_nothing());
        assert_eq!(s.choice("", "").metadata_id, s.metadata_id);
        // A file with no entries, or a selection that vanished, is repaired.
        let p = temp("metadata");
        fs::write(&p, r#"{ "metadata": [], "metadata_id": "gone" }"#).unwrap();
        let s = load_from(&p);
        assert_eq!(s.metadata.len(), 1);
        assert_eq!(s.metadata_id, s.metadata[0].id);
        fs::write(&p, r#"{ "metadata": [ { "id": "a", "name": "A" }, { "id": "b", "name": "B", "mode": "delete" } ], "metadata_id": "gone" }"#).unwrap();
        let s = load_from(&p);
        assert_eq!(s.metadata_id, "a");
        assert_eq!(s.metadata_by("zzz").id, "a", "an unknown id gives the first entry");
        let _ = fs::remove_file(p);
    }

    #[test]
    fn a_picked_folder_becomes_a_saved_destination_once() {
        let mut s = Settings::default();
        let before = s.destinations.len();
        let dir = Path::new("D:\\Out\\Reels");
        let id = s.destination_for_folder(dir);
        assert_eq!(s.destinations.len(), before + 1);
        assert!(s.destination_by(&id).writes_to(dir));
        assert_eq!(s.destination_for_folder(dir), id, "the same folder picked again is the same entry");
        assert_eq!(s.destinations.len(), before + 1);
    }

    #[test]
    fn a_choice_is_the_panel_as_it_is_with_the_cut_and_the_folder_trimmed() {
        let mut s = Settings { overwrite: Overwrite::Rename, ..Settings::default() };
        let mut d = Destination::new("Convert");
        d.folder = "  \".\\convert\" ".to_owned();
        s.destination_id = d.id.clone();
        s.destinations.push(d);
        let s = s.repaired();
        let c = s.choice("0:10", "");
        assert_eq!((c.preset_id.as_str(), c.size_id.as_str(), c.audio_id.as_str()), (s.preset_id.as_str(), ORIGINAL_ID, audio::ORIGINAL_ID));
        assert_eq!((c.trim_start.as_str(), c.trim_end.as_str()), ("0:10", ""));
        assert_eq!(s.destination_by(&c.destination_id).folder, ".\\convert", "a pasted folder loses its spaces and quotes");
        assert_eq!(c.overwrite, Overwrite::Rename);
    }

    #[test]
    fn a_vanished_destination_falls_back_to_beside_the_source() {
        let p = temp("dest");
        fs::write(&p, r#"{ "destination_id": "gone", "destinations": [] }"#).unwrap();
        let s = load_from(&p);
        assert_eq!(s.destination_id, SOURCE_ID);
        assert!(s.destination_by(&s.destination_id).is_source());
        assert!(s.destination_by(&s.destination_id).folder_path().is_none());
        let _ = fs::remove_file(p);
    }

    #[test]
    fn a_partial_file_keeps_the_rest_at_defaults() {
        let p = temp("partial");
        fs::write(&p, r#"{ "theme": "dark" }"#).unwrap();
        let s = load_from(&p);
        assert_eq!((s.lang.as_str(), s.theme.as_str()), ("en", "dark"));
        let _ = fs::remove_file(p);
    }
}
