//! The curves drawer of the work view: one tone curve for the whole picture (Y) and one each for red, green and blue, drawn on a graph as in a
//! grading panel. The graph and its channel buttons are the catalog's `UisCurve` (click to add a point, drag to bend, double-click to take one
//! away); this file keeps what is the converter's own: the presets and what the four controls hold. It takes the place of the file list while
//! it is open (one drawer at a time), like the colour drawer.
//! The curves are the `curves` effect's four controls, each a list of points `x/y` from 0 to 1 joined by spaces (an empty one is a straight line
//! from the bottom left to the top right); the line drawn is the same natural cubic spline ffmpeg's filter makes.

use crate::{
    catalog::{CurveChannel, CurvePoints, UisCurve, straight},
    i18n,
    preview_session::WidgetEdit,
    work_colour::use_throttled,
};
use dioxus::prelude::*;
use dioxus_free_icons::{
    Icon,
    icons::ld_icons::{LdRotateCcw},
};
use engine::{
    effects::{self, Item},
    ffmpeg_effects::{V, Values},
};
use std::collections::HashMap;

/// The catalogue key of the effect this drawer edits.
pub const KEY: &str = "curves";

/// The four curves: the control's id, the letter on its button and its colour.
const CHANNELS: [(&str, &str, &str); 4] = [("master", "Y", "var(--uis-plot-line)"), ("red", "R", "#d4483f"), ("green", "G", "#3fb560"), ("blue", "B", "#4a64e0")];

/// The presets of ffmpeg's `curves` filter, as the points of the four curves (master, red, green, blue; an empty one is straight). Their names
/// are the catalogue's, in English like every title it holds. The points reproduce the filter's own presets (checked against it on a ramp: the
/// same curves to one level in 255, except `Colour negative`, which is within about 5%).
const PRESETS: [(&str, &str, [&str; 4]); 10] = [
    ("color_negative", "Colour negative", ["", "0/1 0.09/0.98 0.463/0.497 0.766/0 1/0", "0/1 0.058/1 0.299/0.503 0.532/0.003 1/0", "0/1 0.062/1 0.232/0.507 0.43/0 1/0"]),
    ("cross_process", "Cross process", ["", "0/0 0.25/0.156 0.501/0.501 0.686/0.745 1/1", "0/0 0.25/0.188 0.38/0.501 0.745/0.815 1/0.815", "0/0 0.231/0.094 0.709/0.874 1/1"]),
    ("darker", "Darker", ["0/0 0.5/0.4 1/1", "", "", ""]),
    ("increase_contrast", "Increase contrast", ["0/0 0.149/0.066 0.831/0.905 0.905/0.98 1/1", "", "", ""]),
    ("lighter", "Lighter", ["0/0 0.4/0.5 1/1", "", "", ""]),
    ("linear_contrast", "Linear contrast", ["0/0 0.305/0.286 0.694/0.713 1/1", "", "", ""]),
    ("medium_contrast", "Medium contrast", ["0/0 0.286/0.219 0.639/0.643 1/1", "", "", ""]),
    ("negative", "Negative", ["0/1 1/0", "", "", ""]),
    ("strong_contrast", "Strong contrast", ["0/0 0.3/0.196 0.615/0.636 0.686/0.737 1/1", "", "", ""]),
    ("vintage", "Vintage", ["", "0/0.11 0.42/0.51 1/0.95", "0/0 0.5/0.48 1/1", "0/0.22 0.49/0.44 1/0.8"]),
];

/// What the four controls hold, as written (the straight line is the empty text).
fn written(values: &Values) -> [String; 4] {
    CHANNELS.map(|(id, ..)| write(&parse(&text_of(values, id))))
}

/// The preset the four curves are, if they are one.
fn preset_of(values: &Values) -> Option<&'static str> {
    let now = written(values);
    PRESETS.iter().find(|(_, _, p)| p.iter().zip(&now).all(|(a, b)| write(&parse(a)) == *b)).map(|(id, ..)| *id)
}

/// The points a control holds (`x/y` pairs); none that read give the straight line.
fn parse(text: &str) -> CurvePoints {
    let mut points: CurvePoints = text
        .split_whitespace()
        .filter_map(|pair| {
            let (x, y) = pair.split_once('/')?;
            Some((x.parse::<f64>().ok()?.clamp(0.0, 1.0), y.parse::<f64>().ok()?.clamp(0.0, 1.0)))
        })
        .collect();
    points.sort_by(|a, b| a.0.total_cmp(&b.0));
    points.dedup_by(|a, b| (a.0 - b.0).abs() < 1e-6);
    if points.len() < 2 { straight() } else { points }
}

/// What the control holds for these points: nothing for the straight line.
fn write(points: &CurvePoints) -> String {
    if *points == straight() {
        return String::new();
    }
    points.iter().map(|(x, y)| format!("{}/{}", trim(*x), trim(*y))).collect::<Vec<_>>().join(" ")
}

/// A number with at most four decimals and no trailing zeros.
fn trim(v: f64) -> String {
    let t = format!("{v:.4}");
    t.trim_end_matches('0').trim_end_matches('.').to_owned()
}

