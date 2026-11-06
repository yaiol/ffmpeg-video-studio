//! The compressor drawer of the work view: the compressor's two controls drawn as its transfer curve, the level that comes out for every level
//! that goes in (both in dB). Under the threshold the sound is left as it is (the curve follows the diagonal); above it the curve bends flatter
//! by the ratio. Drag the knee sideways to move the threshold, drag the right end up or down to change the ratio. It takes the place of the file
//! list while it is open. The limits and the neutral values come from the catalogue's `compressor` effect.

use crate::{
    catalog::{UisFldLabelled, UisNum, UisTransfer},
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

/// The catalogue key of the effect this drawer edits.
pub const KEY: &str = "compressor";

#[component]
pub fn CompressorDrawer(list: Signal<effects::EffectsEntry>, lang: String, on_edit: EventHandler<WidgetEdit>, #[props(default)] mixed: bool) -> Element {
    let (on_edit, values) = use_throttled(KEY, list, on_edit);
    let controls = controls_of(KEY);
    let find = |id: &str| controls.iter().find(|c| c.id == id).cloned();
    let (threshold_c, ratio_c) = (find("threshold"), find("ratio"));
    let t_range = threshold_c.as_ref().map_or((-60.0, 0.0), |c| (c.min.unwrap_or(-60.0), c.max.unwrap_or(0.0)));
    let r_range = ratio_c.as_ref().map_or((1.0, 20.0), |c| (c.min.unwrap_or(1.0), c.max.unwrap_or(20.0)));
    let (threshold, ratio) = (num(&values, "threshold"), num(&values, "ratio").max(1.0));
    let reset = i18n::t(&lang, "tipFxReset");
    let title = Item::new(KEY).title();
    let anything = list.read().items.iter().any(|i| i.key == KEY);
    let t_label = threshold_c.map(|c| c.label).unwrap_or_default();
    let r_label = ratio_c.map(|c| c.label).unwrap_or_default();

    let (side_values, ratio_values) = (values.clone(), values.clone());
    let (side_edit, ratio_edit) = (on_edit, on_edit);
    let edit_with = move |values: &Values, threshold: f64, ratio: f64| {
        let mut v = values.clone();
        v.insert("threshold".to_owned(), V::Num(threshold));
        v.insert("ratio".to_owned(), V::Num(ratio));
        WidgetEdit { key: KEY.to_owned(), values: Some(v), keep: true }
    };

    rsx! {
        div { class: if mixed { "work-compressor work-colour uis-cmp-drawer uis-cmp-panel uis-lvw-scroll wc-mixed" } else { "work-compressor work-colour uis-cmp-drawer uis-cmp-panel uis-lvw-scroll" },
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
                    onclick: move |_| on_edit.call(WidgetEdit { key: KEY.to_owned(), values: None, keep: false }),
                    Icon { width: 14, height: 14, icon: LdRotateCcw }
                }
            }
            div { class: "wc-curve-body",
                UisTransfer {
                    threshold,
                    ratio,
                    threshold_range: t_range,
                    ratio_range: r_range,
                    active: anything,
                    width: 194.0,
                    height: 194.0,
                    on_change: move |(t, r): (f64, f64)| on_edit.call(edit_with(&values, t, r)),
                }
                div { class: "wc-curve-side",
                    UisFldLabelled { compact: true, label: t_label.clone(), title: t_label.clone(),
                        UisNum {
                            value: Some(threshold),
                            min: Some(t_range.0),
                            max: Some(t_range.1),
                            step: 0.5,
                            decimals: Some(1),
                            scrub: (t_range.1 - t_range.0) / 250.0,
                            on_change: move |n: Option<f64>| {
                                if let Some(x) = n {
                                    side_edit.call(edit_with(&side_values, x, ratio));
                                }
                            },
                        }
                    }
                    UisFldLabelled { compact: true, label: r_label.clone(), title: r_label.clone(),
                        UisNum {
                            value: Some(ratio),
                            min: Some(r_range.0),
                            max: Some(r_range.1),
                            step: 1.0,
                            decimals: Some(0),
                            scrub: (r_range.1 - r_range.0) / 250.0,
                            on_change: move |n: Option<f64>| {
                                if let Some(x) = n {
                                    ratio_edit.call(edit_with(&ratio_values, threshold, x));
                                }
                            },
                        }
                    }
                }
            }
        }
    }
}
