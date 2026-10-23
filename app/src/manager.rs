//! The preset editor, a page of the Settings view: a list on the left and the options of the selected
//! preset on the right. Every change is stored at once, there is no Save button.
//!
//! ⚠ CLAUDE: NO HARDCODED UI STRINGS. Container and codec names are proper nouns and stay literal;
//! everything else goes through the t() lookup with a literal key at the call site.

use crate::{catalog::NumberInput, i18n, settings::Settings};
use dioxus::{html::input_data::MouseButton, prelude::*};
use dioxus_free_icons::{
    Icon,
    icons::ld_icons::{LdCopy, LdFolderOpen, LdPlus, LdTrash2},
};
use engine::{
    AudioEntry, Destination, EffectsEntry, Lut, Preset, Rotation, Size,
    audio::Audio,
    effects::{BlurFit, Corner, Crop, Effect, FRAME_RATES, Kind, TextOverlay, Watermark},
    preset::{Container, ProResProfile, RateMode, Speed, Tune, VideoCodec},
};

/// The keys of the Settings tabs this module provides.
pub const PRESETS: &str = "presets";
pub const SIZES: &str = "sizes";
pub const AUDIO: &str = "audio";
pub const LUTS: &str = "luts";
pub const DESTINATIONS: &str = "destinations";
pub const EFFECTS: &str = "effects";

fn container_label(c: Container) -> &'static str {
    match c {
        Container::Mkv => "MKV",
        Container::Mp4 => "MP4",
        Container::Webm => "WebM",
        Container::Mov => "MOV",
        Container::Avi => "AVI",
    }
}

fn container_id(c: Container) -> &'static str {
    c.extension()
}

fn codec_id(c: VideoCodec) -> &'static str {
    match c {
        VideoCodec::H264 => "h264",
        VideoCodec::H265 => "h265",
        VideoCodec::H264Amd => "h264amd",
        VideoCodec::H265Amd => "h265amd",
        VideoCodec::Av1 => "av1",
        VideoCodec::Vp9 => "vp9",
        VideoCodec::ProRes => "prores",
        VideoCodec::Raw => "raw",
        VideoCodec::H264Nvenc => "h264nvenc",
        VideoCodec::H265Nvenc => "h265nvenc",
        VideoCodec::H264Qsv => "h264qsv",
        VideoCodec::H265Qsv => "h265qsv",
        VideoCodec::Copy => "copy",
    }
}

fn tune_id(t: Tune) -> &'static str {
    match t {
        Tune::None => "none",
        Tune::Film => "film",
        Tune::Animation => "animation",
        Tune::Grain => "grain",
        Tune::StillImage => "stillimage",
        Tune::FastDecode => "fastdecode",
        Tune::ZeroLatency => "zerolatency",
    }
}

fn tune_label(tune: Tune, lang: &str) -> String {
    let t = |key: &str| i18n::t(lang, key);
    match tune {
        Tune::None => t("lblPrsTuneNone"),
        Tune::Film => t("lblPrsTuneFilm"),
        Tune::Animation => t("lblPrsTuneAnimation"),
        Tune::Grain => t("lblPrsTuneGrain"),
        Tune::StillImage => t("lblPrsTuneStill"),
        Tune::FastDecode => t("lblPrsTuneFastDecode"),
        Tune::ZeroLatency => t("lblPrsTuneZeroLatency"),
    }
}

fn audio_id(a: Audio) -> &'static str {
    match a {
        Audio::Copy => "copy",
        Audio::Aac => "aac",
        Audio::Opus => "opus",
        Audio::Ac3 => "ac3",
        Audio::Pcm => "pcm",
        Audio::None => "none",
    }
}

fn profile_id(p: ProResProfile) -> &'static str {
    p.ffmpeg_name()
}

/// ProRes' own names for its profiles: product names, so they stay literal.
fn profile_label(p: ProResProfile) -> &'static str {
    match p {
        ProResProfile::Proxy => "422 Proxy",
        ProResProfile::Lt => "422 LT",
        ProResProfile::Standard => "422",
        ProResProfile::Hq => "422 HQ",
        ProResProfile::Fourk => "4444",
    }
}

/// Put the dragged entry into the slot of the one it was dragged over. The order of the saved list IS
/// the order everywhere (this list and the window's drop-down), so this is the whole feature.
fn reorder<T>(items: &mut Vec<T>, id_of: impl Fn(&T) -> &str, dragged: &str, target: &str) {
    let from = items.iter().position(|x| id_of(x) == dragged);
    let to = items.iter().position(|x| id_of(x) == target);
    if let (Some(from), Some(to)) = (from, to) {
        if from != to {
            let item = items.remove(from);
            items.insert(to, item);
        }
    }
}

/// Apply `edit` to the preset with this id and re-normalize it, so no field can be left holding a
/// value the new container does not accept.
fn edit_preset(mut settings: Signal<Settings>, id: &str, edit: impl FnOnce(&mut Preset)) {
    let mut s = settings.write();
    if let Some(p) = s.presets.iter_mut().find(|p| p.id == id) {
        edit(p);
        *p = p.clone().normalized();
    }
}

/// The width of a number field, in px, from the characters its largest value takes: the digits (13 px
/// monospace, about 8 px each) plus the field's padding and its stepper column.
fn number_width(max: u32) -> u32 {
    max.to_string().len() as u32 * 8 + 34
}

/// A labelled catalog number field, as wide as its largest value needs.
#[component]
fn NumberField(
    label: String,
    value: u32,
    min: u32,
    max: u32,
    /// The unit, in plain text at the right of the field - never part of the title.
    #[props(default)]
    unit: String,
    on_change: EventHandler<u32>,
) -> Element {
    rsx! {
        div { class: "dlg-field",
            label { class: "dlg-field-label", "{label}" }
            div { class: "manager-row",
                NumberInput {
                    value: Some(f64::from(value)),
                    min: Some(f64::from(min)),
                    max: Some(f64::from(max)),
                    step: 1.0,
                    width: format!("{}px", number_width(max)),
                    on_change: move |v: Option<f64>| {
                        if let Some(n) = v {
                            on_change.call(n.round().max(0.0) as u32);
                        }
                    },
                }
                if !unit.is_empty() {
                    span { "{unit}" }
                }
            }
        }
    }
}

/// A choice from a short list.
#[component]
fn ChoiceField(label: String, value: String, options: Vec<(String, String)>, on_change: EventHandler<String>) -> Element {
    rsx! {
        div { class: "dlg-field",
            label { class: "dlg-field-label", "{label}" }
            select {
                class: "select",
                value: "{value}",
                onchange: move |e| on_change.call(e.value()),
                for (id, text) in options {
                    option { key: "{id}", value: "{id}", selected: id == value, "{text}" }
                }
            }
        }
    }
}

