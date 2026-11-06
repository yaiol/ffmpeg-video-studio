//! Effects: what is done to the picture and the sound besides its size. A named entry the user keeps in Settings and picks in the
//! window, apart from the preset, holding a list of effects; the same entry goes with any codec.
//!
//! The effects themselves are the catalogue installed with ffmpeg (`effects.json` in the pair's folder, see `download`; a form per effect,
//! read by `ffmpeg_effects`, the module shared with the FFmpeg Audio Studio). No copy of it is embedded: without it no effect can be built; an entry keeps, per effect, its key and the values of its controls. The size cap is not an effect (it is a
//! setting of its own) but takes a place among them: the effects that need the delivered size (a picture laid over, text, stereoscopic,
//! playing backwards) run AFTER it, the others BEFORE it, each group in the order listed (`Catalog::order`). A watermark is a second input
//! and the blurred background splits the picture in two, so either turns the video filter into a filter graph (`compose`); the rest
//! are ordinary filters on the one input. A copied video stream takes none of it.

use crate::{
    ffmpeg_effects::{self as fx, Built, Catalog, Ctx, Values, V},
    size::Size,
    trim::Trim,
};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, path::Path, sync::{Arc, RwLock}};

pub use fx::VideoFilter as Filter;

/// What the app converts, for the catalogue's menu: the effects that work on it.
pub const WORKS: &str = "video";

/// This app's id, as the catalogue names it (`hiddenIn`).
pub const APP: &str = "ffmpeg-video-studio";

/// Id of the window-only choice that applies no effects. Not an entry of Settings; the window names it.
pub const NONE_ID: &str = "none";

/// The key of the effect that draws the file's own subtitles into the picture.
const BURN: &str = "burn-subtitles";

/// The catalogue as read, with the words of every language; `CATALOG` is the copy in the language of the window.
static BASE: RwLock<Option<Arc<Catalog>>> = RwLock::new(None);
static LANG: RwLock<String> = RwLock::new(String::new());
static CATALOG: RwLock<Option<Arc<Catalog>>> = RwLock::new(None);

fn publish() {
    let base = BASE.read().unwrap().clone();
    let lang = LANG.read().unwrap().clone();
    *CATALOG.write().unwrap() = base.map(|b| Arc::new(b.localized(&lang)));
}

/// The language the catalogue speaks (`fr`, `pt_BR`...): the window calls it with its own, at launch and on every change.
pub fn set_language(lang: &str) {
    {
        let mut l = LANG.write().unwrap();
        if *l == lang {
            return;
        }
        *l = lang.to_owned();
    }
    publish();
}

/// The catalogue installed with ffmpeg, when it is loaded (`load_installed`).
pub fn catalog() -> Option<Arc<Catalog>> {
    #[cfg(test)]
    test_fixture();
    CATALOG.read().unwrap().clone()
}

/// Read the catalogue from its text (the app's tests give it the fixture).
pub fn set_catalog_text(text: &str) -> Result<(), String> {
    let catalog = Catalog::from_json(text).map_err(|e| format!("the effects catalogue could not be read: {e}"))?;
    *BASE.write().unwrap() = Some(Arc::new(catalog));
    publish();
    Ok(())
}

/// The favourites live beside the catalogue of the installed pair.
fn load_favourites(dir: Option<&Path>) {
    FAVOURITES.load(dir.and_then(crate::download::catalogue_path).as_deref().and_then(Path::parent));
}

/// Load the catalogue of the pair installed in `dir`, or none when there is no pair there (or it cannot be read). Called at launch and
/// after every install.
#[cfg(not(debug_assertions))]
pub fn load_installed(dir: Option<&Path>) -> Result<(), String> {
    load_favourites(dir);
    let loaded = dir
        .and_then(crate::download::catalogue_path)
        .map(|path| std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display())).and_then(|text| set_catalog_text(&text)));
    match loaded {
        Some(result) => {
            if result.is_err() {
                *BASE.write().unwrap() = None;
                publish();
            }
            result
        }
        None => {
            *BASE.write().unwrap() = None;
            publish();
            Ok(())
        }
    }
}

/// A debug build (the "(Dev)" app) works on the catalogue of the source tree - the one the shared folder makes, which the tests also read - and not on the
/// one installed from the site, so an effect being written shows at once and nothing is published to try it. Never part of a release build.
#[cfg(debug_assertions)]
pub fn load_installed(dir: Option<&Path>) -> Result<(), String> {
    load_favourites(dir);
    set_catalog_text(include_str!("../testdata/effects.json"))
}

/// The tests' catalogue: a copy of the one the shared folder makes (never part of a build).
#[cfg(test)]
fn test_fixture() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| set_catalog_text(include_str!("../testdata/effects.json")).expect("the fixture reads"));
}

/// The order of the STANDARD effects (the catalogue's `standard` level), which the work view shows as rows and runs in: the crop FIRST (decided
/// 2026-10-29: it is drawn on the original picture and every other effect works on what it leaves), then the picture's geometry, then repairs, light and colour (ONE `colour` effect, the grading panel: lift, gamma, gain, offset, temperature, saturation...), finishing touches, the overlays, what changes time, and the fade last of all (it fades the picture that is delivered: after playing backwards, the end of the clip is what fades out). Fixed on purpose (decided
/// 2026-10-28): the user has no order to manage there. A standard effect not named here comes after these, by title.
pub const STANDARD_ORDER: &[&str] = &[
    "crop-edges", "blurfit", "try:yadif", "try:hqdn3d", "try:deflicker", "try:deshake", "colour", "curves",
    "try:lut3d", "try:unsharp", "try:noise", "try:vignette", "text", "watermark", "burn-subtitles", "try:fps",
    "play-backwards", "picture-fade",
];

/// The order of the STANDARD effects of the sound, the same as FFmpeg Audio Studio's work view (decided 2026-11-01: the default sound effects of
/// the two converters are the same). The ones that need the file measured (cut silence, the range, the length, the four ways to set the
/// volume, silence at the end, ReplayGain tags) are in the catalogue's list but marked `hiddenIn` for this app, so they are not offered.
pub const SOUND_STANDARD_ORDER: &[&str] = crate::measured::SOUND_STANDARD_ORDER;

/// The id of the sound entry a fresh install starts with (FFmpeg Audio Studio's: an ordinary entry with no effect in it).
pub const SOUND_STANDARD_ID: &str = "default-effects";

/// What the user chose to list in the work view, kept beside the catalogue (`favourites.json`).
static FAVOURITES: fx::favourites::Choice = fx::favourites::Choice::new("ffmpeg-video-studio");

