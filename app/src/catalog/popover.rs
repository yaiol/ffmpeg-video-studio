// twin: uis-ctl-popover.jsx jsx@cebca24f981e rsx@f19b1a2e75b1
//! UisPopover — twin of `uis-ctl-popover.jsx`: the ONE floating-panel MECHANISM of the popover family. It
//! positions a `.uis-pop-surface` against an anchor element: below it (flipping above when there is no
//! room), width floored at the anchor (optional) and capped, anchored to the left or right edge so it
//! stays on screen, repositioned on scroll/resize, closed on Esc and on a click outside. It owns
//! POSITION + DISMISS only, not the panel's contents. `UisMenu`, `UisCboStandard`, `UisPkrColor` and `UisTableGrid`
//! are built on it.
//!
//! `placement: Side` puts the panel BESIDE the anchor (a submenu flyout), flipping to the other side
//! when there is no room. `ignore_selector` exempts a selector from click-away, so a child panel does
//! not dismiss its parent out from under a click. `on_mouse_enter` / `on_mouse_leave` let a
//! hover-opened flyout stay open while the pointer is over itself.
//!
//! The anchor is named by its element id (`anchor_id`): the caller gives its trigger an `id`.
//! The placement MATH is Rust (below), and the page is asked for what rsx cannot do: the anchor's box, the
//! document-level events (a press outside, Esc, scroll, resize), and the PORTAL. The JSX renders the panel into
//! `document.body`; rsx has no portal, so once the panel is in the page the bridge moves its node to the end of
//! `<body>` (a node keeps its id, so the vdom still updates and removes it) and forwards the bubbling events of
//! that node to the interpreter's handler, because Dioxus listens on its root element and a node outside the root
//! would otherwise hear nothing. That is the one place this control touches the JS bridge.
//! Differs: behaviour none. Form only: the trigger is named by id (`anchor_id`) because rsx has no element refs, and
//! the portal is done after the render (the node is created in place, then moved), so a `<!---->` placeholder the
//! vdom keeps may sit in `<body>` while the panel is closed; the side the panel is not placed from is written `auto`
//! where the JSX leaves it out (Dioxus never drops a property from a rewritten style).

// Shared by every app: one that uses the menu but not the pickers would otherwise warn on `use_dismiss`
// and the `Left` alignment, which only the pickers and some callers use.
#![allow(dead_code)]

use dioxus::document::Eval;
use dioxus::prelude::*;
use serde::{Deserialize, Serialize};
use std::cell::Cell;
use std::rc::Rc;
use std::sync::atomic::{AtomicU32, Ordering};

static NEXT_ID: AtomicU32 = AtomicU32::new(1);

const MARGIN: f64 = 8.0; // breathing room kept between the panel and the window edge
const MIN_PANEL: f64 = 160.0; // never squeeze a panel below this, even in a cramped window
const FLIP_BELOW: f64 = 240.0; // less room than this under the anchor -> consider opening upwards

/// Which edge of the anchor a `Below` panel lines up with. `Auto` = right-half anchors right, else left.
#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub enum UisPopAlign {
    #[default]
    Auto,
    Left,
    Right,
}

/// `Below` = a dropdown under its anchor. `Side` = a submenu flyout beside its row.
#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub enum UisPopPlacement {
    #[default]
    Below,
    Side,
}

/// What the page-side bridge needs to know, resent whenever it changes.
#[derive(Clone, PartialEq, Serialize)]
struct Cfg {
    open: bool,
    anchor: String,
    ignore: String,
}

/// A message from the page: the anchor's box and the window's size.
#[derive(Deserialize, Default)]
struct Msg {
    #[serde(default)]
    t: String,
    #[serde(default)]
    l: f64,
    #[serde(default)]
    tp: f64,
    #[serde(default)]
    r: f64,
    #[serde(default)]
    b: f64,
    #[serde(default)]
    w: f64,
    #[serde(default)]
    ww: f64,
    #[serde(default)]
    wh: f64,
}

#[derive(Clone, Copy, PartialEq)]
struct Rect {
    left: f64,
    top: f64,
    right: f64,
    bottom: f64,
    width: f64,
    win_w: f64,
    win_h: f64,
}

