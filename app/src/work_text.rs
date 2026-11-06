//! The text editor of the work view: the `text` effect's widget (kind `texts`), drawn over the preview. Each text of the file is a box on the
//! picture - the place where ffmpeg will draw it, never a copy of the letters - that is clicked to select and dragged to move; the bar above
//! the picture carries what the selected text is written with (the words, the letter height, bold and italic, the colour, a background box, a
//! border, the time played or left) and adds, copies and deletes. The picture under the boxes is the one ffmpeg makes, with every text on it.
//! Nothing here knows how a text is drawn: the controls, their names and their neutral values come from the catalogue (`WidgetSpec`).
//!
//! A text is one item of the effect in the work list, in the order added; the list keeps the texts together at the effect's place
//! (`EffectsEntry::arranged`), so a text's number among them stays what it was.

use crate::{
    catalog::{UisPkrColor, UisNum},
    i18n,
    preview_session::TextEdit,
};
use dioxus::prelude::*;
use dioxus_free_icons::{
    Icon,
    icons::ld_icons::{LdBold, LdClock, LdCopy, LdHourglass, LdItalic, LdPlus, LdSquare, LdTrash2, LdX},
};
use engine::{
    effects::{self, Item, WidgetSpec},
    ffmpeg_effects::{V, Values},
};

/// The text widget, when the catalogue has one.
pub fn widget() -> Option<WidgetSpec> {
    effects::catalog().and_then(|c| effects::widgets(&c, crate::effects_editor::SYSTEM).into_iter().find(|w| w.kind == effects::TEXTS))
}

/// The watermark widget (kind `pictures`), when the catalogue has one. It is placed and sized on the picture exactly as a text is (`Face` reads
/// and writes the place and the width of either), so the boxes, the dragging and the grip are the same code.
pub fn pictures() -> Option<WidgetSpec> {
    effects::catalog().and_then(|c| effects::widgets(&c, crate::effects_editor::SYSTEM).into_iter().find(|w| w.kind == effects::PICTURES))
}

/// The widget of the effect `key` when it is one that is placed on the picture in boxes, the text or the watermark.
pub fn layer_widget(key: &str) -> Option<WidgetSpec> {
    widget().into_iter().chain(pictures()).find(|w| w.key == key)
}

/// How a text is written: every input of the widget read from the values of one item.
#[derive(Clone, Debug, PartialEq)]
pub struct Face {
    pub text: String,
    /// The letter size, in percent of the picture's width.
    pub size: f64,
    pub colour: String,
    /// The font family by name; empty = the default font.
    pub family: String,
    pub bold: bool,
    pub italic: bool,
    /// The zone's width, as a fraction of the picture's width: the text is broken into lines to fit it.
    pub width: f64,
    pub boxed: bool,
    pub box_colour: String,
    pub border: f64,
    /// Where the top left corner of the text goes, as fractions of the picture (0..1).
    pub x: f64,
    pub y: f64,
}

fn id<'a>(w: &'a WidgetSpec, input: &str) -> &'a str {
    w.controls.get(input).map_or("", String::as_str)
}

fn number(w: &WidgetSpec, v: &Values, input: &str, neutral: f64) -> f64 {
    match v.get(id(w, input)) {
        Some(V::Num(n)) if n.is_finite() => *n,
        _ => neutral,
    }
}

fn string(w: &WidgetSpec, v: &Values, input: &str) -> String {
    v.get(id(w, input)).map(V::js_string).unwrap_or_default()
}

impl Face {
    pub fn read(w: &WidgetSpec, v: &Values) -> Face {
        let style = string(w, v, "style");
        Face {
            text: string(w, v, "text"),
            size: number(w, v, "size", 4.0),
            colour: string(w, v, "colour"),
            family: string(w, v, "family"),
            bold: style.starts_with("bold"),
            italic: style.ends_with("italic"),
            width: number(w, v, "width", 0.5),
            boxed: v.get(id(w, "box")).is_some_and(V::truthy),
            box_colour: string(w, v, "boxcolour"),
            border: number(w, v, "border", 0.0),
            x: number(w, v, "x", 0.0),
            y: number(w, v, "y", 0.0),
        }
    }

