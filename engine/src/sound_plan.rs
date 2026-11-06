//! What the sound's effects make of one file, once the file has been MEASURED: the sound's whole filter chain, the part of the clip that is
//! kept, how fast it plays, the silence added after it, the tags to write and how long the sound will last. The picture follows all of it
//! (the same cut, the same speed, the same extra seconds), so the two streams end together.
//!
//! The effects and their plan are the FFmpeg Audio Studio's (`measured`, the shared layer); what is added here is the picture's side. Nothing
//! here runs ffmpeg: the queue (and the preview) hand a function that does, and what the passes printed is read here.

use crate::{
    effects::{self, EffectsEntry, Item},
    measured::{self as m, Album, Analysis, Op, Window},
    trim::Trim,
};
use std::{ffi::OsString, path::Path};

/// How far the sound's length may differ from the picture's before the picture is cut or extended to meet it.
const FIT_TOLERANCE_S: f64 = 0.05;

/// One pass of ffmpeg over the file: its arguments (the program adds the output: to nowhere, with the stats), what it printed. `Ok(None)` = cancelled.
pub type Pass<'a> = &'a mut dyn FnMut(&[OsString]) -> Result<Option<String>, String>;

#[derive(Clone, Debug, PartialEq)]
pub struct SoundPlan {
    /// The sound's whole `-af` chain (the cut of the window is NOT in it: it is made on the input, for both streams). `None` = not planned:
    /// the effects' own chain stands; `Some("")` = planned, and no filter.
    pub chain: Option<String>,
    /// The part of the clip that is kept, in seconds from the clip's start; None keeps all of it.
    pub window: Option<(f64, f64)>,
    /// How many times faster the clip plays (the tempo effects).
    pub ratio: f64,
    /// Seconds of silence (and still picture) added after the kept part.
    pub pad_s: f64,
    /// How long the output lasts, when the effects change it.
    pub expected_s: Option<f64>,
    /// The sound's length when an effect changes the time in a way the picture cannot follow by itself (cut, repeat, remove silence): the
    /// picture is cut or extended at its end to meet it.
    pub fit_s: Option<f64>,
    /// The picture is stretched to `fit_s` (the sound was sped up or slowed down as a whole) instead of cut or extended.
    pub stretch: bool,
    /// ReplayGain tags describing the OUTPUT.
    pub tags: Vec<(String, String)>,
}

impl SoundPlan {
    /// Nothing measured, nothing changed: the sound is left to the effects' own chain.
    pub fn none() -> SoundPlan {
        SoundPlan { chain: None, window: None, ratio: 1.0, pad_s: 0.0, expected_s: None, fit_s: None, stretch: false, tags: Vec::new() }
    }

    /// The cut the user made, narrowed to the part the effects keep.
    pub fn trim(&self, user: Trim) -> Trim {
        let Some((from, to)) = self.window else { return user };
        let base = user.start_s.unwrap_or(0.0);
        Trim { start_s: Some(base + from), end_s: Some(base + to) }
    }

    /// Does the plan change how long the picture lasts or how fast it plays?
    pub fn changes_time(&self) -> bool {
        (self.ratio - 1.0).abs() > 1e-9 || self.pad_s > 0.0 || self.fit_s.is_some()
    }

    /// The filters that make the picture follow: its speed, then the extra seconds, then the cut or extension that meets the sound's length.
    /// Empty when the time is not changed.
    pub fn picture_filters(&self, kept_s: f64) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        if (self.ratio - 1.0).abs() > 1e-9 {
            out.push(format!("setpts=PTS/{:.6}", self.ratio));
        }
        if self.pad_s > 0.0 {
            out.push(format!("tpad=stop_mode=clone:stop_duration={:.6}", self.pad_s));
        }
        if let Some(sound) = self.fit_s {
            let picture = kept_s / self.ratio.max(1e-9) + self.pad_s;
            if self.stretch {
                if picture > 0.0 && (sound - picture).abs() > FIT_TOLERANCE_S {
                    out.push(format!("setpts=PTS*{:.6}", sound / picture));
                }
            } else if sound < picture - FIT_TOLERANCE_S {
                out.push(format!("trim=duration={sound:.6}"));
            } else if sound > picture + FIT_TOLERANCE_S {
                out.push(format!("tpad=stop_mode=clone:stop_duration={:.6}", sound - picture));
            }
        }
        out
    }
}

