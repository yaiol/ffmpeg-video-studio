// twin: uis-ctl-equalizer.jsx jsx@8ae3e2ea3f97 rsx@e1706c1fa9fc
//! UisEqualizer — twin of `uis-ctl-equalizer.jsx`: the ONE graphic-equaliser control, in two views of the same bands.
//! `view` is `Sliders` (the default: a row of vertical gain sliders, one per band, with the value above and the caption below) or
//! `Graph` (the bands drawn as one smooth curve on a dark graph, frequency across, gain up and down, as in a mixing desk). Same
//! classes and element order as the JSX, so `uis-ctl-equalizer.css` skins it unchanged; both views say the same thing,
//! `on_change((band index, new value))`, only when the value actually changed.
//!
//! Sliders: grab a band and drag (the thumb goes where the pointer is, quantised to `step`), arrow keys move one step,
//! PageUp/PageDown three, Home/End the ends, double-click puts a band back in the middle.
//! Graph: `freqs` is each band's centre frequency in Hz (a logarithmic scale; without it the bands are spaced evenly),
//! `width` / `height` the graph's size in pixels (the pointer is read against it; the labels go in a strip under it). Press
//! anywhere to take the nearest band, drag up or down to set its gain, double-click to bring it back to 0; left/right choose a
//! band, up/down move it one step, PageUp/PageDown three, Home/End the ends.
//!
//! Pointer capture has no Rust API, so it goes through `document::eval` by element id. The sliders' band box (its scale) comes
//! from the mounted element, not from a constant, so the CSS stays the only place its height is written.
//! Differs: the stylesheet is added to the page once by id through `document::eval` (the JSX imports its CSS), because a `document::Style` inside a control logs a warning on every re-render; the graph reads the pointer in the graph's own pixels (`element_coordinates`), the JSX scales by the element's box.

use dioxus::prelude::*;
use std::sync::atomic::{AtomicU32, Ordering};

const CSS: &str = include_str!("../../assets/uis-ctl-equalizer.css");

static NEXT_ID: AtomicU32 = AtomicU32::new(1);

/// The room kept at the left and right of the graph, so the first and the last dot are not on its edge.
const PAD: f64 = 22.0;
/// The room under the graph for the labels.
const FOOT: f64 = 16.0;

/// Which of the two views of the bands is drawn.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum EqView {
    #[default]
    Sliders,
    Graph,
}

/// Quantise to `step`, clamp to the range, and shed float noise (0.30000000000000004).
fn quantise(v: f64, min: f64, max: f64, step: f64) -> f64 {
    let q = ((v - min) / step).round() * step + min;
    ((q * 1e6).round() / 1e6).clamp(min, max)
}

fn shown(v: f64) -> String {
    if v > 0.0 { format!("+{v}") } else { format!("{v}") }
}

/// Where band `i` of `n` sits across a graph `w` pixels wide: on a logarithmic scale (each octave the same width) when every band has a
/// frequency, evenly spaced otherwise.
fn x_of(i: usize, n: usize, freqs: &[f64], w: f64) -> f64 {
    let t = if freqs.len() == n && n > 0 {
        let logs: Vec<f64> = freqs.iter().map(|f| f.ln()).collect();
        let lo = logs.iter().copied().fold(f64::INFINITY, f64::min);
        let hi = logs.iter().copied().fold(f64::NEG_INFINITY, f64::max);
        if hi > lo { (logs[i] - lo) / (hi - lo) } else { 0.5 }
    } else if n > 1 {
        i as f64 / (n - 1) as f64
    } else {
        0.5
    };
    PAD + t * (w - 2.0 * PAD)
}

/// Where a gain sits up a graph `h` pixels high.
fn y_of(gain: f64, min: f64, max: f64, h: f64) -> f64 {
    (max - gain) / (max - min).max(f64::MIN_POSITIVE) * h
}

/// The gain a height on the graph stands for, quantised to `step` and kept in the range.
fn gain_at(y: f64, min: f64, max: f64, h: f64, step: f64) -> f64 {
    quantise(max - (y / h).clamp(0.0, 1.0) * (max - min), min, max, step)
}

/// The band a pointer is nearest to across the graph.
fn band_at(px: f64, n: usize, freqs: &[f64], w: f64) -> usize {
    (0..n).min_by(|a, b| (x_of(*a, n, freqs, w) - px).abs().total_cmp(&(x_of(*b, n, freqs, w) - px).abs())).unwrap_or(0)
}

