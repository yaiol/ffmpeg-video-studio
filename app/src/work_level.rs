//! The level drawer of the work view: the four ways the catalogue has to set the volume (a fixed gain, a loudness, a peak, ReplayGain) in ONE
//! panel. A choice at the top says which of them the sound gets (or none), and under it every number of that way is a fader you drag up and
//! down, with the same number written below it. Only one way is in the working list at a time: choosing another takes the first out. It takes
//! the place of the file list while it is open. Nothing here knows what a way does: the names, labels, units and limits are the catalogue's.

use crate::{
    catalog::{UisFldLabelled, UisNum},
    i18n,
    preview_session::WidgetEdit,
    work_tone::{controls_of, num, use_throttled},
};
use dioxus::prelude::*;
use dioxus_free_icons::{Icon, icons::ld_icons::LdRotateCcw};
use engine::{
    effects::{self, Item},
    ffmpeg_effects::{V, Values},
};

/// The catalogue's four ways to set the volume, in the order of the choice.
pub const MODES: [&str; 4] = ["volume-gain", "volume-loudness", "volume-peak", "volume-replaygain"];

#[component]
pub fn LevelDrawer(list: Signal<effects::EffectsEntry>, lang: String, on_edit: EventHandler<WidgetEdit>, #[props(default)] mixed: bool) -> Element {
    let raw = on_edit;
    // the way in the list, when there is one; the choice made here while it is still at its neutral values stays shown
    let listed = list.read().items.iter().find(|i| MODES.contains(&i.key.as_str())).map(|i| i.key.clone());
    let mut chosen = use_signal(|| None::<String>);
    let mode = listed.clone().or_else(|| chosen());
    let key = mode.clone().unwrap_or_else(|| MODES[0].to_owned());
    let (on_edit, values) = use_throttled(&key, list, on_edit);
    let title = i18n::t(&lang, "tipWorkVolume");
    let s_none = i18n::t(&lang, "lblFxNone");
    let reset = i18n::t(&lang, "tipFxReset");
    let controls = if mode.is_some() { controls_of(&key) } else { Vec::new() };

    // Every effect of the volume group but the chosen one leaves the list; the chosen one is put in, kept even at its defaults.
    let mut choose = move |to: String| {
        for other in MODES.iter().filter(|m| **m != to) {
            raw.call(WidgetEdit { key: (*other).to_owned(), values: None, keep: true });
        }
        if to.is_empty() {
            chosen.set(None);
            return;
        }
        chosen.set(Some(to.clone()));
        raw.call(WidgetEdit { key: to.clone(), values: Some(Item::new(&to).all_values()), keep: true });
    };
    let choices: Vec<(String, String)> = MODES.iter().map(|m| ((*m).to_owned(), Item::new(m).title())).collect();
    let selected = mode.clone().unwrap_or_default();

    rsx! {
        div { class: if mixed { "work-level work-colour uis-cmp-drawer uis-cmp-panel uis-lvw-scroll wc-mixed" } else { "work-level work-colour uis-cmp-drawer uis-cmp-panel uis-lvw-scroll" },
            div { class: "wc-head uis-cmp-head",
                UisFldLabelled { compact: true, label: title.clone(), title: title.clone(),
                    select {
                        class: "uis-ddn-std wl-mode",
                        title: "{title}",
                        "aria-label": "{title}",
                        value: "{selected}",
                        onchange: move |e| choose(e.value()),
                        option { value: "", selected: selected.is_empty(), "{s_none}" }
                        for (k , name) in choices {
                            option { key: "{k}", value: "{k}", selected: selected == k, "{name}" }
                        }
                    }
                }
                if mixed {
                    span { class: "wfx-mixed", "≠" }
                }
                span { class: "uis-bar-spacer" }
                button {
                    class: "uis-btn ico",
                    title: "{reset}",
                    "aria-label": "{reset}",
                    disabled: mode.is_none(),
                    onclick: move |_| {
                        let key = key.clone();
                        raw.call(WidgetEdit { key: key.clone(), values: Some(Item::new(&key).all_values()), keep: true });
                    },
                    Icon { width: 14, height: 14, icon: LdRotateCcw }
                }
            }
            div { class: "wl-faders wc-row uis-cmp-row",
                for c in controls {
                    {
                        let (id, label, unit) = (c.id.clone(), c.label.clone(), c.unit.clone().unwrap_or_default());
                        let held = values.clone();
                        let edit_key = key.clone();
                        if c.control == "switch" {
                            let on = matches!(values.get(&id), Some(V::Bool(true)));
                            rsx! {
                                label { key: "{id}", class: "wl-switch",
                                    input {
                                        r#type: "checkbox",
                                        checked: on,
                                        onchange: move |e| {
                                            let mut v: Values = held.clone();
                                            v.insert(id.clone(), V::Bool(e.checked()));
                                            raw.call(WidgetEdit { key: edit_key.clone(), values: Some(v), keep: true });
                                        },
                                    }
                                    span { "{label}" }
                                }
                            }
                        } else {
                            let (min, max, step) = (c.min.unwrap_or(-30.0), c.max.unwrap_or(30.0), c.step.unwrap_or(0.1));
                            let now = num(&values, &id);
                            let neutral = match Item::new(&edit_key).all_values().get(&id) {
                                Some(V::Num(n)) => *n,
                                _ => 0.0,
                            };
                            let set = {
                                let (held, id, edit_key) = (held.clone(), id.clone(), edit_key.clone());
                                move |v: f64| {
                                    let mut next: Values = held.clone();
                                    next.insert(id.clone(), V::Num(v.clamp(min, max)));
                                    on_edit.call(WidgetEdit { key: edit_key.clone(), values: Some(next), keep: true });
                                }
                            };
                            let set_number = set.clone();
                            let tip = if unit.is_empty() { label.clone() } else { format!("{label} ({unit})") };
                            rsx! {
                                UisFldLabelled { compact: true, key: "{id}", label: label.clone(), title: tip,
                                    UisNum {
                                        value: Some(now),
                                        min: Some(min),
                                        max: Some(max),
                                        step,
                                        decimals: Some(1),
                                        scrub: (max - min) / 250.0,
                                        on_change: move |v: Option<f64>| set_number(v.unwrap_or(neutral)),
                                    }
                                    span { class: "wl-unit", "{unit}" }
                                    button {
                                        class: "uis-btn ico fx-reset",
                                        r#type: "button",
                                        title: "{reset}",
                                        disabled: (now - neutral).abs() < 1e-9,
                                        onclick: move |_| set(neutral),
                                        Icon { width: 12, height: 12, icon: LdRotateCcw }
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
