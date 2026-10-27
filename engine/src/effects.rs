//! Effects: what is done to the picture and the sound besides its size. A named entry the user keeps in Settings and picks in the
//! window, apart from the preset, holding a list of effects; the same entry goes with any codec.
//!
//! The effects themselves are the catalogue installed with ffmpeg (`effects.json` in the pair's folder, see `download`; a form per effect,
//! read by `ffmpeg_effects`, the module shared with the Audio Converter). No copy of it is embedded: without it no effect can be built; an entry keeps, per effect, its key and the values of its controls. The size cap is not an effect (it is a
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

/// Load the catalogue of the pair installed in `dir`, or none when there is no pair there (or it cannot be read). Called at launch and
/// after every install.
pub fn load_installed(dir: Option<&Path>) -> Result<(), String> {
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

/// The tests' catalogue: a copy of the one the shared folder makes (never part of a build).
#[cfg(test)]
fn test_fixture() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| set_catalog_text(include_str!("../testdata/effects.json")).expect("the fixture reads"));
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

    /// The sound's chain some effect needs, which means the sound cannot be copied (playing backwards).
    pub fn needs_audio_filter(&self) -> bool {
        self.ready().iter().filter_map(|i| i.effect()).any(|e| e.audio.as_ref().is_some_and(|a| !a.is_empty()))
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
fn default_font() -> String {
    let p = Path::new("C:/Windows/Fonts/arial.ttf");
    if cfg!(windows) && p.is_file() { p.to_string_lossy().into_owned() } else { String::new() }
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
        ("font", default_font()),
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
            let effect = item.effect().ok_or_else(|| format!("{key}: not in the catalogue"))?;
            group.push(fx::build(&effect, &cat.filters, &item.all_values(), &ctx).map_err(|e| format!("{}: {e}", item.title()))?);
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