/// The smooth line through the dots, as an SVG path (a Catmull-Rom spline turned into Bezier segments).
fn path(points: &[(f64, f64)]) -> String {
    let Some(first) = points.first() else { return String::new() };
    let mut d = format!("M{:.1},{:.1}", first.0, first.1);
    for i in 0..points.len().saturating_sub(1) {
        let p0 = points[i.saturating_sub(1)];
        let (p1, p2) = (points[i], points[i + 1]);
        let p3 = points[(i + 2).min(points.len() - 1)];
        let (c1x, c1y) = (p1.0 + (p2.0 - p0.0) / 6.0, p1.1 + (p2.1 - p0.1) / 6.0);
        let (c2x, c2y) = (p2.0 - (p3.0 - p1.0) / 6.0, p2.1 - (p3.1 - p1.1) / 6.0);
        d.push_str(&format!(" C{c1x:.1},{c1y:.1} {c2x:.1},{c2y:.1} {:.1},{:.1}", p2.0, p2.1));
    }
    d
}

#[component]
pub fn UisEqualizer(
    labels: Vec<String>,
    values: Vec<f64>,
    #[props(default)] view: EqView,
    #[props(default)] freqs: Vec<f64>,
    #[props(default = -12.0)] min: f64,
    #[props(default = 12.0)] max: f64,
    #[props(default = 1.0)] step: f64,
    #[props(default)] unit: String,
    #[props(default)] label_unit: String,
    #[props(default = false)] disabled: bool,
    #[props(default)] title: String,
    #[props(default = 420.0)] width: f64,
    #[props(default = 180.0)] height: f64,
    /// `(band index, new value)` - only when the value actually changed.
    on_change: EventHandler<(usize, f64)>,
) -> Element {
    let id = use_hook(|| format!("eq-{}", NEXT_ID.fetch_add(1, Ordering::Relaxed)));
    use_hook(|| {
        let _ = document::eval(&format!(
            "if (!document.getElementById({id:?})) {{ const s = document.createElement('style'); s.id = {id:?}; s.textContent = {css:?}; document.head.appendChild(s); }}",
            id = "uis-eqr-css",
            css = CSS
        ));
    });
    // The slider being held: its index and its box (top, height) on screen, read when it was grabbed.
    let mut drag = use_signal(|| Option::<(usize, f64, f64)>::None);
    // The graph's held band, and the band the keys move.
    let mut grab = use_signal(|| Option::<usize>::None);
    let mut sel = use_signal(|| 0_usize);

    let span = if max > min { max - min } else { 1.0 };
    let mid = ((0.0 - min) / span).clamp(0.0, 1.0);

    if view == EqView::Graph {
        let n = values.len();
        let points: Vec<(f64, f64)> = values.iter().enumerate().map(|(i, v)| (x_of(i, n, &freqs, width), y_of(*v, min, max, height))).collect();
        let line = path(&points);
        let guides = (0..n).map(|i| format!("M{:.1},0 V{height}", x_of(i, n, &freqs, width))).collect::<Vec<_>>().join(" ");
        let rules: Vec<(f64, String)> = [min, min / 2.0, 0.0, max / 2.0, max].iter().map(|g| (y_of(*g, min, max, height), shown(*g))).collect();
        let graph_id = format!("{id}-graph");
        let capture_id = graph_id.clone();
        let class = if disabled { "uis-eqr-graph disabled" } else { "uis-eqr-graph" };
        let style = format!("width: {width}px; height: {}px;", height + FOOT);
        let emit = move |i: usize, v: f64, now: &[f64]| {
            if now.get(i) != Some(&v) {
                on_change.call((i, v));
            }
        };
        let (down_values, move_values, key_values, double_values) = (values.clone(), values.clone(), values.clone(), values.clone());
        let (down_freqs, double_freqs) = (freqs.clone(), freqs.clone());
        let selected = sel().min(n.saturating_sub(1));

        return rsx! {
            div {
                id: "{graph_id}",
                class: "{class}",
                role: "group",
                tabindex: if disabled { "-1" } else { "0" },
                title: if title.is_empty() { None } else { Some(title.clone()) },
                style: "{style}",
                onpointerdown: move |e| {
                    if !disabled {
                        let _ = document::eval(&format!("document.getElementById('{capture_id}')?.setPointerCapture({})", e.data().pointer_id()));
                    }
                },
                onmousedown: move |e| {
                    if disabled || n == 0 {
                        return;
                    }
                    e.prevent_default();
                    let at = e.element_coordinates();
                    let i = band_at(at.x, n, &down_freqs, width);
                    grab.set(Some(i));
                    sel.set(i);
                    emit(i, gain_at(at.y, min, max, height, step), &down_values);
                },
                onmousemove: move |e| {
                    let Some(i) = *grab.peek() else { return };
                    if !e.held_buttons().contains(dioxus::html::input_data::MouseButton::Primary) {
                        grab.set(None);
                        return;
                    }
                    emit(i, gain_at(e.element_coordinates().y, min, max, height, step), &move_values);
                },
                onmouseup: move |_| grab.set(None),
                ondoubleclick: move |e| {
                    if disabled || n == 0 {
                        return;
                    }
                    grab.set(None);
                    emit(band_at(e.element_coordinates().x, n, &double_freqs, width), quantise(0.0, min, max, step), &double_values);
                },
                onkeydown: move |e| {
                    if disabled || n == 0 {
                        return;
                    }
                    let now = selected;
                    let v = key_values[now];
                    let target = match e.key() {
                        Key::ArrowLeft => {
                            e.prevent_default();
                            sel.set(now.saturating_sub(1));
                            return;
                        }
                        Key::ArrowRight => {
                            e.prevent_default();
                            sel.set((now + 1).min(n - 1));
                            return;
                        }
                        Key::ArrowUp => v + step,
                        Key::ArrowDown => v - step,
                        Key::PageUp => v + 3.0 * step,
                        Key::PageDown => v - 3.0 * step,
                        Key::Home => min,
                        Key::End => max,
                        _ => return,
                    };
                    e.prevent_default();
                    emit(now, quantise(target, min, max, step), &key_values);
                },
                svg { class: "uis-eqr-gsvg", width: "{width}", height: "{height + FOOT}", view_box: "0 0 {width} {height + FOOT}",
                    path { class: "uis-eqr-grid", d: "{guides}", fill: "none" }
                    for (y , text) in rules {
                        g { key: "{text}",
                            line { class: if text == "0" { "uis-eqr-rule uis-eqr-rule-zero" } else { "uis-eqr-rule" }, x1: "0", x2: "{width}", y1: "{y}", y2: "{y}" }
                            text { class: "uis-eqr-rule-text", x: "3", y: "{y - 2.0}", "{text}" }
                        }
                    }
                    path { class: "uis-eqr-line", d: "{line}", fill: "none" }
                    for (i , v) in values.iter().copied().enumerate() {
                        g { key: "{i}",
                            circle {
                                class: match (v != 0.0, i == selected) {
                                    (true, true) => "uis-eqr-dot on sel",
                                    (true, false) => "uis-eqr-dot on",
                                    (false, true) => "uis-eqr-dot sel",
                                    (false, false) => "uis-eqr-dot",
                                },
                                cx: "{points[i].0}",
                                cy: "{points[i].1}",
                                r: "4.5",
                            }
                            text { class: "uis-eqr-tick", x: "{points[i].0}", y: "{height + FOOT - 3.0}", "{labels.get(i).cloned().unwrap_or_default()}" }
                        }
                    }
                }
            }
        };
    }

    let class = if disabled { "uis-eqr disabled" } else { "uis-eqr" };

    rsx! {
        div { class: "{class}", role: "group", title: if title.is_empty() { None } else { Some(title.clone()) },
            for (i, v) in values.iter().copied().enumerate() {
                {
                    let frac = ((v - min) / span).clamp(0.0, 1.0);
                    let (lo, hi) = (frac.min(mid), frac.max(mid));
                    let label = labels.get(i).cloned().unwrap_or_default();
                    let caption = if label_unit.is_empty() { label.clone() } else { format!("{label} {label_unit}") };
                    let text = if unit.is_empty() { shown(v) } else { format!("{} {unit}", shown(v)) };
                    let track_id = format!("{id}-{i}");
                    let track_id_down = track_id.clone();
                    rsx! {
                        div { class: "uis-eqr-band", key: "{i}",
                            span { class: "uis-eqr-value", "{shown(v)}" }
                            div {
                                id: "{track_id}",
                                class: "uis-eqr-track",
                                role: "slider",
                                tabindex: if disabled { "-1" } else { "0" },
                                "aria-orientation": "vertical",
                                "aria-label": "{caption}",
                                "aria-valuemin": "{min}",
                                "aria-valuemax": "{max}",
                                "aria-valuenow": "{v}",
                                "aria-valuetext": "{text}",
                                title: "{caption}: {text}",
                                onpointerdown: move |e| {
                                    if disabled {
                                        return;
                                    }
                                    e.prevent_default();
                                    let pointer = e.pointer_id();
                                    let y = e.client_coordinates().y;
                                    let tid = track_id_down.clone();
                                    // The band's box is asked of the page - it is the scale - then the
                                    // pointer's height on it is the value, and the drag carries on.
                                    spawn(async move {
                                        let mut ev = document::eval(&format!(
                                            "const el = document.getElementById('{tid}'); \
                                             if (el) {{ el.setPointerCapture({pointer}); el.focus(); \
                                             const r = el.getBoundingClientRect(); dioxus.send([r.top, r.height]); }} \
                                             else {{ dioxus.send([0, 0]); }}"
                                        ));
                                        if let Ok([top, height]) = ev.recv::<[f64; 2]>().await {
                                            if height > 0.0 {
                                                drag.set(Some((i, top, height)));
                                                let next = quantise(min + (1.0 - (y - top) / height) * span, min, max, step);
                                                if next != v {
                                                    on_change.call((i, next));
                                                }
                                            }
                                        }
                                    });
                                },
                                onpointermove: move |e| {
                                    if let Some((band, top, height)) = drag() {
                                        if band == i {
                                            let next = quantise(
                                                min + (1.0 - (e.client_coordinates().y - top) / height) * span,
                                                min,
                                                max,
                                                step,
                                            );
                                            if next != v {
                                                on_change.call((i, next));
                                            }
                                        }
                                    }
                                },
                                onpointerup: move |_| drag.set(None),
                                onpointercancel: move |_| drag.set(None),
                                ondoubleclick: move |_| {
                                    if !disabled {
                                        let mid_value = quantise(0.0, min, max, step);
                                        if mid_value != v {
                                            on_change.call((i, mid_value));
                                        }
                                    }
                                },
                                onkeydown: move |e| {
                                    if disabled {
                                        return;
                                    }
                                    let target = match e.key() {
                                        Key::ArrowUp | Key::ArrowRight => v + step,
                                        Key::ArrowDown | Key::ArrowLeft => v - step,
                                        Key::PageUp => v + 3.0 * step,
                                        Key::PageDown => v - 3.0 * step,
                                        Key::Home => min,
                                        Key::End => max,
                                        _ => return,
                                    };
                                    e.prevent_default();
                                    let next = quantise(target, min, max, step);
                                    if next != v {
                                        on_change.call((i, next));
                                    }
                                },
                                span { class: "uis-eqr-rail" }
                                span { class: "uis-eqr-zero", style: "bottom: {mid * 100.0}%" }
                                span { class: "uis-eqr-fill", style: "bottom: {lo * 100.0}%; height: {(hi - lo) * 100.0}%" }
                                span { class: "uis-eqr-thumb", style: "bottom: {frac * 100.0}%" }
                            }
                            span { class: "uis-eqr-label", "{label}" }
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FREQS: [f64; 10] = [31.0, 62.0, 125.0, 250.0, 500.0, 1000.0, 2000.0, 4000.0, 8000.0, 16000.0];

    #[test]
    fn the_scale_is_logarithmic_and_the_ends_keep_their_room() {
        let w = 420.0;
        assert!((x_of(0, 10, &FREQS, w) - PAD).abs() < 1e-9 && (x_of(9, 10, &FREQS, w) - (w - PAD)).abs() < 1e-9);
        let (a, b, c) = (x_of(2, 10, &FREQS, w), x_of(3, 10, &FREQS, w), x_of(4, 10, &FREQS, w));
        assert!(((b - a) - (c - b)).abs() < 1e-9, "an octave is the same width everywhere");
        // no frequencies: evenly spaced
        let (a, b, c) = (x_of(0, 3, &[], w), x_of(1, 3, &[], w), x_of(2, 3, &[], w));
        assert!(((b - a) - (c - b)).abs() < 1e-9);
    }

    #[test]
    fn a_height_is_a_gain_inside_the_range_quantised_to_the_step() {
        let h = 180.0;
        assert_eq!(gain_at(h / 2.0, -12.0, 12.0, h, 1.0), 0.0);
        assert_eq!(gain_at(0.0, -12.0, 12.0, h, 1.0), 12.0);
        assert_eq!(gain_at(h * 3.0, -12.0, 12.0, h, 1.0), -12.0, "below the graph is the lowest gain");
        assert_eq!(gain_at(-50.0, -12.0, 12.0, h, 1.0), 12.0);
        assert!((y_of(gain_at(60.0, -12.0, 12.0, h, 1.0), -12.0, 12.0, h) - 60.0).abs() <= h / 24.0, "a gain is drawn within one step of where it was set");
    }

    #[test]
    fn the_pointer_takes_the_nearest_band() {
        let (w, f) = (420.0, &FREQS[..3]);
        assert_eq!(band_at(x_of(1, 3, f, w) + 3.0, 3, f, w), 1);
        assert_eq!(band_at(0.0, 3, f, w), 0);
        assert_eq!(band_at(w, 3, f, w), 2);
        assert_eq!(band_at(10.0, 0, &[], w), 0);
    }

    #[test]
    fn the_line_goes_through_every_dot() {
        let d = path(&[(10.0, 20.0), (50.0, 90.0), (90.0, 40.0)]);
        assert!(d.starts_with("M10.0,20.0") && d.contains("50.0,90.0") && d.ends_with("90.0,40.0"), "{d}");
        assert_eq!(path(&[]), "");
    }
}
