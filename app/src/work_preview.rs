//! The preview: the selected file as a picture with the chosen effects on it, a timeline to move along, and the
//! buttons to step, play and mark where the cut starts and ends. ONE pane (`PreviewPane`), in the work
//! view above the file list.
//!
//! A pane is told what to show through a signal (`State`: the file, the size cap, the effects, the cut) and says
//! what the user marked through `on_mark`. The work view writes the marks into the panel's cut boxes, so the marks
//! and the boxes are one setting.
//!
//! The picture is made by ffmpeg (`engine::preview`): a still for a moment, a running stream for playback, both
//! from the same plan as a conversion, so what shows is what the effects will do. Each picture reaches the
//! pane's image as a motion-JPEG stream from a server on this machine; no script runs per picture.
//! The sound is a second ffmpeg (`engine::sound`) started with the stream, played on the default output.
//!
//! ⚠ CLAUDE: NO HARDCODED UI STRINGS. Every user-visible string goes through the t() lookup with the key
//! a string literal at the call site.

use crate::{
    catalog::{UisCropRotate, UisFldLabelled, UisNum},
    effects_editor::SYSTEM,
    i18n,
    preview_session::{Drawer, Mark, State, TextEdit, WidgetEdit},
    runner::RUNNER,
    work_colour::{self, Pick},
    work_effects::standard,
    work_picture::{self, PictureBar},
    work_text::{self, Drag as TextDrag, TextBar},
};
use dioxus::prelude::*;
use dioxus_free_icons::{
    Icon,
    icons::ld_icons::{LdChevronLeft, LdChevronRight, LdChevronsDown, LdChevronsUp, LdCrop, LdFlipHorizontal, LdFlipVertical, LdGauge, LdImage, LdList, LdPalette, LdSpline, LdSlidersVertical, LdActivity, LdArrowLeftRight, LdPause, LdPlay, LdRotateCcw, LdType, LdVolume2, LdVolumeX, LdX},
};
use engine::{
    effects::{self, EffectsEntry, Look},
    ffmpeg::Probe,
    ffmpeg_effects::{V, Values},
    preview::{self, Channel, Spec, Stream},
    sound_plan::SoundPlan,
    trim::{Trim, parse_time},
};
use std::{rc::Rc, sync::Arc, time::Duration};

/// Pictures are made no wider than this: the preview is never larger on screen, and a smaller picture is quicker.
const PICTURE_WIDTH: u32 = 960;
/// A stream makes no more pictures a second than this, whatever the file's own rate.
const MAX_FPS: f64 = 30.0;

/// A time written in a cut box, when it holds one.
fn written(text: &str) -> Option<f64> {
    parse_time(text).flatten()
}

/// What the picture is shown with: the effects as they stand - and, while a widget is being edited over the picture, only what that widget
/// RECEIVES: the effects that run before it in the fixed order, never the widget's own effect (the crop is then the rectangle, not a crop of the
/// picture, so it can be dragged outward as well as inward) and never what runs after it, which would change the picture the rectangle is
/// measured on. The crop runs first, so while it is edited the picture is the original, with no effect at all; the rotation receives the
/// cropped picture.
fn picture_effects(effects: &EffectsEntry, editing: Option<&str>) -> EffectsEntry {
    // The fade is made by ffmpeg at the ends of the file it converts. A picture or a stream made from the middle of the file starts its own clock
    // there, so it would show the fade-in at once (a black still): the preview never carries the fade, the bar shows it.
    let fades: Vec<String> = [fade_widget(), sound_fade_widget()].into_iter().flatten().map(|w| w.key).collect();
    let stripped = EffectsEntry { items: effects.items.iter().filter(|i| !fades.contains(&i.key)).cloned().collect(), ..effects.clone() };
    let effects = &stripped;
    // The texts and the watermarks are edited on the picture as it will be: every effect is kept, they too (the boxes are drawn over what ffmpeg makes).
    if editing.is_some_and(|key| work_text::layer_widget(key).is_some()) {
        return effects.clone();
    }
    let standard = standard();
    let Some(first) = editing.and_then(|key| standard.iter().position(|s| s == key)) else { return effects.clone() };
    let mut before: Vec<engine::effects::Item> = effects.items.iter().filter(|i| standard.iter().position(|s| *s == i.key).is_some_and(|rank| rank < first)).cloned().collect();
    // a mirror and a quarter turn are made on the picture itself (the frame is then drawn on the turned picture); only a turn by another angle is shown by the handle
    if let Some(item) = effects.items.iter().find(|i| Some(i.key.as_str()) == editing) {
        let values = item.all_values();
        let mut kept: Vec<(&str, &str)> = Vec::new();
        for id in ["flip", "quarter"] {
            if let Some(V::Str(v)) = values.get(id) {
                if v != "none" {
                    kept.push((id, v.as_str()));
                }
            }
        }
        if !kept.is_empty() {
            before.push(engine::effects::Item::with(&item.key, &kept));
        }
    }
    EffectsEntry { items: before, ..effects.clone() }
}

/// The pictures along the timeline. A piece of its own that reads its own signal: the preview's redraw (at every value a number field
/// drags through) leaves its 24 images alone and only a new set of pictures redraws them.
#[component]
fn FilmRow(film: Signal<Vec<Option<String>>>) -> Element {
    rsx! {
        div { class: "wv-film-row",
            for (i, src) in film().into_iter().enumerate() {
                if let Some(src) = src {
                    img { key: "{i}", class: "wv-film-pic", src: "{src}", draggable: false }
                } else {
                    span { key: "{i}", class: "wv-film-pic" }
                }
            }
        }
    }
}

/// The bars of the sound band, a piece of its own for the same reason.
#[component]
fn WaveBars(wave: Signal<Vec<f32>>) -> Element {
    rsx! {
        for (i, p) in wave().into_iter().enumerate() {
            span { key: "{i}", style: "height: {(p * 100.0).max(2.0):.0}%" }
        }
    }
}

/// What decides the picture: `picture_effects` without the effects that act on the sound alone (they do not change a single pixel).
fn picture_only(effects: &EffectsEntry, editing: Option<&str>) -> EffectsEntry {
    let mut e = picture_effects(effects, editing);
    e.items.retain(|i| !engine::effects::is_sound(&i.key));
    e
}

/// The effects that act on the sound alone, as they stand.
fn sound_items(effects: &EffectsEntry) -> Vec<engine::effects::Item> {
    effects.items.iter().filter(|i| engine::effects::is_sound(&i.key)).cloned().collect()
}

/// The fade widget (the fade in and the fade out, drawn as two handles on the bar under the picture), when the catalogue has it.
fn fade_widget() -> Option<effects::WidgetSpec> {
    effects::catalog().and_then(|c| effects::widgets(&c, SYSTEM).into_iter().find(|w| w.kind == "fades"))
}

/// The same for the sound (FFmpeg Audio Studio's fade): a second strip under the first.
fn sound_fade_widget() -> Option<effects::WidgetSpec> {
    effects::catalog().and_then(|c| effects::sound_widgets(&c, SYSTEM).into_iter().find(|w| w.kind == "fades"))
}

/// The values of the fade effect's controls now (their defaults while the effect is not in the list).
fn fade_values(w: &effects::WidgetSpec, effects: &EffectsEntry) -> Values {
    effects.items.iter().find(|i| i.key == w.key).cloned().unwrap_or_else(|| engine::effects::Item::new(&w.key)).all_values()
}

/// The two lengths in seconds, the fade in and the fade out.
fn fades_of(w: &effects::WidgetSpec, values: &Values) -> (f64, f64) {
    let length = |end: &str| match values.get(w.controls.get(end).map_or("", String::as_str)) {
        Some(V::Num(n)) => n.max(0.0),
        _ => 0.0,
    };
    (length("in"), length("out"))
}

/// The values of the effect with these two lengths.
fn with_fades(w: &effects::WidgetSpec, mut values: Values, (fade_in, fade_out): (f64, f64)) -> Values {
    for (end, v) in [("in", fade_in), ("out", fade_out)] {
        values.insert(w.controls.get(end).cloned().unwrap_or_default(), V::Num(v));
    }
    values
}

/// The label and the unit of each length of a fade, as the catalogue words them.
fn fade_words_of(w: &effects::WidgetSpec) -> Option<((String, String), (String, String))> {
    let cat = effects::catalog()?;
    let effect = engine::effects::Item::new(&w.key).effect()?;
    let controls = engine::ffmpeg_effects::resolve_controls(&effect, &cat.filters).ok()?;
    let word = |end: &str| controls.iter().find(|c| Some(&c.id) == w.controls.get(end)).map(|c| (c.label.clone(), c.unit.clone().unwrap_or_default()));
    Some((word("in")?, word("out")?))
}

/// Which end of the kept part a fade handle belongs to.
#[derive(Clone, Copy, Debug, PartialEq)]
enum FadeEnd {
    In,
    Out,
}

/// A fade handle being dragged: which one, and where the bar is on screen (its left edge and width).
#[derive(Clone, Copy, Debug)]
struct FadeDrag {
    end: FadeEnd,
    left: f64,
    width: f64,
}

/// The longest fade the control takes, in seconds.
const MAX_FADE: f64 = 60.0;

