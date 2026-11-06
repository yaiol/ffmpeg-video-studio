// twin: uis-ctl-yrgb.jsx jsx@4f1aeee21ebf rsx@234525dec6af
//! UisYrgb - twin of `uis-ctl-yrgb.jsx`: the master and the red, green and blue controls of a colour grade (lift, gamma,
//! gain, offset), shown either as four vertical sliders side by side on one neutral line or as a colour wheel, with the four
//! number boxes and their coloured underlines under both. Same classes and element order, so the paired `uis-ctl-yrgb.css`
//! skins it. The heading, the colour picker, the reset and the switch between the two views are the app's.
//!
//! `bars` are the four controls, master first, then red, green, blue (id, label, colour, min, max, step, neutral); `values`
//! maps a bar id to its number (a missing one is the neutral value); `on_change` says which bars moved and their new values, always as a
//! LIST (a drag on the wheel moves the three colour bars at once). `view` is `Bars` (default) or `Wheel`.
//! Bars: a slider with `min` 0 and a neutral above it is a gain: one-sided, filled from the bottom, its rest position a
//! little under the line; every other bar is two-sided around its neutral value, which sits on the line. Double-click a
//! slider to put it back to neutral; the number box is a drag field. Wheel: the angle is the colour and the distance from
//! the middle how much of it; the three colour bars move by amounts that add up to nothing, so the master is left alone; the
//! dot at the rim is an eighth of the range. `size` is the sliders' travel and the wheel's diameter in pixels.
//! Pointer capture has no Rust API, so the wheel's goes through `document::eval` by element id.
//! Differs: the stylesheet is added to the page once by id through `document::eval` (the JSX imports its CSS), because a `document::Style` inside a control logs a warning on every re-render; `view` is an enum here, a string there.

use super::UisNum;
use dioxus::prelude::*;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, Ordering};

const CSS: &str = include_str!("../../assets/uis-ctl-yrgb.css");
static NEXT_ID: AtomicU32 = AtomicU32::new(1);

