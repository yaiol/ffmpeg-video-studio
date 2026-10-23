//! Effects: what is done to the picture besides its size and its colour LUT - a turn or flip, a
//! watermark picture, a line of text. A named entry the user keeps in Settings and picks in the window,
//! apart from the preset, holding a list of effects; the same entry goes with any codec.
//!
//! The kinds run in a fixed order, each at most once. BEFORE the size cap, so it applies to the picture as
//! it is delivered: **deinterlace** (it must see the fields as they are), **crop**, **frame rate**,
//! **turn or flip**, **fit in 16:9 on a blurred background**, **burn the subtitles in**.
//! AFTER it, so margins are in the delivered picture's own pixels and a reversed clip holds smaller frames:
//! **stereoscopic**, **play backwards**, the **watermark**, the **text** (last, so a countdown is not reversed).
//! A watermark is a second input file and the blurred background splits the picture in two, so either turns
//! the video filter into a filter graph; the rest are ordinary filters on the one input. A copied video
//! stream takes none of it - except **remove the metadata**, which is not a filter and works on a copy too.

use crate::{lut::Lut, rotate::Rotation, size::Size, trim::Trim};
use serde::{Deserialize, Serialize};
use std::path::Path;

/// Id of the window-only choice that applies no effects. Not an entry of Settings; the window names it.
pub const NONE_ID: &str = "none";

/// The corner of the picture an overlay sits in.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Corner {
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
}

impl Corner {
    pub const ALL: [Corner; 4] = [Corner::TopLeft, Corner::TopRight, Corner::BottomLeft, Corner::BottomRight];

    pub fn id(self) -> &'static str {
        match self {
            Corner::TopLeft => "topleft",
            Corner::TopRight => "topright",
            Corner::BottomLeft => "bottomleft",
            Corner::BottomRight => "bottomright",
        }
    }

    pub fn from_id(id: &str) -> Option<Corner> {
        Corner::ALL.into_iter().find(|c| c.id() == id)
    }

    /// `x` and `y` of an overlay, from the picture's edges: `own_w` / `own_h` are what the overlay's own
    /// width and height are called in the filter (`overlay_w`, or `tw` for text) and `w` / `h` the picture's.
    fn position(self, margin: u32, w: &str, h: &str, own_w: &str, own_h: &str) -> (String, String) {
        let left = margin.to_string();
        let right = format!("{w}-{own_w}-{margin}");
        let top = margin.to_string();
        let bottom = format!("{h}-{own_h}-{margin}");
        match self {
            Corner::TopLeft => (left, top),
            Corner::TopRight => (right, top),
            Corner::BottomLeft => (left, bottom),
            Corner::BottomRight => (right, bottom),
        }
    }
}

/// A picture laid over the video.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Watermark {
    /// The image file (PNG with transparency is the usual).
    pub path: String,
    pub corner: Corner,
    /// Distance from the edges, in pixels of the delivered picture.
    pub margin: u32,
    /// Scale the image to this width (its shape kept); None keeps its own size.
    pub width: Option<u32>,
    /// 100 = as it is; less lets the video show through.
    pub opacity: u32,
}

impl Default for Watermark {
    fn default() -> Self {
        Watermark { path: String::new(), corner: Corner::BottomRight, margin: 20, width: None, opacity: 100 }
    }
}

impl Watermark {
    pub fn file_exists(&self) -> bool {
        !self.path.is_empty() && Path::new(&self.path).is_file()
    }
}

/// A line of text laid over the video.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct TextOverlay {
    /// The text. `{name}` is the file's name, `{elapsed}` and `{remaining}` the time in minutes:seconds.
    pub text: String,
    pub corner: Corner,
    pub margin: u32,
    /// The letters' height as a percentage of the picture's height.
    pub size: u32,
    /// A dark box behind the text, so it reads on any picture.
    pub boxed: bool,
}

impl Default for TextOverlay {
    fn default() -> Self {
        TextOverlay { text: "{name}".to_owned(), corner: Corner::TopLeft, margin: 20, size: 3, boxed: true }
    }
}

/// Pixels cut from each edge of the picture.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Crop {
    pub left: u32,
    pub right: u32,
    pub top: u32,
    pub bottom: u32,
}

impl Crop {
    /// The most that can be cut from one edge.
    pub const MAX: u32 = 4000;

