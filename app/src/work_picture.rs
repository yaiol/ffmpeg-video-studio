//! The watermark editor of the work view: the `watermark` effect's widget (kind `pictures`), drawn over the preview exactly as the texts are
//! (`work_text`): each picture is a box on the video - where ffmpeg will lay it - that is clicked to select, dragged to move, and widened by the
//! grip on its right edge. What a picture is (its file, its opacity) is on the bar above the picture. The picture under the boxes is the one ffmpeg
//! makes, with every watermark on it. The place and the width are the same two fractions of the video's width and height a text has, so
//! `work_text::Face` reads and writes them for both.

use crate::{catalog::UisNum, i18n, preview_session::TextEdit, work_text};
use dioxus::prelude::*;
use dioxus_free_icons::{
    Icon,
    icons::ld_icons::{LdCopy, LdImage, LdPlus, LdTrash2, LdX},
};
use engine::{
    effects::WidgetSpec,
    ffmpeg_effects::{V, Values},
};
use std::path::Path;

/// The file of a picture, empty while none is chosen.
pub fn file_of(w: &WidgetSpec, v: &Values) -> String {
    v.get(w.controls.get("file").map_or("", String::as_str)).map(V::js_string).unwrap_or_default()
}

/// How opaque a picture is, from 0 to 1.
pub fn opacity_of(w: &WidgetSpec, v: &Values) -> f64 {
    match v.get(w.controls.get("opacity").map_or("", String::as_str)) {
        Some(V::Num(n)) if n.is_finite() => n.clamp(0.0, 1.0),
        _ => 0.6,
    }
}

/// The same values with this file.
pub fn with_file(w: &WidgetSpec, base: &Values, file: &str) -> Values {
    let mut v = base.clone();
    v.insert(w.controls.get("file").cloned().unwrap_or_default(), V::Str(file.to_owned()));
    v
}

/// The same values with this opacity, to a hundredth.
pub fn with_opacity(w: &WidgetSpec, base: &Values, opacity: f64) -> Values {
    let mut v = base.clone();
    v.insert(w.controls.get("opacity").cloned().unwrap_or_default(), V::Num((opacity.clamp(0.0, 1.0) * 100.0).round() / 100.0));
    v
}

/// The room a picture takes on the video, in percent of the video's width and height: the width is the box's own, the height keeps the
/// proportions of the picture (a file that cannot be read, or none yet, is drawn square).
pub fn extent(file: &str, width: f64, video: (u32, u32)) -> (f64, f64) {
    let (vw, vh) = (f64::from(video.0.max(1)), f64::from(video.1.max(1)));
    let (iw, ih) = engine::pictures::size(Path::new(file)).map_or((1.0, 1.0), |(w, h)| (f64::from(w), f64::from(h)));
    let across = width.clamp(0.0, 1.0);
    ((across * 100.0).min(100.0), (across * vw * ih / iw / vh * 100.0).min(100.0))
}

/// What a file is called on the bar: its name without the folder.
fn name_of(file: &str) -> String {
    Path::new(file).file_name().map_or_else(|| file.to_owned(), |n| n.to_string_lossy().into_owned())
}

/// Asks for a picture file; None when the dialog was closed.
async fn choose() -> Option<String> {
    let file = rfd::AsyncFileDialog::new().add_filter("picture", &["png", "jpg", "jpeg", "gif", "bmp", "webp"]).pick_file().await?;
    Some(file.path().to_string_lossy().into_owned())
}