/// The two lengths after the pointer moved to `x`, on a bar that stands for `length` seconds with the kept part `kept` (start, end) on it and the
/// lengths `now`: the handle moved sets its own length in tenths of a second, never past the kept part or into the other fade.
fn dragged_fades(d: &FadeDrag, x: f64, length: f64, kept: (f64, f64), now: (f64, f64)) -> (f64, f64) {
    let at = ((x - d.left) / d.width * length).clamp(0.0, length);
    let room = (kept.1 - kept.0).max(0.0);
    let tenth = |v: f64| ((v * 10.0).round() / 10.0).min(MAX_FADE);
    match d.end {
        FadeEnd::In => (tenth((at - kept.0).clamp(0.0, (room - now.1).max(0.0))), now.1),
        FadeEnd::Out => (now.0, tenth((kept.1 - at).clamp(0.0, (room - now.0).max(0.0)))),
    }
}

/// The fade handle is let go; the lengths stay as dragged until the work view's list has followed.
fn release_fade(mut drag: Signal<Option<FadeDrag>>, mut live: Signal<Option<(f64, f64)>>) {
    if drag.peek().is_some() {
        drag.set(None);
        spawn(async move {
            tokio::time::sleep(Duration::from_millis(600)).await;
            if drag.peek().is_none() {
                live.set(None);
            }
        });
    }
}

/// A text is let go; the place stays as dragged until the work view's list has followed.
fn release_text(mut drag: Signal<Option<TextDrag>>, mut live: Signal<Option<(usize, f64, f64, f64)>>) {
    if drag.peek().is_some() {
        drag.set(None);
        spawn(async move {
            tokio::time::sleep(Duration::from_millis(600)).await;
            if drag.peek().is_none() {
                live.set(None);
            }
        });
    }
}

/// The user pressed on a text: remember where the picture is on screen and where in the text's corner it was held (asked of the browser once, here).
fn begin_text(index: usize, grab: work_text::Grab, pointer: (f64, f64), corner: (f64, f64), layer: Signal<Option<Rc<MountedData>>>, mut drag: Signal<Option<TextDrag>>) {
    let Some(element) = layer.peek().clone() else { return };
    spawn(async move {
        if let Ok(r) = element.get_client_rect().await {
            if r.size.width > 0.0 && r.size.height > 0.0 {
                let (left, top, width, height) = (r.origin.x, r.origin.y, r.size.width, r.size.height);
                drag.set(Some(TextDrag { grab, corner, index, left, top, width, height, dx: pointer.0 - (left + corner.0 * width), dy: pointer.1 - (top + corner.1 * height) }));
            }
        }
    });
}

/// The user pressed on a fade handle: remember where the bar is on screen (asked of the browser once, here).
fn begin_fade(end: FadeEnd, now: (f64, f64), strip: Signal<Option<Rc<MountedData>>>, mut drag: Signal<Option<FadeDrag>>, mut live: Signal<Option<(f64, f64)>>) {
    let Some(element) = strip.peek().clone() else { return };
    spawn(async move {
        if let Ok(r) = element.get_client_rect().await {
            if r.size.width > 0.0 {
                drag.set(Some(FadeDrag { end, left: r.origin.x, width: r.size.width }));
                live.set(Some(now));
            }
        }
    });
}

/// The rectangle widget: the file's own size in pixels (the picture it is drawn on is the original: the crop runs first), the values of the
/// effect's controls, and its four margins now.
fn geometry(w: &effects::WidgetSpec, effects: &EffectsEntry, probe: &Probe) -> Option<(Values, (f64, f64), [f64; 4])> {
    if probe.width == 0 || probe.height == 0 {
        return None;
    }
    let mut full = (f64::from(probe.width), f64::from(probe.height));
    let item = effects.items.iter().find(|i| i.key == w.key).cloned().unwrap_or_else(|| engine::effects::Item::new(&w.key));
    let values = item.all_values();
    // a quarter turn to the side swaps the picture's width and height
    if matches!(quarter_from(w, &values).as_deref(), Some("cw" | "ccw")) {
        full = (full.1, full.0);
    }
    let number = |edge: &str| match values.get(w.controls.get(edge).map_or("", String::as_str)) {
        Some(V::Num(n)) => *n,
        _ => 0.0,
    };
    let margins = ["left", "top", "right", "bottom"].map(number);
    Some((values, full, margins))
}

/// The values of the effect with these margins in its four controls.
fn with_margins(w: &effects::WidgetSpec, mut values: Values, margins: [f64; 4]) -> Values {
    for (edge, m) in ["left", "top", "right", "bottom"].into_iter().zip(margins) {
        values.insert(w.controls.get(edge).cloned().unwrap_or_default(), V::Num(m));
    }
    values
}

/// At least this much picture stays, as the crop effect itself keeps.
const MIN_KEPT: f64 = 2.0;

/// The rotation of the frame in degrees, clockwise, from the effect's tilt control. Between -180 (excluded) and 180.
fn angle_of(w: &effects::WidgetSpec, effects: &EffectsEntry) -> f64 {
    let item = effects.items.iter().find(|i| i.key == w.key).cloned().unwrap_or_else(|| engine::effects::Item::new(&w.key));
    angle_from(w, &item.all_values())
}

/// The same, from the values of the effect's controls.
fn angle_from(w: &effects::WidgetSpec, values: &Values) -> f64 {
    match quarter_from(w, values).as_deref() {
        Some("cw") => return 90.0,
        Some("ccw") => return -90.0,
        Some("half") => return 180.0,
        _ => {}
    }
    match values.get(w.controls.get("tilt").map_or("", String::as_str)) {
        Some(V::Num(n)) => normal_angle(*n),
        _ => 0.0,
    }
}

/// The mirror the effect holds: none, h, v or both.
fn flip_from(w: &effects::WidgetSpec, values: &Values) -> &'static str {
    match w.controls.get("flip").and_then(|id| values.get(id)) {
        Some(V::Str(f)) if f == "h" => "h",
        Some(V::Str(f)) if f == "v" => "v",
        Some(V::Str(f)) if f == "both" => "both",
        _ => "none",
    }
}

/// The values with one of the two mirrors switched (`horizontal` or vertical).
fn with_flip(w: &effects::WidgetSpec, mut values: Values, horizontal: bool) -> Values {
    let now = flip_from(w, &values);
    let (h, v) = (matches!(now, "h" | "both"), matches!(now, "v" | "both"));
    let (h, v) = if horizontal { (!h, v) } else { (h, !v) };
    let next = match (h, v) {
        (false, false) => "none",
        (true, false) => "h",
        (false, true) => "v",
        (true, true) => "both",
    };
    if let Some(id) = w.controls.get("flip") {
        values.insert(id.clone(), V::Str(next.to_owned()));
    }
    values
}

/// The quarter turn the effect holds (cw, ccw or half), if it has one: a quarter or half turn is the effect's own `quarter` control (made exactly,
/// with the picture's sides swapped for a quarter), the tilt is only for the other angles.
fn quarter_from(w: &effects::WidgetSpec, values: &Values) -> Option<String> {
    match values.get(w.controls.get("quarter")?) {
        Some(V::Str(q)) if q != "none" => Some(q.clone()),
        _ => None,
    }
}

/// The angle of the quarter turn the picture already carries (the picture shown is made with it), 0 when it has none.
fn quarter_angle(w: &effects::WidgetSpec, values: &Values) -> f64 {
    match quarter_from(w, values).as_deref() {
        Some("cw") => 90.0,
        Some("ccw") => -90.0,
        Some("half") => 180.0,
        _ => 0.0,
    }
}

/// An angle between -180 (excluded) and 180.
fn normal_angle(degrees: f64) -> f64 {
    let a = (degrees + 180.0).rem_euclid(360.0) - 180.0;
    if a <= -180.0 { 180.0 } else { a }
}

/// The values of the effect for a rotation of `degrees`: the tilt control, to a tenth of a degree.
fn with_angle(w: &effects::WidgetSpec, mut values: Values, degrees: f64) -> Values {
    let angle = normal_angle(degrees);
    let tilt = w.controls.get("tilt").cloned().unwrap_or_default();
    // 90, -90 and 180 are the quarter turn control (and no tilt); any other angle is the tilt (and no quarter turn)
    let quarter = w.controls.get("quarter").map(|id| (id.clone(), if angle == 90.0 { "cw" } else if angle == -90.0 { "ccw" } else if angle == 180.0 { "half" } else { "none" }));
    match quarter {
        Some((id, q)) => {
            values.insert(id, V::Str(q.to_owned()));
            values.insert(tilt, V::Num(if q == "none" { (angle * 10.0).round() / 10.0 } else { 0.0 }));
        }
        None => {
            values.insert(tilt, V::Num((angle * 10.0).round() / 10.0));
        }
    }
    values
}

static NEXT_STAGE: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

/// The stage takes the pointer for as long as the button is held: the browser then keeps sending its moves and its release to the page
/// even when the pointer leaves the window, which it does not do for a plain press.
fn capture(stage_id: &str, pointer: i32) {
    let _ = document::eval(&format!("document.getElementById('{stage_id}')?.setPointerCapture({pointer})"));
}

/// The pictures along the timeline and their height in pixels.
const FILM_SLOTS: usize = 24;
const FILM_HEIGHT: u32 = 90;
/// How many bars the sound band under the pictures is drawn with.
const WAVE_BARS: usize = 240;