/// The page side: runs for the popover's whole life, reports the anchor's box (on open, scroll and
/// resize) and a press outside / Esc while it is open. `anchor` is looked up by id each time, so a
/// trigger that re-renders is still found.
const BRIDGE: &str = r#"
let cfg = null;
const place = () => {
  if (!cfg || !cfg.open) return;
  const a = document.getElementById(cfg.anchor);
  if (!a) return;
  const r = a.getBoundingClientRect();
  dioxus.send({ t: 'rect', l: r.left, tp: r.top, r: r.right, b: r.bottom, w: r.width,
                ww: window.innerWidth, wh: window.innerHeight });
};
const onDown = (e) => {
  if (!cfg || !cfg.open) return;
  const a = document.getElementById(cfg.anchor);
  if (a && a.contains(e.target)) return;
  if (e.target && e.target.closest && e.target.closest('.uis-pop-surface')) {
    // inside SOME panel; it is ours only if the press is inside the panel that follows this anchor
    const mine = document.querySelector('.uis-pop-surface[data-anchor="' + cfg.anchor + '"]');
    if (mine && mine.contains(e.target)) return;
  }
  if (cfg.ignore && e.target && e.target.closest && e.target.closest(cfg.ignore)) return;
  dioxus.send({ t: 'close' });
};
const onKey = (e) => { if (cfg && cfg.open && e.key === 'Escape') dioxus.send({ t: 'close' }); };
document.addEventListener('mousedown', onDown);
document.addEventListener('keydown', onKey);
window.addEventListener('scroll', place, true);
window.addEventListener('resize', place);
try {
  while (true) {
    const m = await dioxus.recv();
    if (m && m.cmd === 'dispose') break;
    cfg = m;
    place();
  }
} finally {
  document.removeEventListener('mousedown', onDown);
  document.removeEventListener('keydown', onKey);
  window.removeEventListener('scroll', place, true);
  window.removeEventListener('resize', place);
}
"#;

/// The portal: move the panel's node to the end of `<body>` and forward its bubbling events to the interpreter
/// (Dioxus listens on its root element, which no longer contains the node). Retries until the vdom's mutation has
/// reached the page. `__ID__` is the panel's element id.
const PORTAL: &str = r#"
(function () {
  const id = "__ID__";
  const types = ['click', 'dblclick', 'mousedown', 'mouseup', 'mousemove', 'mouseover', 'mouseout', 'contextmenu',
    'pointerdown', 'pointerup', 'pointermove', 'pointerover', 'pointerout', 'keydown', 'keyup', 'input', 'change',
    'focusin', 'focusout', 'wheel'];
  let tries = 0;
  const go = () => {
    const n = document.getElementById(id);
    if (!n) { if (++tries < 40) requestAnimationFrame(go); return; }
    if (n.parentNode !== document.body) document.body.appendChild(n);
    if (!n.__uisFwd) {
      n.__uisFwd = true;
      for (const t of types) n.addEventListener(t, (e) => window.interpreter.handler(e));
    }
  };
  go();
})();
"#;

/// Moves the element with this id to the end of `<body>` once it is in the page and while `active` (see the
/// module notes: the JSX portals the same panels), and removes it if it is still there when the owner goes
/// away. Call it once per component, every render, before any early return.
pub(super) fn use_portal(id: String, active: bool) {
    {
        let id = id.clone();
        use_effect(use_reactive!(|(active,)| {
            if active {
                let _ = document::eval(&PORTAL.replace("__ID__", &id));
            }
        }));
    }
    use_drop(move || {
        let _ = document::eval(&format!("{{ const n = document.getElementById('{id}'); if (n && n.parentNode === document.body) n.remove(); }}"));
    });
}

