// twin: uis-ctl-wave-bars.jsx jsx@54b7d45114d4 rsx@47e83e690693
//! UisWaveBars - twin of `uis-ctl-wave-bars.jsx`: the ONE way a sound's loudness is drawn, for every yaiol app: the bars of a
//! waveform, mirrored round the middle, filling the box they are put in (a `position: relative` box). The Waveform control builds on it,
//! and an app that draws only the bars (a band under a film strip) uses it directly, so the two cannot differ in look or in cost.
//!
//! `peaks` are the buckets, 0 to 1 each. Nothing is drawn for none. One filled outline, never a stroked line per bucket: see the JSX.
//! Differs: the stylesheet is added to the page once by id through `document::eval` (the JSX imports its CSS), because a `document::Style` inside a control logs a warning on every re-render.

use dioxus::prelude::*;

const CSS: &str = include_str!("../../assets/uis-ctl-wave-bars.css");

#[component]
pub fn UisWaveBars(#[props(default)] peaks: Vec<f32>) -> Element {
    // the stylesheet goes into the page once; a `document::Style` here logs a warning on every re-render
    use_hook(|| {
        let _ = document::eval(&format!(
            "if (!document.getElementById({id:?})) {{ const s = document.createElement('style'); s.id = {id:?}; s.textContent = {css:?}; document.head.appendChild(s); }}",
            id = "uis-wvb-css",
            css = CSS
        ));
    });
    let n = peaks.len();
    if n == 0 {
        return rsx! {};
    }
    let mut top = String::new();
    let mut bottom = String::new();
    for (i, v) in peaks.iter().enumerate() {
        let h = (v * 46.0).max(0.8);
        top.push_str(&format!("{}{},{:.1}", if i == 0 { 'M' } else { 'L' }, i as f64 + 0.5, 50.0 - h));
        bottom = format!("L{},{:.1}{bottom}", i as f64 + 0.5, 50.0 + h);
    }
    let d = format!("{top}{bottom}Z");
    rsx! {
        div { class: "uis-wvb",
            svg { class: "uis-wvb-svg", view_box: "0 0 {n} 100", preserve_aspect_ratio: "none",
                path { d: "{d}", fill: "currentColor" }
            }
        }
    }
}
