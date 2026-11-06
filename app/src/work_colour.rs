//! The colour drawer of the work view: the `colour` effect's controls as a grading panel (lift, gamma, gain and offset with a master and a
//! red, green and blue bar each, then temperature, tint, contrast around a pivot, colour boost, saturation and hue), in the place of the file
//! list while it is open. It takes no more room than the panel it is copied from (the owner's limit, 2026-10-29) and is a little more compact.
//! Nothing here knows what a control does: the labels, ranges, steps and neutral values come from the catalogue, the edits go through the same
//! `WidgetEdit` as the frame widget, and the effect is in the working list only while a control is off neutral.

use crate::{
    catalog::{UisFldLabelled, UisNum, UisYrgb, YrgbBar, YrgbView},
    i18n,
    preview_session::WidgetEdit,
};
use dioxus::prelude::*;
use dioxus_free_icons::{
    Icon,
    icons::ld_icons::{LdCircleDot, LdPipette, LdRotateCcw, LdSlidersVertical},
};
use engine::{
    effects::{self, Item},
    ffmpeg_effects::{self, V, Values},
};
use std::{
    collections::HashMap,
    time::{Duration, Instant},
};

/// The longest the work list waits for an edit of the drawer while a drag goes on.
const GAP: Duration = Duration::from_millis(80);

/// The catalogue key of the effect this drawer edits.
pub const KEY: &str = "colour";

/// A control as the bars need it.
#[derive(Clone, Debug, PartialEq)]
struct Bar {
    id: String,
    label: String,
    min: f64,
    max: f64,
    step: f64,
    neutral: f64,
}

/// The effect's controls by id, from the catalogue (empty when it has none).
fn bars() -> HashMap<String, Bar> {
    let Some(cat) = effects::catalog() else { return HashMap::new() };
    let Some(effect) = cat.form(KEY) else { return HashMap::new() };
    ffmpeg_effects::resolve_controls(&effect, &cat.filters)
        .unwrap_or_default()
        .into_iter()
        .map(|c| {
            let neutral = match c.default {
                Some(V::Num(n)) => n,
                _ => 0.0,
            };
            let bar = Bar { id: c.id.clone(), label: c.label, min: c.min.unwrap_or(0.0), max: c.max.unwrap_or(1.0), step: c.step.unwrap_or(0.01), neutral };
            (c.id, bar)
        })
        .collect()
}

fn number(values: &Values, bar: &Bar) -> f64 {
    match values.get(&bar.id) {
        Some(V::Num(n)) => *n,
        _ => bar.neutral,
    }
}

/// As many decimals as the step needs.
fn decimals_of(step: f64) -> usize {
    if step >= 1.0 { 0 } else if step >= 0.1 { 1 } else if step >= 0.01 { 2 } else { 3 }
}

/// What a click on the picture sets: the white balance (temperature and tint), the black point (the lift) or the white point (the gain).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Pick {
    Balance,
    Black,
    White,
}

/// The edit a picked colour makes, on top of what the effect holds now (`balance` is the temperature and tint found for a `Balance` pick).
pub fn picked(list: &effects::EffectsEntry, pick: Pick, rgb: [f64; 3], balance: Option<(f64, f64)>) -> WidgetEdit {
    let values = current(list);
    let num = |id: &str, neutral: f64| match values.get(id) {
        Some(V::Num(n)) => *n,
        _ => neutral,
    };
    let channels = ["r", "g", "b"];
    let changes: Vec<(String, f64)> = match pick {
        Pick::Balance => balance.map(|(temp, tint)| vec![("temp".to_owned(), temp), ("tint".to_owned(), tint)]).unwrap_or_default(),
        Pick::Black => {
            let lift = engine::grade_pick::black_lift(rgb, num("lift_m", 0.0));
            channels.iter().zip(lift).map(|(c, l)| (format!("lift_{c}"), l)).collect()
        }
        Pick::White => {
            let gain = engine::grade_pick::white_gain(rgb, num("gain_m", 1.0), num("tint", 0.0));
            channels.iter().zip(gain).map(|(c, g)| (format!("gain_{c}"), g)).collect()
        }
    };
    with(&values, &changes)
}