    /// The values of the item with this face written into them (every other control keeps what it held).
    pub fn write(&self, w: &WidgetSpec, base: &Values) -> Values {
        let mut v = base.clone();
        let style = match (self.bold, self.italic) {
            (false, false) => "regular",
            (true, false) => "bold",
            (false, true) => "italic",
            (true, true) => "bolditalic",
        };
        for (input, value) in [
            ("text", V::Str(self.text.clone())),
            ("size", V::Num(self.size)),
            ("colour", V::Str(self.colour.clone())),
            ("family", V::Str(self.family.clone())),
            ("style", V::Str(style.to_owned())),
            ("width", V::Num(self.width)),
            ("box", V::Bool(self.boxed)),
            ("boxcolour", V::Str(self.box_colour.clone())),
            ("border", V::Num(self.border)),
            ("x", V::Num(self.x)),
            ("y", V::Num(self.y)),
        ] {
            // (a widget that has no such input - the picture has no letters - is left as it is)
            if !id(w, input).is_empty() {
                v.insert(id(w, input).to_owned(), value);
            }
        }
        v
    }
}

/// The values of a new text with its corner at `at` (fractions of the picture).
pub fn fresh(w: &WidgetSpec, at: (f64, f64)) -> Values {
    let base = Item::new(&w.key).all_values();
    let mut face = Face::read(w, &base);
    (face.x, face.y) = place(at);
    face.write(w, &base)
}

/// Where the next new text goes, so that texts added one after another do not sit on top of each other: a step down and to the right each time.
pub fn next_place(count: usize) -> (f64, f64) {
    let step = (count % 8) as f64;
    (0.1 + 0.04 * step, 0.3 + 0.07 * step)
}

/// A place of the picture as the catalogue holds it: inside the picture, to a thousandth.
pub fn place(at: (f64, f64)) -> (f64, f64) {
    let round = |v: f64| (v.clamp(0.0, 1.0) * 1000.0).round() / 1000.0;
    (round(at.0), round(at.1))
}

/// The same values with the zone this wide.
pub fn widened(w: &WidgetSpec, values: &Values, width: f64) -> Values {
    let mut face = Face::read(w, values);
    face.width = width;
    face.write(w, values)
}

/// The same values with the text moved to `at`.
pub fn moved(w: &WidgetSpec, values: &Values, at: (f64, f64)) -> Values {
    let mut face = Face::read(w, values);
    (face.x, face.y) = place(at);
    face.write(w, values)
}

/// How tall a line is, in em (Arial's ascent and descent; other fonts differ a little).
const LINE: f64 = 1.15;

/// The room a text's zone takes on the picture, in percent of the picture's width and height: the width is the zone's, the height is what its
/// lines take - the text is broken to fit the zone by measuring the font it is drawn in, as the converter does, so the lines are the same.
pub fn extent(face: &Face, picture: (u32, u32)) -> (f64, f64) {
    let (pw, ph) = (f64::from(picture.0.max(1)), f64::from(picture.1.max(1)));
    let file = engine::fonts::face_file(&face.family, face.bold, face.italic);
    let lines = engine::fonts::wrap(&file, &face.text, face.width * 100.0 / face.size.max(0.1)).lines().count().max(1) as f64;
    let em = face.size / 100.0 * pw;
    let pad = if face.boxed { 16.0 } else { 0.0 } + face.border * 2.0;
    let height = lines * em * LINE + pad;
    ((face.width * 100.0).min(100.0), (height / ph * 100.0).min(100.0))
}