/// The bar above the picture while the watermarks are edited. `values` are the selected picture's, none when none is selected; `count` is how many
/// pictures there are (a new one takes the next number).
#[component]
pub fn PictureBar(
    lang: String,
    values: Option<Values>,
    on_edit: EventHandler<TextEdit>,
    index: Option<usize>,
    count: usize,
    on_select: EventHandler<usize>,
    on_copy: EventHandler<()>,
    on_close: EventHandler<()>,
) -> Element {
    let t = |key: &str| i18n::t(&lang, key);
    let Some(w) = work_text::pictures() else { return rsx! {} };
    let (s_add, s_change, s_opacity, s_copy, s_delete, s_close, s_none) =
        (t("btnPictureAdd"), t("tipPictureChange"), t("tipPictureOpacity"), t("tipPictureCopy"), t("tipPictureDelete"), t("btnNewsClose"), t("lblPictureNone"));
    let selected = values.clone().map(|v| {
        let file = file_of(&w, &v);
        let shown = if file.is_empty() { s_none.clone() } else { name_of(&file) };
        (file, shown, opacity_of(&w, &v), v)
    });
    rsx! {
        div { class: "wv-text-bar uis-cmp-bar",
            button {
                class: "uis-btn",
                onclick: {
                    let w = w.clone();
                    move |_| {
                        let w = w.clone();
                        spawn(async move {
                            if let Some(file) = choose().await {
                                let fresh = work_text::fresh(&w, work_text::next_place(count));
                                on_select.call(count);
                                on_edit.call(TextEdit::Add(with_file(&w, &fresh, &file)));
                            }
                        });
                    }
                },
                Icon { width: 14, height: 14, icon: LdPlus }
                span { "{s_add}" }
            }
            if let Some((file, shown, opacity, base)) = selected {
                span { class: "wv-picture-name", title: "{file}", "{shown}" }
                button {
                    class: "uis-btn ico",
                    title: "{s_change}",
                    "aria-label": "{s_change}",
                    onclick: {
                        let (w, base) = (w.clone(), base.clone());
                        move |_| {
                            let (w, base) = (w.clone(), base.clone());
                            spawn(async move {
                                if let (Some(file), Some(i)) = (choose().await, index) {
                                    on_edit.call(TextEdit::Set(i, with_file(&w, &base, &file)));
                                }
                            });
                        }
                    },
                    Icon { width: 14, height: 14, icon: LdImage }
                }
                UisNum {
                    value: Some((opacity * 100.0).round()),
                    min: Some(0.0),
                    max: Some(100.0),
                    step: 5.0,
                    decimals: Some(0),
                    width: "56px".to_owned(),
                    title: s_opacity.clone(),
                    on_change: {
                        let (w, base) = (w.clone(), base.clone());
                        move |v: Option<f64>| {
                            if let (Some(n), Some(i)) = (v, index) {
                                on_edit.call(TextEdit::Set(i, with_opacity(&w, &base, n / 100.0)));
                            }
                        }
                    },
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
        let controls = [("file", "file"), ("opacity", "opacity"), ("width", "pw"), ("x", "px"), ("y", "py")];
        WidgetSpec { key: "watermark".to_owned(), kind: "pictures".to_owned(), controls: controls.into_iter().map(|(a, b)| (a.to_owned(), b.to_owned())).collect() }
    }

    #[test]
    fn a_picture_keeps_the_file_and_the_opacity_it_was_given() {
        let w = spec();
        let v = with_opacity(&w, &with_file(&w, &Values::new(), "C:/logo.png"), 0.456);
        assert_eq!(file_of(&w, &v), "C:/logo.png");
        assert_eq!(opacity_of(&w, &v), 0.46);
        assert_eq!(opacity_of(&w, &Values::new()), 0.6, "no value, the catalogue's own");
        assert_eq!(opacity_of(&w, &with_opacity(&w, &Values::new(), 7.0)), 1.0, "never more than opaque");
    }

    #[test]
    fn the_place_and_the_width_of_a_picture_are_read_and_written_like_a_text_s() {
        let w = spec();
        let v = work_text::widened(&w, &work_text::moved(&w, &with_file(&w, &Values::new(), "C:/logo.png"), (0.25, 0.5)), 0.3);
        let face = work_text::Face::read(&w, &v);
        assert_eq!((face.x, face.y, face.width), (0.25, 0.5, 0.3));
        assert_eq!(v.len(), 4, "nothing of a text's was written into a picture: {v:?}");
    }

    #[test]
    fn a_picture_that_cannot_be_read_is_drawn_square_and_never_out_of_the_video() {
        // a fifth of a 1000 x 500 video is 200 px: square, that is 40 % of the height
        assert_eq!(extent("C:/nowhere/none.png", 0.2, (1000, 500)), (20.0, 40.0));
        assert_eq!(extent("C:/nowhere/none.png", 1.0, (1000, 500)).1, 100.0);
    }
}
