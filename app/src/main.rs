// ⚠ CLAUDE - NO HARDCODED UI STRINGS IN THIS FILE.
//   Every string a user can read - text, title, aria-label - MUST go through the t() lookup, and
//   the key MUST be a string literal at the call site. Add the key to locales/en.json, then
//   translate, sort and audit every language with the i18n scripts. Never paste translations by hand.

mod catalog;
mod converter;
mod effects_editor;
mod effects_favourites;
mod effects_form;
mod i18n;
mod icons;
mod ffmpeg_panel;
mod manager;
mod notify;
mod runner;
mod settings;
mod site;
mod window_accent;
mod window_menu;
mod work;
mod work_channels;
mod work_colour;
mod work_compressor;
mod work_command;
mod work_curves;
mod work_effects;
mod work_level;
mod preview_session;
mod work_preview;
mod work_picture;
mod work_text;
mod work_tone;

use catalog::{UisBarWindow, UisAppTab, UisIcoGithub, UisBtnNews, UisBtnUpdate, UisNewsLoader, UisStgView, news};
use converter::{Converter, ConverterState};
use ffmpeg_panel::FfmpegSetting;
use effects_editor::EffectsEditor;
use effects_favourites::EffectsFavourites;
use runner::RunState;
use work::{Work, WorkState};
use manager::{AudioEditor, DestinationEditor, MetadataEditor, NotifyEditor, PresetEditor, SizeEditor};
use dioxus::prelude::*;
use dioxus_free_icons::{
    Icon,
    icons::ld_icons::{LdAudioLines, LdBell, LdCircleHelp, LdEraser, LdFileText, LdFolder, LdLayers, LdScaling, LdSettings, LdVideo, LdSparkles, LdTerminal, LdVolume2, LdWrench},
};

pub(crate) const CSS: [&str; 8] = [
    include_str!("../assets/uis-colors.css"),
    include_str!("../assets/uis-app.css"),
    include_str!("../assets/uis-bar-window.css"),
    include_str!("../assets/uis-listview.css"),
    include_str!("../assets/uis-fld-labelled.css"),
    include_str!("../assets/uis-sld.css"),
    include_str!("../assets/app.css"),
    include_str!("../assets/effects_form.css"),
];

/// The two ways to use the app: the batch view (many files, presets) or the work view (one file at a time).
/// Not saved: the app always opens on the batch view.
#[derive(Clone, Copy, PartialEq)]
enum Mode {
    Batch,
    Work,
}

// ─── App identity - single source of truth: Cargo.toml (see build.rs) ───────────────────────────
const APP_NAME: &str = env!("YAIOL_PRODUCT_NAME");
const APP_ID: &str = env!("CARGO_PKG_NAME");
const APP_VERSION: &str = env!("CARGO_PKG_VERSION");

/// The narrowest the window may be: the file list and the options panel side by side.
pub const MIN_WIDTH: f64 = 640.0;
/// The shortest the window may be until the main view has measured what its panel needs, and if it
/// never can.
pub const MIN_HEIGHT_FALLBACK: f64 = 440.0;

/// This build's identity, `<version>.<counter>`, as ONE contiguous string that survives into the
/// executable. The apps-cockpit finds it by its `yaiol-build:` prefix to tell a buffered build from
/// the release it follows - the counter cannot ride in Cargo's three-part version, and `dx bundle`
/// writes its own Windows version resource, so this is the one channel that is entirely ours.
/// ⚠ CLAUDE: keep it a single `concat!` of literals, and read it through `black_box` (as
/// app_version_build does). A plain `BUILD_ID["yaiol-build:".len()..]` is constant-folded and the
/// optimiser keeps only the suffix - the PREFIX the cockpit searches for disappears from the exe and
/// every build reads as the bare release (found 2026-10-12 by grepping the staged exe).
const BUILD_ID: &str = concat!("yaiol-build:", env!("YAIOL_BUILD_ID"));