    /// The ffmpeg filter, or None when nothing is cut. The width and the height are made even (an encoder
    /// refuses an odd picture), after the cut.
    pub fn filter(&self) -> Option<String> {
        if self.left + self.right + self.top + self.bottom == 0 {
            return None;
        }
        Some(format!(
            "crop=w=trunc((iw-{})/2)*2:h=trunc((ih-{})/2)*2:x={}:y={}",
            self.left + self.right,
            self.top + self.bottom,
            self.left,
            self.top
        ))
    }
}

/// The frame rates the window offers, as (what the user reads, what ffmpeg is given). The NTSC rates are
/// exact fractions.
pub const FRAME_RATES: [(&str, &str); 9] = [
    ("15", "15"),
    ("23.976", "24000/1001"),
    ("24", "24"),
    ("25", "25"),
    ("29.97", "30000/1001"),
    ("30", "30"),
    ("50", "50"),
    ("59.94", "60000/1001"),
    ("60", "60"),
];

/// The picture is re-timed to a fixed number of frames a second (frames are dropped or repeated).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct FrameRate {
    /// What ffmpeg's `fps` filter is given: `25`, `30000/1001`.
    pub value: String,
}

impl Default for FrameRate {
    fn default() -> Self {
        FrameRate { value: "30".to_owned() }
    }
}

impl FrameRate {
    /// One of the offered rates; anything else (a hand-edited file) becomes 30.
    fn normalized(mut self) -> FrameRate {
        if !FRAME_RATES.iter().any(|(_, v)| *v == self.value) {
            self.value = "30".to_owned();
        }
        self
    }
}

/// A vertical video centred on a blurred copy of itself, in a 16:9 picture.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct BlurFit {
    /// The height of the 16:9 picture.
    pub height: u32,
    /// How strongly the background is blurred (gblur's sigma).
    pub blur: u32,
}

impl Default for BlurFit {
    fn default() -> Self {
        BlurFit { height: 1080, blur: 20 }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Effect {
    /// Turn interlaced fields into whole frames (`yadif`): old TV and DV material.
    Deinterlace,
    Crop(Crop),
    Fps(FrameRate),
    Rotate { rotation: Rotation },
    BlurFit(BlurFit),
    /// Draw the file's first subtitle stream into the picture; the output then carries no subtitle track.
    Burn,
    /// Frames that alternate left eye, right eye become one picture with both side by side.
    Stereo,
    /// Play the video and its sound backwards.
    Reverse,
    Watermark(Watermark),
    Text(TextOverlay),
    /// Leave out the file's metadata and chapters. Not a filter: it works on a copied stream too.
    Strip,
}

/// The kinds of effect, in the order they run.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Kind {
    Deinterlace,
    Crop,
    Fps,
    Rotate,
    BlurFit,
    Burn,
    Stereo,
    Reverse,
    Watermark,
    Text,
    Strip,
}

impl Kind {
    pub const ALL: [Kind; 11] = [
        Kind::Deinterlace,
        Kind::Crop,
        Kind::Fps,
        Kind::Rotate,
        Kind::BlurFit,
        Kind::Burn,
        Kind::Stereo,
        Kind::Reverse,
        Kind::Watermark,
        Kind::Text,
        Kind::Strip,
    ];

    pub fn id(self) -> &'static str {
        match self {
            Kind::Deinterlace => "deinterlace",
            Kind::Crop => "crop",
            Kind::Fps => "fps",
            Kind::Rotate => "rotate",
            Kind::BlurFit => "blurfit",
            Kind::Burn => "burn",
            Kind::Stereo => "stereo",
            Kind::Reverse => "reverse",
            Kind::Watermark => "watermark",
            Kind::Text => "text",
            Kind::Strip => "strip",
        }
    }

    pub fn from_id(id: &str) -> Option<Kind> {
        Kind::ALL.into_iter().find(|k| k.id() == id)
    }

    /// A new effect of this kind with its sensible starting values.
    pub fn new_effect(self) -> Effect {
        match self {
            Kind::Deinterlace => Effect::Deinterlace,
            Kind::Crop => Effect::Crop(Crop::default()),
            Kind::Fps => Effect::Fps(FrameRate::default()),
            Kind::Rotate => Effect::Rotate { rotation: Rotation::Cw },
            Kind::BlurFit => Effect::BlurFit(BlurFit::default()),
            Kind::Burn => Effect::Burn,
            Kind::Stereo => Effect::Stereo,
            Kind::Reverse => Effect::Reverse,
            Kind::Watermark => Effect::Watermark(Watermark::default()),
            Kind::Text => Effect::Text(TextOverlay::default()),
            Kind::Strip => Effect::Strip,
        }
    }
}

