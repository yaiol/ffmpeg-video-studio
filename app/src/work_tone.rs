//! The tone drawer of the work view: the equaliser's ten bands drawn as one curve on a graph (frequency across, gain up and down), as in a
//! mixing desk - the catalog's `UisEqualizer` in its graph view. It takes the place of the file list while it is open. Nothing here knows
//! what a band is: the controls, their labels, ranges and neutral values come from the catalogue's `equalizer` effect, the edits go through
//! the same `WidgetEdit` as every drawer, and the effect is in the working list only while a band is off neutral.

use crate::{
    catalog::{EqView, UisEqualizer},
    i18n,
    preview_session::WidgetEdit,
};
use dioxus::prelude::*;
use dioxus_free_icons::{Icon, icons::ld_icons::LdRotateCcw};
use engine::{
    effects::{self, Item},
    ffmpeg_effects::{V, Values},
};
use std::time::{Duration, Instant};

/// The catalogue key of the effect this drawer edits.
pub const KEY: &str = "equalizer";

/// How often an edit is handed to the work list while a dot is dragged.
const GAP: Duration = Duration::from_millis(80);

/// One band of the equaliser: its control, its centre frequency and the gain range the catalogue allows.
#[derive(Clone, Debug, PartialEq)]
struct Band {
    id: String,
    label: String,
    hz: f64,
    min: f64,
    max: f64,
}

/// The centre frequency a band's control id names: `g62` is 62 Hz, `g1k` is 1000 Hz.
fn hz_of(id: &str) -> Option<f64> {
    let rest = id.strip_prefix('g')?;
    match rest.strip_suffix('k') {
        Some(k) => k.parse::<f64>().ok().map(|v| v * 1000.0),
        None => rest.parse::<f64>().ok(),
    }
}

/// The bands the catalogue's equaliser has, lowest first.
fn bands() -> Vec<Band> {
    let Some(effect) = effects::catalog().and_then(|c| c.effects.get(KEY).cloned()) else { return Vec::new() };
    let mut out: Vec<Band> = effect
        .controls
        .iter()
        .filter_map(|c| {
            Some(Band { id: c.id.clone(), label: c.label.clone().unwrap_or_else(|| c.id.clone()), hz: hz_of(&c.id)?, min: c.min.unwrap_or(-12.0), max: c.max.unwrap_or(12.0) })
        })
        .collect();
    out.sort_by(|a, b| a.hz.total_cmp(&b.hz));
    out
}

fn gain_of(values: &Values, band: &Band) -> f64 {
    match values.get(&band.id) {
        Some(V::Num(n)) => *n,
        _ => 0.0,
    }
}

/// The controls of an effect as the catalogue words them (label, unit, limits, choices), in its own language.
pub fn controls_of(key: &str) -> Vec<engine::ffmpeg_effects::Control> {
    let Some(cat) = effects::catalog() else { return Vec::new() };
    let Some(effect) = Item::new(key).effect() else { return Vec::new() };
    engine::ffmpeg_effects::resolve_controls(&effect, &cat.filters).unwrap_or_default()
}

/// A number the values hold (0 when there is none).
pub fn num(values: &Values, id: &str) -> f64 {
    match values.get(id) {
        Some(V::Num(n)) => *n,
        _ => 0.0,
    }
}

