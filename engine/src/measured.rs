//! The measured effects: the ones that need the file MEASURED before they can be written (where the sound starts and stops, its loudness, its
//! peak, the ReplayGain figures), and the plan that turns an effects entry and a file's measures into exact filter arguments, the length the
//! output will have and the tags to write.
//!
//! ⚠ SOURCE OF TRUTH: the shared ffmpeg-effects folder (`rust/glue/measured.rs`). A copy in an app is never edited there. It is the layer between
//! the catalogue (`ffmpeg_effects`) and the app's own `effects` module, and relies on the two having the same `Item` and `EffectsEntry`
//! (`catalog()`, `Item::effect/all_values/title/is_ready`, `EffectsEntry::items`).
//!
//! The work is split in three. [`Analysis`] is what the measuring passes of ffmpeg report about a file; [`window`] decides which part of the
//! source is kept; [`plan`] turns an entry and an analysis into exact filter arguments. Nothing here runs ffmpeg: the queue does, and hands the
//! text over.

use crate::effects::{catalog, EffectsEntry, Item};
use crate::ffmpeg_effects::{self as fx, Ctx, Values, V};
use std::collections::BTreeMap;

/// What this app can do for a measured effect: the values `measure.op` of the catalogue can take. The effects that use them, their
/// names, their controls and the limits of these are all the catalogue's.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Op {
    /// Cut the silence at the start and / or the end of the song.
    CutSilence,
    /// Keep the part between two times.
    CutRange,
    /// Keep at most some seconds.
    CutLength,
    /// Set the volume to a loudness.
    GainLoudness,
    /// Set the volume so the highest sample is at a level.
    GainPeak,
    /// Set the volume to the ReplayGain figure.
    GainReplayGain,
    /// A fixed gain.
    GainFixed,
    /// Cut the song where it stops and leave some silence after it.
    PadSilence,
    /// Write the ReplayGain tags.
    TagsReplayGain,
}

impl Op {
    pub fn named(name: &str) -> Option<Op> {
        Some(match name {
            "cut-silence" => Op::CutSilence,
            "cut-range" => Op::CutRange,
            "cut-length" => Op::CutLength,
            "gain-loudness" => Op::GainLoudness,
            "gain-peak" => Op::GainPeak,
            "gain-replaygain" => Op::GainReplayGain,
            "gain-fixed" => Op::GainFixed,
            "pad-silence" => Op::PadSilence,
            "tags-replaygain" => Op::TagsReplayGain,
            _ => return None,
        })
    }

    fn is_gain(self) -> bool {
        matches!(self, Op::GainLoudness | Op::GainPeak | Op::GainReplayGain | Op::GainFixed)
    }
}

/// The inputs of a measured effect: the value of the control the catalogue names for each, kept inside that control's own limits.
struct Inputs {
    values: Values,
    roles: BTreeMap<String, String>,
    bounds: BTreeMap<String, (Option<f64>, Option<f64>)>,
}

impl Inputs {
    fn number(&self, role: &str) -> f64 {
        let Some(id) = self.roles.get(role) else { return 0.0 };
        let (min, max) = self.bounds.get(id).copied().unwrap_or((None, None));
        self.values.get(id).map_or(0.0, V::to_number).clamp(min.unwrap_or(f64::MIN), max.unwrap_or(f64::MAX))
    }

    fn switch(&self, role: &str) -> bool {
        self.roles.get(role).and_then(|id| self.values.get(id)).is_some_and(V::truthy)
    }
}

/// Trim silence at the ends: the start, the end, or both. Never the middle of the song.
#[derive(Clone, Debug, PartialEq)]
pub struct TrimSilence {
    pub start: bool,
    pub end: bool,
    /// Below this level (dBFS) the sound counts as silence.
    pub threshold_db: f64,
    /// A little of the silence kept at each cut, so a quiet attack or decay is not clipped.
    pub margin_ms: u32,
}

/// Keep a part of the song: from `from_s` to `to_s` (0 = to the end), counted on what is left after the effects before it.
#[derive(Clone, Debug, PartialEq)]
pub struct CutRange {
    pub from_s: f64,
    pub to_s: f64,
}

/// Keep at most this many seconds.
#[derive(Clone, Debug, PartialEq)]
pub struct LimitDuration {
    pub seconds: f64,
}

