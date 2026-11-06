// twin: uis-cbo-standard.jsx jsx@587d74d2109b rsx@3f58305866e0
//! UisCboStandard — twin of `uis-cbo-standard.jsx`: the typeahead / autocomplete control. A text field plus a
//! filtered dropdown list on the shared `UisPopover`, so the list is never clipped and looks like `UisMenu`
//! (they share `.uis-pop-surface` + `.uis-pop-item`).
//!
//! The control owns the INPUT + open/close + KEYBOARD NAV (up/down move the highlight, Enter picks the
//! highlighted row); the CONSUMER owns the DATA - it pre-filters `items` by `value` and says how each is
//! keyed, labelled and picked. Enter with no row highlighted falls through to the consumer's
//! `on_key_down` (submit).
//!
//! Controlled input: `value` + `on_change(text)`. Optional `on_open_change(open)` for a consumer that
//! must show a different value while open. `render_header` adds a fixed top row (e.g. a "clear
//! selection" row) and is handed the close action. A `header: true` item is a section label
//! (`.uis-men-label`): skipped by up/down and never picked.
//! Differs: behaviour none. Form only: the rows are a `UisComboItem` list (key, label, active, header) the consumer
//! builds, with an optional `render_item` for a custom row body - Rust is typed, so it cannot take the JSX's
//! free-form `itemKey` / `renderItem` / `itemActive` / `itemHeader` callbacks.

use super::popover::UisPopover;
use dioxus::prelude::*;
use std::sync::atomic::{AtomicU32, Ordering};

static NEXT_ID: AtomicU32 = AtomicU32::new(1);

/// One row. The consumer builds the list; `label` is what the row says.
#[derive(Clone, PartialEq)]
pub struct UisComboItem {
    pub key: String,
    pub label: String,
    /// The currently selected row (`.active`).
    pub active: bool,
    /// A section label: rendered as `.uis-men-label`, never pickable.
    pub header: bool,
}

#[component]
pub fn UisCboStandard(
    value: String,
    on_change: EventHandler<String>,
    #[props(default)] on_focus: Option<EventHandler<()>>,
    #[props(default)] on_blur: Option<EventHandler<()>>,
    #[props(default)] on_open_change: Option<EventHandler<bool>>,
    #[props(default)] on_key_down: Option<EventHandler<KeyboardEvent>>,
    #[props(default)] placeholder: String,
    #[props(default = false)] auto_focus: bool,
    #[props(default = "uis-txt".to_string())] input_class: String,
    #[props(default)] input_style: String,
    #[props(default)] items: Vec<UisComboItem>,
    /// Told the INDEX (into `items`) of the row that was picked.
    #[props(default)] on_pick: Option<EventHandler<usize>>,
    /// A fixed top row; receives the close action.
    #[props(default)] render_header: Option<Callback<Callback<()>, Element>>,
    /// A custom row body for item `i`; default is the item's label.
    #[props(default)] render_item: Option<Callback<usize, Element>>,
) -> Element {
    let id = use_hook(|| format!("combo-{}", NEXT_ID.fetch_add(1, Ordering::Relaxed)));
    let mut open = use_signal(|| false);
    let mut hi = use_signal(|| -1i32); // keyboard-highlighted row index (-1 = none)

    let mut set_open = move |v: bool| {
        open.set(v);
        if let Some(h) = on_open_change {
            h.call(v);
        }
        if !v {
            hi.set(-1);
        }
    };
    let close = use_callback(move |_: ()| set_open(false));
    let show_list = open() && (!items.is_empty() || render_header.is_some());

    // Keep the highlighted row in view while arrowing through a long list.
    {
        let id = id.clone();
        use_effect(move || {
            let h = hi();
            if h >= 0 {
                let _ = document::eval(&format!(
                    "document.getElementById('{id}-row-{h}')?.scrollIntoView({{ block: 'nearest' }});"
                ));
            }
        });
    }

    // Next pickable row in a direction, stepping over header rows. None at either end - Down then stays
    // put, Up falls back to -1 (no row).
    let items_for_keys = items.clone();
    let pickable = move |from: i32, dir: i32| -> Option<i32> {
        let mut i = from + dir;
        while i >= 0 && (i as usize) < items_for_keys.len() {
            if !items_for_keys[i as usize].header {
                return Some(i);
            }
            i += dir;
        }
        None
    };
    let n_items = items.len();
    let hi_pickable = {
        let items = items.clone();
        move |h: i32| h >= 0 && (h as usize) < items.len() && !items[h as usize].header
    };

    let hi_now = hi();
    let row_class = move |active: bool, i: usize| {
        let mut c = String::from("uis-pop-item");
        if active {
            c.push_str(" active");
        }
        if hi_now == i as i32 {
            c.push_str(" hi");
        }
        c
    };

    rsx! {
        input {
            id: "{id}",
            class: "{input_class}",
            style: "{input_style}",
            placeholder: "{placeholder}",
            autofocus: auto_focus,
            value: "{value}",
            oninput: move |e| {
                on_change.call(e.value());
                hi.set(-1);
                if !open() {
                    set_open(true);
                }
            },
            onfocus: move |_| {
                set_open(true);
                if let Some(h) = on_focus {
                    h.call(());
                }
            },
            // Opening on focus ALONE strands the field after a pick - picking closes the list but leaves
            // the input focused, so the next click fires no focus event and the list stays shut. A press
            // on the anchor is exempt from the popover's click-away, so reopening here cannot fight it.
            onmousedown: move |_| {
                if !open() {
                    set_open(true);
                }
            },
            // Click-away / pick (via UisPopover) handle close; blur must not race the pick.
            onblur: move |_| {
                if let Some(h) = on_blur {
                    h.call(());
                }
            },
            onkeydown: move |e| {
                if open() && n_items > 0 {
                    match e.key() {
                        Key::ArrowDown => {
                            e.prevent_default();
                            let h = hi();
                            hi.set(pickable(h, 1).unwrap_or(h));
                            return;
                        }
                        Key::ArrowUp => {
                            e.prevent_default();
                            hi.set(pickable(hi(), -1).unwrap_or(-1));
                            return;
                        }
                        Key::Enter if hi_pickable(hi()) => {
                            e.prevent_default();
                            if let Some(p) = on_pick {
                                p.call(hi() as usize);
                            }
                            set_open(false);
                            return;
                        }
                        _ => {}
                    }
                }
                // Enter with no highlight (and every other key) is the consumer's.
                if let Some(h) = on_key_down {
                    h.call(e);
                }
            },
        }
        UisPopover {
            anchor_id: id.clone(),
            open: show_list,
            on_close: move |_| close.call(()),
            min_width_anchor: true,
            max_height: Some(220.0),
            if let Some(header) = render_header {
                {header.call(close)}
            }
            for (i, it) in items.iter().enumerate() {
                if it.header {
                    div { key: "{it.key}", class: "uis-men-label", "{it.label}" }
                } else {
                    button {
                        key: "{it.key}",
                        id: "{id}-row-{i}",
                        r#type: "button",
                        class: "{row_class(it.active, i)}",
                        onmouseenter: move |_| hi.set(i as i32),
                        onmousedown: move |e| {
                            e.prevent_default();
                            if let Some(p) = on_pick {
                                p.call(i);
                            }
                            set_open(false);
                        },
                        if let Some(render) = render_item {
                            {render.call(i)}
                        } else {
                            "{it.label}"
                        }
                    }
                }
            }
        }
    }
}
