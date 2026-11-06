// twin: uis-num.jsx jsx@c6d688d7f82c rsx@1213c41becd8
//! UisNum — twin of `uis-num.jsx`: a numeric input with themed ▲▼ steppers instead
//! of the browser's own spinner. Same classes and element order (`.uis-num`, `.uis-num-input`,
//! `.uis-num-steppers`, `.uis-num-step`), so `uis-app.css` skins it unchanged.
//!
//! Behaviour is the JSX's: the field keeps its own draft so the user can type freely (including a value below
//! `min` on the way to a larger one), it emits a valid number while typing, and it CLAMPS to `[min, max]` on blur
//! and on the steppers. Any real number: decimals and negatives (the value is `f64`). `allow_empty` lets it be
//! blank, which emits `None` - for "automatic" fields (the JSX emits `''`).
//! `width` is a CSS length (`"56px"`, `"100%"`), as the JSX's string form; `class` is its `className`.
//! `scrub` (units per pixel, 0 = off) makes it a drag control: a left-right arrow over it, press and drag sideways to change the value
//! (snapped to `step`, clamped), a press that does not move focuses the field to type; the steppers are not drawn then. The pointer is
//! captured and locked (so the screen's edge does not stop a drag), and the field focused, by id through `document::eval` (Rust has no API for
//! any of it); the page sums the pointer's movement and sends it back.
//! Differs: behaviour none. A caller's extra attributes are spread onto the input as the JSX's `...rest` is (event
//! handlers go through the same spread); the value is `Option<f64>` where the JSX's is a number or `''`.

use dioxus::prelude::*;
use std::sync::atomic::{AtomicU32, Ordering};

static NEXT_ID: AtomicU32 = AtomicU32::new(1);

#[component]
fn Chevron(up: bool) -> Element {
    let points = if up { "6 15 12 9 18 15" } else { "6 9 12 15 18 9" };
    rsx! {
        svg {
            width: "9",
            height: "9",
            view_box: "0 0 24 24",
            fill: "none",
            stroke: "currentColor",
            stroke_width: "3",
            stroke_linecap: "round",
            stroke_linejoin: "round",
            polyline { points }
        }
    }
}