#[component]
pub fn PresetEditor(settings: Signal<Settings>) -> Element {
    let mut selected = use_signal(|| settings.read().preset_id.clone());
    let mut dragging = use_signal(|| Option::<String>::None);

    let lang = settings.read().lang.clone();
    let t = |key: &str| i18n::t(&lang, key);

    // The selection can point at a preset that was just deleted; fall back to the first.
    let presets = settings.read().presets.clone();
    let current = presets.iter().find(|p| p.id == selected()).unwrap_or(&presets[0]).clone();
    let cur_id = current.id.clone();
    let can_delete = presets.len() > 1;

    let s_new = t("btnMngNew");
    let s_dup = t("btnMngDuplicate");
    let s_del = t("btnMngDelete");
    let s_reorder = t("msgMngReorderHint");

    let codec_label = |c: VideoCodec| match c {
        VideoCodec::H264 => "H.264".to_owned(),
        VideoCodec::H265 => "H.265".to_owned(),
        VideoCodec::H264Amd => "H.264 (AMD)".to_owned(),
        VideoCodec::H265Amd => "H.265 (AMD)".to_owned(),
        VideoCodec::Av1 => "AV1".to_owned(),
        VideoCodec::Vp9 => "VP9".to_owned(),
        VideoCodec::ProRes => "ProRes".to_owned(),
        VideoCodec::Raw => t("lblPrsVideoRaw"),
        VideoCodec::H264Nvenc => "H.264 (NVIDIA)".to_owned(),
        VideoCodec::H265Nvenc => "H.265 (NVIDIA)".to_owned(),
        VideoCodec::H264Qsv => "H.264 (Intel)".to_owned(),
        VideoCodec::H265Qsv => "H.265 (Intel)".to_owned(),
        VideoCodec::Copy => t("lblPrsVideoCopy"),
    };

    let container_options: Vec<(String, String)> =
        Container::ALL.iter().map(|c| (container_id(*c).to_owned(), container_label(*c).to_owned())).collect();
    let codec_options: Vec<(String, String)> =
        current.container.video_codecs().iter().map(|c| (codec_id(*c).to_owned(), codec_label(*c))).collect();
    // Only the speeds the codec has: x264 and x265 add `ultrafast`.
    let speed_options: Vec<(String, String)> = current
        .video
        .speeds()
        .iter()
        .map(|sp| match sp {
            Speed::Ultrafast => ("ultrafast".to_owned(), t("lblPrsSpeedUltrafast")),
            Speed::Fast => ("fast".to_owned(), t("lblPrsSpeedFast")),
            Speed::Balanced => ("balanced".to_owned(), t("lblPrsSpeedBalanced")),
            Speed::Slow => ("slow".to_owned(), t("lblPrsSpeedSlow")),
        })
        .collect();

    let (crf_lo, crf_hi, _) = current.video.crf_range();
    let has_crf = current.video.has_crf();
    let is_prores = current.video == VideoCodec::ProRes;
    let profile_options: Vec<(String, String)> =
        ProResProfile::ALL.iter().map(|p| (profile_id(*p).to_owned(), profile_label(*p).to_owned())).collect();
    let needs_x26x = matches!(current.video, VideoCodec::H264 | VideoCodec::H265);
    let is_amd = current.video.is_amd();
    let rate_choice = current.video.has_rate_choice();
    let by_bitrate = rate_choice && current.rate == RateMode::Bitrate;
    let ten_bit_choice = current.video.supports_ten_bit();
    let is_h265 = current.video == VideoCodec::H265;
    let is_raw = current.video == VideoCodec::Raw;
    let has_speed = current.video.has_speed();

    let id_for_name = cur_id.clone();
    let id_for_container = cur_id.clone();
    let id_for_codec = cur_id.clone();
    let id_for_crf = cur_id.clone();
    let id_for_speed = cur_id.clone();
    let id_for_profile = cur_id.clone();
    let id_for_tune = cur_id.clone();
    let id_for_rate = cur_id.clone();
    let id_for_bitrate = cur_id.clone();
    let id_for_depth = cur_id.clone();
    let id_for_x265 = cur_id.clone();
    let id_for_dup = cur_id.clone();
    let id_for_del = cur_id.clone();

    rsx! {
        div { class: "manager",
            div { class: "manager-list",
                div { class: "manager-actions",
                    button {
                        class: "btn icon",
                        title: "{s_new}",
                        "aria-label": "{s_new}",
                        onclick: move |_| {
                            let p = Preset::new(&s_new);
                            selected.set(p.id.clone());
                            settings.write().presets.push(p);
                        },
                        Icon { width: 14, height: 14, icon: LdPlus }
                    }
                    button {
                        class: "btn icon",
                        title: "{s_dup}",
                        "aria-label": "{s_dup}",
                        onclick: move |_| {
                            let src = settings.read().presets.iter().find(|p| p.id == id_for_dup).cloned();
                            if let Some(mut p) = src {
                                p.id = engine::preset::new_id();
                                p.name = format!("{} 2", p.name);
                                selected.set(p.id.clone());
                                settings.write().presets.push(p);
                            }
                        },
                        Icon { width: 14, height: 14, icon: LdCopy }
                    }
                    button {
                        class: "btn icon",
                        title: "{s_del}",
                        "aria-label": "{s_del}",
                        disabled: !can_delete,
                        onclick: move |_| {
                            settings.write().presets.retain(|p| p.id != id_for_del);
                            let fixed = settings.read().clone().repaired();
                            selected.set(fixed.presets[0].id.clone());
                            settings.set(fixed);
                        },
                        Icon { width: 14, height: 14, icon: LdTrash2 }
                    }
                }
                div { class: "lv-scroll",
                    for p in presets.iter() {
                        {
                            let id = p.id.clone();
                            let id_down = p.id.clone();
                            let id_over = p.id.clone();
                            let active = p.id == cur_id;
                            let moving = dragging().as_deref() == Some(p.id.as_str());
                            rsx! {
                                button {
                                    key: "{p.id}",
                                    class: match (active, moving) {
                                        (_, true) => "lv-item reordering",
                                        (true, _) => "lv-item active",
                                        _ => "lv-item",
                                    },
                                    onpointerdown: move |e| {
                                        if e.trigger_button() == Some(MouseButton::Primary) {
                                            dragging.set(Some(id_down.clone()));
                                        }
                                    },
                                    onpointerenter: move |e| {
                                        let Some(from) = dragging() else { return };
                                        if !e.held_buttons().contains(MouseButton::Primary) {
                                            dragging.set(None);
                                            return;
                                        }
                                        reorder(&mut settings.write().presets, |p| p.id.as_str(), &from, &id_over);
                                    },
                                    onpointerup: move |_| dragging.set(None),
                                    onclick: move |_| {
                                        selected.set(id.clone());
                                        settings.write().preset_id = id.clone();
                                    },
                                    div { class: "lv-item-body",
                                        div { class: "lv-item-name", "{p.name}" }
                                    }
                                    span { class: "lv-item-badge", "{container_label(p.container)}" }
                                }
                            }
                        }
                    }
                }
                p { class: "hint", "{s_reorder}" }
            }
            div { class: "manager-form",
                div { class: "dlg-field",
                    label { class: "dlg-field-label", {t("lblPrsName")} }
                    input {
                        class: "input",
                        value: "{current.name}",
                        oninput: move |e| {
                            let v = e.value();
                            edit_preset(settings, &id_for_name, |p| p.name = v);
                        },
                    }
                }
                ChoiceField {
                    label: t("lblPrsContainer"),
                    value: container_id(current.container).to_owned(),
                    options: container_options,
                    on_change: move |v: String| {
                        if let Some(c) = Container::ALL.iter().copied().find(|c| container_id(*c) == v) {
                            edit_preset(settings, &id_for_container, |p| p.container = c);
                        }
                    },
                }
                ChoiceField {
                    label: t("lblPrsVideo"),
                    value: codec_id(current.video).to_owned(),
                    options: codec_options,
                    on_change: move |v: String| {
                        if let Some(c) = VideoCodec::ALL.iter().copied().find(|c| codec_id(*c) == v) {
                            edit_preset(settings, &id_for_codec, |p| {
                                p.video = c;
                                // A new codec starts from its own sensible quality, not the old one's number.
                                p.crf = c.crf_range().2;
                            });
                        }
                    },
                }
                if needs_x26x {
                    p { class: "hint", {t("msgPrsEncoderHint")} }
                }
                if is_prores {
                    ChoiceField {
                        label: t("lblPrsProResProfile"),
                        value: profile_id(current.prores).to_owned(),
                        options: profile_options,
                        on_change: move |v: String| {
                            if let Some(pr) = ProResProfile::ALL.iter().copied().find(|p| profile_id(*p) == v) {
                                edit_preset(settings, &id_for_profile, |p| p.prores = pr);
                            }
                        },
                    }
                }
                if let Some(maker) = current.video.graphics_maker() {
                    p {
                        class: "hint",
                        {match maker {
                            "amd" => t("msgPrsAmdHint"),
                            "nvidia" => t("msgPrsNvidiaHint"),
                            _ => t("msgPrsIntelHint"),
                        }}
                    }
                }
                if is_raw {
                    p { class: "hint", {t("msgPrsRawHint")} }
                }
                if rate_choice {
                    ChoiceField {
                        label: t("lblPrsRateMode"),
                        value: (if by_bitrate { "bitrate" } else { "crf" }).to_owned(),
                        options: vec![
                            ("crf".to_owned(), t("lblPrsRateCrf")),
                            ("bitrate".to_owned(), t("lblPrsRateBitrate")),
                        ],
                        on_change: move |v: String| {
                            let mode = if v == "bitrate" { RateMode::Bitrate } else { RateMode::Crf };
                            edit_preset(settings, &id_for_rate, |p| p.rate = mode);
                        },
                    }
                }
                if by_bitrate {
                    NumberField {
                        label: t("lblPrsBitrate"),
                        value: current.bitrate_kbps,
                        min: 50,
                        max: 200_000,
                        unit: "kbps".to_owned(),
                        on_change: move |v: u32| edit_preset(settings, &id_for_bitrate, |p| p.bitrate_kbps = v),
                    }
                } else if has_crf {
                    // The key makes the field remount when the codec changes, so it never shows a value
                    // from the old codec's scale.
                    NumberField {
                        key: "{codec_id(current.video)}",
                        label: if is_amd { t("lblPrsQualityQp") } else { t("lblPrsQuality") },
                        value: current.crf,
                        min: crf_lo,
                        max: crf_hi,
                        on_change: move |v: u32| edit_preset(settings, &id_for_crf, |p| p.crf = v),
                    }
                    p { class: "hint", {t("msgPrsQualityHint")} }
                }
                if ten_bit_choice {
                    ChoiceField {
                        label: t("lblPrsBitDepth"),
                        value: (if current.ten_bit { "10" } else { "8" }).to_owned(),
                        options: vec![("8".to_owned(), "8-bit".to_owned()), ("10".to_owned(), "10-bit".to_owned())],
                        on_change: move |v: String| {
                            let ten = v == "10";
                            edit_preset(settings, &id_for_depth, |p| p.ten_bit = ten);
                        },
                    }
                }
                if is_h265 {
                    div { class: "dlg-field",
                        label { class: "dlg-field-label", {t("lblPrsX265Params")} }
                        input {
                            class: "input",
                            value: "{current.x265_params}",
                            oninput: move |e| {
                                let v = e.value();
                                edit_preset(settings, &id_for_x265, |p| p.x265_params = v);
                            },
                        }
                        p { class: "hint", {t("msgPrsX265Hint")} }
                    }
                }
                if !current.video.tunes().is_empty() {
                    ChoiceField {
                        label: t("lblPrsTune"),
                        value: tune_id(current.tune).to_owned(),
                        options: std::iter::once(Tune::None)
                            .chain(current.video.tunes().iter().copied())
                            .map(|tu| (tune_id(tu).to_owned(), tune_label(tu, &lang)))
                            .collect::<Vec<_>>(),
                        on_change: move |v: String| {
                            let tune = [Tune::None, Tune::Film, Tune::Animation, Tune::Grain, Tune::StillImage, Tune::FastDecode, Tune::ZeroLatency]
                                .into_iter()
                                .find(|tu| tune_id(*tu) == v);
                            if let Some(tune) = tune {
                                edit_preset(settings, &id_for_tune, |p| p.tune = tune);
                            }
                        },
                    }
                }
                if has_speed {
                    ChoiceField {
                        label: t("lblPrsSpeed"),
                        value: match current.speed { Speed::Ultrafast => "ultrafast", Speed::Fast => "fast", Speed::Balanced => "balanced", Speed::Slow => "slow" }.to_owned(),
                        options: speed_options,
                        on_change: move |v: String| {
                            let speed = match v.as_str() {
                                "ultrafast" => Speed::Ultrafast,
                                "fast" => Speed::Fast,
                                "slow" => Speed::Slow,
                                _ => Speed::Balanced,
                            };
                            edit_preset(settings, &id_for_speed, |p| p.speed = speed);
                        },
                    }
                }
            }
        }
    }
}