/// The inline style of the panel: the JSX's placement math, line for line.
/// ⚠ All four sides are always written, the unused one as `auto`: Dioxus keeps a property the new string no longer sets, so a panel
/// that flipped while open would otherwise stay pinned to both sides. (`min-width` follows a prop that does not change while mounted.)
#[allow(clippy::too_many_arguments)]
fn placement_style(
    r: Rect,
    align: UisPopAlign,
    placement: UisPopPlacement,
    min_width_anchor: bool,
    max_width: f64,
    max_height: Option<f64>,
) -> String {
    let mw = max_width.min(r.win_w - 16.0);

    // The height cap is the ROOM ON SCREEN, not a constant; only an explicit `max_height` caps a panel
    // below the room on purpose (a long list stays a short list).
    let room_below = r.win_h - r.bottom - MARGIN * 2.0;
    let room_above = r.top - MARGIN * 2.0;
    let fit = |room: f64| match max_height {
        Some(h) => MIN_PANEL.max(h.min(room)),
        None => MIN_PANEL.max(room),
    };

    // 'side' - a submenu opens BESIDE its parent row and flips to the other side when there is no room.
    // Edges touch deliberately (no gap), so the pointer can travel from the row into the flyout.
    if placement == UisPopPlacement::Side {
        let flip_left = r.right + mw + MARGIN > r.win_w && r.left > mw;
        // A flyout hangs DOWN from its row, so its room is everything below that row's top.
        let cap = fit(r.win_h - r.top - MARGIN * 2.0);
        let horizontal = if flip_left {
            format!("left: auto; right: {}px;", MARGIN.max(r.win_w - r.left))
        } else {
            format!("left: {}px; right: auto;", r.right)
        };
        // -4 cancels .uis-pop-surface's padding so the first row lines up with the parent row.
        let top = MARGIN.max((r.top - 4.0).min(r.win_h - cap - MARGIN));
        return format!("max-width: {mw}px; max-height: {cap}px; {horizontal} top: {top}px; bottom: auto;");
    }

    // Flip up only when below is genuinely cramped AND above is roomier.
    let go_up = room_below < max_height.unwrap_or(FLIP_BELOW).min(FLIP_BELOW) && room_above > room_below;
    let anchor_right = align == UisPopAlign::Right || (align == UisPopAlign::Auto && r.left > r.win_w / 2.0);
    let cap = fit(if go_up { room_above } else { room_below });
    let min_w = if min_width_anchor { format!("min-width: {}px; ", r.width) } else { String::new() };
    let horizontal = if anchor_right {
        format!("left: auto; right: {}px;", MARGIN.max(r.win_w - r.right))
    } else {
        format!("left: {}px; right: auto;", r.left)
    };
    let vertical = if go_up {
        format!("top: auto; bottom: {}px;", r.win_h - r.top + 4.0)
    } else {
        format!("top: {}px; bottom: auto;", r.bottom + 4.0)
    };
    format!("max-width: {mw}px; max-height: {cap}px; {min_w}{horizontal} {vertical}")
}

#[component]
pub fn UisPopover(
    /// The `id` of the trigger element the panel is placed against.
    anchor_id: String,
    open: bool,
    on_close: EventHandler<()>,
    #[props(default)] align: UisPopAlign,
    #[props(default)] placement: UisPopPlacement,
    /// Floor the panel width at the anchor's width (pickers do; menus don't).
    #[props(default = false)] min_width_anchor: bool,
    #[props(default = 400.0)] max_width: f64,
    /// `None` = fit the viewport (cap at the room on the chosen side).
    #[props(default)] max_height: Option<f64>,
    /// Click-away exception: a press inside this selector does NOT dismiss.
    #[props(default)] ignore_selector: String,
    #[props(default)] on_mouse_enter: Option<EventHandler<()>>,
    #[props(default)] on_mouse_leave: Option<EventHandler<()>>,
    #[props(default)] class: String,
    children: Element,
) -> Element {
    let pop_id = use_hook(|| format!("pop-{}", NEXT_ID.fetch_add(1, Ordering::Relaxed)));
    let mut rect = use_signal(|| Option::<Rect>::None);
    // The bridge handle, outside any signal so the cleanup below can still reach it as the component
    // goes away (`Eval::send` on a finished eval is an error, never a panic).
    let bridge = use_hook(|| Rc::new(Cell::new(Option::<Eval>::None)));
    let latest_cfg = use_hook(|| Rc::new(std::cell::RefCell::new(Option::<Cfg>::None)));
    let close = use_callback(move |_: ()| on_close.call(()));

    {
        let bridge = bridge.clone();
        let latest_cfg = latest_cfg.clone();
        use_future(move || {
            let bridge = bridge.clone();
            let latest_cfg = latest_cfg.clone();
            async move {
                let mut ev = document::eval(BRIDGE);
                bridge.set(Some(ev));
                if let Some(c) = latest_cfg.borrow().clone() {
                    let _ = ev.send(&c);
                }
                while let Ok(msg) = ev.recv::<Msg>().await {
                    match msg.t.as_str() {
                        "rect" => rect.set(Some(Rect {
                            left: msg.l,
                            top: msg.tp,
                            right: msg.r,
                            bottom: msg.b,
                            width: msg.w,
                            win_w: msg.ww,
                            win_h: msg.wh,
                        })),
                        "close" => close.call(()),
                        _ => {}
                    }
                }
            }
        });
    }
    {
        let bridge = bridge.clone();
        use_drop(move || {
            if let Some(ev) = bridge.get() {
                let _ = ev.send(serde_json::json!({ "cmd": "dispose" }));
            }
        });
    }

    let cfg = Cfg { open, anchor: anchor_id.clone(), ignore: ignore_selector.clone() };
    {
        let bridge = bridge.clone();
        let latest_cfg = latest_cfg.clone();
        use_effect(use_reactive!(|(cfg,)| {
            *latest_cfg.borrow_mut() = Some(cfg.clone());
            if !cfg.open {
                rect.set(None);
            }
            if let Some(ev) = bridge.get() {
                let _ = ev.send(&cfg);
            }
        }));
    }

    use_portal(pop_id.clone(), open && rect().is_some());

    let Some(r) = rect() else { return rsx! {} };
    if !open {
        return rsx! {};
    }
    let style = placement_style(r, align, placement, min_width_anchor, max_width, max_height);
    let extra = if class.is_empty() { String::new() } else { format!(" {class}") };

    rsx! {
        div {
            id: "{pop_id}",
            class: "uis-pop-surface{extra}",
            "data-anchor": "{anchor_id}",
            style: "{style}",
            onmouseenter: move |_| {
                if let Some(h) = on_mouse_enter {
                    h.call(());
                }
            },
            onmouseleave: move |_| {
                if let Some(h) = on_mouse_leave {
                    h.call(());
                }
            },
            {children}
        }
    }
}

