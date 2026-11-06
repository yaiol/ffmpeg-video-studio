//! Which graphics-card encoders this machine can actually run.
//!
//! The ffmpeg we install has `h264_nvenc`, `h264_amf`, `h264_qsv` and the rest compiled in on every machine, so the encoder list says
//! nothing about the card. The only way to know is to ask the encoder to encode one tiny picture: it fails at once, with ffmpeg's own
//! line, when there is no card or driver. That is done in the background whenever an ffmpeg is found (at launch and after an install),
//! and the answer is kept for the process. `resolve` turns a preset's codec family (H.264, H.265) into the card encoder the Settings choice
//! names when this machine runs it; until the answer is known every job is encoded in software.
//!
//! The answer is also written beside the ffmpeg (`hardware.json`, keyed by the ffmpeg's version line), so a launch with the same ffmpeg
//! does not ask again. A card or a driver can change without ffmpeg changing, so the user can ask again (`recheck`, a button in Settings).

use crate::{
    ffmpeg,
    preset::{Encoder, Preset, PresetEncoder, RateMode, Tune, VideoCodec},
};
use serde::{Deserialize, Serialize};
use std::{
    path::{Path, PathBuf},
    process::Stdio,
    sync::{
        RwLock,
        atomic::{AtomicBool, Ordering},
    },
};

/// The graphics-card encoders and the name ffmpeg gives each.
const ENCODERS: [(VideoCodec, &str); 6] = [
    (VideoCodec::H264Nvenc, "h264_nvenc"),
    (VideoCodec::H265Nvenc, "hevc_nvenc"),
    (VideoCodec::H264Amd, "h264_amf"),
    (VideoCodec::H265Amd, "hevc_amf"),
    (VideoCodec::H264Qsv, "h264_qsv"),
    (VideoCodec::H265Qsv, "hevc_qsv"),
];

/// The encoder names that ran, or None while nothing has been probed.
static WORKING: RwLock<Option<Vec<&'static str>>> = RwLock::new(None);

/// True while a probe runs.
static PROBING: AtomicBool = AtomicBool::new(false);

/// True when the encoder ran one picture.
fn runs(ffmpeg_exe: &Path, encoder: &str) -> bool {
    // NVENC refuses a picture smaller than about 145x49, so the test picture is not tiny.
    let out = ffmpeg::command(ffmpeg_exe)
        .args(["-hide_banner", "-loglevel", "error", "-f", "lavfi", "-i", "color=c=black:s=320x240:d=0.2:r=10", "-frames:v", "1", "-c:v", encoder, "-f", "null", "-"])
        .stdin(Stdio::null())
        .output();
    matches!(out, Ok(o) if o.status.success())
}

/// What is written beside the ffmpeg.
#[derive(Serialize, Deserialize)]
struct Cache {
    /// The first line of the ffmpeg's `-version`: another ffmpeg is another question.
    ffmpeg: String,
    working: Vec<String>,
}

fn cache_path(ffmpeg_exe: &Path) -> Option<PathBuf> {
    Some(ffmpeg_exe.parent()?.join("hardware.json"))
}

/// The encoder names a cache holds for this ffmpeg, if it holds any.
fn cached(ffmpeg_exe: &Path) -> Option<Vec<&'static str>> {
    let text = std::fs::read_to_string(cache_path(ffmpeg_exe)?).ok()?;
    let cache: Cache = serde_json::from_str(&text).ok()?;
    if cache.ffmpeg != ffmpeg::version_line(ffmpeg_exe) {
        return None;
    }
    Some(ENCODERS.iter().filter(|(_, n)| cache.working.iter().any(|w| w == n)).map(|(_, n)| *n).collect())
}

fn publish(working: Vec<&'static str>) {
    if let Ok(mut w) = WORKING.write() {
        *w = Some(working);
    }
}

/// Try every graphics-card encoder the ffmpeg has, one after the other, keep what ran and write it beside the ffmpeg. Slow (the drivers
/// load), so run it off the UI thread.
pub fn probe(ffmpeg_exe: &Path) {
    PROBING.store(true, Ordering::SeqCst);
    let listed = ffmpeg::command(ffmpeg_exe).args(["-hide_banner", "-encoders"]).stdin(Stdio::null()).output();
    let listed = listed.map(|o| String::from_utf8_lossy(&o.stdout).into_owned()).unwrap_or_default();
    let working: Vec<&'static str> =
        ENCODERS.iter().filter(|(_, name)| listed.contains(name) && runs(ffmpeg_exe, name)).map(|(_, name)| *name).collect();
    crate::log::info("hardware", &format!("encoders that ran on this machine: {}", if working.is_empty() { "none".to_owned() } else { working.join(", ") }));
    if let Some(path) = cache_path(ffmpeg_exe) {
        let cache = Cache { ffmpeg: ffmpeg::version_line(ffmpeg_exe), working: working.iter().map(|n| (*n).to_owned()).collect() };
        if let Ok(text) = serde_json::to_string_pretty(&cache) {
            let _ = std::fs::write(path, text);
        }
    }
    publish(working);
    PROBING.store(false, Ordering::SeqCst);
}

/// At launch and after an install: what the cache says for this ffmpeg, or a probe in a thread of its own when it says nothing.
pub fn load_or_probe_in_background(ffmpeg_exe: PathBuf) {
    std::thread::spawn(move || match cached(&ffmpeg_exe) {
        Some(working) => publish(working),
        None => probe(&ffmpeg_exe),
    });
}

/// Ask the encoders again whatever the cache says (the user changed a card or a driver), in a thread of its own.
pub fn recheck_in_background(ffmpeg_exe: PathBuf) {
    if PROBING.swap(true, Ordering::SeqCst) {
        return;
    }
    std::thread::spawn(move || probe(&ffmpeg_exe));
}