/// Apply `edit` to the size with this id and keep its sides inside what a picture can have.
fn edit_size(mut settings: Signal<Settings>, id: &str, edit: impl FnOnce(&mut Size)) {
    let mut s = settings.write();
    if let Some(z) = s.sizes.iter_mut().find(|z| z.id == id) {
        edit(z);
        *z = z.clone().normalized();
    }
}

/// A labelled catalog number field that may be left empty ("no limit").
#[component]
fn OptionalNumberField(
    label: String,
    value: Option<u32>,
    min: u32,
    max: u32,
    placeholder: String,
    #[props(default)] unit: String,
    on_change: EventHandler<Option<u32>>,
) -> Element {
    rsx! {
        div { class: "dlg-field",
            label { class: "dlg-field-label", "{label}" }
            div { class: "manager-row",
                NumberInput {
                    value: value.map(f64::from),
                    min: Some(f64::from(min)),
                    max: Some(f64::from(max)),
                    step: 1.0,
                    width: format!("{}px", number_width(max) + 32),
                    placeholder,
                    allow_empty: true,
                    on_change: move |v: Option<f64>| on_change.call(v.map(|n| n.round().max(0.0) as u32)),
                }
                if !unit.is_empty() {
                    span { "{unit}" }
                }
            }
        }
    }
}

/// The size editor, a page of the Settings view: the sizes on the left, the box of the selected one on the right.
#[component]
pub fn SizeEditor(settings: Signal<Settings>) -> Element {
    let mut selected = use_signal(|| settings.read().size_id.clone());
    let mut dragging = use_signal(|| Option::<String>::None);

    let lang = settings.read().lang.clone();
    let t = |key: &str| i18n::t(&lang, key);

    let sizes = settings.read().sizes.clone();
    let current = sizes.iter().find(|z| z.id == selected()).or(sizes.first()).cloned();
    let cur_id = current.as_ref().map(|z| z.id.clone()).unwrap_or_default();

    let s_new = t("btnMngNew");
    let s_dup = t("btnMngDuplicate");
    let s_del = t("btnMngDelete");
    let s_reorder = t("msgMngReorderHint");

    let id_for_dup = cur_id.clone();
    let id_for_del = cur_id.clone();
    let id_for_name = cur_id.clone();
    let id_for_w = cur_id.clone();
    let id_for_h = cur_id.clone();

    rsx! {
        div { class: "manager",
            div { class: "manager-list",
                div { class: "manager-actions",
                    button {
                        class: "btn icon",
                        title: "{s_new}",
                        "aria-label": "{s_new}",
                        onclick: move |_| {
                            let z = Size::new(&s_new);
                            selected.set(z.id.clone());
                            settings.write().sizes.push(z);
                        },
                        Icon { width: 14, height: 14, icon: LdPlus }
                    }
                    button {
                        class: "btn icon",
                        title: "{s_dup}",
                        "aria-label": "{s_dup}",
                        disabled: current.is_none(),
                        onclick: move |_| {
                            let src = settings.read().sizes.iter().find(|z| z.id == id_for_dup).cloned();
                            if let Some(mut z) = src {
                                z.id = engine::preset::new_id();
                                z.name = format!("{} 2", z.name);
                                selected.set(z.id.clone());
                                settings.write().sizes.push(z);
                            }
                        },
                        Icon { width: 14, height: 14, icon: LdCopy }
                    }
                    button {
                        class: "btn icon",
                        title: "{s_del}",
                        "aria-label": "{s_del}",
                        disabled: current.is_none(),
                        onclick: move |_| {
                            settings.write().sizes.retain(|z| z.id != id_for_del);
                            let fixed = settings.read().clone().repaired();
                            selected.set(fixed.sizes.first().map(|z| z.id.clone()).unwrap_or_default());
                            settings.set(fixed);
                        },
                        Icon { width: 14, height: 14, icon: LdTrash2 }
                    }
                }
                div { class: "lv-scroll",
                    for z in sizes.iter() {
                        {
                            let id = z.id.clone();
                            let id_down = z.id.clone();
                            let id_over = z.id.clone();
                            let active = z.id == cur_id;
                            let moving = dragging().as_deref() == Some(z.id.as_str());
                            let limit = match (z.max_width, z.max_height) {
                                (Some(w), Some(h)) => format!("{w} x {h}"),
                                (Some(w), None) => format!("{w} x -"),
                                (None, Some(h)) => format!("- x {h}"),
                                (None, None) => String::new(),
                            };
                            rsx! {
                                button {
                                    key: "{z.id}",
                                    class: match (active, moving) {
                                        (_, true) => "lv-item reordering",
                                        (true, _) => "lv-item active",
                                        _ => "lv-item",
                                    },
                                    onpointerdown: move |e| {
                                        if e.trigger_button() == Some(MouseButton::Primary) {
                                            dragging.set(Some(id_down.clone()));
                                        }
                                    },
                                    onpointerenter: move |e| {
                                        let Some(from) = dragging() else { return };
                                        if !e.held_buttons().contains(MouseButton::Primary) {
                                            dragging.set(None);
                                            return;
                                        }
                                        reorder(&mut settings.write().sizes, |z| z.id.as_str(), &from, &id_over);
                                    },
                                    onpointerup: move |_| dragging.set(None),
                                    onclick: move |_| {
                                        selected.set(id.clone());
                                        settings.write().size_id = id.clone();
                                    },
                                    div { class: "lv-item-body",
                                        div { class: "lv-item-name", "{z.name}" }
                                    }
                                    span { class: "lv-item-badge", "{limit}" }
                                }
                            }
                        }
                    }
                }
                p { class: "hint", "{s_reorder}" }
            }
            // The key makes the fields remount when another size is selected, so they never keep a
            // half-typed draft of the previous one.
            div { class: "manager-form", key: "{cur_id}",
                if let Some(cur) = &current {
                    div { class: "dlg-field",
                        label { class: "dlg-field-label", {t("lblPrsName")} }
                        input {
                            class: "input",
                            value: "{cur.name}",
                            oninput: move |e| {
                                let v = e.value();
                                edit_size(settings, &id_for_name, |z| z.name = v);
                            },
                        }
                    }
                    OptionalNumberField {
                        label: t("lblSizeMaxWidth"),
                        value: cur.max_width,
                        min: engine::size::MIN_SIDE,
                        max: engine::size::MAX_SIDE,
                        placeholder: t("lblSizeNoLimit"),
                        unit: "px".to_owned(),
                        on_change: move |v: Option<u32>| edit_size(settings, &id_for_w, |z| z.max_width = v),
                    }
                    OptionalNumberField {
                        label: t("lblSizeMaxHeight"),
                        value: cur.max_height,
                        min: engine::size::MIN_SIDE,
                        max: engine::size::MAX_SIDE,
                        placeholder: t("lblSizeNoLimit"),
                        unit: "px".to_owned(),
                        on_change: move |v: Option<u32>| edit_size(settings, &id_for_h, |z| z.max_height = v),
                    }
                    p { class: "hint", {t("msgSizeHint")} }
                }
            }
        }
    }
}

