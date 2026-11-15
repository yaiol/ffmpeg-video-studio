//! The command panel of the work view: the exact ffmpeg command the selected file would be converted with (the panel's choices, the
//! effects as they are now), on one block that can be copied. It is the command the queue builds - `engine::command_line` asks the same code.

use crate::{
    catalog::UisBtnCopy,
    i18n,
};
use dioxus::prelude::*;

/// `text` is the command, or the reason there is none (`failed`), or empty when no file is selected.
#[component]
pub fn CommandPanel(text: String, failed: bool, lang: String) -> Element {
    let t = |key: &str| i18n::t(&lang, key);
    let s_title = t("lblWorkCommand");
    let s_copy = t("btnWorkCommandCopy");
    let s_tip = t("tipWorkCommandCopy");
    let shown = if text.is_empty() { t("msgWorkCommandNone") } else { text.clone() };
    let can_copy = !failed && !text.is_empty();
    rsx! {
        div { class: "uis-cmp-panel",
            div { class: "uis-cmp-head between",
                span { class: "uis-fld-labelled-label", "{s_title}" }
                UisBtnCopy { text: text.clone(), title: "{s_tip}", label: s_copy, small: false, disabled: !can_copy }
            }
            div { class: "uis-code-box", "{shown}" }
        }
    }
}
