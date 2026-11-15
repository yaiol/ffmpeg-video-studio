// twin: uis-ctl-chan-tiles.jsx jsx@a476bda7b47a rsx@06e1c7394162
//! UisChanTiles - twin of `uis-ctl-chan-tiles.jsx`: a row of tiles to click, each a small diagram of what goes where (two inputs, L above
//! R, on the left; the outputs on the right; arrows between) with its name under it. The tile in use is lit; a tile with no diagram shows
//! its name only. The control only says WHICH tile was clicked: what a second click on the lit one does is the app's.
//! Differs: the stylesheet is added to the page once by id through `document::eval` (the JSX imports its CSS), because a `document::Style` inside a control logs a warning on every re-render.

use dioxus::prelude::*;

const CSS: &str = include_str!("../../assets/uis-ctl-chan-tiles.css");

/// One tile: its id, its name, and the path `d` of its diagram in a 110 x 60 box (None = name only).
#[derive(Clone, PartialEq)]
pub struct ChanTile {
    pub id: String,
    pub label: String,
    pub diagram: Option<String>,
}

#[component]
pub fn UisChanTiles(tiles: Vec<ChanTile>, #[props(default)] value: Option<String>, on_pick: EventHandler<String>) -> Element {
    // the stylesheet goes into the page once; a `document::Style` here logs a warning on every re-render
    use_hook(|| {
        let _ = document::eval(&format!(
            "if (!document.getElementById({id:?})) {{ const s = document.createElement('style'); s.id = {id:?}; s.textContent = {css:?}; document.head.appendChild(s); }}",
            id = "uis-cht-css",
            css = CSS
        ));
    });
    rsx! {
        div { class: "uis-cht",
            for t in tiles {
                {
                    let lit = value.as_deref() == Some(t.id.as_str());
                    let id = t.id.clone();
                    rsx! {
                        button {
                            key: "{t.id}",
                            r#type: "button",
                            class: if lit { "uis-cht-tile active" } else { "uis-cht-tile" },
                            title: "{t.label}",
                            "aria-label": "{t.label}",
                            "aria-pressed": "{lit}",
                            onclick: move |_| on_pick.call(id.clone()),
                            if let Some(d) = t.diagram.clone() {
                                svg { width: "73", height: "40", view_box: "0 0 110 60",
                                    text { x: "4", y: "18", "font-size": "11", fill: "currentColor", "L" }
                                    text { x: "4", y: "50", "font-size": "11", fill: "currentColor", "R" }
                                    text { x: "98", y: "18", "font-size": "11", fill: "currentColor", "L" }
                                    text { x: "98", y: "50", "font-size": "11", fill: "currentColor", "R" }
                                    path { d: "{d}", stroke: "currentColor", "stroke-width": "1.6", fill: "none" }
                                }
                            }
                            span { class: "uis-cht-label", "{t.label}" }
                        }
                    }
                }
            }
        }
    }
}