/// The audio-entry editor, a page of the Settings view: the entries on the left, the options of the selected one on the right.
#[component]
pub fn AudioEditor(settings: Signal<Settings>) -> Element {
    let mut selected = use_signal(|| settings.read().audio_id.clone());
    let mut dragging = use_signal(|| Option::<String>::None);

    let lang = settings.read().lang.clone();
    let t = |key: &str| i18n::t(&lang, key);

    let entries = settings.read().audios.clone();
    let current = entries.iter().find(|a| a.id == selected()).or(entries.first()).cloned();
    let cur_id = current.as_ref().map(|a| a.id.clone()).unwrap_or_default();

    let s_new = t("btnMngNew");
    let s_dup = t("btnMngDuplicate");
    let s_del = t("btnMngDelete");
    let s_reorder = t("msgMngReorderHint");

    let mode_label = |a: Audio| match a {
        Audio::Aac => "AAC".to_owned(),
        Audio::Opus => "Opus".to_owned(),
        Audio::Ac3 => "AC-3".to_owned(),
        Audio::Pcm => "PCM".to_owned(),
        Audio::None => t("lblAudNone"),
        // Never an entry's mode: a copy comes from the original-audio rule only.
        Audio::Copy => String::new(),
    };
    let mode_options: Vec<(String, String)> = Audio::ENTRY_MODES.iter().map(|a| (audio_id(*a).to_owned(), mode_label(*a))).collect();

    let id_for_dup = cur_id.clone();
    let id_for_del = cur_id.clone();
    let id_for_name = cur_id.clone();
    let id_for_mode = cur_id.clone();
    let id_for_kbps = cur_id.clone();
    let id_for_channels = cur_id.clone();
    let id_for_gain = cur_id.clone();
    let id_for_loud = cur_id.clone();
    let channel_options = vec![
        (String::new(), t("lblPrsKeep")),
        ("1".to_owned(), t("lblPrsMono")),
        ("2".to_owned(), t("lblPrsStereo")),
    ];

    rsx! {
        div { class: "manager",
            div { class: "manager-list",
                div { class: "manager-actions",
                    button {
                        class: "btn icon",
                        title: "{s_new}",
                        "aria-label": "{s_new}",
                        onclick: move |_| {
                            let a = AudioEntry::new(&s_new);
                            selected.set(a.id.clone());
                            settings.write().audios.push(a);
                        },
                        Icon { width: 14, height: 14, icon: LdPlus }
                    }
                    button {
                        class: "btn icon",
                        title: "{s_dup}",
                        "aria-label": "{s_dup}",
                        disabled: current.is_none(),
                        onclick: move |_| {
                            let src = settings.read().audios.iter().find(|a| a.id == id_for_dup).cloned();
                            if let Some(mut a) = src {
                                a.id = engine::preset::new_id();
                                a.name = format!("{} 2", a.name);
                                selected.set(a.id.clone());
                                settings.write().audios.push(a);
                            }
                        },
                        Icon { width: 14, height: 14, icon: LdCopy }
                    }
                    button {
                        class: "btn icon",
                        title: "{s_del}",
                        "aria-label": "{s_del}",
                        disabled: current.is_none(),
                        onclick: move |_| {
                            settings.write().audios.retain(|a| a.id != id_for_del);
                            let fixed = settings.read().clone().repaired();
                            selected.set(fixed.audios.first().map(|a| a.id.clone()).unwrap_or_default());
                            settings.set(fixed);
                        },
                        Icon { width: 14, height: 14, icon: LdTrash2 }
                    }
                }
                div { class: "lv-scroll",
                    for a in entries.iter() {
                        {
                            let id = a.id.clone();
                            let id_down = a.id.clone();
                            let id_over = a.id.clone();
                            let active = a.id == cur_id;
                            let moving = dragging().as_deref() == Some(a.id.as_str());
                            rsx! {
                                button {
                                    key: "{a.id}",
                                    class: match (active, moving) {
                                        (_, true) => "lv-item reordering",
                                        (true, _) => "lv-item active",
                                        _ => "lv-item",
                                    },
                                    onpointerdown: move |e| {
                                        if e.trigger_button() == Some(MouseButton::Primary) {
                                            dragging.set(Some(id_down.clone()));
                                        }
                                    },
                                    onpointerenter: move |e| {
                                        let Some(from) = dragging() else { return };
                                        if !e.held_buttons().contains(MouseButton::Primary) {
                                            dragging.set(None);
                                            return;
                                        }
                                        reorder(&mut settings.write().audios, |a| a.id.as_str(), &from, &id_over);
                                    },
                                    onpointerup: move |_| dragging.set(None),
                                    onclick: move |_| {
                                        selected.set(id.clone());
                                        settings.write().audio_id = id.clone();
                                    },
                                    div { class: "lv-item-body",
                                        div { class: "lv-item-name", "{a.name}" }
                                    }
                                }
                            }
                        }
                    }
                }
                p { class: "hint", "{s_reorder}" }
            }
            // The key makes the fields remount when another entry is selected.
            div { class: "manager-form", key: "{cur_id}",
                if let Some(cur) = &current {
                    div { class: "dlg-field",
                        label { class: "dlg-field-label", {t("lblPrsName")} }
                        input {
                            class: "input",
                            value: "{cur.name}",
                            oninput: move |e| {
                                let v = e.value();
                                edit_audio(settings, &id_for_name, |a| a.name = v);
                            },
                        }
                    }
                    ChoiceField {
                        label: t("lblAudMode"),
                        value: audio_id(cur.mode).to_owned(),
                        options: mode_options,
                        on_change: move |v: String| {
                            if let Some(m) = Audio::ENTRY_MODES.iter().copied().find(|m| audio_id(*m) == v) {
                                edit_audio(settings, &id_for_mode, |a| a.mode = m);
                            }
                        },
                    }
                    if cur.mode.has_bitrate() {
                        NumberField {
                            label: t("lblAudBitrate"),
                            value: cur.kbps,
                            min: 32,
                            max: 512,
                            unit: "kbps".to_owned(),
                            on_change: move |v: u32| edit_audio(settings, &id_for_kbps, |a| a.kbps = v),
                        }
                    }
                    if cur.mode.encodes() {
                        div { class: "dlg-field",
                            label { class: "dlg-field-label", {t("lblAudGain")} }
                            div { class: "manager-row",
                                NumberInput {
                                    value: Some(f64::from(cur.gain_db)),
                                    min: Some(-30.0),
                                    max: Some(30.0),
                                    step: 1.0,
                                    width: "74px".to_owned(),
                                    on_change: move |v: Option<f64>| {
                                        if let Some(n) = v {
                                            edit_audio(settings, &id_for_gain, |a| a.gain_db = n.round() as i32);
                                        }
                                    },
                                }
                                span { "dB" }
                            }
                        }
                        div { class: "dlg-field",
                            label { class: "check-label",
                                input {
                                    class: "check-box",
                                    r#type: "checkbox",
                                    checked: cur.loudness,
                                    onchange: move |ev| {
                                        let on = ev.checked();
                                        edit_audio(settings, &id_for_loud, |a| a.loudness = on);
                                    },
                                }
                                {t("lblAudLoudness")}
                            }
                        }
                        ChoiceField {
                            label: t("lblPrsChannels"),
                            value: cur.channels.map(|n| n.to_string()).unwrap_or_default(),
                            options: channel_options,
                            on_change: move |v: String| {
                                let n = v.parse::<u8>().ok();
                                edit_audio(settings, &id_for_channels, |a| a.channels = n);
                            },
                        }
                    }
                    p { class: "hint", {t("msgAudioHint")} }
                }
            }
        }
    }
}