/// What a drawer edits with: a drag makes an edit at every pointer move, so the drawer shows its own latest values at once and hands them to the
/// work list (which re-renders the whole view and prepares the sound again) at most every `GAP`, the last one always (a short pause after it).
/// Returns the handler to edit through and the values to show (the drawer's own while they are fresh, else the list's).
pub fn use_throttled(key: &str, list: Signal<effects::EffectsEntry>, commit: EventHandler<WidgetEdit>) -> (EventHandler<WidgetEdit>, Values) {
    let mut local = use_signal(|| None::<(Values, Instant)>);
    let mut sent = use_signal(Instant::now);
    let mut waiting = use_signal(|| None::<WidgetEdit>);
    let mut timer = use_signal(|| false);
    let on_edit = EventHandler::new(move |e: WidgetEdit| {
        let Some(v) = e.values.clone() else {
            local.set(None);
            waiting.set(None);
            commit.call(e);
            return;
        };
        local.set(Some((v, Instant::now())));
        if sent.peek().elapsed() >= GAP {
            sent.set(Instant::now());
            waiting.set(None);
            commit.call(e);
            return;
        }
        waiting.set(Some(e));
        if !*timer.peek() {
            timer.set(true);
            spawn(async move {
                tokio::time::sleep(GAP).await;
                timer.set(false);
                let last = waiting.write().take();
                if let Some(e) = last {
                    sent.set(Instant::now());
                    commit.call(e);
                }
            });
        }
    });
    let values = match local() {
        Some((v, at)) if at.elapsed() < Duration::from_millis(400) => v,
        _ => list.read().items.iter().find(|i| i.key == key).map_or_else(|| Item::new(key).all_values(), Item::all_values),
    };
    (on_edit, values)
}

#[component]
pub fn ToneDrawer(list: Signal<effects::EffectsEntry>, lang: String, on_edit: EventHandler<WidgetEdit>, #[props(default)] mixed: bool) -> Element {
    let (on_edit, values) = use_throttled(KEY, list, on_edit);
    let bands = bands();
    let reset = i18n::t(&lang, "tipFxReset");
    let title = Item::new(KEY).title();
    let anything = list.read().items.iter().any(|i| i.key == KEY);
    let (min, max) = bands.first().map_or((-12.0, 12.0), |b| (b.min, b.max));

    let labels: Vec<String> = bands.iter().map(|b| b.label.clone()).collect();
    let freqs: Vec<f64> = bands.iter().map(|b| b.hz).collect();
    let gains: Vec<f64> = bands.iter().map(|b| gain_of(&values, b)).collect();
    let (reset_values, reset_bands) = (values.clone(), bands.clone());
    let (held_values, held_bands) = (values.clone(), bands.clone());

    rsx! {
        div { class: if mixed { "work-tone work-colour uis-cmp-drawer uis-cmp-panel uis-lvw-scroll wc-mixed" } else { "work-tone work-colour uis-cmp-drawer uis-cmp-panel uis-lvw-scroll" },
            div { class: "wc-head uis-cmp-head",
                span { class: "uis-fld-labelled-label", "{title}" }
                if mixed {
                    span { class: "wfx-mixed", "≠" }
                }
                span { class: "uis-bar-spacer" }
                button {
                    class: "uis-btn ico",
                    title: "{reset}",
                    "aria-label": "{reset}",
                    disabled: !anything,
                    onclick: move |_| {
                        let mut v = reset_values.clone();
                        for b in &reset_bands {
                            v.insert(b.id.clone(), V::Num(0.0));
                        }
                        on_edit.call(WidgetEdit { key: KEY.to_owned(), values: Some(v), keep: false });
                    },
                    Icon { width: 14, height: 14, icon: LdRotateCcw }
                }
            }
            div { class: "wc-curve-body",
                UisEqualizer {
                    view: EqView::Graph,
                    width: 420.0,
                    height: 178.0,
                    labels,
                    values: gains,
                    freqs,
                    min,
                    max,
                    on_change: move |(i, gain): (usize, f64)| {
                        let Some(band) = held_bands.get(i) else { return };
                        let mut v = held_values.clone();
                        v.insert(band.id.clone(), V::Num(gain));
                        on_edit.call(WidgetEdit { key: KEY.to_owned(), values: Some(v), keep: false });
                    },
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_band_names_its_frequency_in_its_id() {
        assert_eq!(hz_of("g31"), Some(31.0));
        assert_eq!(hz_of("g1k"), Some(1000.0));
        assert_eq!(hz_of("g16k"), Some(16_000.0));
        assert_eq!(hz_of("threshold"), None);
    }
}
