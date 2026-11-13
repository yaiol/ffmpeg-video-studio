// twin: uis-bar-window.jsx jsx@c7ddf26de248 rsx@fb6510a4961a
//! UisBarWindow — twin of `uis-bar-window.jsx`: the window bar, the app's own title bar on a window that has no
//! system frame. The app's icon (`icon_src`, a picture URL; empty = none) and the app-wide buttons (the children:
//! settings, help, source, then the two reminders of `news_loader.rs`: `UisBtnUpdate`, a newer version is out,
//! and `UisBtnNews`, yaiol has news for this app)
//! at the start, the app name and the version pill in the middle, the three window buttons at the end. Same classes and element order, so `uis-bar-window.css` skins it
//! unchanged. It holds no words and does nothing to the window itself: the app passes the tooltips, whether the
//! window is maximized, and what each button does. `on_maximize` is one handler for both states.
//! Differs: moving the window. In Electron the bar is a CSS `app-region`, which also maximizes on a double click;
//! a WebView ignores that property, so this twin reports a press on the bar's empty part (`on_drag`, for the app
//! to start the move), calls `on_maximize` on a double click there, and reports a right-click there (`on_menu`,
//! for the app to open the system's window menu, which Electron opens by itself). A LEFT click on the icon reports
//! `on_menu` too, as on a system title bar: Electron cannot (it has no call that opens that menu, and no way to
//! declare the icon as the system's), so there the icon only moves the window. The JSX file also holds
//! `LbsHokWindow`, the page's calls to the main process's window routes: a Rust app has no such routes and calls the
//! window directly, so it has no counterpart here. The stylesheet: the JSX imports its paired
//! CSS; this twin does NOT inject it (a `document::Style` that re-renders logs a warning), the app lists
//! `uis-bar-window.css` once, in its CSS array, like `uis-app.css`.

use dioxus::prelude::*;
use dioxus_free_icons::{
    icons::ld_icons::{LdCopy, LdMinus, LdSquare, LdX},
    Icon,
};

#[component]
pub fn UisBarWindow(
    app_name: String,
    app_version: String,
    #[props(default)] icon_src: String,
    #[props(default)] maximized: bool,
    tip_minimize: String,
    tip_maximize: String,
    tip_restore: String,
    tip_close: String,
    on_minimize: EventHandler<()>,
    on_maximize: EventHandler<()>,
    on_close: EventHandler<()>,
    /// A press of the main button on the bar's empty part: the app starts moving the window.
    on_drag: EventHandler<()>,
    /// A right-click on the bar (a button excluded), or any click on the icon: the app opens the system's window menu.
    on_menu: EventHandler<()>,
    children: Element,
) -> Element {
    let tip_max = if maximized { tip_restore } else { tip_maximize };
    rsx! {
        div {
            class: "uis-barh-window",
            onmousedown: move |e| {
                if e.trigger_button() == Some(dioxus::html::input_data::MouseButton::Primary) {
                    on_drag.call(());
                }
            },
            ondoubleclick: move |_| on_maximize.call(()),
            oncontextmenu: move |_| on_menu.call(()),
            div { class: "uis-barh-window-start",
                if !icon_src.is_empty() {
                    // A click on the icon opens the window menu; so a press on it does not start a move.
                    img {
                        class: "uis-barh-window-icon",
                        src: "{icon_src}",
                        alt: "",
                        draggable: false,
                        onmousedown: move |e| e.stop_propagation(),
                        ondoubleclick: move |e| e.stop_propagation(),
                        onclick: move |_| on_menu.call(()),
                    }
                }
                // A press, a double click or a right-click on a button is the button's, never the bar's.
                div {
                    class: "uis-barh-window-acts",
                    onmousedown: move |e| e.stop_propagation(),
                    ondoubleclick: move |e| e.stop_propagation(),
                    oncontextmenu: move |e| e.stop_propagation(),
                    {children}
                }
            }
            div { class: "uis-barh-window-title",
                span { class: "uis-barh-window-name", "{app_name}" }
                span { class: "uis-barh-app-version", "v{app_version}" }
            }
            div {
                class: "uis-barh-window-ctl",
                onmousedown: move |e| e.stop_propagation(),
                ondoubleclick: move |e| e.stop_propagation(),
                oncontextmenu: move |e| e.stop_propagation(),
                button {
                    class: "uis-barh-window-btn",
                    title: "{tip_minimize}",
                    "aria-label": "{tip_minimize}",
                    onclick: move |_| on_minimize.call(()),
                    Icon { icon: LdMinus }
                }
                button {
                    class: "uis-barh-window-btn",
                    title: "{tip_max}",
                    "aria-label": "{tip_max}",
                    onclick: move |_| on_maximize.call(()),
                    if maximized {
                        Icon { icon: LdCopy }
                    } else {
                        Icon { icon: LdSquare }
                    }
                }
                button {
                    class: "uis-barh-window-btn close",
                    title: "{tip_close}",
                    "aria-label": "{tip_close}",
                    onclick: move |_| on_close.call(()),
                    Icon { icon: LdX }
                }
            }
        }
    }
}