/// Apply `edit` to the audio entry with this id and keep it inside what an entry can be.
fn edit_audio(mut settings: Signal<Settings>, id: &str, edit: impl FnOnce(&mut AudioEntry)) {
    let mut s = settings.write();
    if let Some(a) = s.audios.iter_mut().find(|a| a.id == id) {
        edit(a);
        *a = a.clone().normalized();
    }
}

/// Apply `edit` to the LUT with this id and keep its path clean.
fn edit_lut(mut settings: Signal<Settings>, id: &str, edit: impl FnOnce(&mut Lut)) {
    let mut s = settings.write();
    if let Some(l) = s.luts.iter_mut().find(|l| l.id == id) {
        edit(l);
        *l = l.clone().normalized();
    }
}

/// The LUT editor, a page of the Settings view: the LUTs on the left, the name and the file of the selected one on the right.
#[component]
pub fn LutEditor(settings: Signal<Settings>) -> Element {
    let mut selected = use_signal(|| settings.read().lut_id.clone());
    let mut dragging = use_signal(|| Option::<String>::None);

    let lang = settings.read().lang.clone();
    let t = |key: &str| i18n::t(&lang, key);

    let luts = settings.read().luts.clone();
    let current = luts.iter().find(|l| l.id == selected()).or(luts.first()).cloned();
    let cur_id = current.as_ref().map(|l| l.id.clone()).unwrap_or_default();

    let s_new = t("btnMngNew");
    let s_dup = t("btnMngDuplicate");
    let s_del = t("btnMngDelete");
    let s_reorder = t("msgMngReorderHint");
    let s_browse = t("btnDstBrowse");

    let id_for_dup = cur_id.clone();
    let id_for_del = cur_id.clone();
    let id_for_name = cur_id.clone();
    let id_for_path = cur_id.clone();
    let id_for_browse = cur_id.clone();

    rsx! {
        div { class: "manager",
            div { class: "manager-list",
                div { class: "manager-actions",
                    button {
                        class: "btn icon",
                        title: "{s_new}",
                        "aria-label": "{s_new}",
                        onclick: move |_| {
                            let l = Lut::new(&s_new);
                            selected.set(l.id.clone());
                            settings.write().luts.push(l);
                        },
                        Icon { width: 14, height: 14, icon: LdPlus }
                    }
                    button {
                        class: "btn icon",
                        title: "{s_dup}",
                        "aria-label": "{s_dup}",
                        disabled: current.is_none(),
                        onclick: move |_| {
                            let src = settings.read().luts.iter().find(|l| l.id == id_for_dup).cloned();
                            if let Some(mut l) = src {
                                l.id = engine::preset::new_id();
                                l.name = format!("{} 2", l.name);
                                selected.set(l.id.clone());
                                settings.write().luts.push(l);
                            }
                        },
                        Icon { width: 14, height: 14, icon: LdCopy }
                    }
                    button {
                        class: "btn icon",
                        title: "{s_del}",
                        "aria-label": "{s_del}",
                        disabled: current.is_none(),
                        onclick: move |_| {
                            settings.write().luts.retain(|l| l.id != id_for_del);
                            let fixed = settings.read().clone().repaired();
                            selected.set(fixed.luts.first().map(|l| l.id.clone()).unwrap_or_default());
                            settings.set(fixed);
                        },
                        Icon { width: 14, height: 14, icon: LdTrash2 }
                    }
                }
                div { class: "lv-scroll",
                    for l in luts.iter() {
                        {
                            let id = l.id.clone();
                            let id_down = l.id.clone();
                            let id_over = l.id.clone();
                            let active = l.id == cur_id;
                            let moving = dragging().as_deref() == Some(l.id.as_str());
                            rsx! {
                                button {
                                    key: "{l.id}",
                                    class: match (active, moving) {
                                        (_, true) => "lv-item reordering",
                                        (true, _) => "lv-item active",
                                        _ => "lv-item",
                                    },
                                    onpointerdown: move |e| {
                                        if e.trigger_button() == Some(MouseButton::Primary) {
                                            dragging.set(Some(id_down.clone()));
                                        }
                                    },
                                    onpointerenter: move |e| {
                                        let Some(from) = dragging() else { return };
                                        if !e.held_buttons().contains(MouseButton::Primary) {
                                            dragging.set(None);
                                            return;
                                        }
                                        reorder(&mut settings.write().luts, |l| l.id.as_str(), &from, &id_over);
                                    },
                                    onpointerup: move |_| dragging.set(None),
                                    onclick: move |_| {
                                        selected.set(id.clone());
                                        settings.write().lut_id = id.clone();
                                    },
                                    div { class: "lv-item-body",
                                        div { class: "lv-item-name", "{l.name}" }
                                    }
                                }
                            }
                        }
                    }
                }
                p { class: "hint", "{s_reorder}" }
            }
            // The key makes the fields remount when another LUT is selected.
            div { class: "manager-form", key: "{cur_id}",
                if let Some(cur) = &current {
                    div { class: "dlg-field",
                        label { class: "dlg-field-label", {t("lblPrsName")} }
                        input {
                            class: "input",
                            value: "{cur.name}",
                            oninput: move |e| {
                                let v = e.value();
                                edit_lut(settings, &id_for_name, |l| l.name = v);
                            },
                        }
                    }
                    div { class: "dlg-field",
                        label { class: "dlg-field-label", {t("lblLutFile")} }
                        div { class: "manager-row",
                            input {
                                class: "input",
                                value: "{cur.path}",
                                oninput: move |e| {
                                    let v = e.value();
                                    edit_lut(settings, &id_for_path, |l| l.path = v);
                                },
                            }
                            button {
                                class: "btn",
                                onclick: move |_| {
                                    let id = id_for_browse.clone();
                                    spawn(async move {
                                        if let Some(f) = rfd::AsyncFileDialog::new().add_filter("LUT", &["cube"]).pick_file().await {
                                            let v = f.path().display().to_string();
                                            edit_lut(settings, &id, |l| l.path = v);
                                        }
                                    });
                                },
                                Icon { width: 14, height: 14, icon: LdFolderOpen }
                                "{s_browse}"
                            }
                        }
                        if !cur.file_exists() {
                            p { class: "hint warn", {t("msgLutMissing")} }
                        }
                    }
                    p { class: "hint", {t("msgLutHint")} }
                }
            }
        }
    }
}