/// One of the three pickers: a pipette that waits for a click on the picture (pressed again, it lets go).
fn picker(mut pick: Signal<Option<Pick>>, kind: Pick, tip: String) -> Element {
    let on = pick() == Some(kind);
    rsx! {
        button {
            class: if on { "uis-btn ico wc-pick cmp-start active" } else { "uis-btn ico wc-pick cmp-start" },
            title: "{tip}",
            "aria-label": "{tip}",
            "aria-pressed": "{on}",
            onclick: move |_| pick.set(if on { None } else { Some(kind) }),
            Icon { width: 12, height: 12, icon: LdPipette }
        }
    }
}

/// The width of a colour wheel in pixels.
/// What the list holds for the effect now, or its neutral values.
fn current(list: &effects::EffectsEntry) -> Values {
    list.items.iter().find(|i| i.key == KEY).map_or_else(|| Item::new(KEY).all_values(), Item::all_values)
}

/// The values with these controls set.
fn with(values: &Values, changes: &[(String, f64)]) -> WidgetEdit {
    let mut v = values.clone();
    for (id, x) in changes {
        v.insert(id.clone(), V::Num(*x));
    }
    WidgetEdit { key: KEY.to_owned(), values: Some(v), keep: false }
}

/// A control of a row (temperature, contrast...): the catalog's UisFldLabelled - label, number box, reset. The box is a drag field (`scrub`): a
/// left-right arrow over it, press and drag sideways to change it, a click types; a drag of 250 px spans the whole range.
fn row(bar: &Bar, values: &Values, on_edit: EventHandler<WidgetEdit>, reset: String) -> Element {
    let now = number(values, bar);
    let changed = (now - bar.neutral).abs() > f64::EPSILON;
    let set = EventHandler::new({
        let (values, bar) = (values.clone(), bar.clone());
        move |x: f64| on_edit.call(with(&values, &[(bar.id.clone(), x.clamp(bar.min, bar.max))]))
    });
    let neutral = bar.neutral;
    rsx! {
        UisFldLabelled { compact: true, label: bar.label.clone(), title: bar.label.clone(),
            UisNum {
                value: Some(now),
                min: Some(bar.min),
                max: Some(bar.max),
                step: bar.step,
                decimals: Some(decimals_of(bar.step)),
                scrub: (bar.max - bar.min) / 250.0,
                on_change: move |n: Option<f64>| {
                    if let Some(x) = n {
                        set.call(x);
                    }
                },
            }
            button {
                class: "uis-btn ico fx-reset",
                r#type: "button",
                title: "{reset}",
                disabled: !changed,
                onclick: move |_| set.call(neutral),
                Icon { width: 12, height: 12, icon: LdRotateCcw }
            }
        }
    }
}