/// The full build identity shown to the user: the release version plus the build counter (the 4th
/// segment a buffered commit bumps), or just the release version on a clean build.
/// ⚠ CLAUDE: DISPLAY ONLY. The release version (`CARGO_PKG_VERSION`) stays plain three-part semver
/// and must not absorb the counter: an update check compares it against the published version, and
/// four parts is not semver.
fn app_version_build() -> String {
    let full = std::hint::black_box(BUILD_ID);
    let id = &full["yaiol-build:".len()..];
    id.strip_suffix(".0").unwrap_or(id).to_owned()
}

/// A window that opens at a known size and place. Only a debug build needs it written out: see main().
#[cfg(debug_assertions)]
fn explicit_placement(window: dioxus::desktop::WindowBuilder) -> dioxus::desktop::WindowBuilder {
    window
        .with_inner_size(dioxus::desktop::LogicalSize::new(1000.0, 700.0))
        .with_position(dioxus::desktop::LogicalPosition::new(120.0, 80.0))
}

#[cfg(not(debug_assertions))]
fn explicit_placement(window: dioxus::desktop::WindowBuilder) -> dioxus::desktop::WindowBuilder {
    window
}

/// Opens the log file in the system's text viewer (its folder when there is no file yet).
fn open_log() {
    let Some(dir) = settings::log_dir() else { return };
    let file = dir.join("ffmpeg-video-studio.log");
    let _ = open::that(if file.is_file() { file } else { dir });
}

/// The window has no system frame: the app draws its own bar (`UisBarWindow`), which moves it and holds the three
/// window buttons. The system still resizes it from its edges, and on Windows it keeps its shadow.
fn frameless(window: dioxus::desktop::WindowBuilder) -> dioxus::desktop::WindowBuilder {
    let window = window.with_decorations(false);
    #[cfg(windows)]
    let window = {
        use dioxus::desktop::tao::platform::windows::WindowBuilderExtWindows;
        window.with_undecorated_shadow(true)
    };
    window
}

fn main() {
    // The log file comes first: everything after it, panics included, can be reported.
    if let Some(dir) = settings::log_dir() {
        engine::log::init(
            &dir,
            "ffmpeg-video-studio",
            &[
                ("app", format!("{APP_NAME} {} ({})", env!("CARGO_PKG_VERSION"), app_version_build())),
                ("kind", (if cfg!(debug_assertions) { "debug" } else { "release" }).to_owned()),
                ("system", format!("{} {}", std::env::consts::OS, std::env::consts::ARCH)),
                ("exe", std::env::current_exe().map(|p| p.display().to_string()).unwrap_or_default()),
            ],
        );
        engine::log::install_panic_hook();
    }
    // ⚠ CLAUDE: in a DEBUG build Dioxus writes the window's position and size to `window-state.json` in
    // the temp folder when the app ends and puts the window back there on the next launch - for EVERY
    // Dioxus app on the machine, from the one shared file. A session that ended while the window was
    // MINIMIZED saved (-32000, -32000) and a 200 px square, so the next `cargo run` opened off screen
    // at that size and could not be brought back. Dioxus restores only what the app did NOT set
    // explicitly, so a debug build sets both, here. A release build never restores anything.
    let window = frameless(explicit_placement(dioxus::desktop::WindowBuilder::new().with_title(APP_NAME)));

    dioxus::LaunchBuilder::desktop()
        .with_cfg(
            dioxus::desktop::Config::new()
                // ⚠ CLAUDE: the browser's own right-click menu (Back, Reload, Save as, Print, Inspect) is
                // never offered. Dioxus keeps it in a debug build by default - which is how it reached the
                // list on 2026-10-13 - and an app that wants a context menu draws its own.
                .with_disable_context_menu(true)
                .with_window(window)
                .with_menu(None::<dioxus::desktop::muda::Menu>),
        )
        .launch(App);
}