/// Apply `edit` to the destination with this id.
fn edit_destination(mut settings: Signal<Settings>, id: &str, edit: impl FnOnce(&mut Destination)) {
    let mut s = settings.write();
    if let Some(d) = s.destinations.iter_mut().find(|d| d.id == id) {
        edit(d);
    }
}

/// The destination editor, a page of the Settings view: the destinations on the left, the folder of the selected one on the right.
#[component]
pub fn DestinationEditor(settings: Signal<Settings>) -> Element {
    let mut selected = use_signal(|| settings.read().destination_id.clone());
    let mut dragging = use_signal(|| Option::<String>::None);

    let lang = settings.read().lang.clone();
    let t = |key: &str| i18n::t(&lang, key);

    let entries = settings.read().destinations.clone();
    let current = entries.iter().find(|d| d.id == selected()).or(entries.first()).cloned();
    let cur_id = current.as_ref().map(|d| d.id.clone()).unwrap_or_default();

    let s_new = t("btnMngNew");
    let s_dup = t("btnMngDuplicate");
    let s_del = t("btnMngDelete");
    let s_reorder = t("msgMngReorderHint");
    let s_browse = t("btnDstBrowse");

    let id_for_dup = cur_id.clone();
    let id_for_del = cur_id.clone();
    let id_for_name = cur_id.clone();
    let id_for_folder = cur_id.clone();
    let id_for_browse = cur_id.clone();

    rsx! {
        div { class: "manager",
            div { class: "manager-list",
                div { class: "manager-actions",
                    button {
                        class: "btn icon",
                        title: "{s_new}",
                        "aria-label": "{s_new}",
                        onclick: move |_| {
                            let d = Destination::new(&s_new);
                            selected.set(d.id.clone());
                            settings.write().destinations.push(d);
                        },
                        Icon { width: 14, height: 14, icon: LdPlus }
                    }
                    button {
                        class: "btn icon",
                        title: "{s_dup}",
                        "aria-label": "{s_dup}",
                        disabled: current.is_none(),
                        onclick: move |_| {
                            let src = settings.read().destinations.iter().find(|d| d.id == id_for_dup).cloned();
                            if let Some(mut d) = src {
                                d.id = engine::preset::new_id();
                                d.name = format!("{} 2", d.name);
                                selected.set(d.id.clone());
                                settings.write().destinations.push(d);
                            }
                        },
                        Icon { width: 14, height: 14, icon: LdCopy }
                    }
                    button {
                        class: "btn icon",
                        title: "{s_del}",
                        "aria-label": "{s_del}",
                        disabled: current.is_none(),
                        onclick: move |_| {
                            settings.write().destinations.retain(|d| d.id != id_for_del);
                            let fixed = settings.read().clone().repaired();
                            selected.set(fixed.destinations.first().map(|d| d.id.clone()).unwrap_or_default());
                            settings.set(fixed);
                        },
                        Icon { width: 14, height: 14, icon: LdTrash2 }
                    }
                }
                div { class: "lv-scroll",
                    for d in entries.iter() {
                        {
                            let id = d.id.clone();
                            let id_down = d.id.clone();
                            let id_over = d.id.clone();
                            let active = d.id == cur_id;
                            let moving = dragging().as_deref() == Some(d.id.as_str());
                            rsx! {
                                button {
                                    key: "{d.id}",
                                    class: match (active, moving) {
                                        (_, true) => "lv-item reordering",
                                        (true, _) => "lv-item active",
                                        _ => "lv-item",
                                    },
                                    onpointerdown: move |e| {
                                        if e.trigger_button() == Some(MouseButton::Primary) {
                                            dragging.set(Some(id_down.clone()));
                                        }
                                    },
                                    onpointerenter: move |e| {
                                        let Some(from) = dragging() else { return };
                                        if !e.held_buttons().contains(MouseButton::Primary) {
                                            dragging.set(None);
                                            return;
                                        }
                                        reorder(&mut settings.write().destinations, |d| d.id.as_str(), &from, &id_over);
                                    },
                                    onpointerup: move |_| dragging.set(None),
                                    onclick: move |_| {
                                        selected.set(id.clone());
                                        settings.write().destination_id = id.clone();
                                    },
                                    div { class: "lv-item-body",
                                        div { class: "lv-item-name", "{d.name}" }
                                        div { class: "lv-item-sub", "{d.folder}" }
                                    }
                                }
                            }
                        }
                    }
                }
                p { class: "hint", "{s_reorder}" }
            }
            // The key makes the fields remount when another destination is selected.
            div { class: "manager-form", key: "{cur_id}",
                if let Some(cur) = &current {
                    div { class: "dlg-field",
                        label { class: "dlg-field-label", {t("lblPrsName")} }
                        input {
                            class: "input",
                            value: "{cur.name}",
                            oninput: move |e| {
                                let v = e.value();
                                edit_destination(settings, &id_for_name, |d| d.name = v);
                            },
                        }
                    }
                    div { class: "dlg-field",
                        label { class: "dlg-field-label", {t("lblDstFolder")} }
                        div { class: "manager-row",
                            input {
                                class: "input",
                                placeholder: t("lblMainDestSource"),
                                value: "{cur.folder}",
                                oninput: move |e| {
                                    let v = e.value();
                                    edit_destination(settings, &id_for_folder, |d| d.folder = v);
                                },
                            }
                            button {
                                class: "btn",
                                title: t("tipMainChooseFolder"),
                                onclick: move |_| {
                                    let id = id_for_browse.clone();
                                    spawn(async move {
                                        if let Some(dir) = rfd::AsyncFileDialog::new().pick_folder().await {
                                            let v = dir.path().display().to_string();
                                            edit_destination(settings, &id, |d| d.folder = v);
                                        }
                                    });
                                },
                                Icon { width: 14, height: 14, icon: LdFolderOpen }
                                "{s_browse}"
                            }
                        }
                    }
                    p { class: "hint", {t("msgDstHint")} }
                }
            }
        }
    }
}

/// Apply `edit` to the effects entry with this id and keep it inside what an entry can be.
fn edit_effects(mut settings: Signal<Settings>, id: &str, edit: impl FnOnce(&mut EffectsEntry)) {
    let mut s = settings.write();
    if let Some(e) = s.effects.iter_mut().find(|e| e.id == id) {
        edit(e);
        *e = e.clone().normalized();
    }
}

/// One effect of the entry as a card: its title, its fields, and the button that removes it. Being on the
/// page means it is on; removing it is turning it off.
#[component]
fn EffectCard(label: String, remove: String, on_remove: EventHandler<()>, #[props(default)] children: Element) -> Element {
    rsx! {
        details { class: "manager-advanced", open: true,
            summary { span { class: "dlg-title", "{label}" } }
            div { class: "manager-advanced-body",
                {children}
                div { class: "manager-actions",
                    button {
                        class: "btn icon",
                        title: "{remove}",
                        "aria-label": "{remove}",
                        onclick: move |_| on_remove.call(()),
                        Icon { width: 14, height: 14, icon: LdTrash2 }
                    }
                }
            }
        }
    }
}

fn corner_label(c: Corner, lang: &str) -> String {
    let t = |key: &str| i18n::t(lang, key);
    match c {
        Corner::TopLeft => t("lblFxTopLeft"),
        Corner::TopRight => t("lblFxTopRight"),
        Corner::BottomLeft => t("lblFxBottomLeft"),
        Corner::BottomRight => t("lblFxBottomRight"),
    }
}

fn rotation_label(r: Rotation, lang: &str) -> String {
    let t = |key: &str| i18n::t(lang, key);
    match r {
        // Never offered: "no turn" is an entry without the effect.
        Rotation::None => String::new(),
        Rotation::Cw => t("lblRotCw"),
        Rotation::Ccw => t("lblRotCcw"),
        Rotation::CwFlip => t("lblRotCwFlip"),
        Rotation::CcwFlip => t("lblRotCcwFlip"),
        Rotation::Half => t("lblRotHalf"),
        Rotation::FlipH => t("lblRotFlipH"),
        Rotation::FlipV => t("lblRotFlipV"),
    }
}

