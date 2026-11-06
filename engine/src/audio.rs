//! Audio: what happens to the sound track. A named entry the user keeps in Settings and picks in the
//! window, apart from the preset - the same audio goes with any codec, and what a container can hold
//! decides what is on offer, not what a preset says.
//!
//! The window's own first choice, **Original audio**, is a rule resolved per file when the job runs:
//! keep the source's audio untouched when the container can hold it, otherwise convert it to what the
//! container takes. An entry the container cannot hold is treated the same way.

use crate::preset::{Container, Preset, VideoCodec};
use serde::{Deserialize, Serialize};

/// Id of the window-only choice that keeps the audio. Not an entry of Settings; the window names it.
pub const ORIGINAL_ID: &str = "original";

/// What ffmpeg does with the audio.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Audio {
    /// The stream is copied. Never an entry's mode: it comes only from the original-audio rule.
    Copy,
    Aac,
    Opus,
    /// Dolby Digital, ffmpeg's own `ac3` encoder.
    Ac3,
    /// Uncompressed 16-bit PCM: what an editing format carries.
    Pcm,
    None,
}

impl Audio {
    /// What an entry can be, in the order the editor lists them.
    pub const ENTRY_MODES: [Audio; 5] = [Audio::Aac, Audio::Opus, Audio::Ac3, Audio::Pcm, Audio::None];

    /// Whether the mode re-encodes the sound, so a channel count can be asked of it.
    pub fn encodes(self) -> bool {
        !matches!(self, Audio::Copy | Audio::None)
    }

    /// Whether the mode has a bitrate to set.
    pub fn has_bitrate(self) -> bool {
        matches!(self, Audio::Aac | Audio::Opus | Audio::Ac3)
    }
}

