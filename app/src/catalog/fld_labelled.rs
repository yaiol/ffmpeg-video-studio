// twin: uis-fld-labelled.jsx jsx@b3b3a4eade4d rsx@7d0547bd41e9
//! UisFldLabelled — twin of `uis-fld-labelled.jsx`: a labelled field, the caption and the control it names.
//! Normal: the caption above its control. `compact`: the caption and the control on ONE line. Same classes and
//! element order, so `uis-fld-labelled.css` skins it unchanged.
//! `label` is the already-translated caption; `title` is its tooltip (the label when empty).
//! Differs: the stylesheet. The JSX imports its paired CSS; this twin does NOT inject it, because a panel
//! holds many fields and every `document::Style` that re-renders logs "Changing the props of `Style {}`
//! is not supported". The app lists `uis-fld-labelled.css` once, in its CSS array, like `uis-app.css`.

use dioxus::prelude::*;

#[component]
pub fn UisFldLabelled(
    label: String,
    #[props(default)] title: String,
    #[props(default)] compact: bool,
    children: Element,
) -> Element {
    let tip = if title.is_empty() { label.clone() } else { title };
    if compact {
        rsx! {
            div { class: "uis-fld-labelled compact",
                span { class: "uis-fld-labelled-label", title: "{tip}", "{label}" }
                div { class: "uis-fld-labelled-ctl", {children} }
            }
        }
    } else {
        rsx! {
            div { class: "uis-fld-labelled",
                span { class: "uis-fld-labelled-label", title: "{tip}", "{label}" }
                {children}
            }
        }
    }
}