fn kind_label(k: Kind, lang: &str) -> String {
    let t = |key: &str| i18n::t(lang, key);
    match k {
        Kind::Deinterlace => t("lblFxDeinterlace"),
        Kind::Crop => t("lblFxCrop"),
        Kind::Fps => t("lblFxFps"),
        Kind::Strip => t("lblFxStrip"),
        Kind::Rotate => t("lblFxRotate"),
        Kind::BlurFit => t("lblFxBlurFit"),
        Kind::Burn => t("lblFxBurn"),
        Kind::Stereo => t("lblFxStereo"),
        Kind::Reverse => t("lblFxReverse"),
        Kind::Watermark => t("lblFxWatermark"),
        Kind::Text => t("lblFxText"),
    }
}

/// The fields of one effect, written back to the entry at once.
#[component]
fn EffectItem(settings: Signal<Settings>, id: String, lang: String, item: Effect) -> Element {
    let t = |key: &str| i18n::t(&lang, key);
    let s_remove = t("tipFxRemove");
    let kind = item.kind();
    let (id_remove, id_a, id_b, id_c, id_d, id_e, id_f) = (id.clone(), id.clone(), id.clone(), id.clone(), id.clone(), id.clone(), id.clone());
    let corner_options: Vec<(String, String)> = Corner::ALL.iter().map(|c| (c.id().to_owned(), corner_label(*c, &lang))).collect();
    let corner_options_b = corner_options.clone();

    rsx! {
        EffectCard {
            label: kind_label(kind, &lang),
            remove: s_remove,
            on_remove: move |_| edit_effects(settings, &id_remove, |e| e.items.retain(|x| x.kind() != kind)),
            match item {
                Effect::Rotate { rotation } => rsx! {
                    ChoiceField {
                        label: t("lblFxTurn"),
                        value: rotation.id().to_owned(),
                        options: Rotation::ALL.iter().filter(|r| **r != Rotation::None).map(|r| (r.id().to_owned(), rotation_label(*r, &lang))).collect::<Vec<_>>(),
                        on_change: move |v: String| {
                            if let Some(r) = Rotation::from_id(&v).filter(|r| *r != Rotation::None) {
                                edit_effects(settings, &id_a, |e| {
                                    for x in &mut e.items {
                                        if let Effect::Rotate { rotation } = x {
                                            *rotation = r;
                                        }
                                    }
                                });
                            }
                        },
                    }
                },
                Effect::Deinterlace => rsx! {
                    p { class: "hint", {t("msgFxDeinterlaceHint")} }
                },
                Effect::Strip => rsx! {
                    p { class: "hint", {t("msgFxStripHint")} }
                },
                Effect::Fps(f) => rsx! {
                    ChoiceField {
                        label: t("lblFxFpsRate"),
                        value: f.value.clone(),
                        options: FRAME_RATES.iter().map(|(label, value)| (value.to_string(), format!("{label} fps"))).collect::<Vec<_>>(),
                        on_change: move |v: String| edit_effects(settings, &id_a, |e| {
                            for x in &mut e.items {
                                if let Effect::Fps(f) = x {
                                    f.value = v.clone();
                                }
                            }
                        }),
                    }
                },
                Effect::Crop(c) => rsx! {
                    NumberField {
                        label: t("lblFxCropLeft"),
                        value: c.left,
                        min: 0,
                        max: Crop::MAX,
                        unit: "px".to_owned(),
                        on_change: move |v: u32| edit_crop(settings, &id_a, |c| c.left = v),
                    }
                    NumberField {
                        label: t("lblFxCropRight"),
                        value: c.right,
                        min: 0,
                        max: Crop::MAX,
                        unit: "px".to_owned(),
                        on_change: move |v: u32| edit_crop(settings, &id_b, |c| c.right = v),
                    }
                    NumberField {
                        label: t("lblFxCropTop"),
                        value: c.top,
                        min: 0,
                        max: Crop::MAX,
                        unit: "px".to_owned(),
                        on_change: move |v: u32| edit_crop(settings, &id_c, |c| c.top = v),
                    }
                    NumberField {
                        label: t("lblFxCropBottom"),
                        value: c.bottom,
                        min: 0,
                        max: Crop::MAX,
                        unit: "px".to_owned(),
                        on_change: move |v: u32| edit_crop(settings, &id_d, |c| c.bottom = v),
                    }
                    p { class: "hint", {t("msgFxCropHint")} }
                },
                Effect::BlurFit(b) => rsx! {
                    NumberField {
                        label: t("lblFxBlurHeight"),
                        value: b.height,
                        min: 120,
                        max: 4320,
                        unit: "px".to_owned(),
                        on_change: move |v: u32| edit_blur(settings, &id_a, |b| b.height = v),
                    }
                    NumberField {
                        label: t("lblFxBlurSigma"),
                        value: b.blur,
                        min: 1,
                        max: 100,
                        on_change: move |v: u32| edit_blur(settings, &id_b, |b| b.blur = v),
                    }
                    p { class: "hint", {t("msgFxBlurFitHint")} }
                },
                Effect::Burn => rsx! {
                    p { class: "hint", {t("msgFxBurnHint")} }
                },
                Effect::Stereo => rsx! {
                    p { class: "hint", {t("msgFxStereoHint")} }
                },
                Effect::Reverse => rsx! {
                    p { class: "hint", {t("msgFxReverseHint")} }
                },
                Effect::Watermark(w) => {
                    let missing = !w.file_exists();
                    let s_browse = t("btnDstBrowse");
                    rsx! {
                        div { class: "dlg-field",
                            label { class: "dlg-field-label", {t("lblFxImage")} }
                            div { class: "manager-row",
                                input {
                                    class: "input",
                                    value: "{w.path}",
                                    oninput: move |ev| {
                                        let v = ev.value();
                                        edit_watermark(settings, &id_a, |w| w.path = v);
                                    },
                                }
                                button {
                                    class: "btn",
                                    onclick: move |_| {
                                        let id = id_f.clone();
                                        spawn(async move {
                                            let filter = ["png", "jpg", "jpeg", "webp", "bmp"];
                                            if let Some(f) = rfd::AsyncFileDialog::new().add_filter("Image", &filter).pick_file().await {
                                                let v = f.path().display().to_string();
                                                edit_watermark(settings, &id, |w| w.path = v);
                                            }
                                        });
                                    },
                                    Icon { width: 14, height: 14, icon: LdFolderOpen }
                                    "{s_browse}"
                                }
                            }
                            if missing && !w.path.is_empty() {
                                p { class: "hint warn", {t("msgLutMissing")} }
                            }
                        }
                        ChoiceField {
                            label: t("lblFxCorner"),
                            value: w.corner.id().to_owned(),
                            options: corner_options,
                            on_change: move |v: String| {
                                if let Some(c) = Corner::from_id(&v) {
                                    edit_watermark(settings, &id_b, |w| w.corner = c);
                                }
                            },
                        }
                        NumberField {
                            label: t("lblFxMargin"),
                            value: w.margin,
                            min: 0,
                            max: 2000,
                            unit: "px".to_owned(),
                            on_change: move |v: u32| edit_watermark(settings, &id_c, |w| w.margin = v),
                        }
                        OptionalNumberField {
                            label: t("lblFxWidth"),
                            value: w.width,
                            min: 16,
                            max: 4096,
                            placeholder: t("lblSizeNoLimit"),
                            unit: "px".to_owned(),
                            on_change: move |v: Option<u32>| edit_watermark(settings, &id_d, |w| w.width = v),
                        }
                        NumberField {
                            label: t("lblFxOpacity"),
                            value: w.opacity,
                            min: 10,
                            max: 100,
                            unit: "%".to_owned(),
                            on_change: move |v: u32| edit_watermark(settings, &id_e, |w| w.opacity = v),
                        }
                    }
                }
                Effect::Text(tx) => rsx! {
                    div { class: "dlg-field",
                        label { class: "dlg-field-label", {t("lblFxText")} }
                        input {
                            class: "input",
                            value: "{tx.text}",
                            oninput: move |ev| {
                                let v = ev.value();
                                edit_text(settings, &id_a, |x| x.text = v);
                            },
                        }
                        p { class: "hint", {t("msgFxTextHint")} }
                    }
                    ChoiceField {
                        label: t("lblFxCorner"),
                        value: tx.corner.id().to_owned(),
                        options: corner_options_b,
                        on_change: move |v: String| {
                            if let Some(c) = Corner::from_id(&v) {
                                edit_text(settings, &id_b, |x| x.corner = c);
                            }
                        },
                    }
                    NumberField {
                        label: t("lblFxMargin"),
                        value: tx.margin,
                        min: 0,
                        max: 2000,
                        unit: "px".to_owned(),
                        on_change: move |v: u32| edit_text(settings, &id_c, |x| x.margin = v),
                    }
                    NumberField {
                        label: t("lblFxTextSize"),
                        value: tx.size,
                        min: 1,
                        max: 20,
                        unit: "%".to_owned(),
                        on_change: move |v: u32| edit_text(settings, &id_d, |x| x.size = v),
                    }
                    div { class: "dlg-field",
                        label { class: "check-label",
                            input {
                                class: "check-box",
                                r#type: "checkbox",
                                checked: tx.boxed,
                                onchange: move |ev| {
                                    let on = ev.checked();
                                    edit_text(settings, &id_e, |x| x.boxed = on);
                                },
                            }
                            {t("lblFxTextBox")}
                        }
                    }
                },
            }
        }
    }
}