#[component]
pub fn UisNum(
    value: Option<f64>,
    #[props(default)] min: Option<f64>,
    #[props(default)] max: Option<f64>,
    #[props(default = 1.0)] step: f64,
    /// How many places the field shows: a number is written with exactly that many (0.6 as 0.60) once it is not being typed.
    #[props(default)] decimals: Option<usize>,
    /// A CSS length: `"56px"`, `"100%"`. Empty = the stylesheet's own width.
    #[props(default)] width: String,
    #[props(default = false)] disabled: bool,
    #[props(default = false)] allow_empty: bool,
    /// Units per pixel of a sideways drag; 0 = not a drag control.
    #[props(default = 0.0)] scrub: f64,
    #[props(default)] class: String,
    #[props(default)] title: String,
    on_change: EventHandler<Option<f64>>,
    /// Any attribute of an `<input>` (`placeholder`, `aria-label`, `name` ...), put on the field itself - the JSX's `...rest`.
    #[props(extends = GlobalAttributes, extends = input)]
    attributes: Vec<Attribute>,
) -> Element {
    // (a zero reached from below is -0.0, which would read "-0.00": a zero is written plain)
    let show = move |v: Option<f64>| v.map(|n| if n == 0.0 { 0.0 } else { n }).map(|n| match decimals { Some(d) => format!("{n:.d$}"), None => n.to_string() }).unwrap_or_default();
    let mut draft = use_signal(|| show(value));
    let id = use_hook(|| format!("numfield-{}", NEXT_ID.fetch_add(1, Ordering::Relaxed)));
    let mut focused = use_signal(|| false);
    // (pointer x, value at the press, moved) while the field is held.
    let mut held = use_signal(|| Option::<(f64, f64, bool)>::None);
    // Mirror the controlled value into the draft; a keystroke that was in range already pushed the same value up,
    // so this only differs after a clamp or an outside change.
    // A keystroke already pushed its number up, so the draft is left as typed (not reformatted under the cursor) while it still reads as that number.
    use_effect(use_reactive!(|value| {
        let typed = draft.peek().trim().parse::<f64>().ok();
        if typed.is_none() || typed != value {
            draft.set(show(value));
        }
    }));

    // A number into [min, max]; NaN is blank when blank is allowed, else the minimum (or 0).
    let clamp = move |v: f64| -> Option<f64> {
        if v.is_nan() {
            return if allow_empty { None } else { Some(min.unwrap_or(0.0)) };
        }
        let v = min.map_or(v, |m| v.max(m));
        Some(max.map_or(v, |m| v.min(m)))
    };
    let bump = move |dir: f64| {
        if disabled {
            return;
        }
        // a step is added in binary floating point (0.7 + 0.05 is 0.7499999999999999): round to the decimals the step and the value carry
        let places = |x: f64| {
            let t = x.to_string();
            if t.contains('e') { 0 } else { t.split_once('.').map_or(0, |(_, d)| d.len()) }
        };
        let from = value.unwrap_or(0.0);
        let p = places(step).max(places(from)).min(12) as i32;
        let scale = 10f64.powi(p);
        on_change.call(clamp(((from + dir * step) * scale).round() / scale));
    };

    // A drag of `dx` pixels from the press, as a value: snapped to the step and clamped.
    let mut emitted = use_signal(|| f64::NAN);
    // Past an end the drag starts again from that end: (the distance at that moment, the end's value). Moving back then changes the value at
    // once, instead of first undoing the overshoot.
    let mut anchor = use_signal(|| (0.0_f64, f64::NAN));
    let mut drag_to = move |_x: f64, from: f64, dx: f64| {
        let (dx0, held_from) = anchor();
        let (from, dx) = (if held_from.is_nan() { from } else { held_from }, dx - dx0);
        let q = ((from + dx * scrub) / step).round() * step;
        // round to the decimals the step and the start value carry (0.7 + 0.05 is 0.7499999999999999)
        let places = |x: f64| {
            let t = x.to_string();
            if t.contains('e') { 0 } else { t.split_once('.').map_or(0, |(_, d)| d.len()) }
        };
        let scale = 10f64.powi(places(step).max(places(from)).min(12) as i32);
        let rounded = (q * scale).round() / scale;
        let v = clamp(rounded).map(|n| if n == 0.0 { 0.0 } else { n });
        if let Some(c) = v {
            if c != rounded {
                anchor.set((dx + dx0, c));
            }
        }
        // the same value again is not a change: a locked pointer moves many times a frame
        if v.is_some_and(|n| *emitted.peek() == n) {
            return;
        }
        emitted.set(v.unwrap_or(f64::NAN));
        on_change.call(v);
    };

    let class = format!("uis-num{}{}{}", if scrub != 0.0 { " scrub" } else { "" }, if class.is_empty() { "" } else { " " }, class);
    let (id_down, id_up) = (id.clone(), id.clone());
    let style = if width.is_empty() { None } else { Some(format!("width: {width}")) };

    rsx! {
        div { class: "{class}", style, title: "{title}",
            input {
                id: "{id}",
                class: "uis-num-input",
                r#type: "number",
                value: "{draft}",
                min: min.map(|v| v.to_string()),
                max: max.map(|v| v.to_string()),
                step: "{step}",
                disabled,
                onfocus: move |_| focused.set(true),
                onpointerdown: move |e| {
                    if scrub == 0.0 || disabled || focused() || e.trigger_button() != Some(dioxus::html::input_data::MouseButton::Primary) {
                        return;
                    }
                    e.prevent_default();
                    let x = e.client_coordinates().x;
                    held.set(Some((x, value.unwrap_or(0.0), false)));
                    emitted.set(f64::NAN);
                    anchor.set((0.0, f64::NAN));
                    let pointer = e.pointer_id();
                    let _ = document::eval(&format!("document.getElementById('{id_down}')?.setPointerCapture({pointer});"));
                    // The distance is summed by the page from the movement itself (never from the pointer's position: once the pointer is locked at the
                    // edge of the screen, the position it reports is not the one it left), one total a frame. A press that has not moved 3 px is a click.
                    let mut link = document::eval(&format!(
                        "const el = document.getElementById('{id_down}'); let total = 0, locking = false, done = false, queued = false; const mv = (e) => {{ total += e.movementX; if (!locking && Math.abs(total) >= 3) {{ locking = true; try {{ el.requestPointerLock(); }} catch (_) {{}} }} if (!queued) {{ queued = true; requestAnimationFrame(() => {{ queued = false; if (!done) dioxus.send(total); }}); }} }}; const end = () => {{ if (done) return; done = true; document.removeEventListener('pointermove', mv, true); document.removeEventListener('pointerup', end, true); document.removeEventListener('pointercancel', end, true); if (document.pointerLockElement === el) document.exitPointerLock(); dioxus.send(null); }}; document.addEventListener('pointermove', mv, true); document.addEventListener('pointerup', end, true); document.addEventListener('pointercancel', end, true);"
                    ));
                    spawn(async move {
                        while let Ok(Some(total)) = link.recv::<Option<f64>>().await {
                            if let Some((x0, from, moved)) = held() {
                                if !moved && total.abs() < 3.0 {
                                    continue;
                                }
                                held.set(Some((x0, from, true)));
                                drag_to(x0, from, total);
                            }
                        }
                    });
                },
                onpointerup: move |_| {
                    let was = held.write().take();
                    if let Some((_, _, false)) = was {
                        let _ = document::eval(&format!("const e = document.getElementById('{id_up}'); e?.focus(); e?.select();"));
                    }
                },
                oninput: move |e| {
                    let raw = e.value();
                    draft.set(raw.clone());
                    if allow_empty && raw.trim().is_empty() {
                        on_change.call(None);
                    } else if let Ok(n) = raw.trim().parse::<f64>() {
                        if min.is_none_or(|m| n >= m) && max.is_none_or(|m| n <= m) {
                            on_change.call(Some(n));
                        }
                    }
                },
                onblur: move |_| {
                    focused.set(false);
                    let raw = draft();
                    if allow_empty && raw.trim().is_empty() {
                        on_change.call(None);
                        return;
                    }
                    let n = clamp(raw.trim().parse::<f64>().unwrap_or(f64::NAN));
                    draft.set(show(n));
                    on_change.call(n);
                },
                ..attributes,
            }
            span { class: "uis-num-steppers", "aria-hidden": "true",
                button {
                    class: "uis-num-step",
                    r#type: "button",
                    tabindex: "-1",
                    disabled,
                    onclick: move |_| bump(1.0),
                    Chevron { up: true }
                }
                button {
                    class: "uis-num-step",
                    r#type: "button",
                    tabindex: "-1",
                    disabled,
                    onclick: move |_| bump(-1.0),
                    Chevron { up: false }
                }
            }
        }
    }
}