/// What a drawer edits with: a drag makes an edit at every pointer move, so the drawer shows its own latest values at once and hands them to the
/// work list (which re-renders the whole view and asks the preview for another picture) at most every `GAP`, the last one always (a short pause
/// after it). Returns the handler to edit through and the values to show (the drawer's own while they are fresh, else the list's).
pub fn use_throttled(key: &'static str, list: Signal<effects::EffectsEntry>, commit: EventHandler<WidgetEdit>) -> (EventHandler<WidgetEdit>, Values) {
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
pub fn ColourDrawer(list: Signal<effects::EffectsEntry>, lang: String, mut pick: Signal<Option<Pick>>, on_edit: EventHandler<WidgetEdit>, #[props(default)] mixed: bool) -> Element {
    let all = bars();
    let (on_edit, values) = use_throttled(KEY, list, on_edit);
    let reset = i18n::t(&lang, "tipFxReset");
    let title = Item::new(KEY).title();
    let mut wheels = use_signal(|| false);

    let flat = |ids: &[&str]| -> Vec<Element> { ids.iter().filter_map(|id| all.get(*id)).map(|b| row(b, &values, on_edit, reset.clone())).collect() };
    let top = flat(&["temp", "tint", "contrast", "pivot"]);
    let low = flat(&["boost", "saturation", "hue"]);

    let groups: Vec<Element> = ["lift", "gamma", "gain", "offset"]
        .into_iter()
        .filter_map(|group| {
            let members: Vec<&Bar> = ["m", "r", "g", "b"].iter().filter_map(|c| all.get(&format!("{group}_{c}"))).collect();
            let head = members.first()?.label.clone();
            let back: Vec<(String, f64)> = members.iter().map(|b| (b.id.clone(), b.neutral)).collect();
            let yrgb: Vec<YrgbBar> = members
                .iter()
                .zip(["var(--uis-plot-line)", "#d4483f", "#3fb560", "#4a64e0"])
                .map(|(b, color)| YrgbBar { id: b.id.clone(), label: b.label.clone(), color: color.to_owned(), min: b.min, max: b.max, step: b.step, neutral: b.neutral })
                .collect();
            let held: HashMap<String, f64> = members.iter().map(|b| (b.id.clone(), number(&values, b))).collect();
            let at_rest = members.iter().all(|b| (number(&values, b) - b.neutral).abs() <= f64::EPSILON);
            let (values, yrgb_values) = (values.clone(), values.clone());
            let reset = reset.clone();
            let picker_here = match group {
                "lift" => Some(picker(pick, Pick::Black, i18n::t(&lang, "tipFxPickBlack"))),
                "gain" => Some(picker(pick, Pick::White, i18n::t(&lang, "tipFxPickWhite"))),
                _ => None,
            };
            Some(rsx! {
                div { class: "wc-group", key: "{group}",
                    div { class: "wc-group-head uis-cmp-head uis-cmp-pinned",
                        {picker_here}
                        span { class: "uis-fld-labelled-label", "{head}" }
                        button {
                            class: "uis-btn ico",
                            title: "{reset}",
                            "aria-label": "{reset}",
                            disabled: at_rest,
                            onclick: move |_| on_edit.call(with(&values, &back)),
                            Icon { width: 12, height: 12, icon: LdRotateCcw }
                        }
                    }
                    UisYrgb {
                        bars: yrgb,
                        values: held,
                        view: if wheels() { YrgbView::Wheel } else { YrgbView::Bars },
                        on_change: move |changes: Vec<(String, f64)>| on_edit.call(with(&yrgb_values, &changes)),
                    }
                }
            })
        })
        .collect();

    let (wheels_tip, bars_tip) = (i18n::t(&lang, "tipColourWheels"), i18n::t(&lang, "tipColourBars"));
    let anything = list.read().items.iter().any(|i| i.key == KEY);
    rsx! {
        div { class: if mixed { "work-colour uis-cmp-drawer uis-cmp-panel uis-lvw-scroll wc-mixed" } else { "work-colour uis-cmp-drawer uis-cmp-panel uis-lvw-scroll" },
            div { class: "wc-head uis-cmp-head",
                {picker(pick, Pick::Balance, i18n::t(&lang, "tipFxPickBalance"))}
                span { class: "uis-fld-labelled-label", "{title}" }
                if mixed {
                    span { class: "wfx-mixed", "≠" }
                }
                span { class: "uis-bar-spacer" }
                button {
                    class: if wheels() { "uis-btn ico wc-view active" } else { "uis-btn ico wc-view" },
                    title: "{wheels_tip}",
                    "aria-label": "{wheels_tip}",
                    "aria-pressed": "{wheels()}",
                    onclick: move |_| wheels.set(true),
                    Icon { width: 14, height: 14, icon: LdCircleDot }
                }
                button {
                    class: if wheels() { "uis-btn ico wc-view" } else { "uis-btn ico wc-view active" },
                    title: "{bars_tip}",
                    "aria-label": "{bars_tip}",
                    "aria-pressed": "{!wheels()}",
                    onclick: move |_| wheels.set(false),
                    Icon { width: 14, height: 14, icon: LdSlidersVertical }
                }
                button {
                    class: "uis-btn ico",
                    title: "{reset}",
                    "aria-label": "{reset}",
                    disabled: !anything,
                    onclick: move |_| on_edit.call(WidgetEdit { key: KEY.to_owned(), values: None, keep: false }),
                    Icon { width: 14, height: 14, icon: LdRotateCcw }
                }
            }
            div { class: "wc-row uis-cmp-row", {top.into_iter()} }
            div { class: "wc-groups", {groups.into_iter()} }
            div { class: "wc-row uis-cmp-row", {low.into_iter()} }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_step_decides_the_decimals_shown() {
        assert_eq!((decimals_of(1.0), decimals_of(0.1), decimals_of(0.01), decimals_of(0.001)), (0, 1, 2, 3));
    }

    #[test]
    fn an_edit_sets_only_the_controls_it_names() {
        let mut v = Values::new();
        v.insert("temp".to_owned(), V::Num(5.0));
        let e = with(&v, &[("tint".to_owned(), 7.0)]);
        let got = e.values.unwrap();
        assert_eq!((got.get("temp"), got.get("tint")), (Some(&V::Num(5.0)), Some(&V::Num(7.0))));
        assert_eq!(e.key, KEY);
    }
}
