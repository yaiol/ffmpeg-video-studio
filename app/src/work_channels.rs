//! The channels drawer of the work view: the catalogue's ways to treat the two channels, each drawn as a small diagram of what goes where
//! (left and right in, what comes out), as four tiles to click. The tile in use is lit; a click on it again takes the effect out. It takes the
//! place of the file list while it is open. The ways and their names are the catalogue's `channels` effect; the pictures are this drawer's.

use crate::{preview_session::WidgetEdit, work_tone::controls_of};
use dioxus::prelude::*;
use engine::{
    effects::{self, Item},
    ffmpeg_effects::V,
};

/// The catalogue key of the effect this drawer edits.
pub const KEY: &str = "channels";

/// The diagram of a way: two boxes in (L above R) and what each output channel takes, as arrows from the inputs to the outputs. A way the
/// catalogue has and this drawer has no picture for is drawn with its name only.
fn diagram(way: &str) -> Option<&'static str> {
    Some(match way {
        // L -> right output, R -> left output
        "swap" => "M18,14 L92,46 M18,46 L92,14",
        // L -> both outputs
        "left" => "M18,14 L92,14 M18,14 L92,46",
        // R -> both outputs
        "right" => "M18,46 L92,14 M18,46 L92,46",
        // both inputs -> one output
        "mono" => "M18,14 L92,30 M18,46 L92,30",
        // each channel to its own output (a larger layout is mixed down first)
        "stereo" => "M18,14 L92,14 M18,46 L92,46",
        _ => return None,
    })
}

#[component]
pub fn ChannelsDrawer(list: Signal<effects::EffectsEntry>, lang: String, on_edit: EventHandler<WidgetEdit>, #[props(default)] mixed: bool) -> Element {
    let title = Item::new(KEY).title();
    let _ = &lang;
    let controls = controls_of(KEY);
    let ways: Vec<(String, String)> = controls
        .iter()
        .find(|c| c.id == "mode")
        .map(|c| c.choices.iter().map(|ch| (match &ch.value { V::Str(s) => s.clone(), other => format!("{other:?}") }, ch.label.clone())).collect())
        .unwrap_or_default();
    let current = list.read().items.iter().find(|i| i.key == KEY).map(|i| match i.all_values().get("mode") {
        Some(V::Str(s)) => s.clone(),
        _ => String::new(),
    });

    rsx! {
        div { class: if mixed { "work-channels work-colour uis-cmp-drawer uis-cmp-panel uis-lvw-scroll wc-mixed" } else { "work-channels work-colour uis-cmp-drawer uis-cmp-panel uis-lvw-scroll" },
            div { class: "wc-head uis-cmp-head",
                span { class: "uis-fld-labelled-label", "{title}" }
                if mixed {
                    span { class: "wfx-mixed", "≠" }
                }
            }
            div { class: "wch-tiles",
                for (way , label) in ways {
                    {
                        let lit = current.as_deref() == Some(way.as_str());
                        let picture = diagram(&way);
                        let way_click = way.clone();
                        rsx! {
                            button {
                                key: "{way}",
                                r#type: "button",
                                class: if lit { "wch-tile active" } else { "wch-tile" },
                                title: "{label}",
                                "aria-label": "{label}",
                                "aria-pressed": "{lit}",
                                onclick: move |_| {
                                    if lit {
                                        on_edit.call(WidgetEdit { key: KEY.to_owned(), values: None, keep: true });
                                    } else {
                                        let mut v = Item::new(KEY).all_values();
                                        v.insert("mode".to_owned(), V::Str(way_click.clone()));
                                        on_edit.call(WidgetEdit { key: KEY.to_owned(), values: Some(v), keep: true });
                                    }
                                },
                                if let Some(d) = picture {
                                    svg { width: "73", height: "40", view_box: "0 0 110 60",
                                        text { x: "4", y: "18", "font-size": "11", fill: "currentColor", "L" }
                                        text { x: "4", y: "50", "font-size": "11", fill: "currentColor", "R" }
                                        text { x: "98", y: "18", "font-size": "11", fill: "currentColor", "L" }
                                        text { x: "98", y: "50", "font-size": "11", fill: "currentColor", "R" }
                                        path { d: "{d}", stroke: "currentColor", "stroke-width": "1.6", fill: "none" }
                                    }
                                }
                                span { class: "wch-label", "{label}" }
                            }
                        }
                    }
                }
            }
        }
    }
}