#[derive(Clone, Debug, PartialEq)]
pub struct YrgbBar {
    pub id: String,
    pub label: String,
    /// The slider's fill and underline (any CSS colour).
    pub color: String,
    pub min: f64,
    pub max: f64,
    pub step: f64,
    pub neutral: f64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum YrgbView {
    #[default]
    Bars,
    Wheel,
}

/// Where the neutral value rests on a slider, 0..1.
fn rest(b: &YrgbBar) -> f64 {
    if b.min == 0.0 && b.neutral > 0.0 { 0.45 } else { 0.5 }
}

/// Where a value sits on a slider, 0..1: the neutral value is at `rest`, the two parts are scaled on their own.
fn to_pos(b: &YrgbBar, value: f64) -> f64 {
    let (v, at) = (value.clamp(b.min, b.max), rest(b));
    if v <= b.neutral {
        if b.neutral <= b.min { at } else { at * (v - b.min) / (b.neutral - b.min) }
    } else {
        at + (1.0 - at) * (v - b.neutral) / (b.max - b.neutral)
    }
}

/// The value at a slider position: the rest position (a little around it too) is the neutral value, any other is rounded to the step.
fn from_pos(b: &YrgbBar, pos: f64) -> f64 {
    let (p, at) = (pos.clamp(0.0, 1.0), rest(b));
    if (p - at).abs() < 0.008 {
        return b.neutral;
    }
    let v = if p < at { b.min + (b.neutral - b.min) * (p / at) } else { b.neutral + (b.max - b.neutral) * ((p - at) / (1.0 - at)) };
    ((v / b.step).round() * b.step).clamp(b.min, b.max)
}

/// As many decimals as the step needs.
fn decimals_of(step: f64) -> usize {
    if step >= 1.0 { 0 } else if step >= 0.1 { 1 } else if step >= 0.01 { 2 } else { 3 }
}

/// How far the red, green and blue move from their neutral values with the dot at the rim: an eighth of the range.
fn reach(b: &YrgbBar) -> f64 {
    (b.max - b.min) / 8.0
}

/// Where the dot sits, -1..1 in both directions (up is positive). The three colour bars hold three numbers, a wheel shows only their
/// difference from each other: the part they have in common (the master's job) is not a colour, so it is left out.
fn wheel_dot(rgb: &[YrgbBar], now: impl Fn(&YrgbBar) -> f64) -> (f64, f64) {
    let k = reach(&rgb[0]);
    let d: Vec<f64> = rgb.iter().map(|b| now(b) - b.neutral).collect();
    let (x, y) = ((2.0 * d[0] - d[1] - d[2]) / 3.0 / k, (d[1] - d[2]) / (3f64.sqrt() * k));
    let len = x.hypot(y);
    if len > 1.0 { (x / len, y / len) } else { (x, y) }
}

/// The three values a dot at this place makes: the angle is the colour, the distance from the middle how much of it; they add up to nothing.
fn wheel_values(rgb: &[YrgbBar], at: (f64, f64)) -> Vec<f64> {
    let k = reach(&rgb[0]);
    let len = at.0.hypot(at.1);
    let (x, y) = if len > 1.0 { (at.0 / len, at.1 / len) } else { at };
    let half = 3f64.sqrt() / 2.0;
    let d = [x, -x / 2.0 + y * half, -x / 2.0 - y * half];
    rgb.iter().zip(d).map(|(b, d)| (b.neutral + d * k).clamp(b.min, b.max)).collect()
}

#[component]
pub fn UisYrgb(
    bars: Vec<YrgbBar>,
    #[props(default)] values: HashMap<String, f64>,
    #[props(default)] view: YrgbView,
    #[props(default = 92.0)] size: f64,
    on_change: EventHandler<Vec<(String, f64)>>,
) -> Element {
    let id = use_hook(|| format!("uis-yrgb-{}", NEXT_ID.fetch_add(1, Ordering::Relaxed)));
    use_hook(|| {
        let _ = document::eval(&format!(
            "if (!document.getElementById({id:?})) {{ const s = document.createElement('style'); s.id = {id:?}; s.textContent = {css:?}; document.head.appendChild(s); }}",
            id = "uis-yrgb-css",
            css = CSS
        ));
    });
    let mut grab = use_signal(|| false);
    let now = |b: &YrgbBar| values.get(&b.id).copied().unwrap_or(b.neutral);
    let style = format!("--yrgb-size: {size}px");
    let half = size / 2.0;
    let wheel_here = view == YrgbView::Wheel && bars.len() == 4;

    let picture = if wheel_here {
        let rgb: Vec<YrgbBar> = bars[1..].to_vec();
        let (x, y) = wheel_dot(&rgb, now);
        let dot = format!("left: {:.1}px; top: {:.1}px", half + x * half, half - y * half);
        let go = EventHandler::new({
            let rgb = rgb.clone();
            move |p: (f64, f64)| {
                let v = wheel_values(&rgb, ((p.0 - half) / half, -(p.1 - half) / half));
                on_change.call(rgb.iter().zip(v).map(|(b, v)| (b.id.clone(), v)).collect());
            }
        });
        let centre = EventHandler::new({
            let rgb = rgb.clone();
            move |_: ()| {
                let v = wheel_values(&rgb, (0.0, 0.0));
                on_change.call(rgb.iter().zip(v).map(|(b, v)| (b.id.clone(), v)).collect());
            }
        });
        let (wheel_id, capture) = (id.clone(), id.clone());
        rsx! {
            div {
                class: "uis-yrgb-wheel",
                id: "{wheel_id}",
                onpointerdown: move |e| {
                    let _ = document::eval(&format!("document.getElementById('{capture}')?.setPointerCapture({})", e.data().pointer_id()));
                },
                onmousedown: move |e| {
                    e.prevent_default();
                    grab.set(true);
                    let at = e.element_coordinates();
                    go.call((at.x, at.y));
                },
                onmousemove: move |e| {
                    if !*grab.peek() {
                        return;
                    }
                    if !e.held_buttons().contains(dioxus::html::input_data::MouseButton::Primary) {
                        grab.set(false);
                        return;
                    }
                    let at = e.element_coordinates();
                    go.call((at.x, at.y));
                },
                onmouseup: move |_| grab.set(false),
                ondoubleclick: move |_| centre.call(()),
                div { class: "uis-yrgb-dot", style: "{dot}" }
            }
        }
    } else {
        let slots: Vec<Element> = bars
            .iter()
            .map(|b| {
                let pos = to_pos(b, now(b));
                let at = rest(b);
                let (lo, hi) = if at == 0.5 { (pos.min(at) * 100.0, pos.max(at) * 100.0) } else { (0.0, pos * 100.0) };
                let range_style = format!("--a: {lo:.2}%; --b: {hi:.2}%");
                let slot_style = format!("--c: {}", b.color);
                let (b_in, b_dbl) = (b.clone(), b.clone());
                rsx! {
                    div { key: "{b.id}", class: "uis-yrgb-slot", title: "{b.label}", style: "{slot_style}",
                        input {
                            class: "uis-yrgb-range",
                            r#type: "range",
                            min: "0",
                            max: "1",
                            step: "0.001",
                            value: "{pos}",
                            "aria-label": "{b.label}",
                            style: "{range_style}",
                            oninput: move |e| {
                                if let Ok(p) = e.value().parse::<f64>() {
                                    on_change.call(vec![(b_in.id.clone(), from_pos(&b_in, p))]);
                                }
                            },
                            ondoubleclick: move |_| on_change.call(vec![(b_dbl.id.clone(), b_dbl.neutral)]),
                        }
                    }
                }
            })
            .collect();
        rsx! { div { class: "uis-yrgb-bars", {slots.into_iter()} } }
    };

    let reads: Vec<Element> = bars
        .iter()
        .map(|b| {
            let read_style = format!("--c: {}", b.color);
            let b = b.clone();
            rsx! {
                div { key: "{b.id}", class: "uis-yrgb-read uis-cmp-numread", title: "{b.label}", style: "{read_style}",
                    UisNum {
                        value: Some(now(&b)),
                        min: Some(b.min),
                        max: Some(b.max),
                        step: b.step,
                        decimals: Some(decimals_of(b.step)),
                        scrub: (b.max - b.min) / 160.0,
                        on_change: {
                            let b = b.clone();
                            move |n: Option<f64>| {
                                if let Some(x) = n {
                                    on_change.call(vec![(b.id.clone(), x.clamp(b.min, b.max))]);
                                }
                            }
                        },
                    }
                    div { class: "uis-yrgb-under" }
                }
            }
        })
        .collect();

    rsx! {
        div { class: "uis-yrgb", style: "{style}",
            {picture}
            div { class: "uis-yrgb-reads", {reads.into_iter()} }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bar(id: &str, min: f64, max: f64, neutral: f64) -> YrgbBar {
        YrgbBar { id: id.into(), label: String::new(), color: String::new(), min, max, step: 0.01, neutral }
    }

    #[test]
    fn a_step_decides_the_decimals_shown() {
        assert_eq!((decimals_of(1.0), decimals_of(0.1), decimals_of(0.01), decimals_of(0.001)), (0, 1, 2, 3));
    }

    #[test]
    fn the_neutral_value_rests_where_the_slider_says_and_the_positions_round_trip() {
        let gain = bar("g", 0.0, 4.0, 1.0);
        let lift = bar("l", -1.0, 1.0, 0.0);
        assert_eq!(to_pos(&lift, 0.0), 0.5);
        assert_eq!(to_pos(&gain, 1.0), 0.45, "a gain rests a little under the line, filled from the bottom");
        assert_eq!((to_pos(&gain, 0.0), to_pos(&gain, 4.0)), (0.0, 1.0));
        assert_eq!(from_pos(&gain, 0.45), 1.0);
        assert_eq!(from_pos(&gain, 0.453), 1.0, "a little around the rest position is the neutral value");
        for v in [0.0, 0.25, 0.9, 1.5, 2.5, 4.0] {
            assert!((from_pos(&gain, to_pos(&gain, v)) - v).abs() < 0.011, "{v}");
        }
    }

    #[test]
    fn a_wheel_dot_is_a_colour_that_leaves_the_master_alone_and_reads_back() {
        let rgb: Vec<YrgbBar> = ["r", "g", "b"].iter().map(|c| bar(c, -1.0, 1.0, 0.0)).collect();
        let v = wheel_values(&rgb, (0.5, 0.3));
        assert!(v.iter().sum::<f64>().abs() < 1e-9, "the three move by amounts that add up to nothing");
        let held: HashMap<String, f64> = rgb.iter().zip(&v).map(|(b, v)| (b.id.clone(), *v)).collect();
        let (x, y) = wheel_dot(&rgb, |b| held[&b.id]);
        assert!((x - 0.5).abs() < 1e-9 && (y - 0.3).abs() < 1e-9);
        assert_eq!(wheel_values(&rgb, (0.0, 0.0)), vec![0.0, 0.0, 0.0]);
    }
}