/// True while a probe is running.
pub fn probing() -> bool {
    PROBING.load(Ordering::SeqCst)
}

/// The encoder names that ran, or None while nothing is known.
pub fn working() -> Option<Vec<&'static str>> {
    WORKING.read().ok().and_then(|w| w.clone())
}

/// True when a probe found the graphics-card encoder runs here. Before the first probe has finished nothing is known, so nothing runs:
/// a job started in that moment is encoded in software rather than risk a command the card refuses.
fn runs_here(codec: VideoCodec) -> bool {
    let Some((_, name)) = ENCODERS.iter().find(|(c, _)| *c == codec) else { return false };
    WORKING.read().is_ok_and(|w| w.as_ref().is_some_and(|list| list.contains(name)))
}

/// Whether a graphics card can encode the preset as it is written. The card encoders have one quality number and no 10-bit, target
/// bitrate, x265 parameters or tune here, so a preset that uses any of them stays in software rather than lose it silently.
fn fits_a_card(preset: &Preset) -> bool {
    matches!(preset.video, VideoCodec::H264 | VideoCodec::H265)
        && !preset.ten_bit
        && preset.rate == RateMode::Crf
        && preset.x265_params.is_empty()
        && preset.tune == Tune::None
}

/// The preset a job runs: the preset's codec family on the engine `choice` names, when the preset leaves the engine to Settings, a card of
/// that maker runs (`runs`) and the container holds it; otherwise the preset itself, in software.
fn resolve_with(preset: &Preset, choice: Encoder, runs: &dyn Fn(VideoCodec) -> bool) -> Preset {
    if preset.encoder == PresetEncoder::Software || !fits_a_card(preset) {
        return preset.clone();
    }
    for maker in choice.makers() {
        let found = ENCODERS.iter().map(|(c, _)| *c).find(|c| c.family() == preset.video && c.graphics_maker() == Some(*maker));
        if let Some(codec) = found.filter(|c| preset.container.video_codecs().contains(c) && runs(*c)) {
            return Preset { video: codec, ..preset.clone() }.normalized();
        }
    }
    preset.clone()
}

/// `resolve_with` against what the probe found on this machine.
pub fn resolve(preset: &Preset, choice: Encoder) -> Preset {
    resolve_with(preset, choice, &runs_here)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::preset::Container;

    fn preset(video: VideoCodec) -> Preset {
        Preset { video, container: Container::Mkv, ..Preset::new("t") }
    }

    #[test]
    fn nothing_runs_before_the_first_probe() {
        assert!(!runs_here(VideoCodec::H264Nvenc));
        assert_eq!(resolve(&preset(VideoCodec::H264), Encoder::Nvidia).video, VideoCodec::H264);
    }

    #[test]
    fn software_stays_software_and_a_card_is_named_by_its_maker() {
        let only_nvidia = |c: VideoCodec| matches!(c, VideoCodec::H264Nvenc | VideoCodec::H265Nvenc);
        let p = preset(VideoCodec::H265);
        assert_eq!(resolve_with(&p, Encoder::Software, &only_nvidia).video, VideoCodec::H265);
        assert_eq!(resolve_with(&p, Encoder::Nvidia, &only_nvidia).video, VideoCodec::H265Nvenc);
        assert_eq!(resolve_with(&p, Encoder::Auto, &only_nvidia).video, VideoCodec::H265Nvenc);
        // A card this machine lacks falls back to software, for a named maker and for Automatic alike.
        assert_eq!(resolve_with(&p, Encoder::Amd, &only_nvidia).video, VideoCodec::H265);
        assert_eq!(resolve_with(&p, Encoder::Auto, &|_| false).video, VideoCodec::H265);
    }

    #[test]
    fn automatic_prefers_nvidia_then_amd_then_intel() {
        let amd_and_intel = |c: VideoCodec| c.graphics_maker() != Some("nvidia");
        assert_eq!(resolve_with(&preset(VideoCodec::H264), Encoder::Auto, &amd_and_intel).video, VideoCodec::H264Amd);
        let intel = |c: VideoCodec| c.graphics_maker() == Some("intel");
        assert_eq!(resolve_with(&preset(VideoCodec::H264), Encoder::Auto, &intel).video, VideoCodec::H264Qsv);
    }

    #[test]
    fn a_preset_that_asks_for_software_or_needs_more_than_a_card_gives_stays_in_software() {
        let all = |_: VideoCodec| true;
        let soft = Preset { encoder: PresetEncoder::Software, ..preset(VideoCodec::H264) };
        assert_eq!(resolve_with(&soft, Encoder::Nvidia, &all).video, VideoCodec::H264);
        for p in [
            Preset { ten_bit: true, ..preset(VideoCodec::H265) },
            Preset { rate: RateMode::Bitrate, ..preset(VideoCodec::H264) },
            Preset { x265_params: "keyint=250".into(), ..preset(VideoCodec::H265) },
            Preset { tune: Tune::Film, ..preset(VideoCodec::H264) },
        ] {
            assert_eq!(resolve_with(&p, Encoder::Nvidia, &all).video, p.video);
        }
        // A codec with no card version, and a container that does not hold the card's.
        assert_eq!(resolve_with(&preset(VideoCodec::Av1), Encoder::Nvidia, &all).video, VideoCodec::Av1);
        let avi = Preset { container: Container::Avi, ..preset(VideoCodec::H264) };
        assert_eq!(resolve_with(&avi, Encoder::Nvidia, &all).video, VideoCodec::H264);
    }
}