/// How the volume is set.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VolumeMode {
    /// Bring the integrated loudness to a target (LUFS).
    Loudness,
    /// Bring the highest sample to a target level (dBFS).
    Peak,
    /// Apply the ReplayGain track (or album) gain to the audio.
    ReplayGain,
    /// A fixed gain in dB.
    Gain,
}

/// The volume of the track: one of four ways. A limiter follows every gain that could clip (all but the peak mode, whose result cannot
/// exceed its target).
#[derive(Clone, Debug, PartialEq)]
pub struct Volume {
    pub mode: VolumeMode,
    pub target_lufs: f64,
    pub target_peak_db: f64,
    /// The level (dBFS) the limiter keeps the peaks under.
    pub ceiling_db: f64,
    pub gain_db: f64,
    /// ReplayGain mode: use the album's gain (the files of one folder and one album in the run).
    pub album: bool,
}

impl Default for Volume {
    fn default() -> Self {
        Volume { mode: VolumeMode::Loudness, target_lufs: -16.0, target_peak_db: -1.0, ceiling_db: -1.0, gain_db: 0.0, album: false }
    }
}

/// Silence at the end: the song is cut where it stops and exactly `seconds` of silence follow.
#[derive(Clone, Debug, PartialEq)]
pub struct SilenceEnd {
    /// How long the silence after the song is, whatever the track's own tail was.
    pub seconds: f64,
    /// Below this level (dBFS) the sound counts as silence.
    pub threshold_db: f64,
    /// A little of the original tail kept, so a quiet decay is not clipped (counted in `seconds`).
    pub margin_ms: u32,
}

/// Write the ReplayGain tags (the audio is not touched); with `album`, the album's too.
#[derive(Clone, Debug, PartialEq)]
pub struct ReplayGainTags {
    pub album: bool,
}

impl Item {
    /// The operation the catalogue gives this effect, when it is a measured effect this app can do.
    pub fn op(&self) -> Option<Op> {
        Op::named(&catalog()?.effects.get(&self.key)?.measure.as_ref()?.op)
    }

    /// Does the catalogue say the app does this effect itself, from what it measures? (An operation this app lacks still counts: the job
    /// says so, `try_chain`.)
    pub fn is_measured(&self) -> bool {
        catalog().is_some_and(|c| c.effects.get(&self.key).is_some_and(|e| e.measure.is_some()))
    }

    fn is_volume(&self) -> bool {
        self.op().is_some_and(Op::is_gain)
    }

    /// The operation and its inputs, for the measured effect this item is.
    fn measured(&self) -> Option<(Op, Inputs)> {
        let cat = catalog()?;
        let effect = cat.effects.get(&self.key)?;
        let measure = effect.measure.as_ref()?;
        let bounds = fx::resolve_controls(effect, &cat.filters).ok()?.into_iter().map(|c| (c.id, (c.min, c.max))).collect();
        Some((Op::named(&measure.op)?, Inputs { values: self.all_values(), roles: measure.controls.clone(), bounds }))
    }

    /// How many times faster the effect plays the clip, when it is one that changes the speed (the catalogue names the option: `speed`
    /// of the filter).
    pub fn speed(&self) -> Option<f64> {
        let cat = catalog()?;
        let option = cat.filters.get(self.key.strip_prefix("try:")?)?.speed.as_ref()?;
        self.all_values().get(option).map(V::to_number)
    }

    fn as_trim_silence(&self) -> Option<TrimSilence> {
        let (op, r) = self.measured()?;
        (op == Op::CutSilence).then(|| TrimSilence { start: r.switch("start"), end: r.switch("end"), threshold_db: r.number("threshold"), margin_ms: r.number("margin") as u32 })
    }

    fn as_cut_range(&self) -> Option<CutRange> {
        let (op, r) = self.measured()?;
        (op == Op::CutRange).then(|| CutRange { from_s: r.number("from"), to_s: r.number("to") })
    }

    fn as_limit_duration(&self) -> Option<LimitDuration> {
        let (op, r) = self.measured()?;
        (op == Op::CutLength).then(|| LimitDuration { seconds: r.number("seconds") })
    }

