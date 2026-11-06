// twin: uis-ctl-curve.jsx jsx@d3c3369a2dcb rsx@b9541b6c31ab
//! UisCurve - twin of `uis-ctl-curve.jsx`: a tone-curve editor as in a grading panel. One graph with a curve per
//! channel and a column of channel buttons beside it. Click on the graph to add a point, drag a point to bend the
//! curve, double-click a point to take it away; the two end points only move up and down. The line drawn is a
//! natural cubic spline through the points. Same classes and element order, so the paired `uis-ctl-curve.css`
//! skins it.
//!
//! `channels` are the curves (id, the letter on its button, its colour); `value` holds the points of each, `x`/`y`
//! from 0 to 1 (a missing or short one is the straight line from the bottom left to the top right); `on_change`
//! says which channel was edited and its new points; the button under the channels puts the shown one back to the
//! straight line. `width` / `height` are the graph's size in pixels (the pointer is read against it).
//! Pointer capture has no Rust API, so it goes through `document::eval` by element id.
//! Differs: the stylesheet is added to the page once by id through `document::eval` (the JSX imports its CSS), because a `document::Style` inside a control logs a warning on every re-render.

use dioxus::prelude::*;
use dioxus_free_icons::{Icon, icons::ld_icons::LdRotateCcw};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, Ordering};

const CSS: &str = include_str!("../../assets/uis-ctl-curve.css");
static NEXT_ID: AtomicU32 = AtomicU32::new(1);
/// How close, in pixels, a click must be to a point to take hold of it.
const REACH: f64 = 9.0;
/// How near two points may come on the horizontal axis.
const APART: f64 = 0.01;

pub type CurvePoints = Vec<(f64, f64)>;

#[derive(Clone, PartialEq)]
pub struct CurveChannel {
    pub id: String,
    /// The letter on the channel's button.
    pub label: String,
    /// The colour of its line and button (any CSS colour).
    pub color: String,
}

/// The straight line from the bottom left to the top right: what a curve is until it is edited.
pub fn straight() -> CurvePoints {
    vec![(0.0, 0.0), (1.0, 1.0)]
}

/// The points as the graph reads them: clamped to 0..1, in order of x, none twice; fewer than two give the straight line.
fn norm(points: Option<&CurvePoints>) -> CurvePoints {
    let mut p: CurvePoints = points.map(|v| v.iter().map(|(x, y)| (x.clamp(0.0, 1.0), y.clamp(0.0, 1.0))).collect()).unwrap_or_default();
    p.sort_by(|a, b| a.0.total_cmp(&b.0));
    p.dedup_by(|a, b| (a.0 - b.0).abs() < 1e-6);
    if p.len() < 2 { straight() } else { p }
}

/// The value of the natural cubic spline through the points at `x` (a straight line between two points).
fn spline(points: &CurvePoints) -> impl Fn(f64) -> f64 + '_ {
    let n = points.len();
    // the second derivatives at the points, natural ends (zero), by the tridiagonal sweep
    let mut m = vec![0.0; n];
    if n > 2 {
        let h: Vec<f64> = (0..n - 1).map(|i| points[i + 1].0 - points[i].0).collect();
        let mut c = vec![0.0; n];
        let mut d = vec![0.0; n];
        for i in 1..n - 1 {
            let (a, b, e) = (h[i - 1], 2.0 * (h[i - 1] + h[i]), h[i]);
            let rhs = 6.0 * ((points[i + 1].1 - points[i].1) / h[i] - (points[i].1 - points[i - 1].1) / h[i - 1]);
            let w = b - a * c[i - 1];
            c[i] = e / w;
            d[i] = (rhs - a * d[i - 1]) / w;
        }
        for i in (1..n - 1).rev() {
            m[i] = d[i] - c[i] * m[i + 1];
        }
    }
    move |x: f64| {
        let i = points.windows(2).position(|w| x <= w[1].0).unwrap_or(n - 2);
        let ((x0, y0), (x1, y1)) = (points[i], points[i + 1]);
        let h = x1 - x0;
        let (a, b) = ((x1 - x) / h, (x - x0) / h);
        (a * y0 + b * y1 + ((a * a * a - a) * m[i] + (b * b * b - b) * m[i + 1]) * h * h / 6.0).clamp(0.0, 1.0)
    }
}