impl Effect {
    pub fn kind(&self) -> Kind {
        match self {
            Effect::Deinterlace => Kind::Deinterlace,
            Effect::Crop(_) => Kind::Crop,
            Effect::Fps(_) => Kind::Fps,
            Effect::Rotate { .. } => Kind::Rotate,
            Effect::BlurFit(_) => Kind::BlurFit,
            Effect::Burn => Kind::Burn,
            Effect::Stereo => Kind::Stereo,
            Effect::Reverse => Kind::Reverse,
            Effect::Watermark(_) => Kind::Watermark,
            Effect::Text(_) => Kind::Text,
            Effect::Strip => Kind::Strip,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct EffectsEntry {
    pub id: String,
    pub name: String,
    pub items: Vec<Effect>,
}

impl EffectsEntry {
    /// The window-only "(None)": nothing is done.
    pub fn none() -> EffectsEntry {
        EffectsEntry { id: NONE_ID.to_owned(), ..EffectsEntry::default() }
    }

    pub fn new(name: &str) -> EffectsEntry {
        EffectsEntry { id: crate::preset::new_id(), name: name.to_owned(), items: Vec::new() }
    }

    pub fn is_none(&self) -> bool {
        self.id == NONE_ID
    }

    pub fn get(&self, kind: Kind) -> Option<&Effect> {
        self.items.iter().find(|e| e.kind() == kind)
    }

    /// Whether the file's metadata and chapters are left out.
    pub fn strips_metadata(&self) -> bool {
        self.get(Kind::Strip).is_some()
    }

    /// The audio filter some effect needs, which means the sound cannot be copied: playing backwards.
    pub fn audio_filter(&self) -> Option<&'static str> {
        self.get(Kind::Reverse).map(|_| "areverse")
    }

    pub fn watermark(&self) -> Option<&Watermark> {
        self.items.iter().find_map(|e| if let Effect::Watermark(w) = e { Some(w) } else { None })
    }

    /// The same entry with each kind at most once (the first of a kind wins), in running order, and every
    /// number inside what it can be.
    pub fn normalized(mut self) -> EffectsEntry {
        let mut seen = Vec::new();
        self.items.retain(|e| {
            let k = e.kind();
            let first = !seen.contains(&k);
            seen.push(k);
            first
        });
        self.items.sort_by_key(Effect::kind);
        for e in &mut self.items {
            match e {
                Effect::Rotate { rotation } => {
                    if *rotation == Rotation::None {
                        *rotation = Rotation::Cw;
                    }
                }
                Effect::Watermark(w) => {
                    w.path = w.path.trim().trim_matches('"').trim().to_owned();
                    w.margin = w.margin.min(2000);
                    w.width = w.width.filter(|n| *n > 0).map(|n| n.clamp(16, 4096));
                    w.opacity = w.opacity.clamp(10, 100);
                }
                Effect::Text(t) => {
                    t.margin = t.margin.min(2000);
                    t.size = t.size.clamp(1, 20);
                }
                Effect::BlurFit(b) => {
                    b.height = b.height.clamp(120, 4320);
                    b.blur = b.blur.clamp(1, 100);
                }
                Effect::Crop(c) => {
                    for edge in [&mut c.left, &mut c.right, &mut c.top, &mut c.bottom] {
                        *edge = (*edge).min(Crop::MAX);
                    }
                }
                Effect::Fps(f) => *f = std::mem::take(f).normalized(),
                Effect::Deinterlace | Effect::Burn | Effect::Stereo | Effect::Reverse | Effect::Strip => {}
            }
        }
        self
    }
}

/// What a fresh install starts with: one entry for each way of turning or flipping the picture - what
/// the owner's FFmpeg Batch presets did - with stable ids so a saved selection survives.
pub fn defaults() -> Vec<EffectsEntry> {
    [
        ("cw", "Rotate 90° clockwise", Rotation::Cw),
        ("ccw", "Rotate 90° counter-clockwise", Rotation::Ccw),
        ("cwflip", "Rotate 90° clockwise and flip vertically", Rotation::CwFlip),
        ("ccwflip", "Rotate 90° counter-clockwise and flip vertically", Rotation::CcwFlip),
        ("half", "Rotate 180°", Rotation::Half),
        ("fliph", "Flip horizontally", Rotation::FlipH),
        ("flipv", "Flip vertically", Rotation::FlipV),
    ]
    .into_iter()
    .map(|(id, name, rotation)| EffectsEntry {
        id: format!("default-{id}"),
        name: name.to_owned(),
        items: vec![Effect::Rotate { rotation }],
    })
    .collect()
}

/// Everything the picture's treatment depends on for one file.
pub struct Look<'a> {
    pub size: &'a Size,
    pub lut: &'a Lut,
    pub effects: &'a EffectsEntry,
    /// The length of the OUTPUT in seconds, for the countdown (what is left after a trim); 0 when unknown.
    pub duration_s: f64,
    /// The part of the file that is kept.
    pub trim: Trim,
}

/// How the video is filtered.
#[derive(Debug, PartialEq)]
pub enum Filter {
    /// Nothing to do.
    None,
    /// An ordinary video filter chain on the one input (`-vf`).
    Simple(String),
    /// A filter graph over more than one input, ending in the label `[outv]` (`-filter_complex`).
    Complex(String),
}

#[derive(Debug, PartialEq)]
pub struct Plan {
    /// Extra input files, in order: `[1:v]` is the first.
    pub inputs: Vec<String>,
    pub filter: Filter,
}

/// A path as a drawtext option's value: for the filter's own two levels of escaping, forward slashes and
/// the drive colon escaped, inside quotes.
fn quoted_path(path: &str) -> String {
    format!("'{}'", path.replace('\\', "/").replace(':', "\\:").replace('\'', "'\\''"))
}

/// The font file: Arial from the Windows font folder when there is one; elsewhere the filter's own
/// default (fontconfig) stands.
fn default_font() -> Option<String> {
    let p = Path::new("C:/Windows/Fonts/arial.ttf");
    cfg!(windows).then_some(p).filter(|p| p.is_file()).map(|p| p.to_string_lossy().into_owned())
}

/// A seconds expression as minutes and seconds, two digits each (drawtext's `eif`).
fn minutes_seconds(expr: &str) -> String {
    format!("%{{eif\\:trunc(({expr})/60)\\:d\\:2}}\\:%{{eif\\:trunc(mod(({expr})\\,60))\\:d\\:2}}")
}

/// The text of a drawtext filter, for its two levels of escaping, with the tokens replaced. The file's
/// name is plain text (a `%` or `:` in it escaped, a quote turned into a typographic one); the times
/// are drawtext expansions that follow the playing time.
pub fn drawtext_text(template: &str, stem: &str, duration_s: f64) -> String {
    let plain = |s: &str| s.replace('\\', "/").replace('\'', "\u{2019}").replace('%', "\\%").replace(':', "\\:");
    let name = plain(stem);
    let remaining = minutes_seconds(&format!("max(0\\,{duration_s:.3}-t)"));
    let mut out = String::new();
    let mut rest = template;
    while let Some(open) = rest.find('{') {
        out.push_str(&plain(&rest[..open]));
        let after = &rest[open..];
        let (token, len) = if after.starts_with("{name}") {
            (name.clone(), 6)
        } else if after.starts_with("{elapsed}") {
            (minutes_seconds("t"), 9)
        } else if after.starts_with("{remaining}") {
            (remaining.clone(), 11)
        } else {
            ("{".to_owned(), 1)
        };
        out.push_str(&token);
        rest = &after[len..];
    }
    out.push_str(&plain(rest));
    out
}

fn text_filter(t: &TextOverlay, stem: &str, duration_s: f64) -> Option<String> {
    if t.text.trim().is_empty() {
        return None;
    }
    let (x, y) = t.corner.position(t.margin, "w", "h", "tw", "th");
    let mut f = String::from("drawtext=");
    if let Some(font) = default_font() {
        f.push_str(&format!("fontfile={}:", quoted_path(&font)));
    }
    f.push_str(&format!(
        "text='{}':fontcolor=white:fontsize=h*{:.3}:x={x}:y={y}",
        drawtext_text(&t.text, stem, duration_s),
        f64::from(t.size) / 100.0
    ));
    if t.boxed {
        f.push_str(":box=1:boxcolor=black@0.5:boxborderw=8");
    }
    Some(f)
}

/// One step of the picture's treatment, in running order.
enum Step {
    /// A filter on the stream as it is.
    Filter(String),
    /// The 16:9 picture on its blurred copy: a graph of its own.
    Blur(BlurFit),
    /// A picture laid over the video: a second input.
    Overlay(Watermark),
}

/// The treatment in order: the colour LUT; the effects before the size cap; the size cap; the effects after it.
fn steps(look: &Look, input: &Path) -> Vec<Step> {
    let effects = look.effects.clone().normalized();
    let stem = input.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    let mut steps: Vec<Step> = look.lut.filter().map(Step::Filter).into_iter().collect();
    for e in &effects.items {
        match e {
            Effect::Deinterlace => steps.push(Step::Filter("yadif".to_owned())),
            Effect::Crop(c) => steps.extend(c.filter().map(Step::Filter)),
            Effect::Fps(f) => steps.push(Step::Filter(format!("fps={}", f.value))),
            Effect::Rotate { rotation } => steps.extend(rotation.filter().map(|f| Step::Filter(f.to_owned()))),
            Effect::BlurFit(b) => steps.push(Step::Blur(b.clone())),
            Effect::Burn => steps.push(Step::Filter(format!("subtitles=filename={}:si=0", quoted_path(&input.to_string_lossy())))),
            _ => {}
        }
    }
    steps.extend(look.size.clone().normalized().scale_filter().map(Step::Filter));
    for e in &effects.items {
        match e {
            Effect::Stereo => steps.push(Step::Filter("stereo3d=al:sbsl".to_owned())),
            Effect::Reverse => steps.push(Step::Filter("reverse".to_owned())),
            Effect::Watermark(w) if !w.path.is_empty() => steps.push(Step::Overlay(w.clone())),
            Effect::Text(t) => steps.extend(text_filter(t, &stem, look.duration_s).map(Step::Filter)),
            _ => {}
        }
    }
    steps
}

/// A filter graph being written: the finished parts, the stream the next one reads, and the plain filters
/// waiting to be written as one part.
struct Graph {
    parts: Vec<String>,
    current: String,
    pending: Vec<String>,
    next: usize,
}

impl Graph {
    fn new() -> Graph {
        Graph { parts: Vec::new(), current: "[0:v]".to_owned(), pending: Vec::new(), next: 0 }
    }