    fn as_volume(&self) -> Option<Volume> {
        let (op, r) = self.measured()?;
        let mode = match op {
            Op::GainLoudness => VolumeMode::Loudness,
            Op::GainPeak => VolumeMode::Peak,
            Op::GainReplayGain => VolumeMode::ReplayGain,
            Op::GainFixed => VolumeMode::Gain,
            _ => return None,
        };
        let mut v = Volume { mode, ..Volume::default() };
        match mode {
            VolumeMode::Loudness => v.target_lufs = r.number("target"),
            VolumeMode::Peak => v.target_peak_db = r.number("target"),
            VolumeMode::ReplayGain => v.album = r.switch("album"),
            VolumeMode::Gain => v.gain_db = r.number("gain"),
        }
        if mode != VolumeMode::Peak {
            v.ceiling_db = r.number("ceiling");
        }
        Some(v)
    }

    fn as_silence_end(&self) -> Option<SilenceEnd> {
        let (op, r) = self.measured()?;
        (op == Op::PadSilence).then(|| SilenceEnd { seconds: r.number("seconds"), threshold_db: r.number("threshold"), margin_ms: r.number("margin") as u32 })
    }

    fn as_replaygain(&self) -> Option<ReplayGainTags> {
        let (op, r) = self.measured()?;
        (op == Op::TagsReplayGain).then(|| ReplayGainTags { album: r.switch("album") })
    }
}

impl EffectsEntry {
    fn first<T>(&self, read: impl Fn(&Item) -> Option<T>) -> Option<T> {
        self.items.iter().find_map(read)
    }

    pub fn trim_silence(&self) -> Option<TrimSilence> {
        self.first(Item::as_trim_silence)
    }

    pub fn cut_range(&self) -> Option<CutRange> {
        self.first(Item::as_cut_range)
    }

    pub fn limit_duration(&self) -> Option<LimitDuration> {
        self.first(Item::as_limit_duration)
    }

    pub fn volume(&self) -> Option<Volume> {
        self.first(Item::as_volume)
    }

    pub fn silence_end(&self) -> Option<SilenceEnd> {
        self.first(Item::as_silence_end)
    }

    pub fn replaygain(&self) -> Option<ReplayGainTags> {
        self.first(Item::as_replaygain)
    }

    /// The speed ratio of all the tempo effects listed (1.0 when there is none).
    pub fn tempo_ratio(&self) -> f64 {
        self.ratio_before(self.items.len())
    }

    /// The speed ratio of the tempo effects listed before the item at `end`.
    fn ratio_before(&self, end: usize) -> f64 {
        self.items.iter().take(end).filter_map(Item::speed).filter(|r| *r > 0.0).product()
    }


    /// Does any effect need to know where the sound starts or stops?
    pub fn needs_scan(&self) -> bool {
        self.trim_silence().is_some() || self.silence_end().is_some()
    }

    /// Does any effect need the loudness, the peak or the ReplayGain figures of what is kept?
    pub fn needs_meter(&self) -> bool {
        self.replaygain().is_some() || self.volume().is_some_and(|v| v.mode != VolumeMode::Gain)
    }

    /// Does any effect use the album's gain?
    pub fn needs_album(&self) -> bool {
        self.replaygain().is_some_and(|r| r.album) || self.volume().is_some_and(|v| v.mode == VolumeMode::ReplayGain && v.album)
    }

    /// Does any effect need the file measured first?
    pub fn needs_analysis(&self) -> bool {
        self.needs_scan() || self.needs_meter()
    }

    /// Would the audio change?
    pub fn changes_audio(&self) -> bool {
        self.items.iter().any(|i| i.op() != Some(Op::TagsReplayGain))
    }

    /// The level (dBFS) below which the scan counts the sound as silence: the trim's own, else the silence at the end's.
    pub fn scan_threshold_db(&self) -> f64 {
        self.trim_silence().map(|t| t.threshold_db).or_else(|| self.silence_end().map(|s| s.threshold_db)).unwrap_or(-60.0)
    }

    /// The filters of the listed effects that are not measured, in the order listed, for the items in `range`. `length_s` is the length of what is kept before any tempo change: the fades are placed on it, taking the tempo
    /// effects listed above them into account. An effect that cannot be written is left out here (the job says why first, `try_chain`).
    pub fn chain(&self, range: std::ops::Range<usize>, length_s: f64) -> Vec<String> {
        self.try_chain(range, length_s).unwrap_or_default()
    }