fn edit_crop(settings: Signal<Settings>, id: &str, f: impl FnOnce(&mut Crop)) {
    edit_effects(settings, id, |e| {
        for x in &mut e.items {
            if let Effect::Crop(c) = x {
                f(c);
                break;
            }
        }
    });
}

fn edit_blur(settings: Signal<Settings>, id: &str, f: impl FnOnce(&mut BlurFit)) {
    edit_effects(settings, id, |e| {
        for x in &mut e.items {
            if let Effect::BlurFit(b) = x {
                f(b);
                break;
            }
        }
    });
}

fn edit_watermark(settings: Signal<Settings>, id: &str, f: impl FnOnce(&mut Watermark)) {
    edit_effects(settings, id, |e| {
        for x in &mut e.items {
            if let Effect::Watermark(w) = x {
                f(w);
                break;
            }
        }
    });
}

fn edit_text(settings: Signal<Settings>, id: &str, f: impl FnOnce(&mut TextOverlay)) {
    edit_effects(settings, id, |e| {
        for x in &mut e.items {
            if let Effect::Text(t) = x {
                f(t);
                break;
            }
        }
    });
}

/// The effects editor, a page of the Settings view: the entries on the left; on the right the name of the
/// selected one and a card for each effect it holds, with a menu that adds the kinds it does not have yet.
#[component]
pub fn EffectsEditor(settings: Signal<Settings>) -> Element {
    let mut selected = use_signal(|| settings.read().effects_id.clone());
    let mut dragging = use_signal(|| Option::<String>::None);

    let lang = settings.read().lang.clone();
    let t = |key: &str| i18n::t(&lang, key);

    let entries = settings.read().effects.clone();
    let current = entries.iter().find(|e| e.id == selected()).or(entries.first()).cloned();
    let cur_id = current.as_ref().map(|e| e.id.clone()).unwrap_or_default();

    let s_new = t("btnMngNew");
    let s_dup = t("btnMngDuplicate");
    let s_del = t("btnMngDelete");
    let s_reorder = t("msgMngReorderHint");
    let s_add = t("lblFxAdd");

    let id_for_dup = cur_id.clone();
    let id_for_del = cur_id.clone();
    let id_for_name = cur_id.clone();
    let id_for_add = cur_id.clone();

    rsx! {
        div { class: "manager",
            div { class: "manager-list",
                div { class: "manager-actions",
                    button {
                        class: "btn icon",
                        title: "{s_new}",
                        "aria-label": "{s_new}",
                        onclick: move |_| {
                            let e = EffectsEntry::new(&s_new);
                            selected.set(e.id.clone());
                            settings.write().effects.push(e);
                        },
                        Icon { width: 14, height: 14, icon: LdPlus }
                    }
                    button {
                        class: "btn icon",
                        title: "{s_dup}",
                        "aria-label": "{s_dup}",
                        disabled: current.is_none(),
                        onclick: move |_| {
                            let src = settings.read().effects.iter().find(|e| e.id == id_for_dup).cloned();
                            if let Some(mut e) = src {
                                e.id = engine::preset::new_id();
                                e.name = format!("{} 2", e.name);
                                selected.set(e.id.clone());
                                settings.write().effects.push(e);
                            }
                        },
                        Icon { width: 14, height: 14, icon: LdCopy }
                    }
                    button {
                        class: "btn icon",
                        title: "{s_del}",
                        "aria-label": "{s_del}",
                        disabled: current.is_none(),
                        onclick: move |_| {
                            settings.write().effects.retain(|e| e.id != id_for_del);
                            let fixed = settings.read().clone().repaired();
                            selected.set(fixed.effects.first().map(|e| e.id.clone()).unwrap_or_default());
                            settings.set(fixed);
                        },
                        Icon { width: 14, height: 14, icon: LdTrash2 }
                    }
                }
                div { class: "lv-scroll",
                    for e in entries.iter() {
                        {
                            let id = e.id.clone();
                            let id_down = e.id.clone();
                            let id_over = e.id.clone();
                            let active = e.id == cur_id;
                            let moving = dragging().as_deref() == Some(e.id.as_str());
                            rsx! {
                                button {
                                    key: "{e.id}",
                                    class: match (active, moving) {
                                        (_, true) => "lv-item reordering",
                                        (true, _) => "lv-item active",
                                        _ => "lv-item",
                                    },
                                    onpointerdown: move |ev| {
                                        if ev.trigger_button() == Some(MouseButton::Primary) {
                                            dragging.set(Some(id_down.clone()));
                                        }
                                    },
                                    onpointerenter: move |ev| {
                                        let Some(from) = dragging() else { return };
                                        if !ev.held_buttons().contains(MouseButton::Primary) {
                                            dragging.set(None);
                                            return;
                                        }
                                        reorder(&mut settings.write().effects, |e| e.id.as_str(), &from, &id_over);
                                    },
                                    onpointerup: move |_| dragging.set(None),
                                    onclick: move |_| {
                                        selected.set(id.clone());
                                        settings.write().effects_id = id.clone();
                                    },
                                    div { class: "lv-item-body",
                                        div { class: "lv-item-name", "{e.name}" }
                                    }
                                }
                            }
                        }
                    }
                }
                p { class: "hint", "{s_reorder}" }
            }
            // The key makes the fields remount when another entry is selected.
            div { class: "manager-form", key: "{cur_id}",
                if let Some(cur) = &current {
                    div { class: "dlg-field",
                        label { class: "dlg-field-label", {t("lblPrsName")} }
                        input {
                            class: "input",
                            value: "{cur.name}",
                            oninput: move |ev| {
                                let v = ev.value();
                                edit_effects(settings, &id_for_name, |x| x.name = v);
                            },
                        }
                    }
                    for item in cur.items.clone() {
                        EffectItem { key: "{item.kind().id()}", settings, id: cur_id.clone(), lang: lang.clone(), item }
                    }
                    {
                        let addable: Vec<Kind> = Kind::ALL.into_iter().filter(|k| cur.get(*k).is_none()).collect();
                        rsx! {
                            if !addable.is_empty() {
                                div { class: "dlg-field",
                                    select {
                                        class: "select",
                                        title: "{s_add}",
                                        "aria-label": "{s_add}",
                                        value: "",
                                        onchange: move |ev| {
                                            if let Some(k) = Kind::from_id(&ev.value()) {
                                                edit_effects(settings, &id_for_add, |x| x.items.push(k.new_effect()));
                                            }
                                        },
                                        option { value: "", selected: true, "{s_add}" }
                                        for k in addable {
                                            option { key: "{k.id()}", value: "{k.id()}", {kind_label(k, &lang)} }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
