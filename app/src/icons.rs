//! Icons the `dioxus-free-icons` release does not have: lucide glyphs it does not ship yet, drawn the way
//! it draws its own (same view box, stroke and caps), and the icons that are files of ours (`FileIcon`).

use dioxus::prelude::*;
use dioxus_free_icons::IconShape;

/// lucide `brush-cleaning`.
#[derive(Clone, Copy, PartialEq)]
pub struct LdBrushCleaning;

impl IconShape for LdBrushCleaning {
    fn view_box(&self) -> &str {
        "0 0 24 24"
    }
    fn xmlns(&self) -> &str {
        "http://www.w3.org/2000/svg"
    }
    fn fill_and_stroke<'a>(&self, user_color: &'a str) -> (&'a str, &'a str, &'a str) {
        ("none", user_color, "2")
    }
    fn stroke_linecap(&self) -> &str {
        "round"
    }
    fn stroke_linejoin(&self) -> &str {
        "round"
    }
    fn child_elements(&self) -> Element {
        rsx! {
            path { d: "m16 22-1-4" }
            path { d: "M19 14a1 1 0 0 0 1-1v-1a2 2 0 0 0-2-2h-3a1 1 0 0 1-1-1V4a2 2 0 0 0-4 0v5a1 1 0 0 1-1 1H6a2 2 0 0 0-2 2v1a1 1 0 0 0 1 1" }
            path { d: "M19 14H5l-1.973 6.767A1 1 0 0 0 4 22h16a1 1 0 0 0 .973-1.233z" }
            path { d: "m8 22 1-4" }
        }
    }
}

/// The cut icons of the preview bar, drawn by the owner: each is the `.svg` file itself, embedded.
/// A bracket is the cut; nothing about a cut is drawn with a bar (a bar is the file: previous, next).
pub const CUT_START: &str = include_str!("../assets/icon-yaiol-player-cut-start.svg");
pub const CUT_END: &str = include_str!("../assets/icon-yaiol-player-cut-end.svg");
pub const CUT_RESET: &str = include_str!("../assets/icon-yaiol-player-cut-reset.svg");
pub const GOTO_START: &str = include_str!("../assets/icon-yaiol-player-goto-start.svg");
pub const GOTO_END: &str = include_str!("../assets/icon-yaiol-player-goto-end.svg");

/// An icon that is a file: its shapes are put in the page as real elements, so it takes the
/// button's colour (`currentColor`) and the button's icon size, as a lucide glyph does.
#[component]
pub fn FileIcon(svg: &'static str, #[props(default = 14)] size: u32) -> Element {
    let (view_box, shapes) = file_icon_parts(svg);
    rsx! {
        svg {
            width: "{size}",
            height: "{size}",
            view_box: "{view_box}",
            xmlns: "http://www.w3.org/2000/svg",
            "aria-hidden": "true",
            dangerous_inner_html: "{shapes}",
        }
    }
}

/// The view box of an `.svg` file and what is between its `<svg ...>` and `</svg>`.
fn file_icon_parts(svg: &str) -> (&str, &str) {
    let open = svg.find("<svg").unwrap_or(0);
    let head_end = svg[open..].find('>').map_or(svg.len(), |i| open + i);
    let head = &svg[open..head_end];
    let view_box = head
        .find("viewBox=\"")
        .map(|i| &head[i + 9..])
        .and_then(|rest| rest.find('"').map(|end| &rest[..end]))
        .unwrap_or("0 0 24 24");
    let body_start = (head_end + 1).min(svg.len());
    let body_end = svg.rfind("</svg>").unwrap_or(svg.len()).max(body_start);
    (view_box, &svg[body_start..body_end])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_cut_icon_gives_its_view_box_and_its_shapes() {
        for svg in [CUT_START, CUT_END, CUT_RESET, GOTO_START, GOTO_END] {
            let (view_box, shapes) = file_icon_parts(svg);
            assert_eq!(view_box, "0 0 24 24");
            assert!(shapes.contains("<path"));
            assert!(shapes.contains("currentColor"));
            assert!(!shapes.contains("<svg") && !shapes.contains("</svg>"));
        }
    }
}