    /// `chain`, or why an effect cannot be written.
    pub fn try_chain(&self, range: std::ops::Range<usize>, length_s: f64) -> Result<Vec<String>, String> {
        let Some(cat) = catalog() else {
            return if self.items.iter().any(|i| !i.is_measured()) { Err("the effects are not installed yet: they come with ffmpeg".to_owned()) } else { Ok(Vec::new()) };
        };
        let mut out = Vec::new();
        for (at, item) in self.items.iter().enumerate().take(range.end).skip(range.start) {
            if item.is_measured() {
                if item.op().is_none() {
                    return Err(format!("{}: this version of the app cannot do it", item.title()));
                }
                continue;
            }
            let effect = item.effect().ok_or_else(|| format!("{}: not available with this ffmpeg", item.title()))?;
            if !item.is_ready() {
                continue;
            }
            let ctx: Ctx = [("duration".to_owned(), format!("{}", length_s / self.ratio_before(at).max(1e-9)))].into_iter().collect();
            let built = fx::build(&effect, &cat.filters, &item.all_values(), &ctx).map_err(|e| format!("{}: {e}", item.title()))?;
            out.extend(built.audio.filter(|a| !a.is_empty()));
        }
        Ok(out)
    }

    /// Check every effect can be written (the job fails with the reason before anything runs).
    pub fn check(&self) -> Result<(), String> {
        self.try_chain(0..self.items.len(), 1.0).map(|_| ())
    }

    /// The place of the first volume effect in the list (the end of the list when there is none).
    fn volume_at(&self) -> usize {
        self.items.iter().position(Item::is_volume).unwrap_or(self.items.len())
    }
}

/// The sound's standard effects of the work view, in their fixed order: the same list in both converters (FFmpeg Audio Studio's whole standard
/// list, FFmpeg Video Studio's sound block).
pub const SOUND_STANDARD_ORDER: &[&str] = &[
    "trim-silence", "keep-range", "limit-duration", "channels", "equalizer", "compressor", "volume-loudness", "volume-peak", "volume-replaygain",
    "volume-gain", "sound-fade", "silence-end", "replaygain-tags",
];

/// What the measuring passes found out about one file.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Analysis {
    pub duration_s: f64,
    /// Where the sound starts: the end of a silence that begins at the start of the file. `None`
    /// when the file does not begin in silence.
    pub song_start_s: Option<f64>,
    /// Where the sound stops: the start of the silence that runs to the end. `None` when the file
    /// does not end in silence (the sound runs to the last sample).
    pub song_end_s: Option<f64>,
    /// Integrated loudness, LUFS. `None` for a file with no measurable sound.
    pub loudness_lufs: Option<f64>,
    /// True peak, dBFS.
    pub true_peak_db: Option<f64>,
    /// The highest sample, dBFS (`volumedetect`).
    pub max_volume_db: Option<f64>,
    /// ReplayGain's track gain (dB) and peak (linear) from ffmpeg's `replaygain` filter.
    pub rg_gain_db: Option<f64>,
    pub rg_peak: Option<f64>,
}

/// The album's ReplayGain figures: gain (dB) and the highest peak (linear) of its tracks.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Album {
    pub gain_db: f64,
    pub peak: f64,
}

/// The filter of the first pass: where the sound starts and stops.
pub fn scan_filter(threshold_db: f64) -> String {
    format!("silencedetect=n={threshold_db}dB:d=0.1")
}

/// The meters: highest sample, integrated loudness and true peak, ReplayGain.
pub const METER_FILTER: &str = "volumedetect,ebur128=peak=true:framelog=quiet,replaygain";

/// The filter of the second pass: the meters over the part that is kept, after everything that
/// changes how loud it is (the volume is set from what is measured here).
pub fn meter_filter(entry: &EffectsEntry, w: &Window, duration_s: f64) -> String {
    let mut parts: Vec<String> = Vec::new();
    parts.extend(w.cut_filter(duration_s));
    parts.extend(entry.chain(0..entry.volume_at(), w.length()));
    parts.push(METER_FILTER.to_owned());
    parts.join(",")
}

fn number_after(line: &str, key: &str) -> Option<f64> {
    let rest = line.split(key).nth(1)?;
    let token: String = rest
        .trim_start_matches([' ', '=', ':'])
        .chars()
        .take_while(|c| c.is_ascii_digit() || matches!(c, '+' | '-' | '.' | 'e' | 'E'))
        .collect();
    token.parse::<f64>().ok().filter(|v| v.is_finite())
}