/// A colour's `#rrggbb` for the picker: ffmpeg's `colour@alpha` loses its alpha here (it comes back from `with_hex`), the two names ffmpeg
/// is given by default are known, anything else starts the picker at black.
pub fn hex_of(colour: &str) -> String {
    let base = colour.split('@').next().unwrap_or("").trim();
    match base.to_ascii_lowercase().as_str() {
        "white" => return "#ffffff".to_owned(),
        "black" => return "#000000".to_owned(),
        _ => {}
    }
    let body = base.strip_prefix('#').or_else(|| base.strip_prefix("0x")).or_else(|| base.strip_prefix("0X"));
    match body {
        Some(b) if b.len() >= 6 && b[..6].chars().all(|c| c.is_ascii_hexdigit()) => format!("#{}", b[..6].to_ascii_lowercase()),
        _ => "#000000".to_owned(),
    }
}

/// A colour picked in the picker, written the way the control held its old one: the same `@alpha` stays on it.
pub fn with_hex(old: &str, hex: &str) -> String {
    match old.split_once('@') {
        Some((_, alpha)) => format!("{hex}@{alpha}"),
        None => hex.to_owned(),
    }
}

/// What of a text is held: the whole of it (to move it) or the grip on its right edge (to set the zone's width).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Grab {
    Move,
    Width,
}

/// A text being dragged: what is held, its number, the picture's box on screen when the press was made, and where in the text's corner the
/// pointer held it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Drag {
    pub grab: Grab,
    pub corner: (f64, f64),
    pub index: usize,
    pub left: f64,
    pub top: f64,
    pub width: f64,
    pub height: f64,
    pub dx: f64,
    pub dy: f64,
}

impl Drag {
    /// Where the text's corner goes with the pointer at `p` (screen pixels).
    pub fn to(&self, p: (f64, f64)) -> (f64, f64) {
        place(((p.0 - self.dx - self.left) / self.width, (p.1 - self.dy - self.top) / self.height))
    }

    /// The zone's width with the pointer at `p`: from the text's corner to the pointer, at least a little, never out of the picture.
    pub fn zone(&self, p: (f64, f64)) -> f64 {
        let room = (1.0 - self.corner.0).max(0.02);
        (((p.0 - self.left) / self.width - self.corner.0).clamp(0.02, room) * 1000.0).round() / 1000.0
    }
}

/// How long the bar waits after a key before the words reach the list.
const TYPING_PAUSE: std::time::Duration = std::time::Duration::from_millis(350);

/// What the bar does with the selected text: written back as the values of its item.
fn change(set: EventHandler<Face>, face: &Face, edit: impl FnOnce(&mut Face)) {
    let mut next = face.clone();
    edit(&mut next);
    set.call(next);
}

