//! The command panel of the work view: the exact ffmpeg command the selected file would be converted with (the panel's choices, the
//! effects as they are now), on one block that can be copied. It is the command the queue builds - `engine::command_line` asks the same code.

use crate::i18n;
use dioxus::prelude::*;
use dioxus_free_icons::{icons::ld_icons::LdCopy, Icon};

/// `text` is the command, or the reason there is none (`failed`), or empty when no file is selected.
#[component]
pub fn CommandPanel(text: String, failed: bool, lang: String) -> Element {
    let t = |key: &str| i18n::t(&lang, key);
    let mut copied = use_signal(|| false);
    let s_title = t("lblWorkCommand");
    let s_copy = t("btnWorkCommandCopy");
    let s_tip = t("tipWorkCommandCopy");
    let shown = if text.is_empty() { t("msgWorkCommandNone") } else { text.clone() };
    let class = if failed || text.is_empty() { "wcmd-text wcmd-dim" } else { "wcmd-text" };
    let can_copy = !failed && !text.is_empty();
    rsx! {
        div { class: "wcmd uis-cmp-panel",
            div { class: "wcmd-head",
                span { class: "wcmd-title", "{s_title}" }
                button {
                    class: "uis-btn",
                    title: "{s_tip}",
                    disabled: !can_copy,
                    onclick: move |_| {
                        let text = text.clone();
                        spawn(async move {
                            // The system clipboard, not the page's: the web view's own `navigator.clipboard` is not there in a page that is not served securely.
                            let done = tokio::task::spawn_blocking(move || {
                                arboard::Clipboard::new().and_then(|mut c| c.set_text(text)).map_err(|e| e.to_string())
                            })
                            .await;
                            match done {
                                Ok(Ok(())) => {
                                    copied.set(true);
                                    tokio::time::sleep(std::time::Duration::from_millis(1200)).await;
                                    copied.set(false);
                                }
                                Ok(Err(why)) => engine::log::error("clipboard", &why),
                                Err(why) => engine::log::error("clipboard", &why.to_string()),
                            }
                        });
                    },
                    Icon { width: 14, height: 14, icon: LdCopy }
                    if copied() { "✓" } else { "{s_copy}" }
                }
            }
            div { class: "{class}", "{shown}" }
        }
    }
}