/// Read what a measuring pass printed (its stderr) into an [`Analysis`]. `duration_s` is the
/// length of what was measured.
pub fn parse_analysis(stderr: &str, duration_s: f64) -> Analysis {
    let mut a = Analysis { duration_s, ..Analysis::default() };
    let mut last_start: Option<f64> = None;
    let mut in_true_peak = false;
    for raw in stderr.lines() {
        let line = raw.trim();
        if let Some(v) = line.split("silence_start:").nth(1) {
            last_start = v.trim().parse::<f64>().ok();
        } else if line.contains("silence_end:") {
            let end = number_after(line, "silence_end:");
            if let (Some(start), Some(end)) = (last_start, end) {
                // Closed at the end of the file: the sound stops where this silence began.
                if duration_s > 0.0 && (duration_s - end).abs() < 0.05 {
                    a.song_end_s = Some(start.max(0.0));
                }
                // Begun at the start of the file: the sound starts where this silence ends.
                if start <= 0.05 && a.song_start_s.is_none() {
                    a.song_start_s = Some(end.max(0.0));
                }
            }
            last_start = None;
        } else if line.starts_with("I:") && line.contains("LUFS") {
            a.loudness_lufs = number_after(line, "I:");
        } else if line.starts_with("True peak:") {
            in_true_peak = true;
        } else if in_true_peak && line.starts_with("Peak:") {
            a.true_peak_db = number_after(line, "Peak:");
            in_true_peak = false;
        } else if line.contains("max_volume:") {
            a.max_volume_db = number_after(line, "max_volume:");
        } else if line.contains("track_gain") {
            a.rg_gain_db = number_after(line, "track_gain =");
        } else if line.contains("track_peak") {
            a.rg_peak = number_after(line, "track_peak =");
        }
    }
    // A silence that was opened and never closed (ffmpeg printed no end) also runs to the end.
    if a.song_end_s.is_none() {
        if let Some(s) = last_start {
            a.song_end_s = Some(s.max(0.0));
        }
    }
    a
}

/// The part of the source that is kept, and what follows it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Window {
    pub start: f64,
    pub end: f64,
    /// Silence appended after the kept part.
    pub pad: f64,
}

impl Window {
    pub fn length(&self) -> f64 {
        (self.end - self.start).max(0.0)
    }

    /// The filter that keeps only the window, or `None` when the whole file is kept.
    pub fn cut_filter(&self, duration_s: f64) -> Option<String> {
        if self.start > 0.001 || self.end < duration_s - 0.001 {
            Some(format!("atrim=start={:.6}:end={:.6},asetpts=PTS-STARTPTS", self.start, self.end))
        } else {
            None
        }
    }
}

/// Which part of the source is kept, from the effects that cut: trim silence, cut range, limit
/// duration and the cut of silence at the end - in that order, each working on what is left by the
/// one before it.
pub fn window(entry: &EffectsEntry, a: &Analysis) -> Window {
    let dur = a.duration_s.max(0.0);
    let (mut start, mut end) = (0.0_f64, dur);
    let song_start = a.song_start_s.unwrap_or(0.0).clamp(0.0, dur);
    let song_end = a.song_end_s.unwrap_or(dur).clamp(0.0, dur);

    if let Some(t) = entry.trim_silence() {
        let margin = f64::from(t.margin_ms) / 1000.0;
        if t.start {
            start = (song_start - margin).max(0.0);
        }
        if t.end {
            end = (song_end + margin).min(dur);
        }
    }
    if let Some(c) = entry.cut_range() {
        let from = start + c.from_s;
        let to = if c.to_s > 0.0 { (start + c.to_s).min(end) } else { end };
        // A range that falls outside what is left keeps what is left, rather than nothing.
        if from < to {
            start = from;
            end = to;
        }
    }
    if let Some(l) = entry.limit_duration() {
        end = end.min(start + l.seconds);
    }
    let mut pad = 0.0;
    if let Some(s) = entry.silence_end() {
        let music_end = song_end.clamp(start, end);
        let existing = (end - music_end).max(0.0);
        let want = s.seconds.max(0.0);
        let margin = (f64::from(s.margin_ms) / 1000.0).min(want);
        // Keep a little of the original tail (never more than there is, never more than wanted); the
        // rest of the gap is appended, so the silence after the song is exactly `want`.
        let kept = existing.min(margin);
        end = music_end + kept;
        pad = (want - kept).max(0.0);
    }
    Window { start, end, pad }
}