// ── Dismiss-only helper, for panels that place themselves ─────────────────────────────────────────────

#[derive(Clone, PartialEq, Serialize)]
struct DismissCfg {
    open: bool,
    anchor: String,
    panel: String,
}

/// The page side of `use_dismiss`: a press outside both the anchor and the panel, or Esc, while open.
const DISMISS_BRIDGE: &str = r#"
let cfg = null;
const onDown = (e) => {
  if (!cfg || !cfg.open) return;
  const a = document.getElementById(cfg.anchor);
  const p = document.getElementById(cfg.panel);
  if ((a && a.contains(e.target)) || (p && p.contains(e.target))) return;
  dioxus.send({ t: 'close' });
};
const onKey = (e) => { if (cfg && cfg.open && e.key === 'Escape') dioxus.send({ t: 'close' }); };
document.addEventListener('mousedown', onDown);
document.addEventListener('keydown', onKey);
try {
  while (true) {
    const m = await dioxus.recv();
    if (m && m.cmd === 'dispose') break;
    cfg = m;
  }
} finally {
  document.removeEventListener('mousedown', onDown);
  document.removeEventListener('keydown', onKey);
}
"#;

/// Closes a panel that positions ITSELF (the date picker, the colour picker) on a press outside it and
/// its anchor, and on Esc - what `UisPopover` does for the panels it places. Call it once per component,
/// every render, with the current `open`; both elements are named by id.
pub(super) fn use_dismiss(open: bool, anchor_id: &str, panel_id: &str, on_close: Callback<()>) {
    let bridge = use_hook(|| Rc::new(Cell::new(Option::<Eval>::None)));
    let latest = use_hook(|| Rc::new(std::cell::RefCell::new(Option::<DismissCfg>::None)));

    {
        let bridge = bridge.clone();
        let latest = latest.clone();
        use_future(move || {
            let bridge = bridge.clone();
            let latest = latest.clone();
            async move {
                let mut ev = document::eval(DISMISS_BRIDGE);
                bridge.set(Some(ev));
                if let Some(c) = latest.borrow().clone() {
                    let _ = ev.send(&c);
                }
                while let Ok(msg) = ev.recv::<Msg>().await {
                    if msg.t == "close" {
                        on_close.call(());
                    }
                }
            }
        });
    }
    {
        let bridge = bridge.clone();
        use_drop(move || {
            if let Some(ev) = bridge.get() {
                let _ = ev.send(serde_json::json!({ "cmd": "dispose" }));
            }
        });
    }

    let cfg = DismissCfg { open, anchor: anchor_id.to_string(), panel: panel_id.to_string() };
    use_effect(use_reactive!(|(cfg,)| {
        *latest.borrow_mut() = Some(cfg.clone());
        if let Some(ev) = bridge.get() {
            let _ = ev.send(&cfg);
        }
    }));
}