/// The window may not be shorter than the layout needs: the header, the options panel down to its last row, and the status
/// bar. Measured from the laid-out page of the view on screen - the panel's rows decide it, so a row added there raises the
/// floor by itself. The first frames may not be laid out yet (fonts, styles), so it is asked again until the panel has a
/// real height; if it never answers, a fixed floor stands, so the window is never left without one.
pub(crate) async fn fit_window_height() {
    let mut height = 0.0;
    for _ in 0..40 {
        tokio::time::sleep(std::time::Duration::from_millis(250)).await;
        let mut ev = document::eval(
            r#"
            const q = (s) => document.querySelector('.view-slot:not(.view-hidden) ' + s);
            const panel = q('.converter-panel'), head = q('.uis-barh-header');
            const rule = q('.uis-barh-gradient'), foot = q('.uis-barh-footer');
            const bar = document.querySelector('.uis-barh-window');
            if (!panel || !head || !foot || !panel.lastElementChild) { dioxus.send(0); }
            else {
                const end = panel.lastElementChild.getBoundingClientRect().bottom;
                const pad = parseFloat(getComputedStyle(panel).paddingBottom) || 0;
                const frame = (bar ? bar.offsetHeight : 0) + head.offsetHeight + (rule ? rule.offsetHeight : 0) + foot.offsetHeight;
                const right = (end - panel.getBoundingClientRect().top) + pad;
                // the work view's left column: the preview's own minimum, the strip under it and the command
                const wv = q('.wv'), strip = q('.work-strip') || q('.work-colour');
                const left = wv && strip ? parseFloat(getComputedStyle(wv).minHeight) + strip.offsetHeight : 0;
                dioxus.send(frame + Math.max(right, left));
            }
            "#,
        );
        if let Ok(h) = ev.recv::<f64>().await {
            if h > 100.0 {
                height = h.ceil();
                break;
            }
        }
    }
    if height == 0.0 {
        height = MIN_HEIGHT_FALLBACK;
    }
    // ⚠ CLAUDE: setting a minimum size makes the window system re-check the size, and on Windows that un-maximizes a
    // maximized window and puts it back at its old size somewhere else (seen on every switch to the batch view). A
    // maximized window is already above any minimum, so it is left alone; the next switch of view measures again.
    let window = dioxus::desktop::window();
    if window.is_maximized() {
        return;
    }
    window.set_min_inner_size(Some(dioxus::desktop::LogicalSize::new(MIN_WIDTH, height)));
}

