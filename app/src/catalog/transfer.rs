// twin: uis-ctl-transfer.jsx jsx@7e72a6bd790b rsx@cc5a2217201f
//! UisTransfer - twin of `uis-ctl-transfer.jsx`: the level that comes out of a compressor for every level that goes
//! in, both in dB, on a dark grid. Under the threshold the curve follows the diagonal; above it the curve bends flatter
//! by the ratio. Drag the knee sideways to move the threshold, drag the right end up or down to change the ratio. Same
//! classes and element order, so the paired `uis-ctl-transfer.css` skins it.
//!
//! `threshold` (dB) and `ratio` are the two numbers the graph holds; `threshold_range` / `ratio_range` are their limits.
//! The threshold moves in half dB, the ratio in whole numbers. `on_change` says both new values. `active` false draws the
//! curve dim. `width` / `height` are the graph's size in pixels (the pointer is read against it). The numbers beside the
//! graph are the app's. Pointer capture has no Rust API, so it goes through `document::eval` by element id.
//! Differs: the stylesheet is added to the page once by id through `document::eval` (the JSX imports its CSS), because a `document::Style` inside a control logs a warning on every re-render.

use dioxus::prelude::*;
use std::sync::atomic::{AtomicU32, Ordering};

const CSS: &str = include_str!("../../assets/uis-ctl-transfer.css");
static NEXT_ID: AtomicU32 = AtomicU32::new(1);
/// The lowest level drawn, in dB (the highest is 0).
const FLOOR: f64 = -60.0;
/// How close, in pixels, a press must be to a handle to take it.
const REACH: f64 = 40.0;

/// The level that comes out for `input` dB going in.
fn out_of(input: f64, threshold: f64, ratio: f64) -> f64 {
    if input <= threshold { input } else { threshold + (input - threshold) / ratio.max(1.0) }
}

/// The ratio that brings 0 dB in out as `out` dB, for a threshold; kept to the whole numbers the control takes.
fn ratio_for(out: f64, threshold: f64, (lo, hi): (f64, f64)) -> f64 {
    let rise = out - threshold;
    if rise <= 0.05 { hi } else { (-threshold / rise).round().clamp(lo, hi) }
}

/// The two handles: the knee, and the end of the curve at the right edge.
#[derive(Clone, Copy, PartialEq)]
enum Handle {
    Knee,
    End,
}

#[component]
pub fn UisTransfer(
    threshold: f64,
    ratio: f64,
    #[props(default = (-60.0, 0.0))] threshold_range: (f64, f64),
    #[props(default = (1.0, 20.0))] ratio_range: (f64, f64),
    #[props(default = true)] active: bool,
    #[props(default = 240.0)] width: f64,
    #[props(default = 240.0)] height: f64,
    on_change: EventHandler<(f64, f64)>,
) -> Element {
    let id = use_hook(|| format!("uis-trf-{}", NEXT_ID.fetch_add(1, Ordering::Relaxed)));
    let mut grab = use_signal(|| None::<Handle>);
    let (w, h) = (width, height);
    let ratio = ratio.max(1.0);
    let x_of = move |db: f64| (db - FLOOR) / -FLOOR * w;
    let y_of = move |db: f64| (1.0 - (db - FLOOR) / -FLOOR) * h;
    let db_at_y = move |y: f64| FLOOR + (1.0 - (y / h).clamp(0.0, 1.0)) * -FLOOR;
    let threshold_at = move |x: f64| {
        let db = FLOOR + (x / w).clamp(0.0, 1.0) * -FLOOR;
        ((db * 2.0).round() / 2.0).clamp(threshold_range.0, threshold_range.1)
    };

    let knee = (x_of(threshold), y_of(threshold));
    let end = (x_of(0.0), y_of(out_of(0.0, threshold, ratio)));
    let curve = format!("M0,{h} L{:.1},{:.1} L{:.1},{:.1}", knee.0, knee.1, end.0, end.1);
    let diagonal = format!("M0,{h} L{w},0");
    let mut grid = String::new();
    for g in [-50.0, -40.0, -30.0, -20.0, -10.0] {
        grid.push_str(&format!("M{:.1},0 V{h} M0,{:.1} H{w} ", x_of(g), y_of(g)));
    }
    let style = format!("width: {w}px; height: {h}px;");
    let edit = move |handle: Handle, at: (f64, f64)| match handle {
        Handle::Knee => on_change.call((threshold_at(at.0), ratio)),
        Handle::End => on_change.call((threshold, ratio_for(db_at_y(at.1), threshold, ratio_range))),
    };
    let graph_id = id.clone();

    // the stylesheet goes into the page once; a `document::Style` here logs a warning on every re-render
    use_hook(|| {
        let _ = document::eval(&format!(
            "if (!document.getElementById({id:?})) {{ const s = document.createElement('style'); s.id = {id:?}; s.textContent = {css:?}; document.head.appendChild(s); }}",
            id = "uis-trf-css",
            css = CSS
        ));
    });
    rsx! {
        div {
            class: "uis-trf",
            id: "{id}",
            style: "{style}",
            onpointerdown: move |e| {
                let _ = document::eval(&format!("document.getElementById('{graph_id}')?.setPointerCapture({})", e.data().pointer_id()));
            },
            onmousedown: move |e| {
                e.prevent_default();
                let at = e.element_coordinates();
                let near = |p: (f64, f64)| ((p.0 - at.x).powi(2) + (p.1 - at.y).powi(2)).sqrt();
                let (dk, de) = (near(knee), near(end));
                if dk.min(de) > REACH {
                    return;
                }
                let handle = if de <= dk { Handle::End } else { Handle::Knee };
                grab.set(Some(handle));
                edit(handle, (at.x, at.y));
            },
            onmousemove: move |e| {
                let Some(handle) = *grab.peek() else { return };
                if !e.held_buttons().contains(dioxus::html::input_data::MouseButton::Primary) {
                    grab.set(None);
                    return;
                }
                let at = e.element_coordinates();
                edit(handle, (at.x, at.y));
            },
            onmouseup: move |_| grab.set(None),
            svg { class: "uis-trf-svg", width: "{w}", height: "{h}", view_box: "0 0 {w} {h}",
                path { class: "uis-trf-grid", d: "{grid}", fill: "none" }
                path { class: "uis-trf-diag", d: "{diagonal}", fill: "none" }
                path { class: "uis-trf-curve", d: "{curve}", fill: "none", opacity: if active { "1" } else { "0.4" } }
                circle { class: "uis-trf-dot", cx: "{knee.0:.1}", cy: "{knee.1:.1}", r: "5" }
                circle { class: "uis-trf-dot", cx: "{end.0 - 5.0:.1}", cy: "{end.1:.1}", r: "5" }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_curve_follows_the_diagonal_below_the_threshold_and_bends_above_it() {
        assert_eq!(out_of(-30.0, -20.0, 4.0), -30.0);
        assert_eq!(out_of(0.0, -20.0, 4.0), -15.0);
        assert_eq!(out_of(0.0, -20.0, 1.0), 0.0, "a ratio of 1 changes nothing");
    }

    #[test]
    fn the_right_end_gives_back_the_ratio_it_was_drawn_from() {
        for ratio in [1.0, 2.0, 3.0, 4.0, 10.0, 20.0] {
            let out = out_of(0.0, -20.0, ratio);
            assert_eq!(ratio_for(out, -20.0, (1.0, 20.0)), ratio);
        }
        assert_eq!(ratio_for(-30.0, -20.0, (1.0, 20.0)), 20.0, "below the threshold line is the strongest ratio");
    }
}
