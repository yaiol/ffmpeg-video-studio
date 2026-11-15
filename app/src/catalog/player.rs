// twin: uis-ctl-player.jsx jsx@b752806a8cbd rsx@09ca06a4914a
//! UisPlayer - twin of `uis-ctl-player.jsx`: the player of a finished file. The title, the picture of a video, the two
//! times (now, length), the position slider on the full width, then one bar of buttons: previous, play / pause, stop,
//! next, the volume, and close at the far end. Same classes and element order, so the paired `uis-ctl-player.css` skins it.
//!
//! It only DRAWS and reports: the app plays the file. `position` and `length` are seconds, `volume` is 0 to 1, `playing`
//! chooses the play or the pause icon, `picture` is the address of the video's picture (none = a sound, no picture box),
//! `error` a line beside the title. While the position slider is dragged the control shows the dragged time and says
//! nothing; `on_seek` is said once, when it is let go. `has_prev` / `has_next` enable the two step buttons. `labels` holds
//! the tooltips, already translated. It fills the box it is put in (the app decides that box's height, padding, border).
//! It is compact by itself (`.uis-cmp-panel` on its own element, so its buttons are the size of a preview bar's): the
//! app's page must hold `uis-sld.css` and `uis-fld-labelled.css`.
//! Differs: `labels` is a typed struct (`UisPlayerLabels`) where the JSX takes an object. The slider says its last value through the page's own `change` event, where React needs pointer-up / key-up / blur. The stylesheet is added to the page once by id through `document::eval` (the JSX imports its CSS, and `uis-sld.css` with it), because a `document::Style` inside a control logs a warning on every re-render.

use dioxus::prelude::*;
use dioxus_free_icons::{
    Icon,
    icons::ld_icons::{LdPause, LdPlay, LdSkipBack, LdSkipForward, LdSquare, LdVolume1, LdVolume2, LdVolumeX, LdX},
};

const CSS: &str = include_str!("../../assets/uis-ctl-player.css");

/// The tooltips of the buttons, already translated.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct UisPlayerLabels {
    pub play: String,
    pub pause: String,
    pub stop: String,
    pub previous: String,
    pub next: String,
    pub volume: String,
    pub close: String,
}

/// Seconds as m:ss, or h:mm:ss from an hour on.
fn clock(seconds: f64) -> String {
    let s = seconds.max(0.0) as u64;
    let (h, m, s) = (s / 3600, s / 60 % 60, s % 60);
    if h > 0 { format!("{h}:{m:02}:{s:02}") } else { format!("{m}:{s:02}") }
}

#[component]
pub fn UisPlayer(
    #[props(default)] title: String,
    #[props(default)] error: String,
    #[props(default)] picture: Option<String>,
    #[props(default)] position: f64,
    #[props(default)] length: f64,
    #[props(default = 1.0)] volume: f32,
    #[props(default)] playing: bool,
    #[props(default)] has_prev: bool,
    #[props(default)] has_next: bool,
    #[props(default)] labels: UisPlayerLabels,
    #[props(default)] on_seek: EventHandler<f64>,
    #[props(default)] on_toggle: EventHandler<()>,
    #[props(default)] on_stop: EventHandler<()>,
    #[props(default)] on_prev: EventHandler<()>,
    #[props(default)] on_next: EventHandler<()>,
    #[props(default)] on_volume: EventHandler<f32>,
    #[props(default)] on_close: EventHandler<()>,
) -> Element {
    // the time under the dragged slider, None when it is not held
    let mut scrub = use_signal(|| None::<f64>);
    let shown = scrub().unwrap_or(position);
    let toggle = if playing { labels.pause.clone() } else { labels.play.clone() };

    // the stylesheet goes into the page once; a `document::Style` here logs a warning on every re-render
    use_hook(|| {
        let _ = document::eval(&format!(
            "if (!document.getElementById({id:?})) {{ const s = document.createElement('style'); s.id = {id:?}; s.textContent = {css:?}; document.head.appendChild(s); }}",
            id = "uis-ply-css",
            css = CSS
        ));
    });

    rsx! {
        div { class: "uis-ply uis-cmp-panel",
            div { class: "uis-ply-info",
                span { class: "uis-ply-title", title: "{title}", "{title}" }
                if !error.is_empty() {
                    span { class: "uis-ply-error", "{error}" }
                }
            }
            if let Some(url) = &picture {
                div { class: "uis-ply-video",
                    img { class: "uis-ply-picture", src: "{url}", draggable: false, alt: "" }
                }
            }
            div { class: "uis-ply-times",
                span { class: "uis-ply-now", "{clock(shown)}" }
                span { class: "uis-ply-length", "{clock(length)}" }
            }
            input {
                class: "uis-sld-timeline",
                r#type: "range",
                min: "0",
                max: "{length.max(1.0)}",
                step: "0.1",
                value: "{shown}",
                disabled: length <= 0.0,
                oninput: move |e| {
                    if let Ok(v) = e.value().parse::<f64>() {
                        scrub.set(Some(v));
                    }
                },
                onchange: move |e| {
                    if let Ok(v) = e.value().parse::<f64>() {
                        on_seek.call(v);
                    }
                    scrub.set(None);
                },
            }
            div { class: "uis-ply-bar",
                button {
                    r#type: "button",
                    class: "uis-btn ico",
                    title: "{labels.previous}",
                    "aria-label": "{labels.previous}",
                    disabled: !has_prev,
                    onclick: move |_| on_prev.call(()),
                    Icon { width: 14, height: 14, icon: LdSkipBack }
                }
                button {
                    r#type: "button",
                    class: "uis-btn ico",
                    title: "{toggle}",
                    "aria-label": "{toggle}",
                    onclick: move |_| on_toggle.call(()),
                    if playing {
                        Icon { width: 14, height: 14, icon: LdPause }
                    } else {
                        Icon { width: 14, height: 14, icon: LdPlay }
                    }
                }
                button {
                    r#type: "button",
                    class: "uis-btn ico",
                    title: "{labels.stop}",
                    "aria-label": "{labels.stop}",
                    onclick: move |_| on_stop.call(()),
                    Icon { width: 14, height: 14, icon: LdSquare }
                }
                button {
                    r#type: "button",
                    class: "uis-btn ico",
                    title: "{labels.next}",
                    "aria-label": "{labels.next}",
                    disabled: !has_next,
                    onclick: move |_| on_next.call(()),
                    Icon { width: 14, height: 14, icon: LdSkipForward }
                }
                span { class: "uis-ply-vol-icon",
                    if volume <= 0.0 {
                        Icon { width: 14, height: 14, icon: LdVolumeX }
                    } else if volume < 0.5 {
                        Icon { width: 14, height: 14, icon: LdVolume1 }
                    } else {
                        Icon { width: 14, height: 14, icon: LdVolume2 }
                    }
                }
                input {
                    class: "uis-sld compact uis-ply-vol",
                    r#type: "range",
                    min: "0",
                    max: "1",
                    step: "0.02",
                    value: "{volume}",
                    title: "{labels.volume}",
                    "aria-label": "{labels.volume}",
                    oninput: move |e| {
                        if let Ok(v) = e.value().parse::<f32>() {
                            on_volume.call(v);
                        }
                    },
                }
                span { class: "uis-bar-spacer" }
                button {
                    r#type: "button",
                    class: "uis-btn ico subtle",
                    title: "{labels.close}",
                    "aria-label": "{labels.close}",
                    onclick: move |_| on_close.call(()),
                    Icon { width: 14, height: 14, icon: LdX }
                }
            }
        }
    }
}