fn text_of(values: &Values, id: &str) -> String {
    match values.get(id) {
        Some(V::Str(s)) => s.clone(),
        _ => String::new(),
    }
}

/// The values with this curve set to these points.
fn with(values: &Values, id: &str, points: &CurvePoints) -> WidgetEdit {
    let mut v = values.clone();
    v.insert(id.to_owned(), V::Str(write(points)));
    WidgetEdit { key: KEY.to_owned(), values: Some(v), keep: false }
}

#[component]
pub fn CurvesDrawer(list: Signal<effects::EffectsEntry>, lang: String, on_edit: EventHandler<WidgetEdit>, #[props(default)] mixed: bool) -> Element {
    let (on_edit, values) = use_throttled(KEY, list, on_edit);
    let reset = i18n::t(&lang, "tipFxReset");
    let title = Item::new(KEY).title();
    let anything = list.read().items.iter().any(|i| i.key == KEY);
    let chosen = if written(&values).iter().all(String::is_empty) { "none" } else { preset_of(&values).unwrap_or("custom") };
    let s_preset = i18n::t(&lang, "lblMainPreset");
    let s_none = i18n::t(&lang, "lblFxNone");
    let preset_values = values.clone();

    let channels: Vec<CurveChannel> = CHANNELS.iter().map(|(id, label, color)| CurveChannel { id: (*id).to_owned(), label: (*label).to_owned(), color: (*color).to_owned() }).collect();
    let held: HashMap<String, CurvePoints> = CHANNELS.iter().map(|(id, ..)| ((*id).to_owned(), parse(&text_of(&values, id)))).collect();
    let edit_values = values.clone();

    rsx! {
        div { class: if mixed { "work-curves work-colour uis-cmp-drawer uis-cmp-panel uis-lvw-scroll wc-mixed" } else { "work-curves work-colour uis-cmp-drawer uis-cmp-panel uis-lvw-scroll" },
            div { class: "wc-head uis-cmp-head",
                span { class: "uis-fld-labelled-label", "{title}" }
                if mixed {
                    span { class: "wfx-mixed", "≠" }
                }
                span { class: "uis-fld-labelled-label wc-preset-label", "{s_preset}" }
                select {
                    class: "uis-ddn-std wc-preset",
                    title: "{s_preset}",
                    "aria-label": "{s_preset}",
                    value: "{chosen}",
                    onchange: move |e| {
                        let id = e.value();
                        let mut v = preset_values.clone();
                        let texts = PRESETS.iter().find(|(p, ..)| *p == id).map(|(.., t)| *t).unwrap_or(["", "", "", ""]);
                        for ((cid, ..), text) in CHANNELS.iter().zip(texts) {
                            v.insert((*cid).to_owned(), V::Str(write(&parse(text))));
                        }
                        on_edit.call(WidgetEdit { key: KEY.to_owned(), values: Some(v), keep: false });
                    },
                    if chosen == "custom" {
                        option { value: "custom", selected: true, disabled: true, "—" }
                    }
                    option { value: "none", selected: chosen == "none", "{s_none}" }
                    for (pid, name, _) in PRESETS {
                        option { key: "{pid}", value: "{pid}", selected: chosen == pid, "{name}" }
                    }
                }
                span { class: "uis-bar-spacer" }
                button {
                    class: "uis-btn ico",
                    title: "{reset}",
                    "aria-label": "{reset}",
                    disabled: !anything,
                    onclick: move |_| on_edit.call(WidgetEdit { key: KEY.to_owned(), values: None, keep: false }),
                    Icon { width: 14, height: 14, icon: LdRotateCcw }
                }
            }
            UisCurve {
                channels,
                value: held,
                reset_title: reset.clone(),
                width: 264.0,
                height: 194.0,
                on_change: move |(id, points): (String, CurvePoints)| on_edit.call(with(&edit_values, &id, &points)),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_points_of_a_control_read_back_and_the_straight_line_is_empty() {
        assert_eq!(parse(""), straight());
        assert_eq!(parse("0/0 1/1"), straight());
        assert_eq!(write(&straight()), "");
        let p = parse("1/1 0.5/0.6 0/0");
        assert_eq!(p, vec![(0.0, 0.0), (0.5, 0.6), (1.0, 1.0)], "sorted");
        assert_eq!(write(&p), "0/0 0.5/0.6 1/1");
        assert_eq!(parse("junk 0.3/x"), straight());
    }

    #[test]
    fn a_preset_is_recognised_by_its_curves_and_none_is_all_straight() {
        let mut v = Values::new();
        assert_eq!(preset_of(&v), None, "all straight is not a preset");
        for (cid, text) in CHANNELS.iter().map(|c| c.0).zip(PRESETS[2].2) {
            v.insert(cid.to_owned(), V::Str(write(&parse(text))));
        }
        assert_eq!(preset_of(&v), Some("darker"));
        v.insert("red".to_owned(), V::Str("0/0 0.5/0.7 1/1".to_owned()));
        assert_eq!(preset_of(&v), None, "an edited curve is custom");
        for (_, _, p) in PRESETS {
            assert!(p.iter().all(|t| t.is_empty() || parse(t).len() >= 2), "{p:?}");
        }
    }
}