/// The picture's filter with `extra` (a chain) added at its end.
pub fn then(filter: effects::Filter, extra: &str) -> effects::Filter {
    use effects::Filter;
    match filter {
        Filter::None => Filter::Simple(extra.to_owned()),
        Filter::Simple(chain) => Filter::Simple(format!("{chain},{extra}")),
        Filter::Complex(graph) => Filter::Complex(match graph.strip_suffix("[outv]") {
            Some(head) => format!("{head},{extra}[outv]"),
            None => graph,
        }),
    }
}

/// Does this item play the whole clip faster or slower by its own (the catalogue says `stretch`)?
fn stretches(item: &Item) -> bool {
    let Some(cat) = effects::catalog() else { return false };
    item.key.strip_prefix("try:").and_then(|n| cat.filters.get(n)).is_some_and(|f| f.stretch)
}

/// Does this item change the sound's length in a way only a measurement tells (a filter that cuts, repeats, pads or removes silence by itself)?
/// A tempo effect (`speed`) is not one: its ratio is known.
pub fn changes_time_alone(item: &Item) -> bool {
    let Some(cat) = effects::catalog() else { return false };
    if item.is_measured() || item.speed().is_some() || !effects::is_sound(&item.key) {
        return false;
    }
    match item.key.strip_prefix("try:") {
        Some(name) => cat.filters.get(name).is_some_and(|f| f.changes.iter().any(|c| c == "time")),
        None => cat.effects.get(&item.key).is_some_and(|e| e.changes.iter().any(|c| c == "time")),
    }
}

impl EffectsEntry {
    /// Does any sound effect change the length or the speed (so the picture has to follow)?
    pub fn changes_time(&self) -> bool {
        self.items.iter().any(|i| {
            i.speed().is_some_and(|r| (r - 1.0).abs() > 1e-9) || i.op() == Some(Op::PadSilence) || changes_time_alone(i)
        })
    }

    /// The same list without the effects that change the length or the speed of the sound: all a copied picture can still go with.
    pub fn without_time_changes(&self) -> EffectsEntry {
        EffectsEntry { items: self.items.iter().filter(|i| i.speed().is_none() && i.op() != Some(Op::PadSilence) && !changes_time_alone(i)).cloned().collect(), ..self.clone() }
    }

    /// Does the command need the sound's plan: a measured effect (a cut, a volume, the silence, the tags - even one that measures nothing, as
    /// a range does), a change of the speed, or a filter that changes the length by itself?
    pub fn needs_plan(&self) -> bool {
        self.items.iter().any(|i| i.is_measured() || changes_time_alone(i)) || self.changes_time()
    }
}

/// The seconds of the last `time=` ffmpeg printed (`time=00:01:02.50`).
pub fn last_time_s(text: &str) -> Option<f64> {
    let rest = text.rsplit("time=").next().filter(|_| text.contains("time="))?;
    let stamp = rest.split_whitespace().next()?;
    let mut seconds = 0.0;
    for part in stamp.split(':') {
        seconds = seconds * 60.0 + part.parse::<f64>().ok()?;
    }
    Some(seconds)
}

fn clip_args(input: &Path, trim: Trim, filter: &str) -> Vec<OsString> {
    let mut a: Vec<OsString> = Vec::new();
    if let Some(s) = trim.start_s {
        a.push("-ss".into());
        a.push(format!("{s:.3}").into());
    }
    if let Some(l) = trim.length_s() {
        a.push("-t".into());
        a.push(format!("{l:.3}").into());
    }
    a.extend(["-i".into(), input.into(), "-map".into(), "0:a:0".into(), "-vn".into(), "-af".into(), filter.into()]);
    a
}