/// The bar above the picture while the texts are edited. `values` are the selected text's, none when no text is selected.
#[component]
pub fn TextBar(
    lang: String,
    values: Option<Values>,
    on_edit: EventHandler<TextEdit>,
    index: Option<usize>,
    on_add: EventHandler<()>,
    on_copy: EventHandler<()>,
    on_close: EventHandler<()>,
) -> Element {
    // The words being typed: kept here and written to the list a moment after the last key (or when the box is left). The list answers with a
    // new picture every time it changes, and a box that is given its value back at each key loses its caret and runs behind the hand.
    let mut draft = use_signal(|| None::<String>);
    let mut pending = use_signal(|| 0_u64);
    let t = |key: &str| i18n::t(&lang, key);
    let Some(w) = widget() else { return rsx! {} };
    let (s_add, s_bold, s_italic, s_box, s_border, s_played, s_left) =
        (t("btnTextAdd"), t("tipTextBold"), t("tipTextItalic"), t("tipTextBox"), t("tipTextBorder"), t("tipTextPlayed"), t("tipTextLeft"));
    let (s_copy, s_delete, s_close, s_size, s_hold) = (t("tipTextCopy"), t("tipTextDelete"), t("btnNewsClose"), t("tipTextSize"), t("phTextText"));
    let (s_font, s_font_default) = (t("tipTextFont"), t("lblTextFontDefault"));
    let (cancel, apply, pick) = (t("btnFxCancel"), t("btnMainApply"), t("tipFxPickColour"));
    let selected = values.as_ref().map(|v| (Face::read(&w, v), v.clone()));
    let set = EventHandler::new({
        let (w, values) = (w.clone(), values.clone());
        move |face: Face| {
            if let (Some(i), Some(base)) = (index, values.as_ref()) {
                on_edit.call(TextEdit::Set(i, face.write(&w, base)));
            }
        }
    });
    rsx! {
        div { class: "wv-text-bar uis-cmp-bar",
            button {
                class: "uis-btn",
                onclick: move |_| on_add.call(()),
                Icon { width: 14, height: 14, icon: LdPlus }
                span { "{s_add}" }
            }
            if let Some((face, _)) = selected.clone() {
                textarea {
                    class: "uis-txt wv-text-words",
                    rows: "2",
                    value: "{draft().unwrap_or_else(|| face.text.clone())}",
                    placeholder: "{s_hold}",
                    oninput: {
                        let face = face.clone();
                        move |e| {
                            let words = e.value();
                            draft.set(Some(words.clone()));
                            let turn = *pending.peek() + 1;
                            pending.set(turn);
                            let face = face.clone();
                            spawn(async move {
                                tokio::time::sleep(TYPING_PAUSE).await;
                                if *pending.peek() == turn {
                                    change(set, &face, |f| f.text = words);
                                }
                            });
                        }
                    },
                    onblur: {
                        let face = face.clone();
                        move |_| {
                            if let Some(words) = draft.peek().clone() {
                                pending += 1;
                                change(set, &face, |f| f.text = words);
                            }
                            draft.set(None);
                        }
                    },
                }
                select {
                    class: "uis-ddn-std wv-text-font",
                    title: "{s_font}",
                    "aria-label": "{s_font}",
                    value: "{face.family}",
                    onchange: {
                        let face = face.clone();
                        move |e: Event<FormData>| change(set, &face, |f| f.family = e.value())
                    },
                    option { value: "", "{s_font_default}" }
                    for family in engine::fonts::installed() {
                        option { key: "{family.name}", value: "{family.name}", "{family.name}" }
                    }
                }
                UisNum {
                    value: Some(face.size),
                    min: Some(1.0),
                    max: Some(30.0),
                    step: 1.0,
                    decimals: Some(0),
                    width: "56px".to_owned(),
                    title: s_size.clone(),
                    on_change: {
                        let face = face.clone();
                        move |v: Option<f64>| {
                            if let Some(n) = v {
                                change(set, &face, |f| f.size = n.clamp(1.0, 30.0));
                            }
                        }
                    },
                }
                button {
                    class: if face.bold { "uis-btn ico active" } else { "uis-btn ico" },
                    title: "{s_bold}",
                    "aria-label": "{s_bold}",
                    "aria-pressed": "{face.bold}",
                    onclick: {
                        let face = face.clone();
                        move |_| change(set, &face, |f| f.bold = !f.bold)
                    },
                    Icon { width: 14, height: 14, icon: LdBold }
                }
                button {
                    class: if face.italic { "uis-btn ico active" } else { "uis-btn ico" },
                    title: "{s_italic}",
                    "aria-label": "{s_italic}",
                    "aria-pressed": "{face.italic}",
                    onclick: {
                        let face = face.clone();
                        move |_| change(set, &face, |f| f.italic = !f.italic)
                    },
                    Icon { width: 14, height: 14, icon: LdItalic }
                }
                UisPkrColor {
                    color: hex_of(&face.colour),
                    cancel_label: cancel.clone(),
                    apply_label: apply.clone(),
                    pick_title: pick.clone(),
                    on_change: {
                        let face = face.clone();
                        move |hex: String| change(set, &face, |f| f.colour = with_hex(&f.colour, &hex))
                    },
                }
                button {
                    class: if face.boxed { "uis-btn ico active" } else { "uis-btn ico" },
                    title: "{s_box}",
                    "aria-label": "{s_box}",
                    "aria-pressed": "{face.boxed}",
                    onclick: {
                        let face = face.clone();
                        move |_| change(set, &face, |f| f.boxed = !f.boxed)
                    },
                    Icon { width: 14, height: 14, icon: LdSquare }
                }
                if face.boxed {
                    UisPkrColor {
                        color: hex_of(&face.box_colour),
                        cancel_label: cancel.clone(),
                        apply_label: apply.clone(),
                        pick_title: pick.clone(),
                        on_change: {
                            let face = face.clone();
                            move |hex: String| change(set, &face, |f| f.box_colour = with_hex(&f.box_colour, &hex))
                        },
                    }
                }
                UisNum {
                    value: Some(face.border),
                    min: Some(0.0),
                    max: Some(20.0),
                    step: 1.0,
                    decimals: Some(0),
                    width: "52px".to_owned(),
                    title: s_border.clone(),
                    on_change: {
                        let face = face.clone();
                        move |v: Option<f64>| {
                            if let Some(n) = v {
                                change(set, &face, |f| f.border = n.clamp(0.0, 20.0));
                            }
                        }
                    },
                }
                button {
                    class: "uis-btn ico",
                    title: "{s_played}",
                    "aria-label": "{s_played}",
                    onclick: {
                        let face = face.clone();
                        move |_| {
                            let words = format!("{}{{elapsed}}", draft.peek().clone().unwrap_or_else(|| face.text.clone()));
                            pending += 1;
                            draft.set(Some(words.clone()));
                            change(set, &face, |f| f.text = words);
                        }
                    },
                    Icon { width: 14, height: 14, icon: LdClock }
                }
                button {
                    class: "uis-btn ico",
                    title: "{s_left}",
                    "aria-label": "{s_left}",
                    onclick: {
                        let face = face.clone();
                        move |_| {
                            let words = format!("{}{{remaining}}", draft.peek().clone().unwrap_or_else(|| face.text.clone()));
                            pending += 1;
                            draft.set(Some(words.clone()));
                            change(set, &face, |f| f.text = words);
                        }
                    },
                    Icon { width: 14, height: 14, icon: LdHourglass }
                }
                button {
                    class: "uis-btn ico",
                    title: "{s_copy}",
                    "aria-label": "{s_copy}",
                    onclick: move |_| on_copy.call(()),
                    Icon { width: 14, height: 14, icon: LdCopy }
                }
                button {
                    class: "uis-btn ico",
                    title: "{s_delete}",
                    "aria-label": "{s_delete}",
                    onclick: move |_| {
                        if let Some(i) = index {
                            on_edit.call(TextEdit::Remove(i));
                        }
                    },
                    Icon { width: 14, height: 14, icon: LdTrash2 }
                }
            }
            span { class: "uis-bar-spacer" }
            button {
                class: "uis-btn ico",
                title: "{s_close}",
                "aria-label": "{s_close}",
                onclick: move |_| on_close.call(()),
                Icon { width: 14, height: 14, icon: LdX }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec() -> WidgetSpec {
        let controls = [("text", "text"), ("size", "size"), ("colour", "colour"), ("style", "style"), ("family", "family"), ("width", "pw"), ("box", "box"), ("boxcolour", "boxcolour"), ("border", "border"), ("x", "px"), ("y", "py")];
        WidgetSpec { key: "text".to_owned(), kind: "texts".to_owned(), controls: controls.into_iter().map(|(a, b)| (a.to_owned(), b.to_owned())).collect() }
    }

    fn face() -> Face {
        Face { text: "hi".into(), size: 8.0, colour: "white".into(), family: "Arial".into(), bold: true, italic: false, width: 0.4, boxed: true, box_colour: "black@0.5".into(), border: 2.0, x: 0.1, y: 0.2 }
    }

    #[test]
    fn a_face_goes_into_the_controls_and_comes_back_from_them() {
        let w = spec();
        let written = face().write(&w, &Values::new());
        assert_eq!(written.get("style"), Some(&V::Str("bold".to_owned())));
        assert_eq!((written.get("px"), written.get("py")), (Some(&V::Num(0.1)), Some(&V::Num(0.2))));
        assert_eq!(Face::read(&w, &written), face());
        let both = Face { italic: true, ..face() }.write(&w, &written);
        assert_eq!(both.get("style"), Some(&V::Str("bolditalic".to_owned())));
    }

    #[test]
    fn new_texts_step_down_and_to_the_right_and_start_over() {
        assert_eq!(next_place(0), (0.1, 0.3));
        assert!(next_place(1).0 > next_place(0).0 && next_place(1).1 > next_place(0).1);
        assert_eq!(next_place(8), next_place(0));
    }

    #[test]
    fn a_text_is_put_inside_the_picture_to_a_thousandth() {
        assert_eq!(place((-0.3, 1.7)), (0.0, 1.0));
        assert_eq!(place((0.12349, 0.5)), (0.123, 0.5));
    }

    #[test]
    fn a_dragged_text_keeps_the_point_it_was_held_by() {
        // the picture is 400 x 200 on screen from (100, 50); the text's corner was at (0.25, 0.5) = (200, 150) and was held 10 px inside it
        let d = Drag { grab: Grab::Move, corner: (0.25, 0.5), index: 0, left: 100.0, top: 50.0, width: 400.0, height: 200.0, dx: 10.0, dy: 5.0 };
        assert_eq!(d.to((210.0, 155.0)), (0.25, 0.5));
        assert_eq!(d.to((310.0, 255.0)), (0.5, 1.0));
        assert_eq!(d.to((0.0, 0.0)), (0.0, 0.0), "never out of the picture");
    }

    #[test]
    fn the_zone_is_as_wide_as_it_was_set_and_as_tall_as_its_lines() {
        let one = extent(&Face { text: "hi".into(), ..face() }, (1000, 500));
        let many = extent(&Face { text: "a long sentence that cannot fit one line of this narrow zone at all".into(), ..face() }, (1000, 500));
        assert_eq!(one.0, 40.0, "the zone's width, whatever the words");
        assert!(many.1 > one.1, "more lines, a taller zone: {one:?} {many:?}");
        assert!(extent(&Face { size: 30.0, ..face() }, (1000, 500)).1 <= 100.0);
    }

    #[test]
    fn the_grip_sets_the_zone_width_from_the_corner_to_the_pointer_inside_the_picture() {
        // the picture is 400 px wide on screen from x = 100; the corner is at a quarter of it
        let d = Drag { grab: Grab::Width, corner: (0.25, 0.5), index: 0, left: 100.0, top: 50.0, width: 400.0, height: 200.0, dx: 0.0, dy: 0.0 };
        assert_eq!(d.zone((400.0, 0.0)), 0.5);
        assert_eq!(d.zone((100.0, 0.0)), 0.02, "never narrower than a sliver");
        assert_eq!(d.zone((900.0, 0.0)), 0.75, "never out of the picture");
    }

    #[test]
    fn a_colour_goes_to_the_picker_and_comes_back_with_its_alpha() {
        assert_eq!((hex_of("white").as_str(), hex_of("black@0.5").as_str(), hex_of("#FF8800").as_str()), ("#ffffff", "#000000", "#ff8800"));
        assert_eq!(hex_of("0x3366CC@0.4"), "#3366cc");
        assert_eq!(with_hex("black@0.5", "#102030"), "#102030@0.5");
        assert_eq!(with_hex("white", "#102030"), "#102030");
    }
}