/// The effects the user has chosen to list (`None`: he has not chosen, and the catalogue's standard effects are listed).
pub fn favourites() -> Option<Vec<String>> {
    FAVOURITES.get()
}

/// Choose the effects to list in the work view (`None` takes the choice back).
pub fn set_favourites(keys: Option<Vec<String>>) -> Result<(), String> {
    FAVOURITES.set(keys)
}

/// An effect the app itself is wired to, so it is always listed: one drawn by a widget, or a hand-written effect the work view reads by key.
/// (The plain filters of `STANDARD_ORDER`, `try:...`, are only an order: the user may take them out.)
pub fn is_locked(entry: &fx::Entry) -> bool {
    entry.widget.is_some() || ((STANDARD_ORDER.contains(&entry.key.as_str()) || SOUND_STANDARD_ORDER.contains(&entry.key.as_str())) && !entry.key.starts_with("try:"))
}

/// Is this effect listed in the work view, given what the user chose (`None`: nothing): a locked one by the catalogue's level, the others by
/// the choice, or by the level when there is none.
fn is_listed(entry: &fx::Entry, chosen: Option<&[String]>) -> bool {
    if is_locked(entry) {
        return entry.level == "standard";
    }
    chosen.map_or(entry.level == "standard", |keys| keys.contains(&entry.key))
}

/// Is this effect listed in the work view now.
pub fn is_standard(entry: &fx::Entry) -> bool {
    is_listed(entry, FAVOURITES.get().as_deref())
}

/// Does this effect act on the sound alone? It then belongs to the list of sound effects (Effects Audio), not to the picture's.
pub fn is_sound(key: &str) -> bool {
    catalog().and_then(|c| c.works_of(key)).is_some_and(|w| w == "audio")
}

/// The keys of the standard effects this catalogue offers, in the fixed order of the work view.
pub fn standard_keys(cat: &Catalog, system: &str) -> Vec<String> {
    standard_keys_by(cat, system, FAVOURITES.get().as_deref())
}

fn standard_keys_by(cat: &Catalog, system: &str, chosen: Option<&[String]>) -> Vec<String> {
    let mut entries: Vec<_> = cat.picker(system, WORKS).into_iter().filter(|e| is_listed(e, chosen)).collect();
    let rank = |key: &str| STANDARD_ORDER.iter().position(|k| *k == key).unwrap_or(STANDARD_ORDER.len());
    entries.sort_by(|a, b| rank(&a.key).cmp(&rank(&b.key)).then_with(|| a.title.to_lowercase().cmp(&b.title.to_lowercase())));
    entries.into_iter().map(|e| e.key).collect()
}

/// The keys of the standard effects of the SOUND this catalogue offers this app, in the fixed order of the sound's work view: the wired ones
/// first, then the ones the user chose to list (the favourites), by title.
pub fn sound_standard_keys(cat: &Catalog, system: &str) -> Vec<String> {
    sound_standard_keys_by(cat, system, FAVOURITES.get().as_deref())
}

fn sound_standard_keys_by(cat: &Catalog, system: &str, chosen: Option<&[String]>) -> Vec<String> {
    let mut entries: Vec<_> = cat.picker_for(system, "audio", true).into_iter().filter(|e| is_listed(e, chosen)).collect();
    let rank = |key: &str| SOUND_STANDARD_ORDER.iter().position(|k| *k == key).unwrap_or(SOUND_STANDARD_ORDER.len());
    entries.sort_by(|a, b| rank(&a.key).cmp(&rank(&b.key)).then_with(|| a.title.to_lowercase().cmp(&b.title.to_lowercase())));
    entries.into_iter().map(|e| e.key).collect()
}

/// A standard effect the catalogue draws with a widget of its own: the work view shows it as a surface (a rectangle over the picture), never as
/// a row, and it takes no place in the "Add an effect" menu there.
#[derive(Clone, Debug, PartialEq)]
pub struct WidgetSpec {
    pub key: String,
    pub kind: String,
    /// Which control of the effect feeds each input of the widget.
    pub controls: BTreeMap<String, String>,
}

/// Is this effect one that may be listed several times, each with its own values (the text and the watermark, drawn by a widget that places each one)?
pub fn repeats(key: &str) -> bool {
    catalog().and_then(|c| c.effects.get(key).and_then(|e| e.widget.as_ref().map(|w| w.kind == TEXTS || w.kind == PICTURES))).unwrap_or(false)
}

/// The widget kind of the text, an effect that repeats.
pub const TEXTS: &str = "texts";

/// The widget kind of the watermark, the other effect that repeats.
pub const PICTURES: &str = "pictures";

/// The widget effects this catalogue offers an app that converts `works`, in the fixed order of the standard effects.
pub fn widgets(cat: &Catalog, system: &str) -> Vec<WidgetSpec> {
    widgets_of(cat, standard_keys(cat, system))
}

/// The widget effects of the sound (the equaliser's curve, the compressor, the channels, the fade), in the fixed order.
pub fn sound_widgets(cat: &Catalog, system: &str) -> Vec<WidgetSpec> {
    widgets_of(cat, sound_standard_keys(cat, system))
}

fn widgets_of(cat: &Catalog, keys: Vec<String>) -> Vec<WidgetSpec> {
    keys.into_iter()
        .filter_map(|key| {
            let w = cat.effects.get(&key)?.widget.clone()?;
            Some(WidgetSpec { key, kind: w.kind, controls: w.controls })
        })
        .collect()
}

impl Item {
    /// Every control is at its default: the item does nothing, and a widget's item is left out of the list in that case.
    pub fn is_default(&self) -> bool {
        let (Some(cat), Some(effect)) = (catalog(), self.effect()) else { return false };
        self.all_values() == fx::defaults(&effect, &cat.filters).unwrap_or_default()
    }
}

impl EffectsEntry {
    /// The item of effect `key` takes `values` (put in the list when it is not there); with none, or with every value at its default, the
    /// effect leaves the list. Used by a widget, which is always there and only shows in the list once it changes something.
    pub fn set_widget(&mut self, key: &str, values: Option<&Values>) {
        let mut item = self.items.iter().find(|i| i.key == key).cloned().unwrap_or_else(|| Item::new(key));
        if let Some(v) = values {
            item.set_values(v);
        }
        self.items.retain(|i| i.key != key);
        if values.is_some() && !item.is_default() {
            self.items.push(item);
        }
    }

