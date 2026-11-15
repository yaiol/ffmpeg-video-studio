// twin: uis-btn-copy.jsx jsx@9eae4a6f5933 rsx@8517c466d4a7
//! UisBtnCopy — twin of `uis-btn-copy.jsx`: the copy-to-clipboard icon button. A `.uis-btn ico` that writes `text`
//! to the system clipboard and turns into a check mark for 1.2 s, then back. Same classes, so `uis-app.css`
//! skins it.
//!
//! `text` is what to copy; `on_copied` runs after a successful copy; a denied clipboard does nothing and shows no
//! check. `subtle` / `small` pick the `.subtle` / `.small` modifiers (`small` defaults to on, like the jsx).
//! `label` is the word beside the icon (a plain `.uis-btn`, not the icon-only `.ico`). Disabled with no text. `title` is the tooltip and the accessible name, already translated.
//! Needs `arboard` and `tokio` in the app's Cargo.toml (the system clipboard: the web view's own
//! `navigator.clipboard` is missing in a page that is not served securely).
//! Differs: `text` is a `String` (the jsx also takes a function returning the text, sync or async).

use dioxus::prelude::*;
use dioxus_free_icons::{
    Icon,
    icons::ld_icons::{LdCheck, LdCopy},
};

#[component]
pub fn UisBtnCopy(
    text: String,
    #[props(default)] title: String,
    #[props(default)] subtle: bool,
    #[props(default = true)] small: bool,
    #[props(default)] label: String,
    #[props(default)] disabled: bool,
    #[props(default)] on_copied: EventHandler<()>,
) -> Element {
    let mut done = use_signal(|| false);
    let class = format!(
        "uis-btn{}{}{}",
        if label.is_empty() { " ico" } else { "" },
        if small { " small" } else { "" },
        if subtle { " subtle" } else { "" }
    );
    let nothing = text.is_empty();

    rsx! {
        button {
            r#type: "button",
            class: "{class}",
            title: "{title}",
            aria_label: "{title}",
            disabled: disabled || nothing,
            onclick: move |_| {
                let text = text.clone();
                spawn(async move {
                    let copied = tokio::task::spawn_blocking(move || {
                        arboard::Clipboard::new().and_then(|mut c| c.set_text(text)).is_ok()
                    })
                    .await
                    .unwrap_or(false);
                    if copied {
                        done.set(true);
                        on_copied.call(());
                        tokio::time::sleep(std::time::Duration::from_millis(1200)).await;
                        done.set(false);
                    }
                });
            },
            if done() {
                Icon { width: 14, height: 14, icon: LdCheck }
            } else {
                Icon { width: 14, height: 14, icon: LdCopy }
            }
            "{label}"
        }
    }
}