impl Container {
    /// The audio modes this container holds as an ENTRY, the preferred one first (what a conversion
    /// falls back to). A copy is decided by `holds_codec`, not listed here.
    pub fn audio_modes(self) -> &'static [Audio] {
        match self {
            Container::Mkv => &[Audio::Aac, Audio::Opus, Audio::Ac3, Audio::Pcm, Audio::None],
            Container::Mp4 => &[Audio::Aac, Audio::Ac3, Audio::None],
            Container::Mov => &[Audio::Aac, Audio::Pcm, Audio::Ac3, Audio::None],
            Container::Webm => &[Audio::Opus, Audio::None],
            Container::Avi => &[Audio::Ac3, Audio::Pcm, Audio::Aac, Audio::None],
        }
    }

    /// Whether a source audio stream of this codec (ffmpeg's name for it) can be copied into the
    /// container as it is.
    pub fn holds_codec(self, codec: &str) -> bool {
        let codec = codec.to_ascii_lowercase();
        match self {
            Container::Mkv => true,
            Container::Mp4 => matches!(codec.as_str(), "aac" | "mp3" | "ac3" | "eac3" | "alac"),
            Container::Mov => matches!(codec.as_str(), "aac" | "mp3" | "ac3" | "alac") || codec.starts_with("pcm_"),
            Container::Webm => matches!(codec.as_str(), "opus" | "vorbis"),
            Container::Avi => matches!(codec.as_str(), "mp3" | "mp2" | "ac3") || codec.starts_with("pcm_"),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AudioEntry {
    pub id: String,
    pub name: String,
    pub mode: Audio,
    pub kbps: u32,
}

impl Default for AudioEntry {
    fn default() -> Self {
        AudioEntry { id: String::new(), name: String::new(), mode: Audio::Aac, kbps: 192 }
    }
}

impl AudioEntry {
    /// The window-only "(Original)": the audio is kept, whatever that takes.
    pub fn original() -> AudioEntry {
        AudioEntry { id: ORIGINAL_ID.to_owned(), mode: Audio::Copy, ..AudioEntry::default() }
    }

    pub fn new(name: &str) -> AudioEntry {
        AudioEntry { id: crate::preset::new_id(), name: name.to_owned(), ..AudioEntry::default() }
    }

    pub fn is_original(&self) -> bool {
        self.id == ORIGINAL_ID
    }

    /// The same entry with a mode an entry can have and a bitrate in range.
    pub fn normalized(mut self) -> AudioEntry {
        if self.is_original() {
            return self;
        }
        if !Audio::ENTRY_MODES.contains(&self.mode) {
            self.mode = Audio::Aac;
        }
        self.kbps = self.kbps.clamp(32, 512);
        self
    }

    /// Whether the window offers this entry with a preset: the preset's container must be able to hold it.
    pub fn available_for(&self, preset: &Preset) -> bool {
        self.is_original() || preset.container.audio_modes().contains(&self.mode)
    }
}

/// What the command line is told: the mode and, for those that have one, the bitrate.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Resolved {
    pub mode: Audio,
    pub kbps: u32,
}

/// What a conversion falls back to when the sound cannot be kept: an editing format keeps its sound
/// uncompressed, anything else takes its container's usual codec.
fn fallback(preset: &Preset) -> Resolved {
    let mode = if preset.video == VideoCodec::ProRes && preset.container.audio_modes().contains(&Audio::Pcm) {
        Audio::Pcm
    } else {
        preset.container.audio_modes()[0]
    };
    Resolved { mode, kbps: if mode == Audio::Opus { 128 } else { 192 } }
}

/// Decide what to do with this file's audio. `source_codec` is the probe's first audio stream, empty
/// when the file has none.
pub fn resolve(entry: &AudioEntry, preset: &Preset, source_codec: &str) -> Resolved {
    if source_codec.is_empty() {
        // No sound to keep or convert; asking ffmpeg to copy nothing is harmless.
        return Resolved { mode: Audio::Copy, kbps: 0 };
    }
    if entry.is_original() || !entry.available_for(preset) {
        // An editing format wants uncompressed sound whatever the source holds; the rest keep what they can.
        if preset.video == VideoCodec::ProRes && preset.container.audio_modes().contains(&Audio::Pcm) {
            return fallback(preset);
        }
        if preset.container.holds_codec(source_codec) {
            return Resolved { mode: Audio::Copy, kbps: 0 };
        }
        return fallback(preset);
    }
    Resolved {
        mode: entry.mode,
        kbps: entry.kbps,
    }
}

/// The same decision for a sound an effect has to process (played backwards): a copy is impossible, so
/// a stream that would have been copied is converted to what the container usually takes.
pub fn reencoded(resolved: Resolved, preset: &Preset, source_codec: &str) -> Resolved {
    if resolved.mode == Audio::Copy && !source_codec.is_empty() { fallback(preset) } else { resolved }
}

/// What a fresh install starts with, with stable ids so a saved selection survives.
pub fn defaults() -> Vec<AudioEntry> {
    let mk = |id: &str, name: &str, mode: Audio, kbps: u32| AudioEntry { id: format!("default-{id}"), name: name.to_owned(), mode, kbps };
    vec![
        mk("aac-128", "AAC 128 kbps", Audio::Aac, 128),
        mk("aac-192", "AAC 192 kbps", Audio::Aac, 192),
        mk("aac-256", "AAC 256 kbps", Audio::Aac, 256),
        mk("opus-128", "Opus 128 kbps", Audio::Opus, 128),
        mk("ac3-256", "AC-3 256 kbps", Audio::Ac3, 256),
        mk("pcm", "PCM 16-bit", Audio::Pcm, 0),
        mk("none", "No audio", Audio::None, 0),
    ]
    .into_iter()
    .map(AudioEntry::normalized)
    .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn preset(container: Container, video: VideoCodec) -> Preset {
        Preset { container, video, ..Preset::default() }
    }

    #[test]
    fn original_audio_is_copied_when_the_container_holds_it() {
        let mp4 = preset(Container::Mp4, VideoCodec::H264);
        assert_eq!(resolve(&AudioEntry::original(), &mp4, "aac").mode, Audio::Copy);
        assert_eq!(resolve(&AudioEntry::original(), &preset(Container::Mkv, VideoCodec::H264), "dts").mode, Audio::Copy);
    }

    #[test]
    fn original_audio_is_converted_when_the_container_cannot_hold_it() {
        let mp4 = preset(Container::Mp4, VideoCodec::H264);
        assert_eq!(resolve(&AudioEntry::original(), &mp4, "dts"), Resolved { mode: Audio::Aac, kbps: 192 });
        let webm = preset(Container::Webm, VideoCodec::Vp9);
        assert_eq!(resolve(&AudioEntry::original(), &webm, "aac"), Resolved { mode: Audio::Opus, kbps: 128 });
        assert_eq!(resolve(&AudioEntry::original(), &webm, "opus").mode, Audio::Copy);
    }

    #[test]
    fn an_editing_format_keeps_its_sound_uncompressed() {
        let prores = preset(Container::Mov, VideoCodec::ProRes);
        assert_eq!(resolve(&AudioEntry::original(), &prores, "aac").mode, Audio::Pcm);
        // Matroska can hold PCM too, so a ProRes in it follows the same rule.
        assert_eq!(resolve(&AudioEntry::original(), &preset(Container::Mkv, VideoCodec::ProRes), "aac").mode, Audio::Pcm);
    }

    #[test]
    fn an_entry_the_container_cannot_hold_is_treated_as_original() {
        let webm = preset(Container::Webm, VideoCodec::Vp9);
        let aac = AudioEntry { mode: Audio::Aac, ..AudioEntry::new("aac") };
        assert!(!aac.available_for(&webm));
        assert_eq!(resolve(&aac, &webm, "mp3").mode, Audio::Opus);
    }

    #[test]
    fn a_chosen_entry_is_applied_as_it_is() {
        let mp4 = preset(Container::Mp4, VideoCodec::H264);
        let aac = AudioEntry { mode: Audio::Aac, kbps: 256, ..AudioEntry::new("aac") };
        assert_eq!(resolve(&aac, &mp4, "ac3"), Resolved { mode: Audio::Aac, kbps: 256 });
    }

    #[test]
    fn a_channel_count_is_asked_only_of_a_mode_that_encodes() {
        let mp4 = preset(Container::Mp4, VideoCodec::H264);
        let ac3 = AudioEntry { mode: Audio::Ac3, kbps: 256, ..AudioEntry::new("ac3") };
        assert_eq!(resolve(&ac3, &mp4, "dts"), Resolved { mode: Audio::Ac3, kbps: 256 });
        // Only mono and stereo are offered; anything else in a settings file is dropped.
    }

    #[test]
    fn ac3_is_held_by_matroska_mp4_and_mov_but_not_webm() {
        assert!(Container::Mkv.audio_modes().contains(&Audio::Ac3));
        assert!(Container::Mp4.audio_modes().contains(&Audio::Ac3));
        assert!(Container::Mov.audio_modes().contains(&Audio::Ac3));
        assert!(!Container::Webm.audio_modes().contains(&Audio::Ac3));
    }

    #[test]
    fn a_processed_sound_is_never_copied() {
        let mp4 = preset(Container::Mp4, VideoCodec::H264);
        let kept = resolve(&AudioEntry::original(), &mp4, "aac");
        assert_eq!(kept.mode, Audio::Copy);
        assert_eq!(reencoded(kept, &mp4, "aac"), Resolved { mode: Audio::Aac, kbps: 192 });
        // A chosen entry stays as it is, and a file with no sound stays asking for nothing.
        let chosen = Resolved { mode: Audio::Aac, kbps: 128 };
        assert_eq!(reencoded(chosen, &mp4, "aac"), chosen);
        assert_eq!(reencoded(kept, &mp4, "").mode, Audio::Copy);
    }

    #[test]
    fn avi_keeps_the_sounds_it_has_always_held_and_converts_the_rest_to_ac3() {
        let avi = preset(Container::Avi, VideoCodec::Raw);
        assert_eq!(resolve(&AudioEntry::original(), &avi, "mp3").mode, Audio::Copy);
        assert_eq!(resolve(&AudioEntry::original(), &avi, "pcm_s16le").mode, Audio::Copy);
        assert_eq!(resolve(&AudioEntry::original(), &avi, "aac"), Resolved { mode: Audio::Ac3, kbps: 192 });
        assert!(!Container::Avi.audio_modes().contains(&Audio::Opus));
    }

    #[test]
    fn a_file_without_sound_asks_for_nothing() {
        let mp4 = preset(Container::Mp4, VideoCodec::H264);
        let aac = AudioEntry::new("aac");
        assert_eq!(resolve(&aac, &mp4, "").mode, Audio::Copy);
    }

    #[test]
    fn entries_are_normalized_and_the_seeded_ones_are_unique() {
        let e = AudioEntry { mode: Audio::Copy, kbps: 5, ..AudioEntry::new("x") }.normalized();
        assert_eq!((e.mode, e.kbps), (Audio::Aac, 32));
        let all = defaults();
        for a in &all {
            assert_eq!(a.clone(), a.clone().normalized(), "{}", a.name);
        }
        assert_eq!(all.iter().map(|a| &a.id).collect::<std::collections::HashSet<_>>().len(), all.len());
    }

    #[test]
    fn every_container_holds_at_least_one_entry_mode_and_the_first_is_a_real_codec() {
        for c in Container::ALL {
            assert!(!c.audio_modes().is_empty());
            assert!(matches!(c.audio_modes()[0], Audio::Aac | Audio::Opus | Audio::Ac3), "{c:?}");
        }
    }
}