    /// A new item of effect `key` with these values, after the ones already listed: an effect that repeats (the text) is added again and again.
    pub fn add_item(&mut self, key: &str, values: &Values) {
        let mut item = Item::new(key);
        item.set_values(values);
        self.items.push(item);
    }

    /// The `n`th item of effect `key` (in the order listed) takes these values.
    pub fn set_nth(&mut self, key: &str, n: usize, values: &Values) {
        if let Some(item) = self.items.iter_mut().filter(|i| i.key == key).nth(n) {
            item.set_values(values);
        }
    }

    /// The `n`th item of effect `key` leaves the list.
    pub fn remove_nth(&mut self, key: &str, n: usize) {
        if let Some(at) = self.items.iter().enumerate().filter(|(_, i)| i.key == key).nth(n).map(|(at, _)| at) {
            self.items.remove(at);
        }
    }

    /// The same list with the standard effects first, each at its fixed place, and every other effect after them in the order it was
    /// added. A standard effect is listed once - except one that repeats (the text), which keeps every copy, together, in the order they
    /// were added. The work view's list keeps this shape.
    pub fn arranged(mut self, standard: &[String]) -> EffectsEntry {
        let mut rows: Vec<Item> = Vec::new();
        for key in standard {
            if repeats(key) {
                let (mine, rest): (Vec<Item>, Vec<Item>) = self.items.into_iter().partition(|i| &i.key == key);
                rows.extend(mine);
                self.items = rest;
            } else if let Some(at) = self.items.iter().position(|i| &i.key == key) {
                rows.push(self.items.remove(at));
            }
        }
        // a second copy of a standard effect is dropped; the rest keep their order
        self.items.retain(|i| !standard.contains(&i.key));
        rows.extend(self.items);
        self.items = rows;
        self
    }
}

/// One effect of an entry: which (a hand-written effect's id, or `try:` and an ffmpeg filter's name) and the values of its controls.
/// A control with no value has its default.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Item {
    pub key: String,
    pub values: BTreeMap<String, serde_json::Value>,
}

impl Item {
    pub fn new(key: &str) -> Item {
        Item { key: key.to_owned(), values: BTreeMap::new() }
    }

    pub fn with(key: &str, values: &[(&str, &str)]) -> Item {
        Item { key: key.to_owned(), values: values.iter().map(|(k, v)| ((*k).to_owned(), serde_json::Value::String((*v).to_owned()))).collect() }
    }

    /// The effect this item is, when the catalogue has it.
    pub fn effect(&self) -> Option<fx::Effect> {
        catalog()?.form(&self.key)
    }

    /// Whether one of the effect's steps is the ffmpeg filter `name`.
    pub fn uses_filter(&self, name: &str) -> bool {
        self.effect().is_some_and(|e| e.steps.iter().flatten().any(|st| st.filter.as_deref() == Some(name)))
    }

    /// The values of every control: the defaults, then what the user set.
    pub fn all_values(&self) -> Values {
        let (Some(cat), Some(effect)) = (catalog(), self.effect()) else { return Values::new() };
        let mut v = fx::defaults(&effect, &cat.filters).unwrap_or_default();
        for (id, x) in &self.values {
            v.insert(id.clone(), V::from_json(x));
        }
        v
    }

    /// Keep the values as they are after an edit (the whole map: a value equal to its default is kept too).
    pub fn set_values(&mut self, values: &Values) {
        self.values = values.iter().map(|(k, v)| (k.clone(), v.to_json())).collect();
    }

    /// The title the user reads.
    pub fn title(&self) -> String {
        let Some(cat) = catalog() else { return self.key.clone() };
        match self.key.strip_prefix("try:") {
            Some(name) => cat.filters.get(name).and_then(|f| f.plain.clone()).unwrap_or_else(|| name.to_owned()),
            None => cat.effects.get(&self.key).and_then(|e| e.plain.clone()).unwrap_or_else(|| self.key.clone()),
        }
    }

