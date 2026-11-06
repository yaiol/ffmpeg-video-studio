// twin: uis-ctl-crop-rotate.jsx jsx@153c3cc17e8b rsx@592a0946d593
//! UisCropRotate - twin of `uis-ctl-crop-rotate.jsx`: a rectangle with eight grips and a move grip drawn over a picture, what
//! lies outside it dimmed, and a handle that turns the picture by any angle round its centre. It covers the picture (it is laid
//! over a box the picture fills) and holds numbers only: the rectangle as four margins and the angle. Same classes and element
//! order, so the paired `uis-ctl-crop-rotate.css` skins it.
//!
//! `rect` is `[left, top, right, bottom]`, the margins as fractions (0 to 1) of the picture's width and height; `None` draws no
//! rectangle. `angle` is the rotation in degrees, clockwise, between -180 (excluded) and 180; `None` draws no rotation handle.
//! `on_rect` says the new margins while a grip is dragged (never past the other side, never outside the picture, never smaller
//! than `min` of the picture); `on_angle` says the new angle (within `snap` degrees of a quarter turn it snaps to it, otherwise it
//! is kept to a tenth of a degree); `on_end` says a drag is over. `shade` false leaves what is outside the rectangle undimmed.
//! The size of the picture on screen is asked of the browser once per press (`get_client_rect`), not at every move. Pointer
//! capture has no Rust API, so it goes through `document::eval` by element id.
//! Differs: the stylesheet is added to the page once by id through `document::eval` (the JSX imports its CSS), because a `document::Style` inside a control logs a warning on every re-render.

use dioxus::prelude::*;
use std::rc::Rc;
use std::sync::atomic::{AtomicU32, Ordering};

const CSS: &str = include_str!("../../assets/uis-ctl-crop-rotate.css");
static NEXT_ID: AtomicU32 = AtomicU32::new(1);

/// The nine grips: name, cursor, and where each sits on the rectangle (0 to 1 across, 0 to 1 down).
const GRIPS: [(&str, &str, f64, f64); 9] = [
    ("tl", "nwse-resize", 0.0, 0.0),
    ("t", "ns-resize", 0.5, 0.0),
    ("tr", "nesw-resize", 1.0, 0.0),
    ("l", "ew-resize", 0.0, 0.5),
    ("c", "move", 0.5, 0.5),
    ("r", "ew-resize", 1.0, 0.5),
    ("bl", "nesw-resize", 0.0, 1.0),
    ("b", "ns-resize", 0.5, 1.0),
    ("br", "nwse-resize", 1.0, 1.0),
];

/// What is held while a drag goes on: which grip (or `spin`), where the pointer was, the margins then, and how large the picture is on screen.
#[derive(Clone, PartialEq)]
struct Grab {
    kind: String,
    from: (f64, f64),
    rect: [f64; 4],
    size: (f64, f64),
    centre: (f64, f64),
}

/// An angle between -180 (excluded) and 180.
fn normal(degrees: f64) -> f64 {
    let a = (degrees + 180.0).rem_euclid(360.0) - 180.0;
    if a <= -180.0 { 180.0 } else { a }
}

/// The margins after the pointer moved by `(dx, dy)` (fractions of the picture) while `grip` is held, from the margins `m` at the press.
fn dragged(grip: &str, m: [f64; 4], dx: f64, dy: f64, min: f64) -> [f64; 4] {
    let [l, t, r, b] = m;
    if grip == "c" {
        let (mx, my) = (dx.clamp(-l, r), dy.clamp(-t, b));
        return [l + mx, t + my, r - mx, b - my];
    }
    let mut out = m;
    if grip.contains('l') {
        out[0] = (l + dx).clamp(0.0, (1.0 - r - min).max(0.0));
    }
    if grip.contains('r') {
        out[2] = (r - dx).clamp(0.0, (1.0 - l - min).max(0.0));
    }
    if grip.contains('t') {
        out[1] = (t + dy).clamp(0.0, (1.0 - b - min).max(0.0));
    }
    if grip.contains('b') {
        out[3] = (b - dy).clamp(0.0, (1.0 - t - min).max(0.0));
    }
    out
}