    fn label(&mut self) -> String {
        let l = format!("[v{}]", self.next);
        self.next += 1;
        l
    }

    /// Write the waiting plain filters as one part.
    fn flush(&mut self) {
        if self.pending.is_empty() {
            return;
        }
        let out = self.label();
        self.parts.push(format!("{}{}{out}", self.current, self.pending.join(",")));
        self.current = out;
        self.pending.clear();
    }

    /// The whole graph, its last stream named `[outv]`.
    fn finish(mut self) -> String {
        self.flush();
        let mut graph = self.parts.join(";");
        graph.truncate(graph.len() - self.current.len());
        graph.push_str("[outv]");
        graph
    }
}

/// Work out the video filtering for a file: the extra inputs and the filter. Plain filters make one `-vf`
/// chain; a watermark or the blurred background make a filter graph.
pub fn plan(look: &Look, input: &Path) -> Plan {
    let steps = steps(look, input);
    if steps.iter().all(|s| matches!(s, Step::Filter(_))) {
        let chain: Vec<String> = steps.into_iter().filter_map(|s| if let Step::Filter(f) = s { Some(f) } else { None }).collect();
        let filter = if chain.is_empty() { Filter::None } else { Filter::Simple(chain.join(",")) };
        return Plan { inputs: Vec::new(), filter };
    }
    let mut graph = Graph::new();
    let mut inputs = Vec::new();
    for step in steps {
        match step {
            Step::Filter(f) => graph.pending.push(f),
            Step::Blur(b) => {
                graph.flush();
                let (src, out) = (graph.current.clone(), graph.label());
                // The copy is scaled until it is as wide as a 16:9 picture of the same height, cut to that
                // shape and blurred; the picture goes in its middle; the whole is then set to the height.
                graph.parts.push(format!(
                    "{src}split[bo][bc];[bc]scale=ih*16/9:-1,crop=h=iw*9/16,gblur=sigma={}[bb];[bb][bo]overlay=(main_w-overlay_w)/2:(main_h-overlay_h)/2,scale=-2:{}{out}",
                    b.blur, b.height
                ));
                graph.current = out;
            }
            Step::Overlay(w) => {
                graph.flush();
                inputs.push(w.path.clone());
                let mut mark_filters = Vec::new();
                if let Some(width) = w.width {
                    mark_filters.push(format!("scale={width}:-2"));
                }
                if w.opacity < 100 {
                    mark_filters.push(format!("format=rgba,colorchannelmixer=aa={:.2}", f64::from(w.opacity) / 100.0));
                }
                let mark = if mark_filters.is_empty() {
                    "[1:v]".to_owned()
                } else {
                    graph.parts.push(format!("[1:v]{}[wm]", mark_filters.join(",")));
                    "[wm]".to_owned()
                };
                let (src, out) = (graph.current.clone(), graph.label());
                let (x, y) = w.corner.position(w.margin, "main_w", "main_h", "overlay_w", "overlay_h");
                graph.parts.push(format!("{src}{mark}overlay=x={x}:y={y}{out}"));
                graph.current = out;
            }
        }
    }
    Plan { inputs, filter: Filter::Complex(graph.finish()) }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wm(corner: Corner) -> Effect {
        Effect::Watermark(Watermark { path: "D:\\logo.png".into(), corner, ..Watermark::default() })
    }

    fn entry(items: Vec<Effect>) -> EffectsEntry {
        EffectsEntry { items, ..EffectsEntry::new("t") }
    }

    fn plan_of(e: &EffectsEntry, size: &Size, lut: &Lut) -> Plan {
        plan(&Look { size, lut, effects: e, duration_s: 125.0, trim: Trim::none() }, Path::new("D:/Films/Holiday.mp4"))
    }

    #[test]
    fn nothing_chosen_filters_nothing() {
        let p = plan_of(&EffectsEntry::none(), &Size::original(), &Lut::none());
        assert_eq!(p, Plan { inputs: Vec::new(), filter: Filter::None });
    }

    #[test]
    fn a_turn_alone_is_a_plain_video_filter_after_the_lut_and_before_the_size() {
        let e = entry(vec![Effect::Rotate { rotation: Rotation::Ccw }]);
        let size = Size { max_height: Some(720), ..Size::new("s") };
        let lut = Lut { path: "x.cube".into(), ..Lut::new("l") };
        let Filter::Simple(f) = plan_of(&e, &size, &lut).filter else { panic!() };
        let (a, b, c) = (f.find("lut3d").unwrap(), f.find("transpose=2").unwrap(), f.find("scale=").unwrap());
        assert!(a < b && b < c, "{f}");
    }

    #[test]
    fn a_watermark_makes_a_filter_graph_with_a_second_input() {
        let e = entry(vec![wm(Corner::BottomRight)]);
        let p = plan_of(&e, &Size::original(), &Lut::none());
        assert_eq!(p.inputs, vec!["D:\\logo.png".to_owned()]);
        assert_eq!(p.filter, Filter::Complex("[0:v][1:v]overlay=x=main_w-overlay_w-20:y=main_h-overlay_h-20[outv]".to_owned()));
    }

    #[test]
    fn a_resized_translucent_watermark_is_scaled_and_faded_before_the_overlay() {
        let mut w = Watermark { path: "l.png".into(), corner: Corner::BottomLeft, width: Some(150), opacity: 60, ..Watermark::default() };
        w.margin = 10;
        let size = Size { max_height: Some(720), ..Size::new("s") };
        let Filter::Complex(g) = plan_of(&entry(vec![Effect::Watermark(w)]), &size, &Lut::none()).filter else { panic!() };
        assert!(g.starts_with("[0:v]scale=w=iw:h='min(ih,720)'"), "{g}");
        assert!(g.contains("[1:v]scale=150:-2,format=rgba,colorchannelmixer=aa=0.60[wm];"), "{g}");
        assert!(g.ends_with("[v0][wm]overlay=x=10:y=main_h-overlay_h-10[outv]"), "{g}");
    }

    #[test]
    fn the_text_follows_the_watermark_in_the_same_graph() {
        let e = entry(vec![wm(Corner::TopRight), Effect::Text(TextOverlay::default())]);
        let Filter::Complex(g) = plan_of(&e, &Size::original(), &Lut::none()).filter else { panic!() };
        assert!(g.contains("overlay=x=main_w-overlay_w-20:y=20[v0];[v0]drawtext="), "{g}");
        assert!(g.ends_with("[outv]"), "{g}");
    }

    #[test]
    fn text_alone_is_a_plain_filter_and_empty_text_is_nothing() {
        let e = entry(vec![Effect::Text(TextOverlay { corner: Corner::BottomRight, ..TextOverlay::default() })]);
        let Filter::Simple(f) = plan_of(&e, &Size::original(), &Lut::none()).filter else { panic!() };
        assert!(f.starts_with("drawtext=") && f.contains("x=w-tw-20:y=h-th-20") && f.contains("box=1"), "{f}");
        let blank = entry(vec![Effect::Text(TextOverlay { text: "  ".into(), ..TextOverlay::default() })]);
        assert_eq!(plan_of(&blank, &Size::original(), &Lut::none()).filter, Filter::None);
    }

    #[test]
    fn the_tokens_are_replaced_and_the_name_cannot_break_the_filter() {
        assert_eq!(drawtext_text("{name}", "a:b'c%d", 10.0), "a\\:b\u{2019}c\\%d");
        let t = drawtext_text("{elapsed} / {remaining}", "x", 125.0);
        assert!(t.contains("%{eif\\:trunc((t)/60)\\:d\\:2}"), "{t}");
        assert!(t.contains("max(0\\,125.000-t)"), "{t}");
        // An unknown brace stays what it is.
        assert_eq!(drawtext_text("{what}", "x", 1.0), "{what}");
    }

    #[test]
    fn each_kind_is_kept_once_in_running_order_and_numbers_are_bounded() {
        let e = entry(vec![
            Effect::Text(TextOverlay { size: 99, ..TextOverlay::default() }),
            wm(Corner::TopLeft),
            Effect::Rotate { rotation: Rotation::Cw },
            Effect::Rotate { rotation: Rotation::Ccw },
        ])
        .normalized();
        assert_eq!(e.items.iter().map(Effect::kind).collect::<Vec<_>>(), vec![Kind::Rotate, Kind::Watermark, Kind::Text]);
        assert_eq!(e.get(Kind::Rotate), Some(&Effect::Rotate { rotation: Rotation::Cw }));
        let Some(Effect::Text(t)) = e.get(Kind::Text) else { panic!() };
        assert_eq!(t.size, 20);
        // A turn of nothing is not an effect: it becomes a real one.
        let none = entry(vec![Effect::Rotate { rotation: Rotation::None }]).normalized();
        assert_eq!(none.get(Kind::Rotate), Some(&Effect::Rotate { rotation: Rotation::Cw }));
    }

    fn blur_fit(height: u32) -> Effect {
        Effect::BlurFit(BlurFit { height, blur: 20 })
    }

    #[test]
    fn the_blurred_background_is_the_owners_graph_with_its_own_height() {
        let e = entry(vec![blur_fit(720)]);
        let p = plan_of(&e, &Size::original(), &Lut::none());
        assert!(p.inputs.is_empty());
        assert_eq!(
            p.filter,
            Filter::Complex(
                "[0:v]split[bo][bc];[bc]scale=ih*16/9:-1,crop=h=iw*9/16,gblur=sigma=20[bb];[bb][bo]overlay=(main_w-overlay_w)/2:(main_h-overlay_h)/2,scale=-2:720[outv]"
                    .to_owned()
            )
        );
    }

    #[test]
    fn a_turn_comes_before_the_blurred_background_and_the_size_cap_after_it() {
        let e = entry(vec![blur_fit(1080), Effect::Rotate { rotation: Rotation::Cw }]);
        let size = Size { max_height: Some(720), ..Size::new("s") };
        let Filter::Complex(g) = plan_of(&e, &size, &Lut::none()).filter else { panic!() };
        let (turn, split, cap) = (g.find("transpose=1").unwrap(), g.find("split[bo]").unwrap(), g.rfind("scale=w=iw").unwrap());
        assert!(turn < split && split < cap, "{g}");
        assert!(g.ends_with("[outv]"), "{g}");
    }

    #[test]
    fn stereo_and_reverse_are_plain_filters_after_the_size_cap_and_before_the_text() {
        let e = entry(vec![
            Effect::Text(TextOverlay::default()),
            Effect::Reverse,
            Effect::Stereo,
            Effect::Rotate { rotation: Rotation::Ccw },
        ]);
        let size = Size { max_height: Some(720), ..Size::new("s") };
        let Filter::Simple(f) = plan_of(&e, &size, &Lut::none()).filter else { panic!() };
        let at = |needle: &str| f.find(needle).unwrap_or_else(|| panic!("{needle} in {f}"));
        let order = [at("transpose=2"), at("scale=w=iw"), at("stereo3d=al:sbsl"), at("reverse"), at("drawtext")];
        assert!(order.windows(2).all(|w| w[0] < w[1]), "{f}");
        assert_eq!(e.audio_filter(), Some("areverse"));
        assert_eq!(entry(vec![Effect::Stereo]).audio_filter(), None);
    }

    #[test]
    fn burning_the_subtitles_names_the_source_file_with_its_path_escaped() {
        let Filter::Simple(f) = plan_of(&entry(vec![Effect::Burn]), &Size::original(), &Lut::none()).filter else { panic!() };
        assert_eq!(f, "subtitles=filename='D\\:/Films/Holiday.mp4':si=0");
    }

    #[test]
    fn the_new_kinds_are_kept_once_and_bounded() {
        let e = entry(vec![blur_fit(99_999), Effect::Reverse, Effect::Reverse, Effect::Burn]).normalized();
        assert_eq!(e.items.iter().map(Effect::kind).collect::<Vec<_>>(), vec![Kind::BlurFit, Kind::Burn, Kind::Reverse]);
        let Some(Effect::BlurFit(b)) = e.get(Kind::BlurFit) else { panic!() };
        assert_eq!(b.height, 4320);
        for k in Kind::ALL {
            assert_eq!(k.new_effect().kind(), k);
            assert_eq!(Kind::from_id(k.id()), Some(k));
        }
    }

    #[test]
    fn deinterlace_crop_and_frame_rate_come_first_in_that_order_then_the_turn_and_the_size() {
        let e = entry(vec![
            Effect::Rotate { rotation: Rotation::Cw },
            Effect::Fps(FrameRate { value: "30000/1001".into() }),
            Effect::Crop(Crop { left: 10, right: 10, top: 20, bottom: 0 }),
            Effect::Deinterlace,
        ]);
        let size = Size { max_height: Some(720), ..Size::new("s") };
        let Filter::Simple(f) = plan_of(&e, &size, &Lut::none()).filter else { panic!() };
        assert!(
            f.starts_with("yadif,crop=w=trunc((iw-20)/2)*2:h=trunc((ih-20)/2)*2:x=10:y=20,fps=30000/1001,transpose=1,scale="),
            "{f}"
        );
    }

    #[test]
    fn a_crop_of_nothing_is_no_filter_and_a_rate_it_does_not_offer_becomes_30() {
        assert_eq!(Crop::default().filter(), None);
        let e = entry(vec![Effect::Crop(Crop::default()), Effect::Fps(FrameRate { value: "1; rm -rf".into() })]).normalized();
        assert_eq!(e.get(Kind::Fps), Some(&Effect::Fps(FrameRate { value: "30".into() })));
        let e = entry(vec![Effect::Crop(Crop { left: 99_999, ..Crop::default() })]).normalized();
        assert_eq!(e.get(Kind::Crop), Some(&Effect::Crop(Crop { left: 4000, ..Crop::default() })));
    }

    #[test]
    fn stripping_the_metadata_is_not_a_filter() {
        let e = entry(vec![Effect::Strip]);
        assert!(e.strips_metadata());
        assert_eq!(plan_of(&e, &Size::original(), &Lut::none()).filter, Filter::None);
    }

    #[test]
    fn the_seeded_entries_are_normal_and_unique() {
        let all = defaults();
        assert_eq!(all.len(), 7);
        for e in &all {
            assert_eq!(e.clone(), e.clone().normalized(), "{}", e.name);
        }
        assert_eq!(all.iter().map(|e| &e.id).collect::<std::collections::HashSet<_>>().len(), all.len());
    }
}