    /// Every control the effect asks for that is mandatory has a value; an item that is not is left out of the command (a watermark with
    /// no picture chosen yet, text with no words).
    pub fn is_ready(&self) -> bool {
        let (Some(cat), Some(effect)) = (catalog(), self.effect()) else { return false };
        let values = self.all_values();
        fx::resolve_controls(&effect, &cat.filters)
            .map(|cs| cs.iter().filter(|c| c.required).all(|c| values.get(&c.id).is_some_and(|v| !matches!(v, V::Null) && !v.js_string().trim().is_empty())))
            .unwrap_or(false)
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct EffectsEntry {
    pub id: String,
    pub name: String,
    #[serde(deserialize_with = "lenient_items")]
    pub items: Vec<Item>,
}

/// The effects of a saved entry, each read on its own: one that cannot be read (a file of an older build) is left out instead of
/// losing the whole file.
fn lenient_items<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Vec<Item>, D::Error> {
    let raw = Vec::<serde_json::Value>::deserialize(d)?;
    Ok(raw.into_iter().filter_map(|v| serde_json::from_value(v).ok()).collect())
}

impl EffectsEntry {
    /// The window-only "(None)": nothing is done.
    pub fn none() -> EffectsEntry {
        EffectsEntry { id: NONE_ID.to_owned(), ..EffectsEntry::default() }
    }

    /// The item of effect `key` takes `values` and stays in the list even when they are its defaults (an effect whose defaults still DO
    /// something: swap the channels); none takes it out. Used by a drawer that chooses an action.
    pub fn set_item(&mut self, key: &str, values: Option<&Values>) {
        let mut item = self.items.iter().find(|i| i.key == key).cloned().unwrap_or_else(|| Item::new(key));
        self.items.retain(|i| i.key != key);
        if let Some(v) = values {
            item.set_values(v);
            self.items.push(item);
        }
    }

    /// The sound entry a fresh install starts with, the same as FFmpeg Audio Studio's: an ordinary entry with no effect in it.
    pub fn standard_sound(name: &str) -> EffectsEntry {
        EffectsEntry { id: SOUND_STANDARD_ID.to_owned(), name: name.to_owned(), items: Vec::new() }
    }

    pub fn new(name: &str) -> EffectsEntry {
        EffectsEntry { id: crate::preset::new_id(), name: name.to_owned(), items: Vec::new() }
    }

    pub fn is_none(&self) -> bool {
        self.id == NONE_ID
    }

    /// The same entry without an item that names no effect (a saved one of an older build), and with the paths a user pasted cleaned of
    /// their quotes. An effect the catalogue lacks stays: another ffmpeg may bring it back, and the job says it is not available.
    pub fn normalized(mut self) -> EffectsEntry {
        self.items.retain(|i| !i.key.is_empty());
        let Some(cat) = catalog() else { return self };
        for item in &mut self.items {
            let Some(effect) = item.effect() else { continue };
            for c in fx::resolve_controls(&effect, &cat.filters).unwrap_or_default() {
                if !matches!(c.control.as_str(), "file" | "saveFile" | "folder") {
                    continue;
                }
                if let Some(serde_json::Value::String(s)) = item.values.get_mut(&c.id) {
                    *s = s.trim().trim_matches('"').trim().to_owned();
                }
            }
        }
        self
    }

    /// The picture carries the file's subtitles, so the output holds no subtitle track.
    pub fn burns_subtitles(&self) -> bool {
        self.items.iter().any(|i| i.key == BURN && i.is_ready())
    }

    /// The effects that are ready, in the order listed.
    fn ready(&self) -> Vec<&Item> {
        self.items.iter().filter(|i| i.is_ready()).collect()
    }

    /// What is wrong with the order of the effects: one that runs before another which changes what it works on.
    pub fn order_problems(&self) -> Vec<String> {
        let Some(cat) = catalog() else { return Vec::new() };
        cat.order(&self.ready().iter().map(|i| i.key.clone()).collect::<Vec<_>>()).problems
    }

    /// A file an effect names that is not there (a picture to lay over, a colour table), which would make ffmpeg stop with a message about
    /// a filter graph. The words the job fails with.
    pub fn missing_file(&self) -> Option<String> {
        let cat = catalog()?;
        for item in self.ready() {
            let Some(effect) = item.effect() else { continue };
            let values = item.all_values();
            for c in fx::resolve_controls(&effect, &cat.filters).unwrap_or_default() {
                if c.control != "file" {
                    continue;
                }
                let path = values.get(&c.id).map(V::js_string).unwrap_or_default();
                if !path.trim().is_empty() && !Path::new(path.trim()).is_file() {
                    return Some(format!("{} - {} not found: {}", item.title(), c.label, path));
                }
            }
        }
        None
    }

    /// The same list with only what acts on the sound alone: all a copied picture can still go with (the sound is re-encoded, the picture is
    /// not touched).
    pub fn sound_only(&self) -> EffectsEntry {
        EffectsEntry { items: self.items.iter().filter(|i| is_sound(&i.key)).cloned().collect(), ..self.clone() }
    }

    /// The sound's chain some effect needs, which means the sound cannot be copied (playing backwards).
    pub fn needs_audio_filter(&self) -> bool {
        self.ready().iter().filter_map(|i| i.effect()).any(|e| e.audio.as_ref().is_some_and(|a| !a.is_empty())) || self.ready().iter().any(|i| i.is_measured())
    }
}

/// What a fresh install starts with: one entry for each way of turning or flipping the picture - what the owner's FFmpeg Batch presets
/// did - with stable ids so a saved selection survives. A turn, then a flip.
pub fn defaults() -> Vec<EffectsEntry> {
    let turn = |way: &str| Item::with("turn", &[("way", way)]);
    let flip = |way: &str| Item::with("flip", &[("way", way)]);
    [
        ("cw", "Rotate 90° clockwise", vec![turn("cw")]),
        ("ccw", "Rotate 90° counter-clockwise", vec![turn("ccw")]),
        ("cwflip", "Rotate 90° clockwise and flip vertically", vec![turn("cw"), flip("v")]),
        ("ccwflip", "Rotate 90° counter-clockwise and flip vertically", vec![turn("ccw"), flip("v")]),
        ("half", "Rotate 180°", vec![turn("half")]),
        ("fliph", "Flip horizontally", vec![flip("h")]),
        ("flipv", "Flip vertically", vec![flip("v")]),
    ]
    .into_iter()
    .map(|(id, name, items)| EffectsEntry { id: format!("default-{id}"), name: name.to_owned(), items })
    .collect()
}

/// Everything the picture's treatment depends on for one file.
pub struct Look<'a> {
    pub size: &'a Size,
    pub effects: &'a EffectsEntry,
    /// The length of the OUTPUT in seconds, for the countdown (what is left after a trim); 0 when unknown.
    pub duration_s: f64,
    /// The part of the file that is kept.
    pub trim: Trim,
}

#[derive(Debug, PartialEq)]
pub struct Plan {
    /// Extra input files, in order: `[1:v]` is the first.
    pub inputs: Vec<String>,
    pub filter: Filter,
    /// The sound's chain an effect needs (`-af`).
    pub audio: Option<String>,
    /// The picture carries the file's subtitles.
    pub burns: bool,
}

impl Plan {
    pub fn none() -> Plan {
        Plan { inputs: Vec::new(), filter: Filter::None, audio: None, burns: false }
    }
}

/// The font file: Arial from the Windows font folder when there is one; elsewhere the filter's own default (fontconfig) stands.
/// The context of one item: an effect with a `family` control is drawn in that family's four faces (a family the machine lacks leaves the default font).
fn item_ctx(ctx: &Ctx, values: &Values) -> Ctx {
    let mut ctx = ctx.clone();
    if let Some(family) = values.get("family").map(V::js_string).and_then(|name| crate::fonts::find(&name)) {
        for (key, file) in [("font", &family.regular), ("font_bold", &family.bold), ("font_italic", &family.italic), ("font_bolditalic", &family.bold_italic)] {
            ctx.insert(key.to_owned(), file.clone());
        }
    }
    ctx
}

/// A text with a zone width is broken into lines to fit it, in the face it is drawn in (ffmpeg's text filter never wraps, so the app does: it
/// measures the font file). The values of the item are not changed, only what is handed to ffmpeg. The zone is a fraction of the picture's width
/// and the letters' size a percent of it, so the line length in em (the font's height unit) is the one over the other.
fn zoned(cat: &Catalog, key: &str, mut values: Values, ctx: &Ctx) -> Values {
    let Some(widget) = cat.effects.get(key).and_then(|e| e.widget.as_ref()).filter(|w| w.kind == TEXTS) else { return values };
    let id = |input: &str| widget.controls.get(input).cloned().unwrap_or_default();
    let (Some(V::Num(width)), Some(V::Num(size)), Some(V::Str(text))) = (values.get(&id("width")).cloned(), values.get(&id("size")).cloned(), values.get(&id("text")).cloned()) else { return values };
    if width <= 0.0 || size <= 0.0 {
        return values;
    }
    let face = match values.get(&id("style")).map(V::js_string).unwrap_or_default().as_str() {
        "bold" => "font_bold",
        "italic" => "font_italic",
        "bolditalic" => "font_bolditalic",
        _ => "font",
    };
    let file = ctx.get(face).cloned().unwrap_or_default();
    values.insert(id("text"), V::Str(crate::fonts::wrap(&file, &text, width * 100.0 / size)));
    values
}

/// Work out the filtering for a file: the extra inputs and the filters, or why an effect cannot be written.
pub fn try_plan(look: &Look, input: &Path) -> Result<Plan, String> {
    let effects = look.effects.clone().normalized();
    let cap = look.size.clone().normalized().scale_filter();
    if effects.items.is_empty() {
        let c = fx::compose(&[], cap.as_deref(), &[]);
        return Ok(Plan { inputs: c.inputs, filter: c.video, audio: c.audio, burns: false });
    }
    let cat = catalog().ok_or_else(|| "the effects are not installed yet: they come with ffmpeg".to_owned())?;
    if let Some(gone) = effects.items.iter().find(|i| i.effect().is_none()) {
        return Err(format!("{}: not available with this ffmpeg", gone.title()));
    }
    let ctx: Ctx = [
        ("input", input.to_string_lossy().into_owned()),
        ("font", crate::fonts::face_file("", false, false)),
        ("font_bold", crate::fonts::face_file("", true, false)),
        ("font_italic", crate::fonts::face_file("", false, true)),
        ("font_bolditalic", crate::fonts::face_file("", true, true)),
        ("name", input.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default()),
        ("duration", format!("{}", look.duration_s)),
    ]
    .into_iter()
    .map(|(k, v)| (k.to_owned(), v))
    .collect();
    let ready = effects.ready();
    let order = cat.order(&ready.iter().map(|i| i.key.clone()).collect::<Vec<_>>());
    // (the same effect may be listed twice, each with its own values: take the first item of that key not yet used)
    let mut before: Vec<Built> = Vec::new();
    let mut after: Vec<Built> = Vec::new();
    let mut used = vec![false; ready.len()];
    for (group, keys) in [(&mut before, &order.before), (&mut after, &order.after)] {
        for key in keys {
            let at = ready.iter().enumerate().position(|(n, i)| &i.key == key && !used[n]).ok_or_else(|| format!("{key}: not in the entry"))?;
            used[at] = true;
            let item = ready[at];
            if item.is_measured() {
                // measured with the file (the sound plan): it makes no filter of its own here, but an op this version does not know is a failure
                if item.op().is_none() {
                    return Err(format!("{}: this version of the app cannot do it", item.title()));
                }
                continue;
            }
            let effect = item.effect().ok_or_else(|| format!("{key}: not in the catalogue"))?;
            let values = item.all_values();
            let here = item_ctx(&ctx, &values);
            let values = zoned(&cat, &item.key, values, &here);
            group.push(fx::build(&effect, &cat.filters, &values, &here).map_err(|e| format!("{}: {e}", item.title()))?);
        }
    }
    let c = fx::compose(&before, cap.as_deref(), &after);
    Ok(Plan { inputs: c.inputs, filter: c.video, audio: c.audio, burns: effects.burns_subtitles() })
}

/// `try_plan`, or no filtering when an effect cannot be written (the job checks that first and fails with the reason).
pub fn plan(look: &Look, input: &Path) -> Plan {
    try_plan(look, input).unwrap_or_else(|_| Plan::none())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(items: Vec<Item>) -> EffectsEntry {
        EffectsEntry { items, ..EffectsEntry::new("t") }
    }

    #[test]
    fn the_users_favourites_replace_the_standard_level_except_for_what_the_app_is_wired_to() {
        let cat = catalog().expect("the fixture reads");
        let by_level = standard_keys_by(&cat, "windows", None);
        assert!(by_level.contains(&"try:fps".to_owned()) && by_level.contains(&"colour".to_owned()), "{by_level:?}");
        let chosen = vec!["try:vignette".to_owned()];
        let mine = standard_keys_by(&cat, "windows", Some(&chosen));
        assert!(mine.contains(&"try:vignette".to_owned()), "what he ticked is listed: {mine:?}");
        assert!(!mine.contains(&"try:fps".to_owned()), "what he did not tick is not: {mine:?}");
        assert!(mine.contains(&"colour".to_owned()), "an effect the app is wired to is listed whatever he chose: {mine:?}");
        let none: Vec<String> = Vec::new();
        assert!(!standard_keys_by(&cat, "windows", Some(&none)).is_empty(), "an empty choice still leaves the wired effects");
    }

    #[test]
    fn the_standard_effects_come_from_the_catalogue_in_the_fixed_order() {
        let cat = catalog().expect("the fixture reads");
        let keys = standard_keys(&cat, "windows");
        assert!(keys.len() >= 15, "{keys:?}");
        assert!(!keys.iter().any(|k| ["try:eq", "try:exposure", "try:colortemperature", "try:colorbalance", "try:vibrance"].contains(&k.as_str())), "one Colour effect took their place: {keys:?}");
        // the fixed order, not the alphabet: turn before denoise before light before text before frame rate
        let at = |k: &str| keys.iter().position(|x| x == k).unwrap_or_else(|| panic!("{k} is not standard: {keys:?}"));
        assert!(at("crop-edges") < at("blurfit"), "the crop works on the original picture: {keys:?}");
        assert!(at("blurfit") < at("try:hqdn3d") && at("try:hqdn3d") < at("colour") && at("colour") < at("try:lut3d") && at("try:lut3d") < at("text") && at("text") < at("try:fps"), "{keys:?}");
        // something rarely wanted is not on screen
        assert!(!keys.iter().any(|k| k == "try:pixelize" || k == "try:negate"), "{keys:?}");
    }

    #[test]
    fn crop_is_a_widget_that_is_only_in_the_list_while_it_changes_something() {
        test_fixture();
        let cat = catalog().expect("the fixture reads");
        let w = widgets(&cat, "windows");
        let crop = w.iter().find(|w| w.key == "crop-edges").expect("crop has a widget");
        assert_eq!(crop.kind, "frame");
        assert_eq!(crop.controls.get("left").map(String::as_str), Some("left"));

        let mut e = EffectsEntry::new("t");
        let mut v = Item::new("crop-edges").all_values();
        v.insert("left".to_owned(), V::Num(40.0));
        e.set_widget("crop-edges", Some(&v));
        assert_eq!(e.items.len(), 1, "a changed widget joins the list");
        let look = Look { size: &Size::original(), effects: &e, duration_s: 1.0, trim: Trim::none() };
        assert!(matches!(try_plan(&look, Path::new("a.mkv")).unwrap().filter, Filter::Simple(f) if f.contains("crop")), "and is in the command");

        v.insert("left".to_owned(), V::Num(0.0));
        e.set_widget("crop-edges", Some(&v));
        assert!(e.items.is_empty(), "back at its defaults it leaves the list");
    }

    #[test]
    fn the_crop_and_the_rotation_are_one_frame_widget_that_turns_before_it_cuts() {
        test_fixture();
        let cat = catalog().expect("the fixture reads");
        let w = widgets(&cat, "windows");
        assert_eq!(w.iter().map(|x| x.key.as_str()).collect::<Vec<_>>(), ["crop-edges", "colour", "curves", "text", "watermark", "picture-fade"], "the frame, the grading panel, the curves, the texts, the watermarks, the fade: {w:?}");
        let frame = &w[0];
        assert_eq!((frame.key.as_str(), frame.kind.as_str(), frame.controls.get("tilt").map(String::as_str)), ("crop-edges", "frame", Some("tilt")));

        let mut e = EffectsEntry::new("t");
        let mut v = Item::new("crop-edges").all_values();
        v.insert("tilt".to_owned(), V::Num(-10.0));
        e.set_widget("crop-edges", Some(&v));
        assert_eq!(e.items.len(), 1, "a tilt alone joins the list");
        let look = Look { size: &Size::original(), effects: &e, duration_s: 1.0, trim: Trim::none() };
        let Filter::Simple(chain) = try_plan(&look, Path::new("a.mkv")).unwrap().filter else { panic!() };
        assert!(chain.starts_with("rotate=angle=-10*PI/180,crop="), "{chain}");

        v.insert("tilt".to_owned(), V::Num(0.0));
        e.set_widget("crop-edges", Some(&v));
        assert!(e.items.is_empty(), "no tilt and no margins leaves the list");
    }

    #[test]
    fn the_colour_effect_is_a_grade_widget_that_does_nothing_until_a_control_moves() {
        test_fixture();
        let cat = catalog().expect("the fixture reads");
        let grade = widgets(&cat, "windows").into_iter().find(|x| x.key == "colour").expect("colour is a widget");
        assert_eq!(grade.kind, "grade");
        let mut e = EffectsEntry::new("t");
        let mut v = Item::new("colour").all_values();
        e.set_widget("colour", Some(&v));
        assert!(e.items.is_empty(), "every control at its neutral value is no effect");

        v.insert("temp".to_owned(), V::Num(20.0));
        v.insert("gain_r".to_owned(), V::Num(1.2));
        v.insert("saturation".to_owned(), V::Num(1.5));
        e.set_widget("colour", Some(&v));
        assert_eq!(e.items.len(), 1);
        let look = Look { size: &Size::original(), effects: &e, duration_s: 1.0, trim: Trim::none() };
        let Filter::Simple(chain) = try_plan(&look, Path::new("a.mkv")).unwrap().filter else { panic!() };
        assert!(chain.starts_with("colortemperature=") && chain.contains(",lutrgb=") && chain.contains(",hue=h=0:s=1.5"), "{chain}");
        assert!(!chain.contains("vibrance"), "the boost is at rest: {chain}");

        v.insert("temp".to_owned(), V::Num(0.0));
        v.insert("gain_r".to_owned(), V::Num(1.0));
        e.set_widget("colour", Some(&v));
        let look = Look { size: &Size::original(), effects: &e, duration_s: 1.0, trim: Trim::none() };
        let Filter::Simple(chain) = try_plan(&look, Path::new("a.mkv")).unwrap().filter else { panic!() };
        assert_eq!(chain, "hue=h=0:s=1.5", "only the saturation is left");
    }

    #[test]
    fn the_curves_effect_is_a_widget_that_writes_only_the_curves_that_have_points() {
        test_fixture();
        let cat = catalog().expect("the fixture reads");
        let w = widgets(&cat, "windows").into_iter().find(|x| x.key == "curves").expect("curves is a widget");
        assert_eq!(w.kind, "curves");
        let mut e = EffectsEntry::new("t");
        let mut v = Item::new("curves").all_values();
        e.set_widget("curves", Some(&v));
        assert!(e.items.is_empty(), "no points anywhere is no effect");
        v.insert("red".to_owned(), V::Str("0/0 0.5/0.6 1/1".to_owned()));
        e.set_widget("curves", Some(&v));
        let look = Look { size: &Size::original(), effects: &e, duration_s: 1.0, trim: Trim::none() };
        let Filter::Simple(chain) = try_plan(&look, Path::new("a.mkv")).unwrap().filter else { panic!() };
        assert!(chain.starts_with("curves=") && chain.contains("0/0 0.5/0.6 1/1") && !chain.contains("master") && !chain.contains("green"), "{chain}");
    }

    #[test]
    fn the_fade_is_two_lengths_from_the_ends_of_the_kept_part_and_runs_last() {
        test_fixture();
        let cat = catalog().expect("the fixture reads");
        let w = widgets(&cat, "windows").into_iter().find(|x| x.key == "picture-fade").expect("fade is a widget");
        assert_eq!((w.kind.as_str(), w.controls.get("in").map(String::as_str), w.controls.get("out").map(String::as_str)), ("fades", Some("fade_in"), Some("fade_out")));
        assert!(!standard_keys(&cat, "windows").contains(&"try:fade".to_owned()), "the raw filter leaves the menu: the effect covers it");
        let mut e = EffectsEntry::new("t");
        let mut v = Item::new("picture-fade").all_values();
        e.set_widget("picture-fade", Some(&v));
        assert!(e.items.is_empty(), "no length at either end is no effect");
        // 10 s kept: 1 s in, 2.5 s out starting at 7.5 s
        v.insert("fade_in".to_owned(), V::Num(1.0));
        v.insert("fade_out".to_owned(), V::Num(2.5));
        e.set_widget("picture-fade", Some(&v));
        e.items.push(Item::new("play-backwards"));
        let e = e.arranged(&standard_keys(&cat, "windows"));
        let look = Look { size: &Size::original(), effects: &e, duration_s: 10.0, trim: Trim::none() };
        let Filter::Simple(chain) = try_plan(&look, Path::new("a.mkv")).unwrap().filter else { panic!() };
        let at = |part: &str| chain.find(part).unwrap_or_else(|| panic!("{part} in {chain}"));
        assert!(at("reverse") < at("fade=type=in") && at("fade=type=in") < at("fade=type=out"), "{chain}");
        assert!(chain.contains("fade=type=in:start_time=0:duration=1:") && chain.contains("fade=type=out:start_time=7.5:duration=2.5:"), "{chain}");
        // only the end that has a length is written; a length longer than the part is cut down to it
        v.insert("fade_in".to_owned(), V::Num(0.0));
        v.insert("fade_out".to_owned(), V::Num(60.0));
        let mut e = EffectsEntry::new("t");
        e.set_widget("picture-fade", Some(&v));
        let look = Look { size: &Size::original(), effects: &e, duration_s: 10.0, trim: Trim::none() };
        let Filter::Simple(chain) = try_plan(&look, Path::new("a.mkv")).unwrap().filter else { panic!() };
        assert!(!chain.contains("type=in") && chain.contains("fade=type=out:start_time=0:duration=10:"), "{chain}");
    }

    #[test]
    fn a_working_list_keeps_the_standard_effects_first_and_the_added_ones_after() {
        let standard = vec!["turn".to_owned(), "try:eq".to_owned(), "text".to_owned()];
        let list = entry(vec![Item::new("try:negate"), Item::new("text"), Item::new("turn"), Item::new("try:eq"), Item::new("try:eq")]).arranged(&standard);
        let keys: Vec<&str> = list.items.iter().map(|i| i.key.as_str()).collect();
        assert_eq!(keys, ["turn", "try:eq", "text", "try:negate"]);
    }

    fn plan_of(effects: &EffectsEntry, size: &Size) -> Plan {
        let look = Look { size, effects, duration_s: 100.0, trim: Trim::none() };
        try_plan(&look, Path::new("C:/Videos/my clip.mkv")).unwrap()
    }

    #[test]
    fn the_seeded_entries_turn_and_flip_and_nothing_else() {
        let size = Size::original();
        for e in defaults() {
            let p = plan_of(&e, &size);
            assert!(matches!(p.filter, Filter::Simple(_)), "{}: {:?}", e.name, p.filter);
            assert!(p.inputs.is_empty() && p.audio.is_none() && !p.burns);
        }
        let Filter::Simple(chain) = plan_of(&defaults()[2], &size).filter else { panic!() };
        assert_eq!(chain, "transpose=dir=clock,vflip", "a turn, then the flip on the turned picture");
    }

    #[test]
    fn nothing_listed_is_nothing_done() {
        let p = plan_of(&EffectsEntry::none(), &Size::original());
        assert_eq!(p, Plan::none());
    }

    #[test]
    fn the_size_cap_sits_between_what_wants_the_delivered_size_and_the_rest() {
        let size = Size { max_width: Some(1280), max_height: Some(720), ..Size::original() };
        let text = Item::with("text", &[("text", "hi")]);
        let turn = Item::with("turn", &[("way", "cw")]);
        // listed text first: it still runs after the cap, the turn before it
        let Filter::Simple(chain) = plan_of(&entry(vec![text, turn]), &size).filter else { panic!() };
        let at = |n: &str| chain.find(n).unwrap_or_else(|| panic!("{n} in {chain}"));
        assert!(at("transpose") < at("scale") && at("scale") < at("drawtext"), "{chain}");
    }

    #[test]
    fn a_picture_laid_over_makes_a_graph_with_its_second_input() {
        let wm = Item::with("watermark", &[("file", "C:/logo.png")]);
        let p = plan_of(&entry(vec![wm]), &Size::original());
        assert_eq!(p.inputs, ["C:/logo.png"]);
        let Filter::Complex(g) = p.filter else { panic!() };
        assert!(g.starts_with("[1:v]") || g.contains("[1:v]"), "{g}");
        assert!(g.ends_with("[outv]"), "{g}");
    }

    #[test]
    fn an_effect_with_a_mandatory_value_missing_is_left_out() {
        let e = entry(vec![Item::new("watermark"), Item::with("text", &[("text", "  ")])]);
        assert_eq!(plan_of(&e, &Size::original()), Plan::none());
        assert!(e.missing_file().is_none());
    }

    #[test]
    fn a_picture_that_is_not_there_is_named() {
        let e = entry(vec![Item::with("watermark", &[("file", "D:/nowhere/logo.png")])]);
        assert!(e.missing_file().unwrap().contains("nowhere"));
    }

    #[test]
    fn the_same_effect_listed_twice_keeps_each_ones_values() {
        let e = entry(vec![Item::with("text", &[("text", "one")]), Item::with("text", &[("text", "two")])]);
        let Filter::Simple(chain) = plan_of(&e, &Size::original()).filter else { panic!() };
        assert!(chain.contains("one") && chain.contains("two"), "{chain}");
    }

    #[test]
    fn several_texts_keep_their_place_and_each_is_added_changed_and_removed_by_its_number() {
        let standard = vec!["try:eq".to_owned(), "text".to_owned(), "picture-fade".to_owned()];
        let mut e = entry(vec![Item::new("picture-fade"), Item::with("text", &[("text", "one")]), Item::new("try:eq")]);
        let mut values = Item::with("text", &[("text", "two"), ("px", "0.5")]).all_values();
        e.add_item("text", &values);
        let list = e.clone().arranged(&standard);
        let keys: Vec<&str> = list.items.iter().map(|i| i.key.as_str()).collect();
        assert_eq!(keys, ["try:eq", "text", "text", "picture-fade"], "both texts stay, together, at the text's place");
        values.insert("text".to_owned(), V::Str("second".to_owned()));
        e.set_nth("text", 1, &values);
        let texts: Vec<String> = e.items.iter().filter(|i| i.key == "text").map(|i| i.all_values().get("text").map(V::js_string).unwrap_or_default()).collect();
        assert_eq!(texts, ["one", "second"]);
        e.remove_nth("text", 0);
        assert_eq!(e.items.iter().filter(|i| i.key == "text").count(), 1);
        assert!(repeats("text") && repeats("watermark") && !repeats("colour"));
    }

    #[test]
    fn two_watermarks_are_placed_by_fractions_of_the_picture_and_each_is_its_own_input() {
        test_fixture();
        let a = Item::with("watermark", &[("file", "C:/a.png"), ("px", "0.1"), ("py", "0.2"), ("pw", "0.3")]);
        let b = Item::with("watermark", &[("file", "C:/b.png")]);
        let e = entry(vec![a, b]);
        let look = Look { size: &Size::original(), effects: &e, duration_s: 1.0, trim: Trim::none() };
        let plan = try_plan(&look, Path::new("a.mkv")).unwrap();
        assert_eq!(plan.inputs, ["C:/a.png", "C:/b.png"]);
        let Filter::Complex(g) = plan.filter else { panic!("{:?}", plan.filter) };
        assert!(g.contains("rw*0.3") && g.contains("overlay=x=main_w*0.1:y=main_h*0.2"), "{g}");
        assert!(g.contains("[1:v]") && g.contains("[2:v]"), "{g}");
    }

    #[test]
    fn a_text_is_drawn_where_it_was_put_in_the_style_it_was_given() {
        let place = Item::with("text", &[("text", "hi"), ("px", "0.25"), ("py", "0.5"), ("style", "bold")]);
        let Filter::Simple(chain) = plan_of(&entry(vec![place]), &Size::original()).filter else { panic!() };
        assert!(chain.contains("x=w*0.25") && chain.contains("y=h*0.5"), "{chain}");
        assert!(chain.contains("arialbd"), "the bold face: {chain}");
    }

    #[test]
    fn a_sound_effect_is_in_the_sounds_list_and_reaches_the_audio_chain() {
        let cat = catalog().expect("the fixture reads");
        let sound: Vec<String> = cat.picker_for("windows", "audio", true).into_iter().map(|e| e.key).collect();
        let picture: Vec<String> = cat.picker("windows", WORKS).into_iter().map(|e| e.key).collect();
        assert!(sound.contains(&"try:acontrast".to_owned()) && !picture.contains(&"try:acontrast".to_owned()));
        assert!(is_sound("try:acontrast") && !is_sound("flip") && !is_sound("play-backwards"));
        // what acts on both stays with the picture
        assert!(!sound.contains(&"play-backwards".to_owned()) && picture.contains(&"play-backwards".to_owned()));
        // nothing is left out: the measured effects and the ones that change the time are offered like the rest (the picture follows them)
        for key in ["try:atempo", "try:atrim", "try:apad", "try:aloop", "try:asetrate", "try:rubberband", "try:silenceremove", "volume-loudness", "trim-silence", "silence-end", "keep-range"] {
            assert!(sound.contains(&key.to_owned()) || SOUND_STANDARD_ORDER.contains(&key), "{key} is not offered");
        }
        assert!(sound.contains(&"equalizer".to_owned()) && sound.contains(&"compressor".to_owned()));
        let list = entry(vec![Item::new("flip"), Item::new("try:acontrast")]);
        assert!(list.needs_audio_filter());
        let p = plan_of(&list, &Size::original());
        assert!(p.audio.as_deref().is_some_and(|a| a.contains("acontrast")), "{:?}", p.audio);
    }

    #[test]
    fn the_standard_sound_effects_are_the_same_as_audio_converters() {
        let cat = catalog().expect("the fixture reads");
        // the sound's drawers and strip: the same as FFmpeg Audio Studio's (the Level drawer stands for the four ways to set the volume)
        let keys: Vec<String> = sound_widgets(&cat, "windows").iter().map(|w| w.key.clone()).collect();
        assert_eq!(keys, ["channels", "equalizer", "compressor", "volume-loudness", "volume-peak", "volume-replaygain", "volume-gain", "sound-fade"]);
        let kinds: Vec<String> = sound_widgets(&cat, "windows").iter().map(|w| w.kind.clone()).collect();
        assert_eq!(kinds, ["channels", "tone", "compressor", "level", "level", "level", "level", "fades"]);
        // and the rest are rows with a switch
        let listed = sound_standard_keys(&cat, "windows");
        assert!(SOUND_STANDARD_ORDER.iter().all(|k| listed.contains(&(*k).to_owned())), "{listed:?}");
        assert_eq!(listed[..SOUND_STANDARD_ORDER.len()], *SOUND_STANDARD_ORDER, "the wired ones come first, in their order");
        // the rest is what the catalogue marks standard, as in FFmpeg Audio Studio, and a favourites choice replaces it
        let by_level = cat.picker_for("windows", "audio", true).into_iter().filter(|e| e.level == "standard").count();
        assert_eq!(listed.len(), by_level);
        let mine = sound_standard_keys_by(&cat, "windows", Some(&["try:flanger".to_owned()]));
        assert_eq!(mine.len(), SOUND_STANDARD_ORDER.len() + 1);
        assert_eq!(SOUND_STANDARD_ID, "default-effects");
        assert!(EffectsEntry::standard_sound("No effects").items.is_empty());
    }

    #[test]
    fn the_burn_and_the_sound_are_known_to_the_plan() {
        let p = plan_of(&entry(vec![Item::new("burn-subtitles"), Item::new("play-backwards")]), &Size::original());
        assert!(p.burns);
        assert_eq!(p.audio.as_deref(), Some("areverse"));
        assert!(entry(vec![Item::new("play-backwards")]).needs_audio_filter());
    }

    #[test]
    fn the_order_of_the_effects_is_judged() {
        let bad = entry(vec![Item::with("text", &[("text", "hi")]), Item::new("play-backwards")]);
        assert_eq!(bad.order_problems().len(), 1);
        assert!(entry(vec![Item::new("play-backwards"), Item::with("text", &[("text", "hi")])]).order_problems().is_empty());
    }

    #[test]
    fn an_item_naming_no_effect_is_dropped_one_the_catalogue_lacks_is_kept_and_a_pasted_path_is_cleaned() {
        let e: EffectsEntry = serde_json::from_str(
            r#"{ "id": "a", "name": "A", "items": [ { "kind": "rotate" }, { "key": "gone" }, { "key": "watermark", "values": { "file": " \"D:\\logo.png\" " } } ] }"#,
        )
        .unwrap();
        let e = e.normalized();
        assert_eq!(e.items.len(), 2, "the item of the old format is dropped, the unknown effect stays");
        assert_eq!(e.items[1].values["file"], "D:\\logo.png");
        let look = Look { size: &Size::original(), effects: &e, duration_s: 1.0, trim: Trim::none() };
        assert!(try_plan(&look, Path::new("a.mkv")).unwrap_err().contains("not available with this ffmpeg"));
    }

    #[test]
    fn values_survive_a_round_trip_through_the_settings_file() {
        let mut item = Item::new("crop-edges");
        let mut v = item.all_values();
        v.insert("left".into(), V::Num(20.0));
        item.set_values(&v);
        let back: Item = serde_json::from_str(&serde_json::to_string(&item).unwrap()).unwrap();
        assert_eq!(back.all_values().get("left"), Some(&V::Num(20.0)));
    }
}