/// The rotation the pointer asks for: the direction from the centre to `at`, clockwise from straight up; near a quarter turn it snaps to it.
fn angle_at(centre: (f64, f64), at: (f64, f64), snap: f64) -> f64 {
    let degrees = (at.0 - centre.0).atan2(centre.1 - at.1).to_degrees();
    let quarter = (degrees / 90.0).round() * 90.0;
    normal(if (degrees - quarter).abs() < snap { quarter } else { (degrees * 10.0).round() / 10.0 })
}

#[component]
pub fn UisCropRotate(
    #[props(default)] rect: Option<[f64; 4]>,
    #[props(default)] angle: Option<f64>,
    #[props(default)] on_rect: EventHandler<[f64; 4]>,
    #[props(default)] on_angle: EventHandler<f64>,
    #[props(default)] on_end: EventHandler<()>,
    #[props(default = 0.02)] min: f64,
    #[props(default = 3.0)] snap: f64,
    #[props(default = true)] shade: bool,
) -> Element {
    let id = use_hook(|| format!("uis-crr-{}", NEXT_ID.fetch_add(1, Ordering::Relaxed)));
    use_hook(|| {
        let _ = document::eval(&format!(
            "if (!document.getElementById({id:?})) {{ const s = document.createElement('style'); s.id = {id:?}; s.textContent = {css:?}; document.head.appendChild(s); }}",
            id = "uis-crr-css",
            css = CSS
        ));
    });
    let mut mounted = use_signal(|| None::<Rc<MountedData>>);
    let mut grab = use_signal(|| None::<Grab>);

    let press = move |kind: String, at: (f64, f64)| {
        let Some(element) = mounted.peek().clone() else { return };
        let rect = rect.unwrap_or([0.0; 4]);
        spawn(async move {
            if let Ok(r) = element.get_client_rect().await {
                if r.size.width > 0.0 && r.size.height > 0.0 {
                    let centre = (r.origin.x + r.size.width / 2.0, r.origin.y + r.size.height / 2.0);
                    grab.set(Some(Grab { kind, from: at, rect, size: (r.size.width, r.size.height), centre }));
                }
            }
        });
    };
    let mut release = move || {
        if grab.peek().is_some() {
            grab.set(None);
            on_end.call(());
        }
    };
    let moving = move |e: MouseEvent| {
        let Some(g) = grab.peek().clone() else { return };
        if !e.held_buttons().contains(dioxus::html::input_data::MouseButton::Primary) {
            release();
            return;
        }
        let p = e.client_coordinates();
        if g.kind == "spin" {
            on_angle.call(angle_at(g.centre, (p.x, p.y), snap));
        } else {
            on_rect.call(dragged(&g.kind, g.rect, (p.x - g.from.0) / g.size.0, (p.y - g.from.1) / g.size.1, min));
        }
    };
    let capture = |element: String, e: Event<PointerData>| {
        let _ = document::eval(&format!("document.getElementById('{element}')?.setPointerCapture({})", e.data().pointer_id()));
    };

    let pct = |v: f64| format!("{:.3}%", v * 100.0);
    let place = rect.map(|r| format!("left: {}; top: {}; right: {}; bottom: {}", pct(r[0]), pct(r[1]), pct(r[2]), pct(r[3])));
    let turn = angle.map(|a| format!("transform: rotate({a}deg)"));
    let (rect_id, spin_id) = (format!("{id}-c"), format!("{id}-spin"));
    let (rect_capture, spin_capture) = (rect_id.clone(), spin_id.clone());

    rsx! {
        div { class: "uis-crr", onmounted: move |e| mounted.set(Some(e.data())),
            if let (Some(place), true) = (place.clone(), shade) {
                div { class: "uis-crr-clip", div { class: "uis-crr-shade", style: "{place}" } }
            }
            if let Some(place) = place {
                div {
                    class: "uis-crr-rect",
                    id: "{rect_id}",
                    style: "{place}",
                    onpointerdown: move |e| capture(rect_capture.clone(), e),
                    onmousedown: move |e| {
                        e.stop_propagation();
                        e.prevent_default();
                        let p = e.client_coordinates();
                        press("c".to_owned(), (p.x, p.y));
                    },
                    onmousemove: moving,
                    onmouseup: move |_| release(),
                    for (name, cursor, x, y) in GRIPS {
                        div {
                            key: "{name}",
                            id: "{id}-{name}",
                            class: "uis-crr-grip uis-crr-grip-{name}",
                            style: "left: {x * 100.0}%; top: {y * 100.0}%; cursor: {cursor}",
                            onpointerdown: {
                                let element = format!("{id}-{name}");
                                move |e| capture(element.clone(), e)
                            },
                            onmousedown: move |e| {
                                e.stop_propagation();
                                e.prevent_default();
                                let p = e.client_coordinates();
                                press(name.to_owned(), (p.x, p.y));
                            },
                            onmousemove: moving,
                            onmouseup: move |_| release(),
                        }
                    }
                }
            }
            if let Some(turn) = turn {
                div { class: "uis-crr-spin", style: "{turn}",
                    div { class: "uis-crr-spin-frame" }
                    div { class: "uis-crr-spin-line" }
                    div { class: "uis-crr-spin-hub" }
                    div {
                        class: "uis-crr-spin-handle",
                        id: "{spin_id}",
                        onpointerdown: move |e| capture(spin_capture.clone(), e),
                        onmousedown: move |e| {
                            e.stop_propagation();
                            e.prevent_default();
                            let p = e.client_coordinates();
                            press("spin".to_owned(), (p.x, p.y));
                        },
                        onmousemove: moving,
                        onmouseup: move |_| release(),
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn near(a: [f64; 4], b: [f64; 4]) {
        assert!(a.iter().zip(b).all(|(x, y)| (x - y).abs() < 1e-9), "{a:?} is not {b:?}");
    }

    #[test]
    fn a_corner_moves_two_sides_and_never_passes_the_other_side() {
        let m = [0.1, 0.1, 0.1, 0.1];
        near(dragged("tl", m, 0.05, 0.02, 0.02), [0.15, 0.12, 0.1, 0.1]);
        let past = dragged("l", m, 5.0, 0.0, 0.02);
        assert!((past[0] - 0.88).abs() < 1e-9, "the left margin stops a minimum short of the right side");
        assert_eq!(dragged("tl", m, -5.0, -5.0, 0.02)[..2], [0.0, 0.0], "never outside the picture");
    }

    #[test]
    fn the_whole_rectangle_moves_inside_the_picture() {
        let m = [0.1, 0.2, 0.3, 0.1];
        near(dragged("c", m, 0.05, -0.05, 0.02), [0.15, 0.15, 0.25, 0.15]);
        near(dragged("c", m, 5.0, 5.0, 0.02), [0.4, 0.3, 0.0, 0.0]);
    }

    #[test]
    fn the_handle_snaps_to_a_quarter_turn_and_is_kept_to_a_tenth_of_a_degree() {
        let c = (100.0, 100.0);
        assert_eq!(angle_at(c, (100.0, 0.0), 3.0), 0.0, "straight up");
        assert_eq!(angle_at(c, (200.0, 98.0), 3.0), 90.0, "near a quarter turn it snaps");
        assert_eq!(angle_at(c, (100.0, 200.0), 3.0), 180.0, "straight down is 180, never -180");
        let a = angle_at(c, (150.0, 50.0), 3.0);
        assert!((a - 45.0).abs() < 0.06 && (a * 10.0).round() == a * 10.0);
    }
}