#[component]
fn App() -> Element {
    let mut settings = use_signal(settings::load);
    let mut settings_open = use_signal(|| false);
    // The window bar shows maximize or restore: the window says which, and says so again whenever its size
    // changes (the bar's own button is one way among several: a snap, Win+Up, a double click).
    let mut maximized = use_signal(|| dioxus::desktop::window().is_maximized());
    // The bar is painted like the system's own title bars: the accent colour when the system colours them, and only
    // while this window is the active one (an inactive title bar is neutral). Read again each time the window comes
    // to the front, so a colour changed in the system's settings shows on return.
    let mut bar_colour = use_signal(window_accent::title_bar);
    dioxus::desktop::use_wry_event_handler(move |event, _| {
        if let dioxus::desktop::tao::event::Event::WindowEvent { event, .. } = event {
            match event {
                dioxus::desktop::WindowEvent::Resized(_) => {
                    let now = dioxus::desktop::window().is_maximized();
                    if *maximized.peek() != now {
                        maximized.set(now);
                    }
                }
                dioxus::desktop::WindowEvent::Focused(active) => {
                    let now = if *active { window_accent::title_bar() } else { None };
                    if *bar_colour.peek() != now {
                        bar_colour.set(now);
                    }
                }
                _ => {}
            }
        }
    });
    use_effect(move || {
        let script = match bar_colour() {
            Some(rgb) => {
                let (bgd, text, hov) = window_accent::bar_colours(rgb);
                format!("const s=document.documentElement.style; s.setProperty('--uis-win-bar-bgd','{bgd}'); s.setProperty('--uis-win-bar-text','{text}'); s.setProperty('--uis-win-bar-hov','{hov}');")
            }
            None => "const s=document.documentElement.style; s.removeProperty('--uis-win-bar-bgd'); s.removeProperty('--uis-win-bar-text'); s.removeProperty('--uis-win-bar-hov');".to_owned(),
        };
        let _ = document::eval(&script);
    });
    let mut mode = use_signal(|| Mode::Work);
    // The work view's list, selection, cut and effects: kept here so a visit to the batch view does not lose them.
    let work_state = WorkState::new();
    let converter_state = ConverterState::new();
    // Runs and ffmpeg: one copy for both views, so what starts by itself starts once per launch.
    let run = RunState::new(settings);
    // Which Settings tab is showing (and the one it opens on): general, presets, about.
    let mut settings_tab = use_signal(String::new);
    // "Show news from yaiol": ONE switch per MACHINE, shared by every yaiol app (a small file in the yaiol
    // app-data folder), so it is read from there rather than from this app's own settings.
    let mut news_on = use_signal(|| news::read_state().enabled);


    // <html> carries the language, its text direction and the theme: the catalog CSS keys off
    // `data-theme`, and a right-to-left language needs `dir` for the webview's own text engine.
    use_effect(move || {
        let s = settings.read();
        let dir = if engine::is_rtl(&s.lang) { "rtl" } else { "ltr" };
        let _ = document::eval(&format!(
            "const r=document.documentElement; r.setAttribute('lang','{}'); r.setAttribute('dir','{dir}'); r.setAttribute('data-theme','{}');",
            s.lang.replace('_', "-"),
            s.theme
        ));
    });

    // Every change persists immediately, so leaving the settings page is plain navigation.
    use_effect(move || settings::save(&settings.read()));

    let (lang, theme) = {
        let s = settings.read();
        (s.lang.clone(), s.theme.clone())
    };
    engine::effects::set_language(&lang);
    let t = |key: &str| i18n::t(&lang, key);
    let tip_help = t("tipHdrHelp");
    let tip_settings = t("tipHdrSettings");
    let (tip_minimize, tip_maximize, tip_restore, tip_close) = (t("tipWinMinimize"), t("tipWinMaximize"), t("tipWinRestore"), t("tipWinClose"));
    let tip_batch = t("tipHdrBatch");
    let tip_work = t("tipHdrWork");
    let lbl_news = t("lblStgGeneralNews");
    let lbl_log = t("lblStgLog");
    let btn_log = t("btnOpenLog");
    let tab_presets = t("tabStgVideo");
    let tab_sizes = t("tabStgSizes");
    let tab_audio = t("tabStgAudio");
    let tab_effects = t("tabStgEffects");
    let tab_audio_effects = t("tabStgEffectsAudio");
    let tab_ffmpeg = t("tabStgFfmpeg");
    let tab_metadata = t("tabStgMetadata");
    let tab_destinations = t("tabStgDestinations");
    let tab_notifications = t("tabStgNotifications");

    let version = app_version_build();
    // The app's own icon, shown at the start of the window bar: the file the executable carries, as a page picture.
    let app_icon = use_hook(|| {
        use base64::{Engine, engine::general_purpose::STANDARD};
        format!("data:image/png;base64,{}", STANDARD.encode(include_bytes!("../assets/app.png")))
    });
    let help_url = site::site_url(APP_ID, &lang, "help");
    let github_url = format!("https://github.com/yaiol/{APP_ID}");

    // The header's first group, shared by both views: the two ways to use the app.
    let header_start = rsx! {
        div { class: "uis-barh-grp",
            button {
                class: if mode() == Mode::Work { "uis-btn ico active" } else { "uis-btn ico" },
                title: "{tip_work}",
                "aria-label": "{tip_work}",
                "aria-pressed": "{mode() == Mode::Work}",
                onclick: move |_| mode.set(Mode::Work),
                Icon { width: 14, height: 14, icon: LdWrench }
            }
            button {
                class: if mode() == Mode::Batch { "uis-btn ico active" } else { "uis-btn ico" },
                title: "{tip_batch}",
                "aria-label": "{tip_batch}",
                "aria-pressed": "{mode() == Mode::Batch}",
                onclick: move |_| mode.set(Mode::Batch),
                Icon { width: 14, height: 14, icon: LdLayers }
            }
        }
    };
    // The panel's edit button opens Settings on the right tab.
    let on_manage = move |tab: String| {
        settings_tab.set(tab);
        settings_open.set(true);
    };
    let page = rsx! {
        if settings_open() {
            UisStgView {
                        app_name: APP_NAME,
                        app_version: version.clone(),
                        lang: lang.clone(),
                        theme: theme.clone(),
                        on_set_lang: move |l| settings.write().lang = l,
                        on_set_theme: move |th| settings.write().theme = th,
                        on_close: move |_| settings_open.set(false),
                        app_tabs: vec![
                            UisAppTab {
                                key: manager::FFMPEG.to_owned(),
                                label: tab_ffmpeg.clone(),
                                icon: rsx! { Icon { width: 14, height: 14, icon: LdTerminal } },
                            },
                            UisAppTab {
                                key: manager::PRESETS.to_owned(),
                                label: tab_presets.clone(),
                                icon: rsx! { Icon { width: 14, height: 14, icon: LdVideo } },
                            },
                            UisAppTab {
                                key: manager::SIZES.to_owned(),
                                label: tab_sizes.clone(),
                                icon: rsx! { Icon { width: 14, height: 14, icon: LdScaling } },
                            },
                            UisAppTab {
                                key: manager::AUDIO.to_owned(),
                                label: tab_audio.clone(),
                                icon: rsx! { Icon { width: 14, height: 14, icon: LdVolume2 } },
                            },
                            UisAppTab {
                                key: manager::METADATA.to_owned(),
                                label: tab_metadata.clone(),
                                icon: rsx! { Icon { width: 14, height: 14, icon: LdEraser } },
                            },
                            UisAppTab {
                                key: manager::EFFECTS.to_owned(),
                                label: tab_effects.clone(),
                                icon: rsx! { Icon { width: 14, height: 14, icon: LdSparkles } },
                            },
                            UisAppTab {
                                key: manager::AUDIO_EFFECTS.to_owned(),
                                label: tab_audio_effects.clone(),
                                icon: rsx! { Icon { width: 14, height: 14, icon: LdAudioLines } },
                            },
                            UisAppTab {
                                key: manager::DESTINATIONS.to_owned(),
                                label: tab_destinations.clone(),
                                icon: rsx! { Icon { width: 14, height: 14, icon: LdFolder } },
                            },
                            UisAppTab {
                                key: manager::NOTIFICATIONS.to_owned(),
                                label: tab_notifications.clone(),
                                icon: rsx! { Icon { width: 14, height: 14, icon: LdBell } },
                            },
                        ],
                        initial_tab: settings_tab(),
                        on_tab: move |key: String| settings_tab.set(key),
                        app_page: Some(rsx! {
                            if settings_tab() == manager::PRESETS {
                                PresetEditor { settings }
                            } else if settings_tab() == manager::SIZES {
                                SizeEditor { settings }
                            } else if settings_tab() == manager::AUDIO {
                                AudioEditor { settings }
                            } else if settings_tab() == manager::EFFECTS {
                                EffectsEditor { key: "{manager::EFFECTS}", settings }
                            } else if settings_tab() == manager::AUDIO_EFFECTS {
                                EffectsEditor { key: "{manager::AUDIO_EFFECTS}", settings, sound: true }
                            } else if settings_tab() == manager::METADATA {
                                MetadataEditor { settings }
                            } else if settings_tab() == manager::DESTINATIONS {
                                DestinationEditor { settings }
                            } else if settings_tab() == manager::FFMPEG {
                                div { class: "manager-form manager-wide",
                                    FfmpegSetting { settings, lang: lang.clone() }
                                    EffectsFavourites { lang: lang.clone() }
                                }
                            } else if settings_tab() == manager::NOTIFICATIONS {
                                NotifyEditor { settings }
                            }
                        }),
                        general_extra: Some(rsx! {
                            div { class: "uis-fld-labelled divider",
                                label { class: "uis-chk-lbl",
                                    input {
                                        r#type: "checkbox",
                                        class: "uis-chk-box",
                                        checked: news_on(),
                                        onchange: move |e| {
                                            news::set_enabled(e.checked());
                                            news_on.set(e.checked());
                                        },
                                    }
                                    "{lbl_news}"
                                }
                            }
                            div { class: "uis-fld-labelled divider",
                                label { class: "uis-fld-labelled-label", "{lbl_log}" }
                                div {
                                    button {
                                        class: "uis-btn",
                                        onclick: move |_| open_log(),
                                        Icon { width: 14, height: 14, icon: LdFileText }
                                        " {btn_log}"
                                    }
                                }
                            }
                        }),
                    }
                }
    };

    let no_page = rsx! {};

    rsx! {
        for css in CSS {
            style { "{css}" }
        }
        div { class: "uis-app-root",
            // A new version (first) and the yaiol news, in one dialog; what is shown is decided inside.
            UisNewsLoader { app_id: APP_ID, app_version: APP_VERSION, lang: lang.clone() }
            // The window bar: the app's own title bar, above whichever view is on screen. It holds what both views' headers
            // used to end with: settings, help, source.
            UisBarWindow {
                app_name: APP_NAME,
                app_version: version.clone(),
                icon_src: app_icon,
                maximized: maximized(),
                tip_minimize,
                tip_maximize,
                tip_restore,
                tip_close,
                on_minimize: move |_| dioxus::desktop::window().set_minimized(true),
                on_maximize: move |_| dioxus::desktop::window().toggle_maximized(),
                on_close: move |_| dioxus::desktop::window().close(),
                on_drag: move |_| dioxus::desktop::window().drag(),
                on_menu: move |_| window_menu::show(),
                button {
                    class: if settings_open() { "uis-barh-window-btn uis-stg-toggle active" } else { "uis-barh-window-btn uis-stg-toggle" },
                    title: "{tip_settings}",
                    "aria-label": "{tip_settings}",
                    "aria-pressed": "{settings_open()}",
                    onclick: move |_| {
                        settings_tab.set(String::new());
                        settings_open.toggle();
                    },
                    Icon { width: 14, height: 14, icon: LdSettings }
                }
                button {
                    class: "uis-barh-window-btn",
                    title: "{tip_help}",
                    "aria-label": "{tip_help}",
                    onclick: move |_| {
                        let _ = open::that(&help_url);
                    },
                    Icon { width: 14, height: 14, icon: LdCircleHelp }
                }
                button {
                    class: "uis-barh-window-btn",
                    title: "GitHub",
                    "aria-label": "GitHub",
                    onclick: move |_| {
                        let _ = open::that(&github_url);
                    },
                    UisIcoGithub {}
                }
                // A newer version is out: the reminder, drawn only then; a click reopens the update card.
                UisBtnUpdate { lang: lang.clone() }
                // yaiol has news for this app: the second reminder, drawn only then; a click reopens the announcements.
                UisBtnNews { lang: lang.clone() }
            }
            // ⚠ CLAUDE: BOTH views stay mounted and the other one is only hidden, so nothing either of them holds (a
            // button's state, the volume, a scroll position, an open row) is ever dropped. The Settings page is given to
            // the view on screen only.
            div { class: if mode() == Mode::Work { "view-slot" } else { "view-slot view-hidden" },
                Work {
                    state: work_state,
                    run,
                    active: mode() == Mode::Work,
                    settings,
                    app_name: APP_NAME,
                    app_version: version.clone(),
                    header_start: header_start.clone(),
                    on_manage,
                    {if mode() == Mode::Work { page.clone() } else { no_page.clone() }}
                }
            }
            div { class: if mode() == Mode::Work { "view-slot view-hidden" } else { "view-slot" },
                Converter {
                    state: converter_state,
                    run,
                    active: mode() != Mode::Work,
                    settings,
                    app_name: APP_NAME,
                    app_version: version.clone(),
                    header_start: header_start.clone(),
                    on_manage,
                    {if mode() == Mode::Work { no_page.clone() } else { page.clone() }}
                }
            }
        }
    }
}