#[component]
pub fn PreviewPane(state: Signal<State>, mut pos: Signal<f64>, channel: Signal<Arc<Channel>>, visible: bool, on_mark: EventHandler<Mark>, on_widget: EventHandler<WidgetEdit>, on_texts: EventHandler<(String, TextEdit)>, mut drawer: Signal<Drawer>, mut folded: Signal<bool>, mut pick: Signal<Option<Pick>>) -> Element {
    let mut info = use_signal(|| None::<Result<Probe, String>>);
    let mut playing = use_signal(|| false);
    // The pictures of the timeline (addresses on the preview server), one per slot; an empty slot is a gap.
    let mut film = use_signal(Vec::<Option<String>>::new);
    // The loudness along the file (0 to 1 per bar); empty for a file with no sound.
    let mut wave = use_signal(Vec::<f32>::new);
    let mut failure = use_signal(String::new);
    // The effects ffmpeg refused in the last picture, each with why.
    let mut left_out = use_signal(Vec::<String>::new);
    let mut stream = use_signal(|| None::<Stream>);
    // Where the running stream started, and a number that grows when what a still shows must be made again.
    let mut from = use_signal(|| 0.0_f64);
    let mut refresh = use_signal(|| 0_u64);
    // The picture a still made, carried in the page itself: while nothing plays the pane shows it, and not the motion stream (a stream shows its
    // latest picture late, so a still could appear as the one before it).
    let mut still_src = use_signal(|| None::<String>);
    // The picture's element, to place a click on it (the colour pickers).
    let mut picture_box = use_signal(|| None::<Rc<MountedData>>);
    // The sound level, 0 to 100, and whether it is muted (the level is kept while muted).
    let mut volume = use_signal(|| 100_u32);
    let mut muted = use_signal(|| false);
    // The size of the picture shown (read from the picture itself), the rectangle's element on screen, what the user is dragging, and the
    // margins while dragging (the work view's list follows a moment later).
    let mut picture = use_signal(|| None::<(u32, u32, Option<String>)>);
    // The stage's own id, one per pane (the work view and the preview window both have one).
    let stage_id = use_hook(|| format!("wv-stage-{}", NEXT_STAGE.fetch_add(1, std::sync::atomic::Ordering::Relaxed)));
    let mut live = use_signal(|| None::<[f64; 4]>);
    let mut moves = use_signal(|| 0_u64);
    // A widget is on screen only while the user is editing it (its button in the bar): the key of the effect.
    let mut editing = use_signal(|| None::<String>);
    // The angle while the rotation handle is dragged (the work view's list follows a moment later), and a count that tells a drag that went on from one that ended.
    let mut live_angle = use_signal(|| None::<f64>);
    // Set when the rectangle starts to be edited: the first picture made for it puts the rectangle in (see below).
    let mut seed = use_signal(|| false);
    // The fade handles: the bar's element on screen, the handle held, and the lengths while dragging (the list follows a moment later).
    let strip = use_signal(|| None::<Rc<MountedData>>);
    let fade_drag = use_signal(|| None::<FadeDrag>);
    let mut live_fade = use_signal(|| None::<(f64, f64)>);
    // the sound's fade has its own strip and its own handles
    let strip_sound = use_signal(|| None::<Rc<MountedData>>);
    let fade_drag_sound = use_signal(|| None::<FadeDrag>);
    let mut live_fade_sound = use_signal(|| None::<(f64, f64)>);
    // The texts: the number of the one selected, the picture's element on screen, the text held
    // and its place while dragging (the work view's list follows a moment later).
    let mut text_sel = use_signal(|| None::<usize>);
    let mut layer = use_signal(|| None::<Rc<MountedData>>);
    let text_drag = use_signal(|| None::<TextDrag>);
    let mut live_text = use_signal(|| None::<(usize, f64, f64, f64)>);
    let level = move || if *muted.peek() { 0.0 } else { *volume.peek() as f32 / 100.0 };

    // The frame rate pictures are made at: the file's own, no more than MAX_FPS.
    let out_fps = move || match &*info.peek() {
        Some(Ok(p)) => f64::from(p.fps.unwrap_or(25.0)).min(MAX_FPS),
        _ => 25.0,
    };

    let mut stop_stream = move || {
        let estimate = match stream.peek().as_ref() {
            Some(s) => Some(*from.peek() + s.frames() as f64 / out_fps()),
            None => None,
        };
        stream.set(None);
        playing.set(false);
        if let Some(at) = estimate {
            pos.set(at);
        }
    };

    // A view that is not on screen does not play: the picture it was showing stays, paused, as it was.
    use_effect(use_reactive!(|visible| {
        if !visible && *playing.peek() {
            stop_stream();
        }
    }));

    let mut start_stream = move |at: f64| {
        stream.set(None);
        let st = state.peek().clone();
        let (Some(source), Some(ff)) = (st.source.clone(), RUNNER.ffmpeg()) else { return };
        let probe = match &*info.peek() {
            Some(Ok(p)) => Some(p.clone()),
            _ => None,
        };
        spawn(async move {
            let length = probe.as_ref().map_or(0.0, |p| p.duration_us as f64 / 1_000_000.0);
            // the part the cut keeps, and what the sound's effects make of it (measured once: the pane asks again at every start)
            let cut = Trim { start_s: written(&st.cut_start), end_s: written(&st.cut_end) };
            let planned = {
                let (ff, source, effects) = (ff.clone(), source.clone(), st.effects.clone());
                tokio::task::spawn_blocking(move || match probe {
                    Some(p) => engine::queue::plan_cached(&ff, &effects, &source, cut, &p),
                    None => Ok(SoundPlan::none()),
                })
                .await
                .unwrap_or_else(|e| Err(e.to_string()))
            };
            let planned = match planned {
                Ok(p) => p,
                Err(why) => {
                    failure.set(why);
                    playing.set(false);
                    return;
                }
            };
            let shown = picture_effects(&st.effects, editing.peek().as_deref());
            let look = Look { size: &st.size, effects: &shown, duration_s: length, trim: cut };
            let end = written(&st.cut_end).filter(|e| *e > at);
            let spec = Spec { input: &source, start_s: at, length_s: end.map(|e| e - at), look: &look, width: PICTURE_WIDTH, fps: out_fps(), sound: Some(&planned) };
            // An effect ffmpeg refuses would end the playback at once: play without it, and say so beside the picture.
            let (runnable, skipped) = match preview::playable(&ff, &spec, &channel.peek().hub) {
                Ok(done) => done,
                Err(why) => {
                    failure.set(why);
                    playing.set(false);
                    return;
                }
            };
            left_out.set(skipped);
            let look = Look { size: &st.size, effects: &runnable, duration_s: length, trim: cut };
            let spec = Spec { look: &look, ..spec };
            match Stream::start(&ff, &spec, channel.peek().hub.clone(), level()) {
                Ok(s) => {
                    still_src.set(None);
                    from.set(at);
                    stream.set(Some(s));
                    playing.set(true);
                    failure.set(String::new());
                }
                Err(why) => {
                    failure.set(why);
                    playing.set(false);
                }
            }
        });
    };

    // Follows what it is told: another file, another size cap or effects. (A new cut changes no picture.)
    use_future(move || async move {
        let mut last = State::default();
        let mut edit_was = None::<String>;
        loop {
            let st = state.peek().clone();
            let edit_now = editing.peek().clone();
            if st != last || edit_now != edit_was {
                if st.source != last.source {
                    stop_stream();
                    // Coming back to a file the pane was already on keeps the moment: it is only zeroed when one file
                    // replaces another, or none is left.
                    if st.source.is_none() || last.source.is_some() {
                        pos.set(0.0);
                    }
                    info.set(None);
                    film.set(Vec::new());
                    wave.set(Vec::new());
                    picture.set(None);
                    still_src.set(None);
                    failure.set(String::new());
                    refresh += 1;
                    if let Some(source) = st.source.clone() {
                        let last_source = Some(source.clone());
                        spawn(async move {
                            let probed = tokio::task::spawn_blocking(move || match RUNNER.ffmpeg() {
                                Some(ff) => engine::ffmpeg::probe(&ff, &source),
                                None => Err(String::new()),
                            })
                            .await
                            .unwrap_or_else(|e| Err(e.to_string()));
                            let seconds = probed.as_ref().ok().map_or(0.0, |p| p.duration_us as f64 / 1_000_000.0);
                            info.set(Some(probed));
                            if let (true, Some(src)) = (seconds > 0.0, last_source.clone()) {
                                let for_wave = src.clone();
                                spawn(async move {
                                    let peaks = tokio::task::spawn_blocking(move || RUNNER.ffmpeg().and_then(|ff| engine::preview::waveform(&ff, &for_wave, seconds, WAVE_BARS).ok()).unwrap_or_default())
                                        .await
                                        .unwrap_or_default();
                                    wave.set(peaks);
                                });
                                let made = tokio::task::spawn_blocking(move || {
                                    let Some(ff) = RUNNER.ffmpeg() else { return Vec::new() };
                                    let dir = std::env::temp_dir().join("yaiol-ffmpeg-video-studio-film");
                                    engine::preview::filmstrip(&ff, &src, seconds, FILM_SLOTS, FILM_HEIGHT, &dir)
                                        .into_iter()
                                        .map(|f| f.and_then(|p| engine::preview::serve_file(&p)))
                                        .collect::<Vec<_>>()
                                })
                                .await
                                .unwrap_or_default();
                                // (the file may have changed while the pictures were made)
                                if info.peek().is_some() {
                                    film.set(made);
                                }
                            }
                        });
                    }
                } else if let (picture, sound) = (
                    st.size != last.size
                        || picture_only(&st.effects, edit_now.as_deref()) != picture_only(&last.effects, edit_was.as_deref())
                        // (a new cut changes no picture, but it changes what the sound's effects measure)
                        || ((st.cut_start != last.cut_start || st.cut_end != last.cut_end) && st.effects.needs_plan()),
                    sound_items(&st.effects) != sound_items(&last.effects),
                ) && (picture || sound)
                {
                    // A sound effect changes no picture: a value dragged on it must not make ffmpeg draw the same picture again at every step
                    // (and push it into the page); it only restarts a playback, which carries the sound.
                    if picture {
                        refresh += 1;
                    }
                    if *playing.peek() {
                        let at = *from.peek() + stream.peek().as_ref().map_or(0.0, |s| s.frames() as f64 / out_fps());
                        start_stream(at);
                    }
                }
                last = st;
                edit_was = edit_now;
            }
            tokio::time::sleep(Duration::from_millis(150)).await;
        }
    });

    // Playback: the moment follows the pictures made, and playing ends with the stream.
    use_future(move || async move {
        loop {
            tokio::time::sleep(Duration::from_millis(100)).await;
            if !*playing.peek() {
                continue;
            }
            let (at, ended) = match stream.peek().as_ref() {
                Some(s) => (*from.peek() + s.frames() as f64 / out_fps(), s.finished()),
                None => (*pos.peek(), true),
            };
            pos.set(at);
            if ended {
                stop_stream();
                pos.set(at);
            }
        }
    });

    // A still for the moment the timeline stands on, with the effects on it - while nothing plays.
    use_future(move || {
        let hub = channel.peek().hub.clone();
        async move {
            let mut shown: Option<(f64, u64)> = None;
            loop {
                let want = (*pos.peek(), *refresh.peek());
                let ready = matches!(&*info.peek(), Some(Ok(_)));
                if *playing.peek() || !ready || shown == Some(want) {
                    tokio::time::sleep(Duration::from_millis(40)).await;
                    continue;
                }
                shown = Some(want);
                let st = state.peek().clone();
                let length = match &*info.peek() {
                    Some(Ok(p)) => p.duration_us as f64 / 1_000_000.0,
                    _ => 0.0,
                };
                let into = hub.clone();
                let hide = editing.peek().clone();
                let hide_now = hide.clone();
                let made = tokio::task::spawn_blocking(move || {
                    let (Some(source), Some(ff)) = (st.source.clone(), RUNNER.ffmpeg()) else { return Err(String::new()) };
                    let shown = picture_effects(&st.effects, hide_now.as_deref());
                    let look = Look { size: &st.size, effects: &shown, duration_s: length, trim: Trim::none() };
                    preview::still_noting(&ff, &Spec { input: &source, start_s: want.0, length_s: None, look: &look, width: PICTURE_WIDTH, fps: MAX_FPS, sound: None }, &into)
                })
                .await
                .unwrap_or_else(|e| Err(e.to_string()));
                match made {
                    Ok(done) => {
                        let (jpeg, skipped) = (done.jpeg, done.skipped);
                        let size = preview::jpeg_size(&jpeg);
                        {
                            use base64::{Engine, engine::general_purpose::STANDARD};
                            still_src.set(Some(format!("data:image/jpeg;base64,{}", STANDARD.encode(&jpeg))));
                        }
                        picture.set(size.map(|(w, h)| (w, h, hide.clone())));
                        // an effect ffmpeg refused is left out of the picture and said beside it
                        failure.set(String::new());
                        left_out.set(skipped);
                        // Editing starts with a crop already in place, a little inside the picture: pressing the button again applies it, and the
                        // reset button takes it away. (A crop that is already there is left as it is.)
                        if hide.is_some() && *seed.peek() {
                            seed.set(false);
                            let rect = effects::catalog().and_then(|c| effects::widgets(&c, SYSTEM).into_iter().find(|w| w.kind == "frame" && Some(&w.key) == hide.as_ref()));
                            let probe = info.peek().as_ref().and_then(|r| r.as_ref().ok()).cloned();
                            if let (Some(w), Some(probe)) = (rect, probe) {
                                let st = state.peek().clone();
                                if let Some((values, full, margins)) = geometry(&w, &st.effects, &probe) {
                                    if margins == [0.0; 4] {
                                        let inset = [(full.0 * 0.08).round(), (full.1 * 0.08).round(), (full.0 * 0.08).round(), (full.1 * 0.08).round()];
                                        on_widget.call(WidgetEdit { key: w.key.clone(), values: Some(with_margins(&w, values, inset)), keep: false });
                                    }
                                }
                            }
                        }
                    }
                    Err(why) => failure.set(why),
                }
            }
        }
    });

    let st = state();
    let lang = st.lang.clone();
    let t = |key: &str| i18n::t(&lang, key);
    let s_left_out = t("lblPreviewSkipped");
    let s_set_in = t("tipWorkSetIn");
    let s_set_out = t("tipWorkSetOut");
    let s_to_in = t("tipWorkToIn");
    let s_to_out = t("tipWorkToOut");
    let s_back = t("tipWorkBack");
    let s_forward = t("tipWorkForward");
    let s_play = if playing() { t("tipWorkPause") } else { t("tipWorkPlay") };
    let s_clear = t("tipWorkClearCut");
    let s_mute = if muted() { t("tipWorkUnmute") } else { t("tipWorkMute") };
    let s_volume = t("tipWorkVolume");
    let duration_label = t("lblWorkDuration");
    let no_file = t("msgWorkNoFile");
    let frames_label = t("lblWorkFrames");

    let (line, length, fps) = match &*info.read() {
        Some(Ok(p)) => (
            format!("{}x{} {}", p.width, p.height, p.video_codec),
            p.duration_us as f64 / 1_000_000.0,
            f64::from(p.fps.unwrap_or(25.0)),
        ),
        _ => (String::new(), 0.0, 25.0),
    };
    let step = 1.0 / fps;
    let frames = (length * fps).round() as u64;
    let now = pos();
    let cut_in = written(&st.cut_start);
    let cut_out = written(&st.cut_end);
    let percent = |v: f64| if length > 0.0 { (v / length * 100.0).clamp(0.0, 100.0) } else { 0.0 };
    // No cut at all: the whole file is kept, so nothing on the film strip is dimmed.
    let region = if cut_in.is_none() && cut_out.is_none() {
        "--in: 0%; --out: 100%".to_owned()
    } else {
        format!("--in: {:.2}%; --out: {:.2}%", cut_in.map_or(0.0, percent), cut_out.map_or(100.0, percent))
    };
    // The fade: its two lengths (as dragged, while dragging) and the kept part they count from.
    let fade_w = fade_widget();
    let fade_held_values = fade_w.as_ref().map(|w| fade_values(w, &st.effects)).unwrap_or_default();
    let fade_held = fade_w.as_ref().map_or((0.0, 0.0), |w| fades_of(w, &fade_held_values));
    let fades = live_fade().unwrap_or(fade_held);
    let kept = (cut_in.unwrap_or(0.0).min(length), cut_out.unwrap_or(length).min(length));
    let room = (kept.1 - kept.0).max(0.0);
    let fade_shown = (fades.0.min(room), fades.1.min((room - fades.0.min(room)).max(0.0)));
    // the label and the unit of each length, as the catalogue words them
    let fade_words = fade_w.as_ref().and_then(fade_words_of);
    let fade_sound_w = sound_fade_widget();
    let fade_sound_values = fade_sound_w.as_ref().map(|w| fade_values(w, &st.effects)).unwrap_or_default();
    let fade_sound_held = fade_sound_w.as_ref().map_or((0.0, 0.0), |w| fades_of(w, &fade_sound_values));
    let fades_sound = live_fade_sound().unwrap_or(fade_sound_held);
    let fade_sound_shown = (fades_sound.0.min(room), fades_sound.1.min((room - fades_sound.0.min(room)).max(0.0)));
    let fade_sound_words = fade_sound_w.as_ref().and_then(fade_words_of);
    let tc_now = preview::timecode(now, fps);
    let tc_length = preview::timecode(length, fps);
    let tc_in = cut_in.map_or_else(String::new, |v| preview::timecode(v, fps));
    let tc_out = cut_out.map_or_else(String::new, |v| preview::timecode(v, fps));
    let frame_now = (now * fps).round() as u64;
    let ready = length > 0.0;
    let shown_failure = failure();
    let src = match still_src() {
        Some(still) if !playing() => still,
        _ => channel.peek().url.clone().unwrap_or_default(),
    };
    let s_reset = t("tipFxReset");
    let s_close = t("btnNewsClose");
    let s_flip_h = t("tipWorkFlipH");
    let s_flip_v = t("tipWorkFlipV");
    let s_files = t("tipWorkShowFiles");
    let (s_fold_on, s_fold_off) = (t("tipWorkUnfold"), t("tipWorkFold"));
    let s_fold = if folded() { s_fold_on } else { s_fold_off };

    // The widgets (the crop, the rotation): one button each in the bar, named as the effect is, lit while it is being edited, marked while it
    // changes the picture. What is drawn over the picture is the one being edited, on the picture made for it (not the one shown just before
    // the button was pressed).
    let widgets = effects::catalog().map(|c| effects::widgets(&c, SYSTEM)).unwrap_or_default();
    let bar_buttons: Vec<(String, String, String, bool)> = widgets
        .iter()
        .map(|w| (w.key.clone(), w.kind.clone(), engine::effects::Item::new(&w.key).title(), st.effects.items.iter().any(|i| i.key == w.key)))
        .collect();
    // the sound's widgets that are drawers, in the catalogue's order (the fade is a strip on the bar, not a drawer)
    let sound_drawers: Vec<(Drawer, String, &'static str)> = effects::catalog()
        .map(|c| effects::sound_widgets(&c, SYSTEM))
        .unwrap_or_default()
        .into_iter()
        .filter_map(|w| match w.kind.as_str() {
            "tone" => Some((Drawer::Tone, w.key, "tone")),
            "compressor" => Some((Drawer::Compressor, w.key, "compressor")),
            "channels" => Some((Drawer::Channels, w.key, "channels")),
            // the four ways to set the volume are ONE drawer
            "level" => Some((Drawer::Level, w.key, "level")),
            _ => None,
        })
        .fold(Vec::new(), |mut all, d| {
            if !all.iter().any(|(which, _, _)| *which == d.0) {
                all.push(d);
            }
            all
        });
    let open = editing().and_then(|key| widgets.iter().find(|w| w.key == key).cloned()).filter(|w| picture().is_some_and(|(_, _, made)| made.as_ref() == Some(&w.key)));
    let overlay = match (&open, &*info.read()) {
        (Some(w), Some(Ok(p))) if w.kind == "frame" => geometry(w, &st.effects, p).map(|(values, full, margins)| (w.clone(), values, full, live().unwrap_or(margins), margins)),
        _ => None,
    };
    // The rotation: the angle now, the values of the effect, and the angle the list holds.
    let turning = open.as_ref().filter(|w| w.kind == "frame").map(|w| {
        let held = angle_of(w, &st.effects);
        let values = st.effects.items.iter().find(|i| i.key == w.key).map_or_else(|| engine::effects::Item::new(&w.key).all_values(), |i| i.all_values());
        (w.clone(), values, live_angle().unwrap_or(held), held)
    });
    // the crop is opened from the corner of the picture (its reset and close take the place of the button while it is open)
    let flip_now = overlay.as_ref().map_or("none", |o| flip_from(&o.0, &o.1));
    let frame_button = bar_buttons.iter().find(|b| b.1 == "frame").cloned();
    let text_button = bar_buttons.iter().find(|b| b.1 == effects::TEXTS).cloned();
    let picture_button = bar_buttons.iter().find(|b| b.1 == effects::PICTURES).cloned();
    let tilted = turning.as_ref().is_some_and(|t| t.3 != 0.0);
    // The texts and the watermarks, placed on the picture in boxes: the widget while it is open (the bar is there at once, the boxes once the
    // picture made for it is), every box's values and look.
    let text_w = editing().and_then(|k| work_text::layer_widget(&k));
    let texting_key = text_w.as_ref().map(|w| w.key.clone());
    let text_items: Vec<Values> = match &text_w {
        Some(w) => st.effects.items.iter().filter(|i| i.key == w.key).map(|i| i.all_values()).collect(),
        None => Vec::new(),
    };
    let text_faces: Vec<work_text::Face> = text_w.as_ref().map(|w| text_items.iter().map(|v| work_text::Face::read(w, v)).collect()).unwrap_or_default();
    let text_at = text_sel().filter(|i| *i < text_items.len());
    let texting = open.as_ref().filter(|w| w.kind == effects::TEXTS || w.kind == effects::PICTURES).cloned();
    let text_move = text_w.clone().map(|w| (w, text_items.clone()));
    let pictured = text_w.as_ref().is_some_and(|w| w.kind == effects::PICTURES);
    rsx! {
        div {
            class: "wv uis-cmp-panel",
            // The pointer is followed on the whole pane, and - thanks to the capture taken at the press - outside the window too.
            onmousemove: {
                let fade = fade_w.clone().map(|w| (w, fade_held_values.clone()));
                let fade_sound = fade_sound_w.clone().map(|w| (w, fade_sound_values.clone()));
                let text = text_move.clone();
                move |e| {
                    let held = e.held_buttons().contains(dioxus::html::input_data::MouseButton::Primary);
                    let p = e.client_coordinates();
                    if let (Some(d), Some((w, items))) = (*text_drag.peek(), text.as_ref()) {
                        // The button was let go outside the window: the drag is over.
                        if !held {
                            release_text(text_drag, live_text);
                            return;
                        }
                        let Some(v) = items.get(d.index) else { return };
                        let held = work_text::Face::read(w, v);
                        let (x, y, zone) = match d.grab {
                            work_text::Grab::Move => {
                                let at = d.to((p.x, p.y));
                                (at.0, at.1, held.width)
                            }
                            work_text::Grab::Width => (held.x, held.y, d.zone((p.x, p.y))),
                        };
                        if *live_text.peek() == Some((d.index, x, y, zone)) {
                            return;
                        }
                        live_text.set(Some((d.index, x, y, zone)));
                        let moved = work_text::widened(w, &work_text::moved(w, v, (x, y)), zone);
                        on_texts.call((w.key.clone(), TextEdit::Set(d.index, moved)));
                        return;
                    }
                    if let (Some(d), Some((w, values))) = (*fade_drag_sound.peek(), fade_sound.as_ref()) {
                        // The button was let go outside the window: the drag is over.
                        if !held {
                            release_fade(fade_drag_sound, live_fade_sound);
                            return;
                        }
                        let now = live_fade_sound.peek().unwrap_or(fade_sound_held);
                        let next = dragged_fades(&d, p.x, length, kept, now);
                        if *live_fade_sound.peek() != Some(next) {
                            live_fade_sound.set(Some(next));
                            on_widget.call(WidgetEdit { key: w.key.clone(), values: Some(with_fades(w, values.clone(), next)), keep: false });
                        }
                        return;
                    }
                    if let (Some(d), Some((w, values))) = (*fade_drag.peek(), fade.as_ref()) {
                        // The button was let go outside the window: the drag is over.
                        if !held {
                            release_fade(fade_drag, live_fade);
                            return;
                        }
                        let now = live_fade.peek().unwrap_or(fade_held);
                        let next = dragged_fades(&d, p.x, length, kept, now);
                        if *live_fade.peek() != Some(next) {
                            live_fade.set(Some(next));
                            on_widget.call(WidgetEdit { key: w.key.clone(), values: Some(with_fades(w, values.clone(), next)), keep: false });
                        }
                        return;
                    }
                }
            },
            onmouseup: move |_| {
                release_fade(fade_drag, live_fade);
                release_fade(fade_drag_sound, live_fade_sound);
                release_text(text_drag, live_text);
            },
            // While a widget is dragged, a transparent sheet covers the window (the cursor stays a drag cursor over every control).
            if fade_drag().is_some() || fade_drag_sound().is_some() || text_drag().is_some() {
                div { class: "wv-catcher" }
            }
            if st.source.is_none() {
                div { class: "wv-screen", p { class: "wv-failure", "{no_file}" } }
            } else {
                div { class: "wv-info",
                    span { "{line}" }
                    // the crop and the texts are opened from the corner of the picture
                    if editing().is_none() {
                        div { class: "wv-crop-tools",
                            if let Some((key, _, title, shown)) = frame_button.clone() {
                                button {
                                    class: if shown { "uis-btn ico wv-applied" } else { "uis-btn ico" },
                                    title: "{title}",
                                    "aria-label": "{title}",
                                    disabled: !ready,
                                    onclick: move |_| {
                                        // the widget is drawn on a still: a stream has none to size it by
                                        stop_stream();
                                        live.set(None);
                                        live_angle.set(None);
                                        refresh += 1;
                                        seed.set(true);
                                        editing.set(Some(key.clone()));
                                    },
                                    Icon { width: 14, height: 14, icon: LdCrop }
                                }
                            }
                            if let Some((key, _, title, shown)) = text_button.clone() {
                                button {
                                    class: if shown { "uis-btn ico wv-applied" } else { "uis-btn ico" },
                                    title: "{title}",
                                    "aria-label": "{title}",
                                    disabled: !ready,
                                    onclick: move |_| {
                                        stop_stream();
                                        refresh += 1;
                                        text_sel.set(None);
                                        editing.set(Some(key.clone()));
                                    },
                                    Icon { width: 14, height: 14, icon: LdType }
                                }
                            }
                            if let Some((key, _, title, shown)) = picture_button.clone() {
                                button {
                                    class: if shown { "uis-btn ico wv-applied" } else { "uis-btn ico" },
                                    title: "{title}",
                                    "aria-label": "{title}",
                                    disabled: !ready,
                                    onclick: move |_| {
                                        stop_stream();
                                        refresh += 1;
                                        text_sel.set(None);
                                        editing.set(Some(key.clone()));
                                    },
                                    Icon { width: 14, height: 14, icon: LdImage }
                                }
                            }
                        }
                    }
                    // while the crop is open: its reset and its close, in the corner of the screen and outside the frame, where they are easy to see
                    if let Some((w, values, _, _, state_margins)) = overlay.clone() {
                        div { class: "wv-crop-tools",
                            button {
                                class: if matches!(flip_now, "h" | "both") { "uis-btn ico active" } else { "uis-btn ico" },
                                title: "{s_flip_h}",
                                "aria-label": "{s_flip_h}",
                                onclick: {
                                    let (w, values) = (w.clone(), values.clone());
                                    move |_| on_widget.call(WidgetEdit { key: w.key.clone(), values: Some(with_flip(&w, values.clone(), true)), keep: false })
                                },
                                Icon { width: 14, height: 14, icon: LdFlipHorizontal }
                            }
                            button {
                                class: if matches!(flip_now, "v" | "both") { "uis-btn ico active" } else { "uis-btn ico" },
                                title: "{s_flip_v}",
                                "aria-label": "{s_flip_v}",
                                onclick: {
                                    let (w, values) = (w.clone(), values.clone());
                                    move |_| on_widget.call(WidgetEdit { key: w.key.clone(), values: Some(with_flip(&w, values.clone(), false)), keep: false })
                                },
                                Icon { width: 14, height: 14, icon: LdFlipVertical }
                            }
                            button {
                                class: "uis-btn ico",
                                title: "{s_reset}",
                                "aria-label": "{s_reset}",
                                disabled: !(state_margins != [0.0; 4] || live().is_some() || tilted || flip_now != "none" || live_angle().is_some()),
                                onclick: move |_| {
                                    live.set(None);
                                    live_angle.set(None);
                                    on_widget.call(WidgetEdit { key: w.key.clone(), values: None, keep: false });
                                },
                                Icon { width: 14, height: 14, icon: LdRotateCcw }
                            }
                            button {
                                class: "uis-btn ico",
                                title: "{s_close}",
                                "aria-label": "{s_close}",
                                onclick: move |_| {
                                    live.set(None);
                                    live_angle.set(None);
                                    refresh += 1;
                                    editing.set(None);
                                },
                                Icon { width: 14, height: 14, icon: LdX }
                            }
                        }
                    }
                }
                if let Some(layer_key) = texting_key.clone() {
                    if pictured {
                        PictureBar {
                            key: "{layer_key}{text_at:?}",
                            lang: lang.clone(),
                            values: text_at.and_then(|i| text_items.get(i).cloned()),
                            index: text_at,
                            count: text_items.len(),
                            on_select: move |i: usize| text_sel.set(Some(i)),
                            on_edit: {
                                let layer_key = layer_key.clone();
                                move |edit: TextEdit| on_texts.call((layer_key.clone(), edit))
                            },
                            on_copy: {
                                let (key, w, items) = (layer_key.clone(), text_w.clone(), text_items.clone());
                                move |_| {
                                    let (Some(w), Some(i)) = (w.as_ref(), text_at) else { return };
                                    let Some(v) = items.get(i) else { return };
                                    let face = work_text::Face::read(w, v);
                                    text_sel.set(Some(items.len()));
                                    on_texts.call((key.clone(), TextEdit::Add(work_text::moved(w, v, (face.x + 0.03, face.y + 0.03)))));
                                }
                            },
                            on_close: move |_| {
                                refresh += 1;
                                editing.set(None);
                            },
                        }
                    } else {
                        TextBar {
                            // another text is another box: its words start from the list's again
                            key: "{text_at:?}",
                            lang: lang.clone(),
                            values: text_at.and_then(|i| text_items.get(i).cloned()),
                            index: text_at,
                            on_add: {
                                let (key, w, count) = (layer_key.clone(), text_w.clone(), text_items.len());
                                move |_| {
                                    let Some(w) = w.as_ref() else { return };
                                    text_sel.set(Some(count));
                                    on_texts.call((key.clone(), TextEdit::Add(work_text::fresh(w, work_text::next_place(count)))));
                                }
                            },
                            on_edit: {
                                let layer_key = layer_key.clone();
                                move |edit: TextEdit| on_texts.call((layer_key.clone(), edit))
                            },
                            on_copy: {
                                let (key, w, items) = (layer_key.clone(), text_w.clone(), text_items.clone());
                                move |_| {
                                    let (Some(w), Some(i)) = (w.as_ref(), text_at) else { return };
                                    let Some(v) = items.get(i) else { return };
                                    let face = work_text::Face::read(w, v);
                                    text_sel.set(Some(items.len()));
                                    on_texts.call((key.clone(), TextEdit::Add(work_text::moved(w, v, (face.x + 0.03, face.y + 0.03)))));
                                }
                            },
                            on_close: move |_| {
                                refresh += 1;
                                editing.set(None);
                            },
                        }
                    }
                }
                div {
                    class: "wv-screen",
                    if !src.is_empty() {
                        // ONE stage and one image whatever happens over them: an image made again would open a new stream and show nothing until
                        // the next picture is made (a paused view makes none).
                        div {
                            class: "wv-stage",
                            id: "{stage_id}",
                            // the shape of the picture, for the folded view, where the picture is drawn as large as the screen allows
                            style: picture().map(|(pw, ph, _)| format!("--ar: {}", f64::from(pw) / f64::from(ph.max(1)))),
                            img {
                                class: if pick().is_some() { "wv-picture wv-picking" } else { "wv-picture" },
                                src: "{src}",
                                onmounted: move |e| picture_box.set(Some(e.data())),
                                onclick: move |e| {
                                    let Some(kind) = *pick.peek() else { return };
                                    let at = e.element_coordinates();
                                    let Some(mounted) = picture_box.peek().clone() else { return };
                                    let st = state.peek().clone();
                                    let moment = *pos.peek();
                                    let length = match &*info.peek() {
                                        Some(Ok(p)) => p.duration_us as f64 / 1_000_000.0,
                                        _ => 0.0,
                                    };
                                    pick.set(None);
                                    spawn(async move {
                                        let Ok(area) = mounted.get_client_rect().await else { return };
                                        if area.size.width <= 0.0 || area.size.height <= 0.0 {
                                            return;
                                        }
                                        let fraction = (at.x / area.size.width, at.y / area.size.height);
                                        let effects = st.effects.clone();
                                        let made = tokio::task::spawn_blocking(move || {
                                            let (Some(source), Some(ff)) = (st.source.clone(), RUNNER.ffmpeg()) else { return Err(String::new()) };
                                            // the colour as it is BEFORE the effect: what the pickers are meant to correct
                                            let shown = picture_effects(&st.effects, Some(work_colour::KEY));
                                            let look = Look { size: &st.size, effects: &shown, duration_s: length, trim: Trim::none() };
                                            let rgb = preview::sample(&ff, &Spec { input: &source, start_s: moment, length_s: None, look: &look, width: PICTURE_WIDTH, fps: MAX_FPS, sound: None }, fraction)?;
                                            let balance = if kind == Pick::Balance { Some(engine::grade_pick::white_balance(&ff, rgb)?) } else { None };
                                            Ok(work_colour::picked(&effects, kind, rgb, balance))
                                        })
                                        .await
                                        .unwrap_or_else(|e| Err(e.to_string()));
                                        match made {
                                            Ok(edit) => on_widget.call(edit),
                                            Err(why) => failure.set(why),
                                        }
                                    });
                                },
                                // while the rotation is edited the picture shows what the rotation receives, turned as the handle says
                                style: if let Some((w, values, degrees, _)) = &turning { format!("transform: rotate({}deg)", degrees - quarter_angle(w, values)) } else { "transform: none".to_owned() },
                            }
                            if let Some((w, values, ..)) = overlay.clone().or_else(|| turning.clone().map(|t| (t.0, t.1, (0.0, 0.0), [0.0; 4], [0.0; 4]))) {
                                {
                                    let frame = (overlay.clone(), turning.clone());
                                    let (full, held) = match &frame.0 {
                                        Some(o) => (o.2, o.3),
                                        None => ((0.0, 0.0), [0.0; 4]),
                                    };
                                    let rect = frame.0.as_ref().map(|_| [held[0] / full.0, held[1] / full.1, held[2] / full.0, held[3] / full.1]);
                                    let angle = frame.1.as_ref().map(|t| t.2);
                                    let (w_rect, values_rect) = (w.clone(), values.clone());
                                    let (w_angle, values_angle) = (w.clone(), values.clone());
                                    rsx! {
                                        UisCropRotate {
                                            rect,
                                            angle,
                                            min: MIN_KEPT / full.0.min(full.1).max(1.0),
                                            on_rect: move |r: [f64; 4]| {
                                                let margins = [r[0] * full.0, r[1] * full.1, r[2] * full.0, r[3] * full.1].map(f64::round);
                                                if *live.peek() == Some(margins) {
                                                    return;
                                                }
                                                moves += 1;
                                                live.set(Some(margins));
                                                on_widget.call(WidgetEdit { key: w_rect.key.clone(), values: Some(with_margins(&w_rect, values_rect.clone(), margins)), keep: false });
                                            },
                                            on_angle: move |degrees: f64| {
                                                if *live_angle.peek() == Some(degrees) {
                                                    return;
                                                }
                                                moves += 1;
                                                live_angle.set(Some(degrees));
                                                on_widget.call(WidgetEdit { key: w_angle.key.clone(), values: Some(with_angle(&w_angle, values_angle.clone(), degrees)), keep: false });
                                            },
                                            on_end: move |_| {
                                                // the work view's list takes a moment to follow, so the numbers stay as dragged until it has
                                                let seen = *moves.peek();
                                                spawn(async move {
                                                    tokio::time::sleep(Duration::from_millis(600)).await;
                                                    if *moves.peek() == seen {
                                                        live.set(None);
                                                        live_angle.set(None);
                                                    }
                                                });
                                            },
                                        }
                                    }
                                }
                            }
                            if let (Some(_), Some((pw, ph, _))) = (texting.clone(), picture()) {
                                div {
                                    class: "wv-texts",
                                    onmounted: move |e| layer.set(Some(e.data())),
                                    // a click on the bare picture lets go of the selected text
                                    onmousedown: move |_| text_sel.set(None),
                                    for (i, face) in text_faces.iter().cloned().enumerate() {
                                        {
                                            let (x, y, zone) = match live_text() {
                                                Some((n, x, y, zone)) if n == i => (x, y, zone),
                                                _ => (face.x, face.y, face.width),
                                            };
                                            let (bw, bh) = match (pictured, text_w.as_ref(), text_items.get(i)) {
                                                (true, Some(w), Some(v)) => work_picture::extent(&work_picture::file_of(w, v), zone, (pw, ph)),
                                                _ => work_text::extent(&work_text::Face { width: zone, ..face.clone() }, (pw, ph)),
                                            };
                                            let place = format!("left: {:.3}%; top: {:.3}%; width: {bw:.3}%; height: {bh:.3}%", x * 100.0, y * 100.0);
                                            rsx! {
                                                div {
                                                    key: "{i}",
                                                    class: if text_at == Some(i) { "wv-text wv-text-on" } else { "wv-text" },
                                                    style: "{place}",
                                                    onpointerdown: {
                                                        let stage_id = stage_id.clone();
                                                        move |e| capture(&stage_id, e.data().pointer_id())
                                                    },
                                                    onmousedown: move |e| {
                                                        e.stop_propagation();
                                                        e.prevent_default();
                                                        text_sel.set(Some(i));
                                                        let p = e.client_coordinates();
                                                        begin_text(i, work_text::Grab::Move, (p.x, p.y), (x, y), layer, text_drag);
                                                    },
                                                    if text_at == Some(i) {
                                                        div {
                                                            class: "wv-text-grip",
                                                            onpointerdown: {
                                                                let stage_id = stage_id.clone();
                                                                move |e| capture(&stage_id, e.data().pointer_id())
                                                            },
                                                            onmousedown: move |e| {
                                                                e.stop_propagation();
                                                                e.prevent_default();
                                                                let p = e.client_coordinates();
                                                                begin_text(i, work_text::Grab::Width, (p.x, p.y), (x, y), layer, text_drag);
                                                            },
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    if !shown_failure.is_empty() {
                        p { class: "wv-failure", "{shown_failure}" }
                    } else if !left_out().is_empty() {
                        p { class: "wv-failure", "{s_left_out}
{left_out().join(\"\n\")}" }
                    }
                }
                div { class: "wv-times",
                    span { class: "wv-now", "{tc_now} | {frame_now}" }
                    span { class: "wv-length", "{duration_label}: {tc_length} | {frames} {frames_label}" }
                }
                div { class: "wv-film", style: "{region}",
                  FilmRow { film }
                  div { class: "wv-film-dim" }
                input {
                    class: "wv-timeline",
                    r#type: "range",
                    min: "0",
                    max: "{length}",
                    step: "{step}",
                    value: "{now}",
                    style: "{region}",
                    disabled: !ready,
                    oninput: move |e| {
                        if let Ok(v) = e.value().parse::<f64>() {
                            pos.set(v);
                            if *playing.peek() {
                                start_stream(v);
                            }
                        }
                    },
                }
                    if let (true, Some(w), Some(words)) = (ready, fade_w.clone(), fade_words.clone()) {
                        FadeLane { w, values: fade_held_values.clone(), held: fade_held, shown: fade_shown, kept, length, words, strip, drag: fade_drag, live: live_fade, on_widget }
                    }
                }
                if !wave().is_empty() {
                    div { class: "wv-wave", style: "{region}",
                        WaveBars { wave }
                        div { class: "wv-film-dim" }
                    if let (true, Some(w), Some(words)) = (ready, fade_sound_w.clone(), fade_sound_words.clone()) {
                        FadeLane { w, values: fade_sound_values.clone(), held: fade_sound_held, shown: fade_sound_shown, kept, length, words, strip: strip_sound, drag: fade_drag_sound, live: live_fade_sound, on_widget }
                    }
                    }
                }
                div { class: "wv-bar",
                    button {
                        class: "uis-btn",
                        title: "{s_set_in}",
                        "aria-label": "{s_set_in}",
                        disabled: !ready,
                        onclick: move |_| on_mark.call(Mark::Start(preview::clock(*pos.peek()))),
                        "["
                    }
                    button {
                        class: "uis-btn ico",
                        title: "{s_to_in}",
                        "aria-label": "{s_to_in}",
                        disabled: !ready,
                        onclick: move |_| {
                            let at = written(&state.peek().cut_start).unwrap_or(0.0);
                            pos.set(at);
                            if *playing.peek() {
                                start_stream(at);
                            }
                        },
                        "|<"
                    }
                    button {
                        class: "uis-btn ico",
                        title: "{s_back}",
                        "aria-label": "{s_back}",
                        disabled: !ready,
                        onclick: move |_| {
                            stop_stream();
                            let v = *pos.peek();
                            pos.set((v - step).max(0.0));
                        },
                        Icon { width: 14, height: 14, icon: LdChevronLeft }
                    }
                    button {
                        class: "uis-btn ico",
                        title: "{s_play}",
                        "aria-label": "{s_play}",
                        disabled: !ready,
                        onclick: move |_| {
                            if *playing.peek() {
                                stop_stream();
                            } else {
                                // Play from the end starts again from the start of the cut.
                                let mut at = *pos.peek();
                                if at >= length {
                                    at = written(&state.peek().cut_start).unwrap_or(0.0);
                                    pos.set(at);
                                }
                                start_stream(at);
                            }
                        },
                        if playing() {
                            Icon { width: 14, height: 14, icon: LdPause }
                        } else {
                            Icon { width: 14, height: 14, icon: LdPlay }
                        }
                    }
                    button {
                        class: "uis-btn ico",
                        title: "{s_forward}",
                        "aria-label": "{s_forward}",
                        disabled: !ready,
                        onclick: move |_| {
                            stop_stream();
                            let v = *pos.peek();
                            pos.set((v + step).min(length));
                        },
                        Icon { width: 14, height: 14, icon: LdChevronRight }
                    }
                    button {
                        class: "uis-btn ico",
                        title: "{s_to_out}",
                        "aria-label": "{s_to_out}",
                        disabled: !ready,
                        onclick: move |_| {
                            stop_stream();
                            pos.set(written(&state.peek().cut_end).unwrap_or(length));
                        },
                        ">|"
                    }
                    button {
                        class: "uis-btn",
                        title: "{s_set_out}",
                        "aria-label": "{s_set_out}",
                        disabled: !ready,
                        onclick: move |_| on_mark.call(Mark::End(preview::clock(*pos.peek()))),
                        "]"
                    }
                    button {
                        class: "uis-btn ico subtle",
                        title: "{s_clear}",
                        "aria-label": "{s_clear}",
                        disabled: cut_in.is_none() && cut_out.is_none(),
                        onclick: move |_| on_mark.call(Mark::Clear),
                        Icon { width: 14, height: 14, icon: LdX }
                    }
                    button {
                        class: "uis-btn ico subtle",
                        title: "{s_mute}",
                        "aria-label": "{s_mute}",
                        onclick: move |_| {
                            let now_muted = !*muted.peek();
                            muted.set(now_muted);
                            if let Some(s) = stream.peek().as_ref() {
                                s.set_volume(level());
                            }
                        },
                        if muted() || volume() == 0 {
                            Icon { width: 14, height: 14, icon: LdVolumeX }
                        } else {
                            Icon { width: 14, height: 14, icon: LdVolume2 }
                        }
                    }
                    input {
                        class: "uis-sld wv-volume",
                        r#type: "range",
                        min: "0",
                        max: "100",
                        title: "{s_volume}",
                        "aria-label": "{s_volume}",
                        value: "{volume}",
                        oninput: move |e| {
                            if let Ok(v) = e.value().parse::<u32>() {
                                volume.set(v);
                                muted.set(false);
                                if let Some(s) = stream.peek().as_ref() {
                                    s.set_volume(level());
                                }
                            }
                        },
                    }
                    // the three panels under the picture: the file list, the colour drawer, the curves drawer (one at a time)
                    button {
                        class: if drawer() == Drawer::Files { "uis-btn ico active" } else { "uis-btn ico" },
                        title: "{s_files}",
                        "aria-label": "{s_files}",
                        "aria-pressed": "{drawer() == Drawer::Files}",
                        onclick: move |_| {
                            pick.set(None);
                            folded.set(false);
                            drawer.set(Drawer::Files);
                        },
                        Icon { width: 14, height: 14, icon: LdList }
                    }
                    for (key, kind, title, shown) in bar_buttons.iter().filter(|b| b.1 != "frame" && b.1 != "fades" && b.1 != effects::TEXTS && b.1 != effects::PICTURES).cloned() {
                        button {
                            key: "{key}",
                            class: if (kind == "grade" && drawer() == Drawer::Colour) || (kind == "curves" && drawer() == Drawer::Curves) || editing().as_deref() == Some(key.as_str()) { "uis-btn ico active" } else if shown { "uis-btn ico wv-applied" } else { "uis-btn ico" },
                            title: "{title}",
                            "aria-label": "{title}",
                            "aria-pressed": "{(kind == \"grade\" && drawer() == Drawer::Colour) || (kind == \"curves\" && drawer() == Drawer::Curves) || editing().as_deref() == Some(key.as_str())}",
                            disabled: !ready,
                            onclick: {
                                let (key, kind) = (key.clone(), kind.clone());
                                move |_| {
                                    // The grading panel is not drawn over the picture: it opens in the place of the file list, and the picture shows the effect.
                                    if kind == "grade" || kind == "curves" {
                                        let now = true;
                                        pick.set(None);
                                        if now && editing.peek().is_some() {
                                            live.set(None);
                                            live_angle.set(None);
                                            refresh += 1;
                                            editing.set(None);
                                        }
                                        // one drawer at a time; a drawer opened from the bar shows the panel again
                                        folded.set(false);
                                        drawer.set(if kind == "grade" { Drawer::Colour } else { Drawer::Curves });
                                        return;
                                    }
                                    let now = editing.peek().as_deref() != Some(key.as_str());
                                    if now {
                                        // the widget is drawn on a still: a stream has none to size it by
                                        stop_stream();
                                    }
                                    live.set(None);
                                    live_angle.set(None);
                                    // another picture is wanted (without the effect while editing, with it after), even when nothing else changed
                                    refresh += 1;
                                    seed.set(now && kind == "frame");
                                    editing.set(if now { Some(key.clone()) } else { None });
                                }
                            },
                            if kind == "grade" {
                                Icon { width: 14, height: 14, icon: LdPalette }
                            } else if kind == "curves" {
                                Icon { width: 14, height: 14, icon: LdSpline }
                            } else {
                                Icon { width: 14, height: 14, icon: LdCrop }
                            }
                        }
                    }
                    // the sound's drawers (the equaliser's curve, the compressor, the channels): the same buttons as FFmpeg Audio Studio's
                    for (which, key, kind) in sound_drawers.clone() {
                        {
                            let level = kind == "level";
                            let tip = if level { s_volume.clone() } else { engine::effects::Item::new(&key).title() };
                            let on = st.effects.items.iter().any(|i| i.key == key || (level && crate::work_level::MODES.contains(&i.key.as_str())));
                            rsx! {
                                button {
                                    key: "{key}",
                                    class: if drawer() == which { "uis-btn ico active" } else if on { "uis-btn ico wv-applied" } else { "uis-btn ico" },
                                    title: "{tip}",
                                    "aria-label": "{tip}",
                                    "aria-pressed": "{drawer() == which}",
                                    onclick: move |_| {
                                        pick.set(None);
                                        folded.set(false);
                                        drawer.set(which);
                                    },
                                    match kind {
                                        "tone" => rsx! { Icon { width: 14, height: 14, icon: LdSlidersVertical } },
                                        "compressor" => rsx! { Icon { width: 14, height: 14, icon: LdActivity } },
                                        "level" => rsx! { Icon { width: 14, height: 14, icon: LdGauge } },
                                        _ => rsx! { Icon { width: 14, height: 14, icon: LdArrowLeftRight } },
                                    }
                                }
                            }
                        }
                    }
                    span { class: "uis-bar-spacer" }
                    span { class: "wv-cut", "{tc_in}" }
                    span { class: "wv-cut", "{tc_out}" }
                    button {
                        class: if folded() { "uis-btn ico active" } else { "uis-btn ico" },
                        title: "{s_fold}",
                        "aria-label": "{s_fold}",
                        "aria-pressed": "{folded()}",
                        onclick: move |_| folded.set(!folded()),
                        if folded() {
                            Icon { width: 14, height: 14, icon: LdChevronsUp }
                        } else {
                            Icon { width: 14, height: 14, icon: LdChevronsDown }
                        }
                    }
                }
            }
        }
    }
}

/// One fade drawn ON a band of the timeline (the picture's on the film strip, the sound's on the sound band): a ramp and a round handle at each
/// end of the kept part, and a number box each in the band's corners. The picture's and the sound's are the same lane over two effects.
#[component]
fn FadeLane(
    w: effects::WidgetSpec,
    values: Values,
    held: (f64, f64),
    shown: (f64, f64),
    kept: (f64, f64),
    length: f64,
    words: ((String, String), (String, String)),
    mut strip: Signal<Option<Rc<MountedData>>>,
    drag: Signal<Option<FadeDrag>>,
    live: Signal<Option<(f64, f64)>>,
    on_widget: EventHandler<WidgetEdit>,
) -> Element {
    let ((label_in, unit_in), (label_out, unit_out)) = words;
            {
                let at = |v: f64| if length > 0.0 { (v / length * 100.0).clamp(0.0, 100.0) } else { 0.0 };
                let start = at(kept.0);
                let end = at(kept.1);
                let in_at = at(kept.0 + shown.0);
                let out_at = at(kept.1 - shown.1);
                let held_now = (shown.0, shown.1);
                let (values_in, values_out, w_in, w_out) = (values.clone(), values.clone(), w.clone(), w.clone());
                rsx! {
                    div {
                        class: "wv-fades",
                        onmounted: move |e| strip.set(Some(e.data())),
                        div { class: "wv-fade-ramp wv-fade-ramp-in", style: "left: {start}%; width: {in_at - start}%" }
                        div { class: "wv-fade-ramp wv-fade-ramp-out", style: "left: {out_at}%; width: {end - out_at}%" }
                        div {
                            class: "wv-fade-handle",
                            title: "{label_in}",
                            style: "left: {in_at}%",
                            onmousedown: move |e| {
                                e.stop_propagation();
                                e.prevent_default();
                                begin_fade(FadeEnd::In, held_now, strip, drag, live);
                            },
                        }
                        div {
                            class: "wv-fade-handle",
                            title: "{label_out}",
                            style: "left: {out_at}%",
                            onmousedown: move |e| {
                                e.stop_propagation();
                                e.prevent_default();
                                begin_fade(FadeEnd::Out, held_now, strip, drag, live);
                            },
                        }
                    }
                    div { class: "wv-fadenums uis-cmp-inline",
                        UisFldLabelled { compact: true, label: label_in.clone(),
                            UisNum {
                                value: Some(shown.0),
                                min: Some(0.0),
                                max: Some(MAX_FADE),
                                step: 0.1,
                                decimals: Some(1),
                                width: "64px".to_owned(),
                                on_change: move |v: Option<f64>| on_widget.call(WidgetEdit { key: w_in.key.clone(), values: Some(with_fades(&w_in, values_in.clone(), (v.unwrap_or(0.0), held.1))), keep: false }),
                            }
                            span { "{unit_in}" }
                        }
                        UisFldLabelled { compact: true, label: label_out.clone(),
                            UisNum {
                                value: Some(shown.1),
                                min: Some(0.0),
                                max: Some(MAX_FADE),
                                step: 0.1,
                                decimals: Some(1),
                                width: "64px".to_owned(),
                                on_change: move |v: Option<f64>| on_widget.call(WidgetEdit { key: w_out.key.clone(), values: Some(with_fades(&w_out, values_out.clone(), (held.0, v.unwrap_or(0.0)))), keep: false }),
                            }
                            span { "{unit_out}" }
                        }
                    }
                }
            }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame() -> effects::WidgetSpec {
        effects::WidgetSpec { key: "crop-edges".to_owned(), kind: "frame".to_owned(), controls: [("left", "left"), ("tilt", "tilt"), ("quarter", "quarter"), ("flip", "flip")].into_iter().map(|(a, b)| (a.to_owned(), b.to_owned())).collect() }
    }

    #[test]
    fn a_fade_handle_sets_its_own_length_in_tenths_inside_the_kept_part() {
        // a 100 s file on a 1000 px bar starting at x = 50; the part from 20 s to 60 s is kept
        let bar = |end| FadeDrag { end, left: 50.0, width: 1000.0 };
        let kept = (20.0, 60.0);
        // the in handle at 25.04 s (x = 300.4) is a 5 s fade in; the fade out stays as it was
        assert_eq!(dragged_fades(&bar(FadeEnd::In), 300.4, 100.0, kept, (0.0, 2.0)), (5.0, 2.0));
        // pulled left of the kept part it is none, pulled to the right it stops where the fade out begins
        assert_eq!(dragged_fades(&bar(FadeEnd::In), 100.0, 100.0, kept, (3.0, 2.0)).0, 0.0);
        assert_eq!(dragged_fades(&bar(FadeEnd::In), 900.0, 100.0, kept, (0.0, 15.0)).0, 25.0);
        // the out handle at 56 s (x = 610) is 4 s; past the end it is none, and it stops where the fade in ends
        assert_eq!(dragged_fades(&bar(FadeEnd::Out), 610.0, 100.0, kept, (1.0, 0.0)), (1.0, 4.0));
        assert_eq!(dragged_fades(&bar(FadeEnd::Out), 900.0, 100.0, kept, (1.0, 3.0)).1, 0.0);
        assert_eq!(dragged_fades(&bar(FadeEnd::Out), 100.0, 100.0, kept, (10.0, 0.0)).1, 30.0);
    }

    #[test]
    fn the_angle_goes_into_the_tilt_control_and_comes_back_from_it() {
        let w = frame();
        for degrees in [0.0, 12.5, -12.5, 90.0, 135.0, 180.0, -179.0, -90.0] {
            let values = with_angle(&w, Values::new(), degrees);
            assert!((angle_from(&w, &values) - normal_angle(degrees)).abs() < 0.051, "{degrees}");
        }
        assert_eq!(with_angle(&w, Values::new(), 12.34).get("tilt"), Some(&V::Num(12.3)));
        assert_eq!(angle_from(&w, &Values::new()), 0.0);
    }

    #[test]
    fn a_quarter_or_half_turn_is_the_quarter_control_and_any_other_angle_is_the_tilt() {
        let w = frame();
        let q = |d: f64| with_angle(&w, Values::new(), d);
        assert_eq!(q(90.0).get("quarter"), Some(&V::Str("cw".to_owned())));
        assert_eq!(q(-90.0).get("quarter"), Some(&V::Str("ccw".to_owned())));
        assert_eq!(q(180.0).get("quarter"), Some(&V::Str("half".to_owned())));
        assert_eq!(q(90.0).get("tilt"), Some(&V::Num(0.0)));
        assert_eq!(q(30.0).get("quarter"), Some(&V::Str("none".to_owned())));
        assert_eq!(q(30.0).get("tilt"), Some(&V::Num(30.0)));
    }

    #[test]
    fn the_two_mirrors_switch_one_at_a_time_and_make_both() {
        let w = frame();
        let mut v = Values::new();
        assert_eq!(flip_from(&w, &v), "none");
        v = with_flip(&w, v, true);
        assert_eq!(flip_from(&w, &v), "h");
        v = with_flip(&w, v, false);
        assert_eq!(flip_from(&w, &v), "both");
        v = with_flip(&w, v, true);
        assert_eq!(flip_from(&w, &v), "v");
        v = with_flip(&w, v, false);
        assert_eq!(flip_from(&w, &v), "none");
    }
}
