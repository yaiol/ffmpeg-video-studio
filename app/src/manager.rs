//! The preset editor, a page of the Settings view: a list on the left and the options of the selected
//! preset on the right. Every change is stored at once, there is no Save button.
//!
//! ⚠ CLAUDE: NO HARDCODED UI STRINGS. Container and codec names are proper nouns and stay literal;
//! everything else goes through the t() lookup with a literal key at the call site.

use crate::{catalog::UisNum, i18n, settings::Settings};
use dioxus::{html::input_data::MouseButton, prelude::*};
use dioxus_free_icons::{
    Icon,
    icons::ld_icons::{LdCopy, LdFolderOpen, LdPlus, LdTrash2},
};
use engine::{
    AudioEntry, Destination, MetadataCleanup, Preset, Size,
    audio::Audio,
    metadata::Mode,
    preset::{Container, PresetEncoder, ProResProfile, RateMode, Speed, Tune, VideoCodec},
};

/// The keys of the Settings tabs this module provides.
pub const PRESETS: &str = "presets";
pub const SIZES: &str = "sizes";
pub const AUDIO: &str = "audio";
pub const DESTINATIONS: &str = "destinations";
pub const EFFECTS: &str = "effects";
pub const AUDIO_EFFECTS: &str = "audio-effects";
pub const FFMPEG: &str = "ffmpeg";
pub const METADATA: &str = "metadata";
pub const NOTIFICATIONS: &str = "notifications";

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
pub(crate) fn reorder<T>(items: &mut Vec<T>, id_of: impl Fn(&T) -> &str, dragged: &str, target: &str) {
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
fn LclFldNum(
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
        div { class: "uis-fld-labelled",
            label { class: "uis-fld-labelled-label", "{label}" }
            div { class: "manager-row",
                UisNum {
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
        current.container.video_codecs().iter().filter(|c| c.graphics_maker().is_none()).map(|c| (codec_id(*c).to_owned(), codec_label(*c))).collect();
    // Only H.264 and H.265 have a graphics-card version: the other codecs have no engine to choose.
    let has_engine_choice = matches!(current.video, VideoCodec::H264 | VideoCodec::H265);
    let encoder_options = [
        ("default".to_owned(), t("lblPrsEncoderDefault")),
        ("software".to_owned(), t("lblPrsEncoderSoftware")),
    ];
    let encoder_now = if current.encoder == PresetEncoder::Software { "software" } else { "default" };
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
    let rate_choice = current.video.has_rate_choice();
    let by_bitrate = rate_choice && current.rate == RateMode::Bitrate;
    let ten_bit_choice = current.video.supports_ten_bit();
    let is_h265 = current.video == VideoCodec::H265;
    let is_raw = current.video == VideoCodec::Raw;
    let has_speed = current.video.has_speed();

    let id_for_name = cur_id.clone();
    let id_for_container = cur_id.clone();
    let id_for_codec = cur_id.clone();
    let id_for_encoder = cur_id.clone();
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
            div { class: "manager-list uis-lvw-reorder",
                div { class: "manager-actions",
                    button {
                        class: "uis-btn ico",
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
                        class: "uis-btn ico",
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
                        class: "uis-btn ico",
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
                div { class: "uis-lvw-scroll",
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
                                        (_, true) => "uis-lvw-item reordering",
                                        (true, _) => "uis-lvw-item active",
                                        _ => "uis-lvw-item",
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
                                    div { class: "uis-lvw-item-body",
                                        div { class: "uis-lvw-item-name", "{p.name}" }
                                    }
                                    span { class: "uis-lvw-item-badge", "{container_label(p.container)}" }
                                }
                            }
                        }
                    }
                }
                p { class: "uis-hnt", "{s_reorder}" }
            }
            div { class: "manager-form",
                div { class: "uis-fld-labelled",
                    label { class: "uis-fld-labelled-label", {t("lblPrsName")} }
                    input {
                        class: "uis-txt",
                        value: "{current.name}",
                        oninput: move |e| {
                            let v = e.value();
                            edit_preset(settings, &id_for_name, |p| p.name = v);
                        },
                    }
                }
                div { class: "uis-fld-labelled",
                    label { class: "uis-fld-labelled-label", {t("lblPrsContainer")} }
                    select {
                        class: "uis-ddn-std",
                        value: container_id(current.container).to_owned(),
                        onchange: {
                            let cb = move |v: String| {
                                if let Some(c) = Container::ALL.iter().copied().find(|c| container_id(*c) == v) {
                                    edit_preset(settings, &id_for_container, |p| p.container = c);
                                }
                            };
                            move |e| cb(e.value())
                        },
                        for (oid, text) in container_options {
                            option { key: "{oid}", value: "{oid}", selected: oid == container_id(current.container).to_owned(), "{text}" }
                        }
                    }
                }
                div { class: "uis-fld-labelled",
                    label { class: "uis-fld-labelled-label", {t("lblPrsVideo")} }
                    select {
                        class: "uis-ddn-std",
                        value: codec_id(current.video).to_owned(),
                        onchange: {
                            let cb = move |v: String| {
                                if let Some(c) = VideoCodec::ALL.iter().copied().find(|c| codec_id(*c) == v) {
                                    edit_preset(settings, &id_for_codec, |p| {
                                        p.video = c;
                                        // A new codec starts from its own sensible quality, not the old one's number.
                                        p.crf = c.crf_range().2;
                                    });
                                }
                            };
                            move |e| cb(e.value())
                        },
                        for (oid, text) in codec_options {
                            option { key: "{oid}", value: "{oid}", selected: oid == codec_id(current.video).to_owned(), "{text}" }
                        }
                    }
                }
                if has_engine_choice {
                    div { class: "uis-fld-labelled",
                        label { class: "uis-fld-labelled-label", {t("lblPrsEncoder")} }
                        select {
                            class: "uis-ddn-std",
                            value: encoder_now.to_owned(),
                            onchange: move |e| {
                                let chosen = if e.value() == "software" { PresetEncoder::Software } else { PresetEncoder::Default };
                                edit_preset(settings, &id_for_encoder, |p| p.encoder = chosen);
                            },
                            for (oid, text) in encoder_options {
                                option { key: "{oid}", value: "{oid}", selected: oid == encoder_now, "{text}" }
                            }
                        }
                    }
                }
                if needs_x26x {
                    p { class: "uis-hnt", {t("msgPrsEncoderHint")} }
                }
                if is_prores {
                    div { class: "uis-fld-labelled",
                        label { class: "uis-fld-labelled-label", {t("lblPrsProResProfile")} }
                        select {
                            class: "uis-ddn-std",
                            value: profile_id(current.prores).to_owned(),
                            onchange: {
                                let cb = move |v: String| {
                                    if let Some(pr) = ProResProfile::ALL.iter().copied().find(|p| profile_id(*p) == v) {
                                        edit_preset(settings, &id_for_profile, |p| p.prores = pr);
                                    }
                                };
                                move |e| cb(e.value())
                            },
                            for (oid, text) in profile_options {
                                option { key: "{oid}", value: "{oid}", selected: oid == profile_id(current.prores).to_owned(), "{text}" }
                            }
                        }
                    }
                }
                if is_raw {
                    p { class: "uis-hnt", {t("msgPrsRawHint")} }
                }
                if rate_choice {
                    div { class: "uis-fld-labelled",
                        label { class: "uis-fld-labelled-label", {t("lblPrsRateMode")} }
                        select {
                            class: "uis-ddn-std",
                            value: (if by_bitrate { "bitrate" } else { "crf" }).to_owned(),
                            onchange: {
                                let cb = move |v: String| {
                                    let mode = if v == "bitrate" { RateMode::Bitrate } else { RateMode::Crf };
                                    edit_preset(settings, &id_for_rate, |p| p.rate = mode);
                                };
                                move |e| cb(e.value())
                            },
                            for (oid, text) in vec![
                                ("crf".to_owned(), t("lblPrsRateCrf")),
                                ("bitrate".to_owned(), t("lblPrsRateBitrate")),
                            ] {
                                option { key: "{oid}", value: "{oid}", selected: oid == (if by_bitrate { "bitrate" } else { "crf" }).to_owned(), "{text}" }
                            }
                        }
                    }
                }
                if by_bitrate {
                    LclFldNum {
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
                    LclFldNum {
                        key: "{codec_id(current.video)}",
                        label: t("lblPrsQuality"),
                        value: current.crf,
                        min: crf_lo,
                        max: crf_hi,
                        on_change: move |v: u32| edit_preset(settings, &id_for_crf, |p| p.crf = v),
                    }
                    p { class: "uis-hnt", {t("msgPrsQualityHint")} }
                }
                if ten_bit_choice {
                    div { class: "uis-fld-labelled",
                        label { class: "uis-fld-labelled-label", {t("lblPrsBitDepth")} }
                        select {
                            class: "uis-ddn-std",
                            value: (if current.ten_bit { "10" } else { "8" }).to_owned(),
                            onchange: {
                                let cb = move |v: String| {
                                    let ten = v == "10";
                                    edit_preset(settings, &id_for_depth, |p| p.ten_bit = ten);
                                };
                                move |e| cb(e.value())
                            },
                            for (oid, text) in vec![("8".to_owned(), "8-bit".to_owned()), ("10".to_owned(), "10-bit".to_owned())] {
                                option { key: "{oid}", value: "{oid}", selected: oid == (if current.ten_bit { "10" } else { "8" }).to_owned(), "{text}" }
                            }
                        }
                    }
                }
                if is_h265 {
                    div { class: "uis-fld-labelled",
                        label { class: "uis-fld-labelled-label", {t("lblPrsX265Params")} }
                        input {
                            class: "uis-txt",
                            value: "{current.x265_params}",
                            oninput: move |e| {
                                let v = e.value();
                                edit_preset(settings, &id_for_x265, |p| p.x265_params = v);
                            },
                        }
                        p { class: "uis-hnt", {t("msgPrsX265Hint")} }
                    }
                }
                if !current.video.tunes().is_empty() {
                    div { class: "uis-fld-labelled",
                        label { class: "uis-fld-labelled-label", {t("lblPrsTune")} }
                        select {
                            class: "uis-ddn-std",
                            value: tune_id(current.tune).to_owned(),
                            onchange: {
                                let cb = move |v: String| {
                                    let tune = [Tune::None, Tune::Film, Tune::Animation, Tune::Grain, Tune::StillImage, Tune::FastDecode, Tune::ZeroLatency]
                                        .into_iter()
                                        .find(|tu| tune_id(*tu) == v);
                                    if let Some(tune) = tune {
                                        edit_preset(settings, &id_for_tune, |p| p.tune = tune);
                                    }
                                };
                                move |e| cb(e.value())
                            },
                            for (oid, text) in std::iter::once(Tune::None)
                                .chain(current.video.tunes().iter().copied())
                                .map(|tu| (tune_id(tu).to_owned(), tune_label(tu, &lang)))
                                .collect::<Vec<_>>() {
                                option { key: "{oid}", value: "{oid}", selected: oid == tune_id(current.tune).to_owned(), "{text}" }
                            }
                        }
                    }
                }
                if has_speed {
                    div { class: "uis-fld-labelled",
                        label { class: "uis-fld-labelled-label", {t("lblPrsSpeed")} }
                        select {
                            class: "uis-ddn-std",
                            value: match current.speed { Speed::Ultrafast => "ultrafast", Speed::Fast => "fast", Speed::Balanced => "balanced", Speed::Slow => "slow" }.to_owned(),
                            onchange: {
                                let cb = move |v: String| {
                                    let speed = match v.as_str() {
                                        "ultrafast" => Speed::Ultrafast,
                                        "fast" => Speed::Fast,
                                        "slow" => Speed::Slow,
                                        _ => Speed::Balanced,
                                    };
                                    edit_preset(settings, &id_for_speed, |p| p.speed = speed);
                                };
                                move |e| cb(e.value())
                            },
                            for (oid, text) in speed_options {
                                option { key: "{oid}", value: "{oid}", selected: oid == match current.speed { Speed::Ultrafast => "ultrafast", Speed::Fast => "fast", Speed::Balanced => "balanced", Speed::Slow => "slow" }.to_owned(), "{text}" }
                            }
                        }
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
fn LclFldNumOptional(
    label: String,
    value: Option<u32>,
    min: u32,
    max: u32,
    placeholder: String,
    #[props(default)] unit: String,
    on_change: EventHandler<Option<u32>>,
) -> Element {
    rsx! {
        div { class: "uis-fld-labelled",
            label { class: "uis-fld-labelled-label", "{label}" }
            div { class: "manager-row",
                UisNum {
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
            div { class: "manager-list uis-lvw-reorder",
                div { class: "manager-actions",
                    button {
                        class: "uis-btn ico",
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
                        class: "uis-btn ico",
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
                        class: "uis-btn ico",
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
                div { class: "uis-lvw-scroll",
                    for z in sizes.iter() {
                        {
                            let id = z.id.clone();
                            let id_down = z.id.clone();
                            let id_over = z.id.clone();
                            let active = z.id == cur_id;
                            let moving = dragging().as_deref() == Some(z.id.as_str());
                            // A height alone is the usual case and reads as the video world writes it: 1080p. Both sides
                            // are "width x height"; a width alone is said in words (nothing else could be unambiguous).
                            let limit = match (z.max_width, z.max_height) {
                                (Some(w), Some(h)) => format!("{w} x {h}"),
                                (Some(w), None) => t("lblSizeChipWide").replace("{w}", &w.to_string()),
                                (None, Some(h)) => format!("{h}p"),
                                (None, None) => String::new(),
                            };
                            rsx! {
                                button {
                                    key: "{z.id}",
                                    class: match (active, moving) {
                                        (_, true) => "uis-lvw-item reordering",
                                        (true, _) => "uis-lvw-item active",
                                        _ => "uis-lvw-item",
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
                                    div { class: "uis-lvw-item-body",
                                        div { class: "uis-lvw-item-name", "{z.name}" }
                                    }
                                    span { class: "uis-lvw-item-badge", "{limit}" }
                                }
                            }
                        }
                    }
                }
                p { class: "uis-hnt", "{s_reorder}" }
            }
            // The key makes the fields remount when another size is selected, so they never keep a
            // half-typed draft of the previous one.
            div { class: "manager-form", key: "{cur_id}",
                if let Some(cur) = &current {
                    div { class: "uis-fld-labelled",
                        label { class: "uis-fld-labelled-label", {t("lblPrsName")} }
                        input {
                            class: "uis-txt",
                            value: "{cur.name}",
                            oninput: move |e| {
                                let v = e.value();
                                edit_size(settings, &id_for_name, |z| z.name = v);
                            },
                        }
                    }
                    LclFldNumOptional {
                        label: t("lblSizeMaxWidth"),
                        value: cur.max_width,
                        min: engine::size::MIN_SIDE,
                        max: engine::size::MAX_SIDE,
                        placeholder: t("lblSizeNoLimit"),
                        unit: "px".to_owned(),
                        on_change: move |v: Option<u32>| edit_size(settings, &id_for_w, |z| z.max_width = v),
                    }
                    LclFldNumOptional {
                        label: t("lblSizeMaxHeight"),
                        value: cur.max_height,
                        min: engine::size::MIN_SIDE,
                        max: engine::size::MAX_SIDE,
                        placeholder: t("lblSizeNoLimit"),
                        unit: "px".to_owned(),
                        on_change: move |v: Option<u32>| edit_size(settings, &id_for_h, |z| z.max_height = v),
                    }
                    p { class: "uis-hnt", {t("msgSizeHint")} }
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

    rsx! {
        div { class: "manager",
            div { class: "manager-list uis-lvw-reorder",
                div { class: "manager-actions",
                    button {
                        class: "uis-btn ico",
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
                        class: "uis-btn ico",
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
                        class: "uis-btn ico",
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
                div { class: "uis-lvw-scroll",
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
                                        (_, true) => "uis-lvw-item reordering",
                                        (true, _) => "uis-lvw-item active",
                                        _ => "uis-lvw-item",
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
                                    div { class: "uis-lvw-item-body",
                                        div { class: "uis-lvw-item-name", "{a.name}" }
                                    }
                                }
                            }
                        }
                    }
                }
                p { class: "uis-hnt", "{s_reorder}" }
            }
            // The key makes the fields remount when another entry is selected.
            div { class: "manager-form", key: "{cur_id}",
                if let Some(cur) = &current {
                    div { class: "uis-fld-labelled",
                        label { class: "uis-fld-labelled-label", {t("lblPrsName")} }
                        input {
                            class: "uis-txt",
                            value: "{cur.name}",
                            oninput: move |e| {
                                let v = e.value();
                                edit_audio(settings, &id_for_name, |a| a.name = v);
                            },
                        }
                    }
                    div { class: "uis-fld-labelled",
                        label { class: "uis-fld-labelled-label", {t("lblAudMode")} }
                        select {
                            class: "uis-ddn-std",
                            value: audio_id(cur.mode).to_owned(),
                            onchange: {
                                let cb = move |v: String| {
                                    if let Some(m) = Audio::ENTRY_MODES.iter().copied().find(|m| audio_id(*m) == v) {
                                        edit_audio(settings, &id_for_mode, |a| a.mode = m);
                                    }
                                };
                                move |e| cb(e.value())
                            },
                            for (oid, text) in mode_options {
                                option { key: "{oid}", value: "{oid}", selected: oid == audio_id(cur.mode).to_owned(), "{text}" }
                            }
                        }
                    }
                    if cur.mode.has_bitrate() {
                        LclFldNum {
                            label: t("lblAudBitrate"),
                            value: cur.kbps,
                            min: 32,
                            max: 512,
                            unit: "kbps".to_owned(),
                            on_change: move |v: u32| edit_audio(settings, &id_for_kbps, |a| a.kbps = v),
                        }
                    }
                    p { class: "uis-hnt", {t("msgAudioHint")} }
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

/// Apply `edit` to the metadata setting with this id.
fn edit_metadata(mut settings: Signal<Settings>, id: &str, edit: impl FnOnce(&mut MetadataCleanup)) {
    let mut s = settings.write();
    if let Some(m) = s.metadata.iter_mut().find(|m| m.id == id) {
        edit(m);
    }
}

fn is_separator(c: char) -> bool {
    c == '\n' || c == ';'
}

/// The byte range of the name a caret at `pos` (UTF-16 units; `usize::MAX` = at the end) is in: from after the previous
/// separator to the next one.
fn entry_span(text: &str, pos: usize) -> (usize, usize) {
    let mut units = 0usize;
    let mut at = text.len();
    for (i, ch) in text.char_indices() {
        if units >= pos {
            at = i;
            break;
        }
        units += ch.len_utf16();
    }
    let start = text[..at].rfind(is_separator).map_or(0, |i| i + 1);
    let end = text[at..].find(is_separator).map_or(text.len(), |i| at + i);
    (start, end)
}

/// `text` with the byte range `span` replaced by `name`, and the caret position (UTF-16 units) at the end of that name.
fn replace_span(text: &str, span: (usize, usize), name: &str) -> (String, usize) {
    let (start, end) = span;
    let caret = text[..start].encode_utf16().count() + name.encode_utf16().count();
    (format!("{}{}{}", &text[..start], name, &text[end..]), caret)
}

/// The names a metadata setting is about: one per line, or several separated by `;`. The text is stored as typed, line
/// by line, and tidied only when a conversion uses it, otherwise the blank line a user is about to fill would vanish on
/// every keystroke. The parent gives it a `key` of the setting's id, so it starts over when another one is picked.
///
/// Under the box, the known names that fit the line being typed (the one the caret is on) are offered as chips; a click
/// puts the name in place of what was typed.
#[component]
fn MetaNamesField(settings: Signal<Settings>, id: String, lang: String, mode: Mode) -> Element {
    let initial = settings
        .read()
        .metadata
        .iter()
        .find(|m| m.id == id)
        .map(|m| m.exceptions.join(&char::from(10).to_string()))
        .unwrap_or_default();
    let mut raw = use_signal(|| initial);
    // The caret as a position in the text (UTF-16 units, as the box reports it); the end until known.
    let mut caret = use_signal(|| usize::MAX);
    let t = |key: &str| i18n::t(&lang, key);
    // With "keep all" the list names what is removed; with "remove all" it names what is kept.
    let label = match mode {
        Mode::Keep => t("lblMetaRemoveNames"),
        Mode::Delete => t("lblMetaKeepNames"),
    };
    let hint = t("msgMetaNamesHint");
    let box_id = format!("meta-names-{id}");

    // Ask the box where the caret is. Typing, clicking and the arrow keys all move it.
    let track = {
        let box_id = box_id.clone();
        move || {
            let box_id = box_id.clone();
            spawn(async move {
                let mut ev = document::eval(&format!("dioxus.send(document.getElementById('{box_id}')?.selectionStart ?? -1);"));
                if let Ok(pos) = ev.recv::<i64>().await {
                    caret.set(if pos < 0 { usize::MAX } else { pos as usize });
                }
            });
        }
    };

    let text = raw();
    let (start, end) = entry_span(&text, caret());
    let typed = &text[start..end];
    let taken: Vec<String> = text[..start].split(is_separator).chain(text[end..].split(is_separator)).map(str::to_owned).collect();
    let suggestions = engine::metadata::suggest_names(typed, &taken, 8);

    let id_ev = id.clone();
    let oninput_track = track.clone();
    let onkeyup_track = track.clone();
    let onclick_track = track;
    rsx! {
        div { class: "uis-fld-labelled",
            label { class: "uis-fld-labelled-label", "{label}" }
            textarea {
                id: "{box_id}",
                class: "uis-txt-area",
                rows: "5",
                spellcheck: "false",
                value: "{raw}",
                oninput: move |e| {
                    let text = e.value();
                    let list: Vec<String> = text.lines().map(str::to_owned).collect();
                    raw.set(text);
                    edit_metadata(settings, &id_ev, |m| m.exceptions = list);
                    oninput_track();
                },
                onkeyup: move |_| onkeyup_track(),
                onclick: move |_| onclick_track(),
            }
            if !suggestions.is_empty() {
                div { class: "manager-suggest",
                    for name in suggestions {
                        {
                            let box_id = box_id.clone();
                            let id = id.clone();
                            rsx! {
                                button {
                                    key: "{name}",
                                    class: "uis-chp",
                                    onclick: move |_| {
                                        let (new_text, end) = replace_span(&raw(), (start, end), name);
                                        let list: Vec<String> = new_text.lines().map(str::to_owned).collect();
                                        raw.set(new_text);
                                        caret.set(end);
                                        edit_metadata(settings, &id, |m| m.exceptions = list);
                                        // Back into the box, caret at the end of the name just placed.
                                        let _ = document::eval(&format!(
                                            "const b = document.getElementById('{box_id}'); if (b) {{ b.focus(); b.setSelectionRange({end}, {end}); }}"
                                        ));
                                    },
                                    "{name}"
                                }
                            }
                        }
                    }
                }
            }
            p { class: "uis-hnt", "{hint}" }
        }
    }
}

/// The metadata page of Settings: named entries, each saying which tags are removed and whether the chapters go too.
/// There is never none: a fresh install holds one that keeps everything.
#[component]
pub fn MetadataEditor(settings: Signal<Settings>) -> Element {
    let mut selected = use_signal(|| settings.read().metadata_id.clone());

    let lang = settings.read().lang.clone();
    let t = |key: &str| i18n::t(&lang, key);

    let entries = settings.read().metadata.clone();
    // An unknown selection falls back to the first entry; there is never none.
    let current = entries.iter().find(|m| m.id == selected()).or(entries.first()).cloned();
    let can_delete = entries.len() > 1;
    let cur_id = current.as_ref().map(|m| m.id.clone()).unwrap_or_default();
    // In the saved order, which the user changes by dragging.
    let listed = entries.clone();
    let mut dragging = use_signal(|| Option::<String>::None);
    let s_reorder = t("msgMngReorderHint");

    let s_new = t("btnMngNew");
    let s_dup = t("btnMngDuplicate");
    let s_del = t("btnMngDelete");
    let s_name = t("lblPrsName");
    let s_mode = t("lblMetaMode");
    let s_mode_keep = t("lblMetaModeKeep");
    let s_mode_delete = t("lblMetaModeDelete");
    let s_chapters = t("lblMetaRemoveChapters");

    let (id_name, id_mode, id_chapters, id_dup, id_del) = (cur_id.clone(), cur_id.clone(), cur_id.clone(), cur_id.clone(), cur_id.clone());

    rsx! {
        div { class: "manager",
            div { class: "manager-list uis-lvw-reorder",
                div { class: "manager-actions",
                    button {
                        class: "uis-btn ico",
                        title: "{s_new}",
                        "aria-label": "{s_new}",
                        onclick: move |_| {
                            let m = MetadataCleanup::new(&s_new);
                            selected.set(m.id.clone());
                            let mut s = settings.write();
                            s.metadata_id = m.id.clone();
                            s.metadata.push(m);
                        },
                        Icon { width: 14, height: 14, icon: LdPlus }
                    }
                    button {
                        class: "uis-btn ico",
                        title: "{s_dup}",
                        "aria-label": "{s_dup}",
                        onclick: move |_| {
                            let src = settings.read().metadata.iter().find(|m| m.id == id_dup).cloned();
                            if let Some(mut m) = src {
                                m.id = engine::preset::new_id();
                                m.name = format!("{} 2", m.name);
                                selected.set(m.id.clone());
                                let mut s = settings.write();
                                s.metadata_id = m.id.clone();
                                s.metadata.push(m);
                            }
                        },
                        Icon { width: 14, height: 14, icon: LdCopy }
                    }
                    button {
                        class: "uis-btn ico",
                        title: "{s_del}",
                        "aria-label": "{s_del}",
                        disabled: !can_delete,
                        onclick: move |_| {
                            settings.write().metadata.retain(|m| m.id != id_del);
                            let fixed = settings.read().clone().repaired();
                            selected.set(fixed.metadata_id.clone());
                            settings.set(fixed);
                        },
                        Icon { width: 14, height: 14, icon: LdTrash2 }
                    }
                }
                div { class: "uis-lvw-scroll",
                    for m in listed.iter() {
                        {
                            let id = m.id.clone();
                            let id_down = m.id.clone();
                            let id_over = m.id.clone();
                            let active = m.id == cur_id;
                            let moving = dragging().as_deref() == Some(m.id.as_str());
                            rsx! {
                                button {
                                    key: "{m.id}",
                                    class: match (active, moving) {
                                        (_, true) => "uis-lvw-item reordering",
                                        (true, _) => "uis-lvw-item active",
                                        _ => "uis-lvw-item",
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
                                        reorder(&mut settings.write().metadata, |m| m.id.as_str(), &from, &id_over);
                                    },
                                    onpointerup: move |_| dragging.set(None),
                                    onclick: move |_| {
                                        selected.set(id.clone());
                                        settings.write().metadata_id = id.clone();
                                    },
                                    div { class: "uis-lvw-item-body",
                                        div { class: "uis-lvw-item-name", "{m.name}" }
                                    }
                                }
                            }
                        }
                    }
                }
                p { class: "uis-hnt", "{s_reorder}" }
            }
            div { class: "manager-form",
                if let Some(m) = current {
                  Fragment { key: "{cur_id}",
                    div { class: "uis-fld-labelled",
                        label { class: "uis-fld-labelled-label", "{s_name}" }
                        input {
                            class: "uis-txt",
                            value: "{m.name}",
                            oninput: move |e| {
                                let v = e.value();
                                edit_metadata(settings, &id_name, |x| x.name = v);
                            },
                        }
                    }
                    div { class: "uis-fld-labelled",
                        label { class: "uis-fld-labelled-label", "{s_mode}" }
                        select {
                            class: "uis-ddn-std",
                            value: if m.mode == Mode::Delete { "delete" } else { "keep" },
                            onchange: move |e| {
                                let mode = if e.value() == "delete" { Mode::Delete } else { Mode::Keep };
                                edit_metadata(settings, &id_mode, |x| x.mode = mode);
                            },
                            option { value: "keep", selected: m.mode == Mode::Keep, "{s_mode_keep}" }
                            option { value: "delete", selected: m.mode == Mode::Delete, "{s_mode_delete}" }
                        }
                    }
                    MetaNamesField { settings, id: cur_id.clone(), lang: lang.clone(), mode: m.mode }
                    div { class: "uis-fld-labelled",
                        label { class: "uis-chk-lbl",
                            input {
                                class: "uis-chk-box",
                                r#type: "checkbox",
                                checked: m.remove_chapters,
                                onchange: move |e| {
                                    let v = e.checked();
                                    edit_metadata(settings, &id_chapters, |x| x.remove_chapters = v);
                                },
                            }
                            "{s_chapters}"
                        }
                    }
                  }
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
            div { class: "manager-list uis-lvw-reorder",
                div { class: "manager-actions",
                    button {
                        class: "uis-btn ico",
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
                        class: "uis-btn ico",
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
                        class: "uis-btn ico",
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
                div { class: "uis-lvw-scroll",
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
                                        (_, true) => "uis-lvw-item reordering",
                                        (true, _) => "uis-lvw-item active",
                                        _ => "uis-lvw-item",
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
                                    div { class: "uis-lvw-item-body",
                                        div { class: "uis-lvw-item-name", "{d.name}" }
                                        div { class: "uis-lvw-item-sub", "{d.folder}" }
                                    }
                                }
                            }
                        }
                    }
                }
                p { class: "uis-hnt", "{s_reorder}" }
            }
            // The key makes the fields remount when another destination is selected.
            div { class: "manager-form", key: "{cur_id}",
                if let Some(cur) = &current {
                    div { class: "uis-fld-labelled",
                        label { class: "uis-fld-labelled-label", {t("lblPrsName")} }
                        input {
                            class: "uis-txt",
                            value: "{cur.name}",
                            oninput: move |e| {
                                let v = e.value();
                                edit_destination(settings, &id_for_name, |d| d.name = v);
                            },
                        }
                    }
                    div { class: "uis-fld-labelled",
                        label { class: "uis-fld-labelled-label", {t("lblDstFolder")} }
                        div { class: "manager-row",
                            input {
                                class: "uis-txt",
                                placeholder: t("lblMainDestSource"),
                                value: "{cur.folder}",
                                oninput: move |e| {
                                    let v = e.value();
                                    edit_destination(settings, &id_for_folder, |d| d.folder = v);
                                },
                            }
                            button {
                                class: "uis-btn",
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
                    p { class: "uis-hnt", {t("msgDstHint")} }
                }
            }
        }
    }
}

/// The Notifications tab of Settings.
#[component]
pub fn NotifyEditor(settings: Signal<Settings>) -> Element {
    let lang = settings.read().lang.clone();
    let s_notify = i18n::t(&lang, "lblStgNotify");
    let on = settings.read().notify;
    rsx! {
        div { class: "manager",
            div { class: "manager-form",
                div { class: "uis-fld-labelled",
                    label { class: "uis-chk-lbl",
                        input {
                            class: "uis-chk-box",
                            r#type: "checkbox",
                            checked: on,
                            onchange: move |e| settings.write().notify = e.checked(),
                        }
                        "{s_notify}"
                    }
                }
            }
        }
    }
}
