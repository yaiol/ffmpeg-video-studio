// twin: uis-header.jsx jsx@5c98efa34454 rsx@09afcab5f2ab
//! UisAppHeader — twin of `uis-header.jsx`: app name, version badge, children, then the 2px gradient identity
//! separator, then an OPTIONAL sub-bar (a tag / filter strip) below it - so the order is always
//! "header buttons -> separator -> optional sub-bar".
//! Differs: none.

use dioxus::prelude::*;

#[component]
pub fn UisAppHeader(
    app_name: String,
    app_version: String,
    /// The strip below the separator; omit when there is none.
    #[props(default)]
    sub_bar: Option<Element>,
    children: Element,
) -> Element {
    rsx! {
        div { class: "uis-barh-header",
            span { class: "uis-barh-app-name", "{app_name}" }
            span { class: "uis-barh-app-version", "v{app_version}" }
            {children}
        }
        div { class: "uis-barh-gradient" }
        if let Some(bar) = sub_bar {
            div { class: "uis-barh-subbar", {bar} }
        }
    }
}
