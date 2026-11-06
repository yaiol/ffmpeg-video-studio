// twin: uis-cbo-filter.jsx jsx@e9429b374665 rsx@4765cbeff561
//! UisCboFilter — twin of `uis-cbo-filter.jsx`: a search box with a category filter in front of it, drawn as
//! one compact line. An icon button opens a short list of categories ("All" first); the choice narrows what the
//! search box offers. The control owns the button, the list and its open/close; the CONSUMER owns the data (it
//! filters `items` by the category and the text, as for a plain `UisCboStandard`).
//!
//! Three drawings: the compact line (default), `roomy` (a welded bar group inside a `.uis-fld-labelled`, for a normal
//! form) and `weld` (the same welded group on its own, for a bar). `class` is added to the weld so the APP can
//! place it; it never restyles it. A category may carry an `icon`: it is drawn on its row, and the button shows
//! the chosen category's icon (the `icon` prop stands for "all"). `after` is extra controls drawn inside the weld,
//! after the search box. The remaining options of `UisCboStandard` pass straight through.
//! Differs: form only. The category list is `UisCboCategory` values and the rows are a `UisComboItem` list; the anchor
//! of the category list is an `id` the consumer names (`UisPopover` places itself against an element id, where the
//! JSX holds a ref).

use super::cbo_standard::{UisComboItem, UisCboStandard};
use super::popover::UisPopover;
use dioxus::prelude::*;

/// One category of the filter list. `icon` is drawn on its row and on the button while it is chosen.
#[derive(Clone, PartialEq)]
pub struct UisCboCategory {
    pub id: String,
    pub title: String,
    pub icon: Option<Element>,
}

#[component]
pub fn UisCboFilter(
    /// The `id` of the category button (the list is placed against it); unique on the page.
    id: String,
    /// What the button shows while no category is chosen ("all"); a category's own icon replaces it.
    #[props(default)] icon: Option<Element>,
    all_label: String,
    categories: Vec<UisCboCategory>,
    /// The chosen category id; empty = all.
    category: String,
    on_category: EventHandler<String>,
    value: String,
    on_change: EventHandler<String>,
    #[props(default)] placeholder: String,
    #[props(default)] items: Vec<UisComboItem>,
    #[props(default)] on_pick: Option<EventHandler<usize>>,
    /// Drawn for a normal form (a welded bar group in a `.uis-fld-labelled`) instead of the compact line.
    #[props(default = false)] roomy: bool,
    /// Drawn as a welded bar group on its own (a bar or a toolbar).
    #[props(default = false)] weld: bool,
    /// Added to the weld, for the app to place it.
    #[props(default)] class: String,
    /// Extra controls inside the weld, after the search box.
    #[props(default)] after: Option<Element>,
    #[props(default)] on_focus: Option<EventHandler<()>>,
    #[props(default)] on_blur: Option<EventHandler<()>>,
    #[props(default)] on_open_change: Option<EventHandler<bool>>,
    #[props(default)] on_key_down: Option<EventHandler<KeyboardEvent>>,
    #[props(default = false)] auto_focus: bool,
    #[props(default)] render_header: Option<Callback<Callback<()>, Element>>,
    #[props(default)] render_item: Option<Callback<usize, Element>>,
) -> Element {
    let mut open = use_signal(|| false);
    let current = categories.iter().find(|c| c.id == category).cloned();
    let title = current.as_ref().map(|c| c.title.clone()).unwrap_or_else(|| all_label.clone());
    let shown = current.and_then(|c| c.icon).or(icon);
    let button = rsx! {
        button {
            id: "{id}",
            r#type: "button",
            class: "uis-btn ico",
            title: "{title}",
            "aria-label": "{title}",
            onclick: move |_| open.set(!open()),
            if let Some(shown) = shown { {shown} }
        }
    };
    let search = rsx! {
        UisCboStandard {
            value, on_change, placeholder, items, on_pick,
            on_focus, on_blur, on_open_change, on_key_down, auto_focus, render_header, render_item,
        }
    };
    let weld_class = format!("uis-barh-grp uis-cbo-weld {class}");
    let welded = rsx! {
        div { class: "{weld_class.trim()}",
            {button.clone()}
            div { class: "uis-cbo-cell", {search.clone()} }
            if let Some(after) = after.clone() { {after} }
        }
    };
    rsx! {
        if roomy {
            div { class: "uis-fld-labelled", {welded} }
        } else if weld {
            {welded}
        } else {
            div { class: "uis-fld-labelled compact", div { class: "uis-fld-labelled-ctl", {button} {search} if let Some(after) = after { {after} } } }
        }
        UisPopover { anchor_id: id.clone(), open: open(), on_close: move |_| open.set(false),
            button {
                r#type: "button",
                class: if category.is_empty() { "uis-pop-item active" } else { "uis-pop-item" },
                onmousedown: move |ev| {
                    ev.prevent_default();
                    on_category.call(String::new());
                    open.set(false);
                },
                "{all_label}"
            }
            for c in categories.clone() {
                button {
                    key: "{c.id}",
                    r#type: "button",
                    class: if category == c.id { "uis-pop-item active" } else { "uis-pop-item" },
                    onmousedown: {
                        let cid = c.id.clone();
                        move |ev| {
                            ev.prevent_default();
                            on_category.call(cid.clone());
                            open.set(false);
                        }
                    },
                    if let Some(ic) = c.icon.clone() { {ic} }
                    "{c.title}"
                }
            }
        }
    }
}