/// Exact instructions for one file.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct EffectsPlan {
    /// Audio filters, in order, joined by a comma into one `-af`. Empty = the audio is not changed.
    pub filters: Vec<String>,
    /// How long the output will be, when the chain changes it.
    pub expected_s: Option<f64>,
    /// Tags to write after the cleaning has been applied: they describe the OUTPUT file.
    pub tags: Vec<(String, String)>,
    /// The gain applied, dB (for reporting and tests).
    pub applied_gain_db: Option<f64>,
}

impl EffectsPlan {
    pub fn af(&self) -> Option<String> {
        (!self.filters.is_empty()).then(|| self.filters.join(","))
    }
}

pub fn db_to_linear(db: f64) -> f64 {
    10f64.powf(db / 20.0)
}

/// Turn an entry, a file's analysis and (when an effect asked for it) its album into filter
/// arguments, the output length and the tags.
///
/// The order: what is kept is cut out, then the effects listed above the volume, the volume (a gain, with a limiter where it could
/// clip), the effects listed below it, and last the silence - so the gain was computed from the sound the effects above it leave, and
/// the silence is untouched.
pub fn plan(entry: &EffectsEntry, a: &Analysis, album: Option<&Album>) -> EffectsPlan {
    let mut p = EffectsPlan::default();
    let w = window(entry, a);
    // A tempo change moves every later time: the fades and the silence are placed on the new length.
    let ratio = entry.tempo_ratio();
    let length = w.length() / ratio;

    if let Some(cut) = w.cut_filter(a.duration_s) {
        p.filters.extend(cut.split(',').map(str::to_owned));
    }
    // What is listed above the volume, then the volume, then the rest.
    let split = entry.volume_at();
    p.filters.extend(entry.chain(0..split, w.length()));

    // The volume.
    let mut limit_to: Option<f64> = None;
    if let Some(v) = entry.volume() {
        let gain = match v.mode {
            VolumeMode::Loudness => a.loudness_lufs.map(|i| v.target_lufs - i),
            VolumeMode::Peak => a.max_volume_db.map(|m| v.target_peak_db - m),
            VolumeMode::ReplayGain => match (v.album, album) {
                (true, Some(al)) => Some(al.gain_db),
                _ => a.rg_gain_db,
            },
            VolumeMode::Gain => Some(v.gain_db),
        };
        if let Some(g) = gain {
            p.filters.push(format!("volume={g:.3}dB"));
            if v.mode != VolumeMode::Peak {
                let ceiling = db_to_linear(v.ceiling_db);
                p.filters.push(format!("alimiter=limit={ceiling:.6}:level=0"));
                limit_to = Some(ceiling);
            }
            p.applied_gain_db = Some(g);
        }
    }

    p.filters.extend(entry.chain(split..entry.items.len(), w.length()));

    if entry.silence_end().is_some() {
        p.filters.push(format!("apad=pad_dur={:.6}", w.pad));
    }

    let changed_length =
        w.cut_filter(a.duration_s).is_some() || entry.silence_end().is_some() || (ratio - 1.0).abs() > 1e-9;
    if changed_length {
        p.expected_s = Some(length + w.pad);
    }

    if let Some(r) = entry.replaygain() {
        if let (Some(g), Some(peak)) = (a.rg_gain_db, a.rg_peak) {
            // The tags describe the file that is written. If a gain was applied the remaining
            // adjustment is smaller by that gain, and the peak has moved with it (never past the
            // limiter's ceiling).
            let applied = p.applied_gain_db.unwrap_or(0.0);
            let moved = |peak: f64| {
                let moved = peak * db_to_linear(applied);
                limit_to.map_or(moved, |c| moved.min(c))
            };
            p.tags.push(("REPLAYGAIN_TRACK_GAIN".to_owned(), format!("{:+.2} dB", g - applied)));
            p.tags.push(("REPLAYGAIN_TRACK_PEAK".to_owned(), format!("{:.6}", moved(peak))));
            if let (true, Some(al)) = (r.album, album) {
                p.tags.push(("REPLAYGAIN_ALBUM_GAIN".to_owned(), format!("{:+.2} dB", al.gain_db - applied)));
                p.tags.push(("REPLAYGAIN_ALBUM_PEAK".to_owned(), format!("{:.6}", moved(al.peak))));
            }
        }
    }
    p
}