/// The line of a curve as an SVG path.
fn path(points: &CurvePoints, w: f64, h: f64) -> String {
    let f = spline(points);
    (0..=96)
        .map(|k| {
            let x = f64::from(k) / 96.0;
            format!("{}{:.1},{:.1}", if k == 0 { "M" } else { "L" }, x * w, (1.0 - f(x)) * h)
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// The point of the curve under the pointer (in pixels on the graph), if any.
fn point_at(points: &CurvePoints, px: f64, py: f64, w: f64, h: f64) -> Option<usize> {
    points
        .iter()
        .enumerate()
        .map(|(i, (x, y))| (i, ((x * w - px).powi(2) + ((1.0 - y) * h - py).powi(2)).sqrt()))
        .filter(|(_, d)| *d <= REACH)
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(i, _)| i)
}

#[component]
pub fn UisCurve(
    channels: Vec<CurveChannel>,
    #[props(default)] value: HashMap<String, CurvePoints>,
    #[props(default)] reset_title: String,
    #[props(default = 330.0)] width: f64,
    #[props(default = 240.0)] height: f64,
    on_change: EventHandler<(String, CurvePoints)>,
) -> Element {
    let id = use_hook(|| format!("uis-crv-{}", NEXT_ID.fetch_add(1, Ordering::Relaxed)));
    // the stylesheet goes into the page once; a `document::Style` here logs a warning on every re-render
    use_hook(|| {
        let _ = document::eval(&format!(
            "if (!document.getElementById({id:?})) {{ const s = document.createElement('style'); s.id = {id:?}; s.textContent = {css:?}; document.head.appendChild(s); }}",
            id = "uis-crv-css",
            css = CSS
        ));
    });
    let mut active = use_signal(|| 0_usize);
    // the point being dragged: its place in the active curve
    let mut grab = use_signal(|| None::<usize>);
    if channels.is_empty() {
        return rsx! {};
    }
    let (w, h) = (width, height);
    let now = active().min(channels.len() - 1);
    let channel = channels[now].clone();
    let points = norm(value.get(&channel.id));
    let at_rest = points == straight();

    let lines: Vec<Element> = channels
        .iter()
        .enumerate()
        .map(|(i, c)| {
            let d = path(&norm(value.get(&c.id)), w, h);
            let opacity = if i == now { 1.0 } else { 0.4 };
            rsx! { path { key: "{c.id}", d: "{d}", fill: "none", stroke: "{c.color}", "stroke-width": "1.6", opacity: "{opacity}" } }
        })
        .collect();
    let dots: Vec<Element> = points
        .iter()
        .enumerate()
        .map(|(i, (x, y))| rsx! { circle { key: "{i}", class: "uis-crv-dot", cx: "{x * w:.1}", cy: "{(1.0 - y) * h:.1}", r: "4.5", fill: "{channel.color}" } })
        .collect();

    // the grid: quarters
    let grid = format!(
        "M{q1},0 V{h} M{q2},0 V{h} M{q3},0 V{h} M0,{r1} H{w} M0,{r2} H{w} M0,{r3} H{w}",
        q1 = w / 4.0,
        q2 = w / 2.0,
        q3 = w * 0.75,
        r1 = h / 4.0,
        r2 = h / 2.0,
        r3 = h * 0.75
    );
    let diagonal = format!("M0,{h} L{w},0");
    let graph_style = format!("width: {w}px; height: {h}px;");

    let down_points = points.clone();
    let move_points = points.clone();
    let double_points = points.clone();
    let (down_id, move_id, double_id, reset_id) = (channel.id.clone(), channel.id.clone(), channel.id.clone(), channel.id.clone());
    let graph_id = id.clone();

    rsx! {
        div { class: "uis-crv",
            div {
                class: "uis-crv-graph",
                id: "{graph_id}",
                style: "{graph_style}",
                onpointerdown: move |e| {
                    let _ = document::eval(&format!("document.getElementById('{graph_id}')?.setPointerCapture({})", e.data().pointer_id()));
                },
                onmousedown: move |e| {
                    e.prevent_default();
                    let at = e.element_coordinates();
                    let mut pts = down_points.clone();
                    if let Some(i) = point_at(&pts, at.x, at.y, w, h) {
                        grab.set(Some(i));
                        return;
                    }
                    let (x, y) = ((at.x / w).clamp(0.0, 1.0), (1.0 - at.y / h).clamp(0.0, 1.0));
                    if pts.iter().any(|p| (p.0 - x).abs() < APART) {
                        return;
                    }
                    let i = pts.iter().position(|p| p.0 > x).unwrap_or(pts.len());
                    pts.insert(i, (x, y));
                    grab.set(Some(i));
                    on_change.call((down_id.clone(), pts));
                },
                onmousemove: move |e| {
                    let Some(i) = *grab.peek() else { return };
                    if !e.held_buttons().contains(dioxus::html::input_data::MouseButton::Primary) {
                        grab.set(None);
                        return;
                    }
                    let at = e.element_coordinates();
                    let mut pts = move_points.clone();
                    if i >= pts.len() {
                        return;
                    }
                    let last = pts.len() - 1;
                    let y = (1.0 - at.y / h).clamp(0.0, 1.0);
                    let x = if i == 0 || i == last { pts[i].0 } else { (at.x / w).clamp(pts[i - 1].0 + APART, pts[i + 1].0 - APART) };
                    pts[i] = (x, y);
                    on_change.call((move_id.clone(), pts));
                },
                onmouseup: move |_| grab.set(None),
                ondoubleclick: move |e| {
                    let at = e.element_coordinates();
                    let mut pts = double_points.clone();
                    if let Some(i) = point_at(&pts, at.x, at.y, w, h).filter(|i| *i != 0 && *i + 1 != pts.len()) {
                        pts.remove(i);
                        grab.set(None);
                        on_change.call((double_id.clone(), pts));
                    }
                },
                svg { class: "uis-crv-svg", width: "{w}", height: "{h}", view_box: "0 0 {w} {h}",
                    path { class: "uis-crv-grid", d: "{grid}", fill: "none" }
                    path { class: "uis-crv-diag", d: "{diagonal}", fill: "none" }
                    {lines.into_iter()}
                    {dots.into_iter()}
                }
            }
            div { class: "uis-crv-side",
                for (i , c) in channels.iter().enumerate() {
                    button {
                        key: "{c.id}",
                        class: if now == i { "uis-btn ico uis-crv-chan active" } else { "uis-btn ico uis-crv-chan" },
                        style: "--c: {c.color}",
                        "aria-pressed": "{now == i}",
                        onclick: move |_| {
                            grab.set(None);
                            active.set(i);
                        },
                        "{c.label}"
                    }
                }
                button {
                    class: "uis-btn ico",
                    title: "{reset_title}",
                    "aria-label": "{reset_title}",
                    disabled: at_rest,
                    onclick: move |_| on_change.call((reset_id.clone(), straight())),
                    Icon { width: 14, height: 14, icon: LdRotateCcw }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_spline_goes_through_its_points_and_is_a_straight_line_between_two() {
        let two = straight();
        let f = spline(&two);
        assert!((f(0.25) - 0.25).abs() < 1e-9 && (f(0.8) - 0.8).abs() < 1e-9);
        let bent = vec![(0.0, 0.0), (0.5, 0.7), (1.0, 1.0)];
        let g = spline(&bent);
        assert!(g(0.0).abs() < 1e-9 && (g(0.5) - 0.7).abs() < 1e-9 && (g(1.0) - 1.0).abs() < 1e-9);
        assert!(g(0.25) > 0.25, "the curve bows above the line: {}", g(0.25));
    }

    #[test]
    fn a_point_is_taken_only_when_the_pointer_is_near_it() {
        let p = vec![(0.0, 0.0), (0.5, 0.5), (1.0, 1.0)];
        let (w, h) = (330.0, 240.0);
        assert_eq!(point_at(&p, w * 0.5 + 3.0, h * 0.5 - 2.0, w, h), Some(1));
        assert_eq!(point_at(&p, w * 0.25, h * 0.75, w, h), None);
        assert_eq!(point_at(&p, 0.0, h, w, h), Some(0), "the bottom left end");
    }

    #[test]
    fn the_points_are_read_in_order_clamped_and_short_ones_are_the_straight_line() {
        assert_eq!(norm(None), straight());
        assert_eq!(norm(Some(&vec![(0.3, 0.3)])), straight());
        assert_eq!(norm(Some(&vec![(1.0, 1.0), (0.5, 0.6), (0.0, 0.0)])), vec![(0.0, 0.0), (0.5, 0.6), (1.0, 1.0)]);
        assert_eq!(norm(Some(&vec![(-1.0, 2.0), (2.0, -1.0)])), vec![(0.0, 1.0), (1.0, 0.0)]);
    }
}