/// Measure what `entry` needs of the clip (the file from the user's cut) and make the plan. `clip_s` is the clip's length, `has_sound`
/// whether the file has a sound at all (a file without one is planned from nothing measured). `Ok(None)` = cancelled.
pub fn measure(entry: &EffectsEntry, input: &Path, user: Trim, clip_s: f64, has_sound: bool, album: Option<Album>, run: Pass) -> Result<Option<SoundPlan>, String> {
    let (scan, meter) = (entry.needs_scan(), entry.needs_meter());
    let mut a = Analysis { duration_s: clip_s, ..Analysis::default() };
    if has_sound {
        if scan {
            let Some(text) = run(&clip_args(input, user, &m::scan_filter(entry.scan_threshold_db())))? else { return Ok(None) };
            a = m::parse_analysis(&text, clip_s);
        }
        if meter {
            let w = m::window(entry, &a);
            let Some(text) = run(&clip_args(input, user, &m::meter_filter(entry, &w, clip_s)))? else { return Ok(None) };
            let m = m::parse_analysis(&text, w.length() / entry.tempo_ratio());
            a.loudness_lufs = m.loudness_lufs;
            a.true_peak_db = m.true_peak_db;
            a.max_volume_db = m.max_volume_db;
            a.rg_gain_db = m.rg_gain_db;
            a.rg_peak = m.rg_peak;
        }
    }
    let ep = m::plan(entry, &a, album.as_ref());
    let w: Window = m::window(entry, &a);
    let cut = w.cut_filter(clip_s);
    let mut filters = ep.filters.clone();
    // the cut is made on the input, for both streams: it leaves the sound's chain
    if let Some(cut) = &cut {
        let parts: Vec<&str> = cut.split(',').collect();
        if filters.len() >= parts.len() && filters[..parts.len()].iter().map(String::as_str).eq(parts.iter().copied()) {
            filters.drain(..parts.len());
        }
    }
    let mut plan = SoundPlan {
        chain: Some(filters.join(",")),
        window: cut.is_some().then_some((w.start, w.end)),
        ratio: entry.tempo_ratio(),
        pad_s: if entry.silence_end().is_some() { w.pad } else { 0.0 },
        expected_s: ep.expected_s,
        fit_s: None,
        stretch: entry.items.iter().any(stretches),
        tags: ep.tags.clone(),
    };
    // A filter that cuts, repeats or removes silence by itself: only the sound it makes tells how long it lasts.
    if has_sound && entry.items.iter().any(changes_time_alone) {
        if let Some(chain) = ep.af() {
            let Some(text) = run(&clip_args(input, user, &chain))? else { return Ok(None) };
            plan.fit_s = last_time_s(&text);
            plan.expected_s = plan.fit_s;
        }
    }
    Ok(Some(plan))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_last_time_ffmpeg_printed_is_read() {
        let text = "size=N/A time=00:00:03.20 bitrate=N/A\nsize=N/A time=00:01:02.50 bitrate=N/A speed=900x\n";
        assert_eq!(last_time_s(text), Some(62.5));
        assert_eq!(last_time_s("nothing"), None);
    }

    #[test]
    fn the_picture_follows_the_speed_the_extra_seconds_and_the_length_of_the_sound() {
        let mut p = SoundPlan::none();
        assert!(p.picture_filters(10.0).is_empty() && !p.changes_time());
        p.ratio = 2.0;
        assert_eq!(p.picture_filters(10.0), ["setpts=PTS/2.000000"]);
        p.pad_s = 1.5;
        assert_eq!(p.picture_filters(10.0), ["setpts=PTS/2.000000", "tpad=stop_mode=clone:stop_duration=1.500000"]);
        // the sound turns out shorter than the picture: the picture is cut; longer: it is extended
        p.pad_s = 0.0;
        p.fit_s = Some(3.0);
        assert!(p.picture_filters(10.0).iter().any(|f| f == "trim=duration=3.000000"));
        p.fit_s = Some(7.0);
        assert!(p.picture_filters(10.0).iter().any(|f| f == "tpad=stop_mode=clone:stop_duration=2.000000"));
        // within the tolerance nothing is done
        p.fit_s = Some(5.01);
        assert!(p.picture_filters(10.0).iter().all(|f| f.starts_with("setpts")));
    }

    #[test]
    fn the_window_narrows_the_users_own_cut() {
        let mut p = SoundPlan::none();
        assert_eq!(p.trim(Trim { start_s: Some(4.0), end_s: None }), Trim { start_s: Some(4.0), end_s: None });
        p.window = Some((1.0, 6.0));
        assert_eq!(p.trim(Trim { start_s: Some(4.0), end_s: Some(20.0) }), Trim { start_s: Some(5.0), end_s: Some(10.0) });
        assert_eq!(p.trim(Trim::none()), Trim { start_s: Some(1.0), end_s: Some(6.0) });
    }

    #[test]
    fn a_stretching_filter_scales_the_picture_to_the_new_length_and_a_chain_is_added_to_the_end_of_any_filter() {
        let mut p = SoundPlan::none();
        p.stretch = true;
        p.fit_s = Some(4.0);
        assert_eq!(p.picture_filters(8.0), ["setpts=PTS*0.500000"]);
        use crate::effects::Filter;
        assert_eq!(then(Filter::None, "setpts=PTS/2"), Filter::Simple("setpts=PTS/2".to_owned()));
        assert_eq!(then(Filter::Simple("a".to_owned()), "b"), Filter::Simple("a,b".to_owned()));
        assert_eq!(then(Filter::Complex("[0:v]a[outv]".to_owned()), "b"), Filter::Complex("[0:v]a,b[outv]".to_owned()));
    }
}
