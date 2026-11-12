// twin: uis-header.jsx jsx@74917b32eb7f rsx@7006a2cf2a28
//! UisAppHeader — twin of `uis-header.jsx`: app name, version badge, children, then the 2px gradient identity
//! separator, then an OPTIONAL sub-bar (a tag / filter strip) below it - so the order is always
//! "header buttons -> separator -> optional sub-bar". `bare`: no name and no version, for an app whose window
//! bar (`UisBarWindow`) already shows them.
//! Differs: none.

use dioxus::prelude::*;

#[component]
pub fn UisAppHeader(
    app_name: String,
    app_version: String,
    /// The strip below the separator; omit when there is none.
    #[props(default)]
    sub_bar: Option<Element>,
    /// Without the name and the version: the window bar above shows them.
    #[props(default)]
    bare: bool,
    children: Element,
) -> Element {
    rsx! {
        div { class: "uis-barh-header",
            if !bare {
                span { class: "uis-barh-app-name", "{app_name}" }
                span { class: "uis-barh-app-version", "v{app_version}" }
            }
            {children}
        }
        div { class: "uis-barh-gradient" }
        if let Some(bar) = sub_bar {
            div { class: "uis-barh-subbar", {bar} }
        }
    }
}
