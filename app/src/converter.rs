//! The main view: the file list, the drop target, Convert, and the panel with the preset, where the
//! files go, what to do with an existing output and the number of conversions at once.
//!
//! ⚠ CLAUDE: NO HARDCODED UI STRINGS. Every user-visible string goes through the t() lookup with the key
//! a string literal at the call site.

use crate::{
    catalog::{AppHeader, DropZone},
    i18n, manager,
    ffmpeg_panel::{FfmpegPopup, Popup, download_note, install, installed_version, is_current},
    runner::{DOWNLOAD, LATEST, RUNNER, refresh_ffmpeg},
    settings::{RunChoice, Settings},
};
use dioxus::prelude::*;
use dioxus_free_icons::{
    Icon,
    icons::ld_icons::{LdEraser, LdFilePlus, LdFolder, LdFolderOpen, LdFolderPlus, LdFilm, LdListChecks, LdPlay, LdReplace, LdScaling, LdScissors, LdSlidersHorizontal, LdSparkles, LdSquare, LdVolume2, LdX},
};
use engine::{JobSpec, JobStatus, Overwrite, Trim, download::{DownloadState, LatestState}};
use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::Duration,
};

static NEXT_ID: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, PartialEq)]
struct Item {
    id: u64,
    name: String,
    path: PathBuf,
    /// The folder this file was added through; None for a file added on its own.
    root: Option<PathBuf>,
    /// What this file will be converted with - the panel's choices when it was added or last applied to it.
    run: RunChoice,
}

/// The rows from `a` to `b` in list order, both included (the order the user sees, whichever end came first).
fn range_between(ids: &[u64], a: u64, b: u64) -> Vec<u64> {
    let (Some(x), Some(y)) = (ids.iter().position(|i| *i == a), ids.iter().position(|i| *i == b)) else {
        return vec![b];
    };
    ids[x.min(y)..=x.max(y)].to_vec()
}

/// One tag of a row: the control's icon and the name of what was chosen.
#[derive(Clone, PartialEq)]
enum TagKind {
    Preset,
    Size,
    Audio,
    Effects,
    Metadata,
    Trim,
    Folder,
    Overwrite,
}

#[component]
fn RunTag(kind: TagKind, text: String, title: String) -> Element {
    rsx! {
        span { class: "chip", title: "{title}",
            match kind {
                TagKind::Preset => rsx! { Icon { width: 14, height: 14, icon: LdSlidersHorizontal } },
                TagKind::Size => rsx! { Icon { width: 14, height: 14, icon: LdScaling } },
                TagKind::Audio => rsx! { Icon { width: 14, height: 14, icon: LdVolume2 } },
                TagKind::Effects => rsx! { Icon { width: 14, height: 14, icon: LdSparkles } },
                TagKind::Metadata => rsx! { Icon { width: 14, height: 14, icon: LdEraser } },
                TagKind::Trim => rsx! { Icon { width: 14, height: 14, icon: LdScissors } },
                TagKind::Folder => rsx! { Icon { width: 14, height: 14, icon: LdFolder } },
                TagKind::Overwrite => rsx! { Icon { width: 14, height: 14, icon: LdReplace } },
            }
            "{text}"
        }
    }
}

fn percent_text(p: f32) -> String {
    format!("{} %", p.round() as u32)
}

/// `3 converted · 1 skipped · 2 failed`, empty when none of the three happened.
fn run_summary<'a>(statuses: impl Iterator<Item = &'a JobStatus>, lang: &str) -> String {
    let (done, skipped, failed) = engine::run_counts(statuses);
    let t = |key: &str| i18n::t(lang, key);
    let mut parts = Vec::new();
    if done > 0 {
        parts.push(format!("{done} {}", t("lblMainNotifyDone")));
    }
    if skipped > 0 {
        parts.push(format!("{skipped} {}", t("lblMainNotifySkipped")));
    }
    if failed > 0 {
        parts.push(format!("{failed} {}", t("lblMainNotifyFailed")));
    }
    parts.join(" · ")
}

/// One operating-system notice for a finished run. A run that was only cancelled says nothing.
fn notify_finished(snap: &HashMap<u64, JobStatus>, lang: &str) {
    let body = run_summary(snap.values(), lang);
    if body.is_empty() {
        return;
    }
    let title = i18n::t(lang, "ttlMainNotify");
    std::thread::spawn(move || crate::notify::show(&title, &body));
}

#[component]
pub fn Converter(
    settings: Signal<Settings>,
    app_name: String,
    app_version: String,
    /// The header's right-hand group (GitHub, help, settings) - the app owns those.
    header_end: Element,
    on_manage: EventHandler<String>,
    children: Element,
) -> Element {
    let mut items = use_signal(Vec::<Item>::new);
    let mut statuses = use_signal(HashMap::<u64, JobStatus>::new);
    let mut download = use_signal(|| DownloadState::Idle);
    let mut ffmpeg_ready = use_signal(|| RUNNER.ffmpeg().is_some());
    // The ffmpeg installed now; what the yaiol site says the compatible one is (asked once at launch) is read straight
    // from LATEST by the poll below.
    let mut installed = use_signal(installed_version);
    // The popup the user closed (a failure, a newer version offered): it stays closed until an install starts again.
    let mut dismissed = use_signal(|| false);
    let mut pick_nonce = use_signal(|| 0u32);
    use_hook(|| {
        // Which ffmpeg is the compatible one. Asked on every machine: the app cannot keep its ffmpeg right without
        // the answer, so this is not the optional request the dev-machine switch exists to silence.
        LATEST.check();
        // Nothing installed: install it now, there is nothing for the user to decide.
        if !*ffmpeg_ready.peek() {
            install();
        }
    });
    // The part of each file to keep. NOT saved with the settings: a cut left over from yesterday would
    // quietly shorten every file converted tomorrow.
    let mut trim_start = use_signal(String::new);
    let mut trim_end = use_signal(String::new);
    // The rows the panel's Apply writes to, and the one a Shift+click counts from.
    let mut selected = use_signal(HashSet::<u64>::new);
    let mut anchor = use_signal(|| None::<u64>);

    // The window polls the runner and the download; the engine never calls back into it. Only a
    // CHANGED snapshot is written, so an idle queue does not re-render the list five times a second.
    use_future(move || async move {
        // A newer approved ffmpeg is installed by itself, once per launch (the popup says "Updating").
        let mut update_tried = false;
        let mut was_active = false;
        loop {
            tokio::time::sleep(Duration::from_millis(200)).await;
            let snap: HashMap<u64, JobStatus> = RUNNER.snapshot().into_iter().collect();
            // A run just ended: tell the user, whichever window they are looking at.
            let active = snap.values().any(JobStatus::is_active);
            if was_active && !active && settings.peek().notify {
                notify_finished(&snap, &settings.peek().lang);
            }
            was_active = active;
            if *statuses.peek() != snap {
                statuses.set(snap);
            }
            let dl = DOWNLOAD.snapshot();
            if dl.is_active() && *dismissed.peek() {
                dismissed.set(false);
            }
            if *download.peek() != dl {
                if matches!(dl, DownloadState::Done(_)) {
                    ffmpeg_ready.set(refresh_ffmpeg(&RUNNER));
                    installed.set(installed_version());
                }
                download.set(dl);
            }
            if let LatestState::Known(m) = LATEST.snapshot() {
                if !update_tried
                    && *ffmpeg_ready.peek()
                    && !is_current(&m)
                    && !DOWNLOAD.snapshot().is_active()
                {
                    update_tried = true;
                    install();
                }
            }
        }
    });

    // The window may not be shorter than the layout needs: the header, the options panel down to its
    // last row, and the status bar. Measured from the laid-out page - the panel's rows decide it, so a
    // row added there raises the floor by itself. The first frames may not be laid out yet (fonts,
    // styles), so it is asked again until the panel has a real height; if it never answers, a fixed
    // floor stands, so the window is never left without one.
    use_future(move || async move {
        let mut height = 0.0;
        for _ in 0..40 {
            tokio::time::sleep(Duration::from_millis(250)).await;
            let mut ev = document::eval(
                r#"
                const q = (s) => document.querySelector(s);
                const panel = q('.converter-panel'), head = q('.barh-header');
                const rule = q('.barh-gradient'), foot = q('.barh-footer');
                if (!panel || !head || !foot || !panel.lastElementChild) { dioxus.send(0); }
                else {
                    const end = panel.lastElementChild.getBoundingClientRect().bottom;
                    const pad = parseFloat(getComputedStyle(panel).paddingBottom) || 0;
                    dioxus.send(head.offsetHeight + (rule ? rule.offsetHeight : 0)
                        + (end - panel.getBoundingClientRect().top) + pad + foot.offsetHeight);
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
            height = crate::MIN_HEIGHT_FALLBACK;
        }
        dioxus::desktop::window().set_min_inner_size(Some(dioxus::desktop::LogicalSize::new(crate::MIN_WIDTH, height)));
    });

    let lang = settings.read().lang.clone();
    let t = |key: &str| i18n::t(&lang, key);

    let mut add_paths = move |paths: Vec<PathBuf>| {
        // Added files take the panel's choices as they are now, and become the selection.
        let run = settings.read().choice(&trim_start.peek(), &trim_end.peek());
        let mut fresh = Vec::new();
        let mut list = items.write();
        for p in &paths {
            // A dropped / picked FOLDER is the root its files keep their sub-folders relative to.
            let root = p.is_dir().then(|| p.clone());
            let mut found = Vec::new();
            engine::collect_video(p, &mut found);
            for path in found {
                if list.iter().any(|i| i.path == path) {
                    continue;
                }
                let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
                fresh.push(id);
                list.push(Item { id, name, path, root: root.clone(), run: run.clone() });
            }
        }
        if let Some(last) = fresh.last().copied() {
            selected.set(fresh.into_iter().collect());
            anchor.set(Some(last));
        }
    };

    let pick_files = move |_| {
        spawn(async move {
            let exts: Vec<&str> = engine::VIDEO_EXTENSIONS.to_vec();
            if let Some(files) = rfd::AsyncFileDialog::new().add_filter("video", &exts).pick_files().await {
                add_paths(files.into_iter().map(|f| f.path().to_path_buf()).collect());
            }
        });
    };
    let pick_folder = move |_| {
        spawn(async move {
            if let Some(dir) = rfd::AsyncFileDialog::new().pick_folder().await {
                add_paths(vec![dir.path().to_path_buf()]);
            }
        });
    };

    let ffmpeg_missing = !ffmpeg_ready();
    let busy = statuses.read().values().any(JobStatus::is_active);
    // A cut that does not parse stops Convert: in the panel (it cannot be applied) or in a row that would run.
    let panel_trim_invalid = Trim::parse(&trim_start(), &trim_end()).is_none();
    let trim_invalid = {
        let st = statuses.read();
        items.read().iter().any(|i| !st.get(&i.id).is_some_and(JobStatus::is_active) && Trim::parse(&i.run.trim_start, &i.run.trim_end).is_none())
    };

    // Every file runs with ITS OWN choices; only the number at once belongs to the run.
    let convert = move |_| {
        let s = settings.read();
        let parallel = s.parallel as usize;
        let st = statuses.peek();
        let specs: Vec<JobSpec> = items
            .read()
            .iter()
            // Active ones are already queued or running; everything else - new, done, failed,
            // skipped, cancelled - runs again with its current choices.
            .filter(|i| !st.get(&i.id).is_some_and(JobStatus::is_active))
            .filter_map(|i| {
                let run = &i.run;
                Some(JobSpec {
                    id: i.id,
                    input: i.path.clone(),
                    source_root: i.root.clone(),
                    preset: s.preset_by(&run.preset_id),
                    folder: s.destination_by(&run.destination_id).folder_path(),
                    overwrite: run.overwrite,
                    size: s.size_by(&run.size_id),
                    audio: s.audio_by(&run.audio_id),
                    trim: Trim::parse(&run.trim_start, &run.trim_end)?,
                    effects: s.effects_by(&run.effects_id),
                    metadata: s.metadata_by(&run.metadata_id),
                })
            })
            .collect();
        drop(st);
        drop(s);
        RUNNER.start(specs, parallel);
    };
    let stop = move |_| RUNNER.cancel_all();
    // Writes the panel's choices onto the selected rows (the ones not queued or converting right now).
    let apply = move |_| {
        let run = settings.read().choice(&trim_start.peek(), &trim_end.peek());
        let st = statuses.peek();
        let sel = selected.peek();
        for i in items.write().iter_mut() {
            if sel.contains(&i.id) && !st.get(&i.id).is_some_and(JobStatus::is_active) {
                i.run = run.clone();
            }
        }
    };
    // Empties the list except what is queued or converting right now: a file that was never started
    // has no status at all, and it is the one a user most often wants gone.
    let clear_list = move |_| {
        let st = statuses.peek().clone();
        let idle = |id: &u64| st.get(id).map_or(true, |s| !s.is_active());
        for i in items.read().iter().filter(|i| idle(&i.id)) {
            RUNNER.remove(i.id);
        }
        items.write().retain(|i| !idle(&i.id));
        let left: HashSet<u64> = items.read().iter().map(|i| i.id).collect();
        selected.write().retain(|id| left.contains(id));
    };

    let have_items = !items.read().is_empty();
    let any_idle = items.read().iter().any(|i| statuses.read().get(&i.id).map_or(true, |s| !s.is_active()));

    let s = settings.read();
    let preset_options: Vec<(String, String)> = s.presets.iter().map(|p| (p.id.clone(), p.name.clone())).collect();
    let preset_id = s.preset_id.clone();
    // "(Original)" comes first and is not an entry of Settings.
    let mut size_options: Vec<(String, String)> = vec![(engine::size::ORIGINAL_ID.to_owned(), t("lblMainSizeOriginal"))];
    size_options.extend(s.sizes.iter().map(|z| (z.id.clone(), z.name.clone())));
    let size_id = s.size_id.clone();
    // The audio on offer depends on what the selected preset's container can hold; a selection that
    // the preset cannot take is shown as the original.
    let sel_preset = s.selected_preset();
    let mut audio_options: Vec<(String, String)> = vec![(engine::audio::ORIGINAL_ID.to_owned(), t("lblMainAudioOriginal"))];
    audio_options.extend(s.audios.iter().filter(|a| a.available_for(&sel_preset)).map(|a| (a.id.clone(), a.name.clone())));
    let mut effects_options: Vec<(String, String)> = vec![(engine::effects::NONE_ID.to_owned(), t("lblMainEffectsNone"))];
    effects_options.extend(s.effects.iter().map(|e| (e.id.clone(), e.name.clone())));
    let effects_id = s.effects_id.clone();
    let metadata_options: Vec<(String, String)> = s.metadata.iter().map(|m| (m.id.clone(), m.name.clone())).collect();
    let metadata_id = s.metadata_id.clone();
    let audio_id = s.effective_audio_id();
    let mut dest_options: Vec<(String, String)> = vec![(engine::destination::SOURCE_ID.to_owned(), t("lblMainDestSource"))];
    dest_options.extend(s.destinations.iter().map(|d| (d.id.clone(), d.name.clone())));
    dest_options.push((engine::destination::PICK_ID.to_owned(), t("lblMainDestPick")));
    let destination_id = s.destination_id.clone();
    let saved_folder = s.destination_by(&s.destination_id).folder_path();
    let parallel = s.parallel;
    let overwrite = s.overwrite;
    drop(s);

    let s_main_add_files = t("btnMainAddFiles");
    let s_main_add_folder = t("btnMainAddFolder");
    let s_main_clear = t("btnMainClear");
    let s_main_stop = t("btnMainStop");
    let s_main_convert = t("btnMainConvert");
    let s_main_preset = t("lblMainPreset");
    let s_main_edit_presets = t("tipMainEditPresets");
    let s_main_size = t("tipMainSize");
    let s_main_audio = t("tipMainAudio");
    let s_main_edit_audio = t("tipMainEditAudio");
    let s_main_trim = t("tipMainTrim");
    let s_main_trim_start = t("lblMainTrimStart");
    let s_main_trim_end = t("lblMainTrimEnd");
    let s_main_effects = t("tipMainEffects");
    let s_main_edit_effects = t("tipMainEditEffects");
    let s_main_metadata = t("tipMainMetadata");
    let s_main_edit_metadata = t("tipMainEditMetadata");
    let s_main_edit_sizes = t("tipMainEditSizes");
    let s_main_destination = t("tipMainDestination");
    let s_main_edit_destinations = t("tipMainEditDestinations");
    let s_main_overwrite = t("tipMainOverwrite");
    let s_main_overwrite_skip = t("lblMainOverwriteSkip");
    let s_main_overwrite_overwrite = t("lblMainOverwriteOverwrite");
    let s_main_overwrite_rename = t("lblMainOverwriteRename");
    let s_main_parallel = t("tipMainParallel");
    let s_main_open_output = t("tipMainOpenOutput");
    let s_main_apply = t("btnMainApply");
    let s_main_apply_tip = t("tipMainApply");
    let sel_now = selected.read().clone();
    let can_apply = !panel_trim_invalid && sel_now.iter().any(|id| !statuses.read().get(id).is_some_and(JobStatus::is_active));
    // The names tags are made of, once per render: a row only looks its choices up.
    let cfg = settings.read().clone();
    let list_empty = t("msgMainListEmpty");
    let drop_label = t("msgMainDropLabel");

    // The left of the footer: what the list holds, how far a run is, or how the last run ended.
    let status_text = {
        let st = statuses.read();
        let total = items.read().len();
        let touched = items.read().iter().filter(|i| st.contains_key(&i.id)).count();
        if ffmpeg_missing && !download().is_active() {
            t("msgMainNoFfmpeg")
        } else if trim_invalid {
            t("msgMainTrimInvalid")
        } else if total == 0 {
            String::new()
        } else if busy {
            let finished = st.values().filter(|s| !s.is_active()).count();
            t("msgMainProgress").replace("{done}", &finished.to_string()).replace("{total}", &touched.to_string())
        } else if touched > 0 {
            run_summary(st.values(), &lang)
        } else {
            t("msgMainReady").replace("{n}", &total.to_string())
        }
    };

    let status_text = if !sel_now.is_empty() && !busy && !trim_invalid {
        let n = t("msgMainSelected").replace("{n}", &sel_now.len().to_string());
        if status_text.is_empty() { n } else { format!("{status_text} · {n}") }
    } else {
        status_text
    };

    // The ffmpeg popup: the install's progress, or why it failed. An ffmpeg already in place being replaced is an
    // update; with none, an install.
    let downloading = download().is_active();
    let update = ffmpeg_ready();
    let popup = if downloading {
        download_note(&lang, &download()).map(|text| Popup::Progress { text, update })
    } else if dismissed() {
        None
    } else if matches!(download(), DownloadState::Failed(_)) {
        download_note(&lang, &download()).map(|text| Popup::Failed { text, update })
    } else {
        None
    };

    // Where "open the output folder" goes: the folder of the newest finished file - which is right
    // for "beside the source" too - and otherwise the chosen folder.
    let open_target: Option<PathBuf> = statuses
        .read()
        .iter()
        .filter_map(|(id, s)| match s {
            JobStatus::Done { output } => Some((*id, output.clone())),
            _ => None,
        })
        .max_by_key(|(id, _)| *id)
        .and_then(|(_, out)| out.parent().map(Path::to_path_buf))
        .or_else(|| saved_folder.filter(|p| p.is_absolute()))
        .filter(|p| p.is_dir());

    rsx! {
        // ⚠ CLAUDE: the header holds the file actions only (icon-only, tooltips). The choices that shape a
        // run - preset, where it goes, how many at once, Convert - live in the right-hand panel below,
        // one aligned column.
        AppHeader { app_name, app_version,
            div { class: "barh-grp",
                button {
                    class: "btn icon",
                    title: "{s_main_add_files}",
                    "aria-label": "{s_main_add_files}",
                    onclick: pick_files,
                    Icon { width: 14, height: 14, icon: LdFilePlus }
                }
                button {
                    class: "btn icon",
                    title: "{s_main_add_folder}",
                    "aria-label": "{s_main_add_folder}",
                    onclick: pick_folder,
                    Icon { width: 14, height: 14, icon: LdFolderPlus }
                }
                button {
                    class: "btn icon",
                    title: "{s_main_clear}",
                    "aria-label": "{s_main_clear}",
                    disabled: !any_idle,
                    onclick: clear_list,
                    Icon { width: 14, height: 14, icon: crate::icons::LdBrushCleaning }
                }
            }
            span { class: "bar-spacer" }
            {header_end}
        }
        DropZone {
            class: "app-main",
            label: drop_label,
            on_files: move |dropped: Vec<(String, String)>| {
                add_paths(dropped.into_iter().map(|(_, p)| PathBuf::from(p)).collect());
            },
            {children}
        div { class: "converter-body",
        div { class: "converter-main",
        if let Some(popup) = popup {
            FfmpegPopup { lang: lang.clone(), popup, on_close: move |_| dismissed.set(true) }
        }
        // ⚠ CLAUDE: tabindex makes the list the thing a click focuses, which is what lets Ctrl+A reach it;
        // a click on the empty part clears the selection (a row's own click stops here).
        div {
            class: "lv-scroll converter-list",
            tabindex: "0",
            onclick: move |_| selected.set(HashSet::new()),
            onkeydown: move |e| {
                let m = e.modifiers();
                let ctrl = m.contains(Modifiers::CONTROL) || m.contains(Modifiers::META);
                if ctrl && matches!(e.key(), Key::Character(ref c) if c.eq_ignore_ascii_case("a")) {
                    e.prevent_default();
                    selected.set(items.peek().iter().map(|i| i.id).collect());
                }
            },
            if !have_items {
                p { class: "workspace-empty", "{list_empty}" }
            }
            for item in items.read().iter().cloned() {
                {
                    let status = statuses.read().get(&item.id).cloned();
                    let active = status.as_ref().is_some_and(JobStatus::is_active);
                    let (badge, detail, state) = match &status {
                        None => (String::new(), String::new(), "idle"),
                        Some(JobStatus::Queued) => (t("msgMainStatusQueued"), String::new(), "idle"),
                        Some(JobStatus::Probing) => (t("msgMainStatusReading"), String::new(), "idle"),
                        Some(JobStatus::Running { percent }) => (percent_text(*percent), String::new(), "run"),
                        Some(JobStatus::Done { output }) => (t("msgMainStatusDone"), output.display().to_string(), "ok"),
                        Some(JobStatus::Skipped { output }) => (t("msgMainStatusSkipped"), output.display().to_string(), "idle"),
                        Some(JobStatus::Cancelled) => (t("msgMainStatusCancelled"), String::new(), "idle"),
                        Some(JobStatus::Failed { reason }) => (t("msgMainStatusFailed"), reason.clone(), "err"),
                    };
                    let id = item.id;
                    let path = item.path.display().to_string();
                    let tip = if active { t("tipMainCancel") } else { t("tipMainRemove") };
                    // The row's own choices as tags: the control's icon and the name chosen. What is
                    // "none" or "original" is left out, so a plain row shows only its preset.
                    let r = &item.run;
                    let mut tags: Vec<(TagKind, String, String)> = vec![(TagKind::Preset, cfg.preset_by(&r.preset_id).name, s_main_preset.clone())];
                    if r.size_id != engine::size::ORIGINAL_ID {
                        tags.push((TagKind::Size, cfg.size_by(&r.size_id).name, s_main_size.clone()));
                    }
                    if r.audio_id != engine::audio::ORIGINAL_ID {
                        tags.push((TagKind::Audio, cfg.audio_by(&r.audio_id).name, s_main_audio.clone()));
                    }
                    if r.effects_id != engine::effects::NONE_ID {
                        tags.push((TagKind::Effects, cfg.effects_by(&r.effects_id).name, s_main_effects.clone()));
                    }
                    // What keeps every tag and chapter is left out of the chips, like the other choices that change nothing.
                    let cleaning = cfg.metadata_by(&r.metadata_id);
                    if !cleaning.removes_nothing() {
                        tags.push((TagKind::Metadata, cleaning.name, s_main_metadata.clone()));
                    }
                    if !r.trim_start.trim().is_empty() || !r.trim_end.trim().is_empty() {
                        tags.push((TagKind::Trim, format!("{}–{}", r.trim_start.trim(), r.trim_end.trim()), s_main_trim.clone()));
                    }
                    if r.destination_id != engine::destination::SOURCE_ID {
                        tags.push((TagKind::Folder, cfg.destination_by(&r.destination_id).name, s_main_destination.clone()));
                    }
                    match r.overwrite {
                        Overwrite::Skip => {}
                        Overwrite::Overwrite => tags.push((TagKind::Overwrite, s_main_overwrite_overwrite.clone(), s_main_overwrite.clone())),
                        Overwrite::Rename => tags.push((TagKind::Overwrite, s_main_overwrite_rename.clone(), s_main_overwrite.clone())),
                    }
                    let row_class = if sel_now.contains(&id) { "lv-item converter-row active" } else { "lv-item converter-row" };
                    rsx! {
                        div {
                            key: "{id}",
                            class: "{row_class}",
                            onclick: move |e| {
                                e.stop_propagation();
                                let m = e.modifiers();
                                let ctrl = m.contains(Modifiers::CONTROL) || m.contains(Modifiers::META);
                                let ids: Vec<u64> = items.peek().iter().map(|i| i.id).collect();
                                if m.contains(Modifiers::SHIFT) {
                                    let from = (*anchor.peek()).unwrap_or(id);
                                    let mut set = if ctrl { selected.peek().clone() } else { HashSet::new() };
                                    set.extend(range_between(&ids, from, id));
                                    selected.set(set);
                                } else if ctrl {
                                    let mut set = selected.peek().clone();
                                    if !set.remove(&id) {
                                        set.insert(id);
                                    }
                                    selected.set(set);
                                    anchor.set(Some(id));
                                } else {
                                    selected.set(HashSet::from([id]));
                                    anchor.set(Some(id));
                                }
                            },
                            div { class: "lv-item-icon converter-cover-box",
                                Icon { width: 18, height: 18, icon: LdFilm }
                            }
                            div { class: "lv-item-body",
                                div { class: "lv-item-name", "{item.name}" }
                                div { class: "lv-item-sub", "{path}" }
                                div { class: "converter-tags",
                                    for (n, (kind, text, title)) in tags.into_iter().enumerate() {
                                        RunTag { key: "{n}", kind, text, title }
                                    }
                                }
                            }
                            if !badge.is_empty() {
                                span { class: "lv-item-badge converter-state converter-state-{state}", title: "{detail}", "{badge}" }
                            }
                            button {
                                class: "btn icon subtle",
                                title: "{tip}",
                                "aria-label": "{tip}",
                                onclick: move |e| {
                                    e.stop_propagation();
                                    if active {
                                        RUNNER.cancel(id);
                                    } else {
                                        RUNNER.remove(id);
                                        items.write().retain(|i| i.id != id);
                                        selected.write().remove(&id);
                                    }
                                },
                                Icon { width: 14, height: 14, icon: LdX }
                            }
                        }
                    }
                }
            }
        }
        }
        // The run's choices, one column, every row the same width. Tooltips name them (no labels).
        aside { class: "pnl converter-panel",
            div { class: "barh-grp",
                select {
                    class: "select",
                    title: "{s_main_preset}",
                    "aria-label": "{s_main_preset}",
                    value: "{preset_id}",
                    onchange: move |e| settings.write().preset_id = e.value(),
                    for (id, name) in preset_options {
                        option { key: "{id}", value: "{id}", selected: id == preset_id, "{name}" }
                    }
                }
                button {
                    class: "btn icon",
                    title: "{s_main_edit_presets}",
                    "aria-label": "{s_main_edit_presets}",
                    onclick: move |_| on_manage.call(manager::PRESETS.to_owned()),
                    Icon { width: 14, height: 14, icon: LdSlidersHorizontal }
                }
            }
            div { class: "barh-grp",
                select {
                    class: "select",
                    title: "{s_main_size}",
                    "aria-label": "{s_main_size}",
                    value: "{size_id}",
                    onchange: move |e| settings.write().size_id = e.value(),
                    for (id, name) in size_options {
                        option { key: "{id}", value: "{id}", selected: id == size_id, "{name}" }
                    }
                }
                button {
                    class: "btn icon",
                    title: "{s_main_edit_sizes}",
                    "aria-label": "{s_main_edit_sizes}",
                    onclick: move |_| on_manage.call(manager::SIZES.to_owned()),
                    Icon { width: 14, height: 14, icon: LdScaling }
                }
            }
            div { class: "barh-grp",
                select {
                    class: "select",
                    title: "{s_main_audio}",
                    "aria-label": "{s_main_audio}",
                    value: "{audio_id}",
                    onchange: move |e| settings.write().audio_id = e.value(),
                    for (id, name) in audio_options {
                        option { key: "{id}", value: "{id}", selected: id == audio_id, "{name}" }
                    }
                }
                button {
                    class: "btn icon",
                    title: "{s_main_edit_audio}",
                    "aria-label": "{s_main_edit_audio}",
                    onclick: move |_| on_manage.call(manager::AUDIO.to_owned()),
                    Icon { width: 14, height: 14, icon: LdVolume2 }
                }
            }
            div { class: "barh-grp",
                select {
                    class: "select",
                    title: "{s_main_metadata}",
                    "aria-label": "{s_main_metadata}",
                    value: "{metadata_id}",
                    onchange: move |e| settings.write().metadata_id = e.value(),
                    for (id, name) in metadata_options {
                        option { key: "{id}", value: "{id}", selected: id == metadata_id, "{name}" }
                    }
                }
                button {
                    class: "btn icon",
                    title: "{s_main_edit_metadata}",
                    "aria-label": "{s_main_edit_metadata}",
                    onclick: move |_| on_manage.call(manager::METADATA.to_owned()),
                    Icon { width: 14, height: 14, icon: LdEraser }
                }
            }
            div { class: "barh-grp",
                select {
                    class: "select",
                    title: "{s_main_effects}",
                    "aria-label": "{s_main_effects}",
                    value: "{effects_id}",
                    onchange: move |e| settings.write().effects_id = e.value(),
                    for (id, name) in effects_options {
                        option { key: "{id}", value: "{id}", selected: id == effects_id, "{name}" }
                    }
                }
                button {
                    class: "btn icon",
                    title: "{s_main_edit_effects}",
                    "aria-label": "{s_main_edit_effects}",
                    onclick: move |_| on_manage.call(manager::EFFECTS.to_owned()),
                    Icon { width: 14, height: 14, icon: LdSparkles }
                }
            }
            div { class: "barh-grp",
                input {
                    class: "input",
                    title: "{s_main_trim}",
                    "aria-label": "{s_main_trim_start}",
                    placeholder: "{s_main_trim_start}",
                    value: "{trim_start}",
                    oninput: move |e| trim_start.set(e.value()),
                }
                input {
                    class: "input",
                    title: "{s_main_trim}",
                    "aria-label": "{s_main_trim_end}",
                    placeholder: "{s_main_trim_end}",
                    value: "{trim_end}",
                    oninput: move |e| trim_end.set(e.value()),
                }
            }
            div { class: "barh-grp",
                select {
                    class: "select",
                    title: "{s_main_destination}",
                    "aria-label": "{s_main_destination}",
                    // A new key rebuilds the box after the folder picker closes: cancelled, it must not go on
                    // showing the pick entry as if it were the choice.
                    key: "{pick_nonce}",
                    value: "{destination_id}",
                    onchange: move |e| {
                        let chosen = e.value();
                        if chosen == engine::destination::PICK_ID {
                            // Owned by the app's window: opened from the drop-down's own pop-up, a dialog without
                            // an owner came up BEHIND the window.
                            let window = dioxus::desktop::window();
                            spawn(async move {
                                if let Some(dir) = rfd::AsyncFileDialog::new().set_parent(&window.window).pick_folder().await {
                                    let id = settings.write().destination_for_folder(dir.path());
                                    settings.write().destination_id = id;
                                }
                                pick_nonce += 1;
                            });
                        } else {
                            settings.write().destination_id = chosen;
                        }
                    },
                    for (id, name) in dest_options {
                        option { key: "{id}", value: "{id}", selected: id == destination_id, "{name}" }
                    }
                }
                button {
                    class: "btn icon",
                    title: "{s_main_edit_destinations}",
                    "aria-label": "{s_main_edit_destinations}",
                    onclick: move |_| on_manage.call(manager::DESTINATIONS.to_owned()),
                    Icon { width: 14, height: 14, icon: LdFolder }
                }
            }
            div { class: "barh-grp",
                select {
                    class: "select",
                    title: "{s_main_overwrite}",
                    "aria-label": "{s_main_overwrite}",
                    value: match overwrite { Overwrite::Skip => "skip", Overwrite::Overwrite => "overwrite", Overwrite::Rename => "rename" },
                    onchange: move |e| {
                        settings.write().overwrite = match e.value().as_str() {
                            "overwrite" => Overwrite::Overwrite,
                            "rename" => Overwrite::Rename,
                            _ => Overwrite::Skip,
                        };
                    },
                    option { value: "skip", selected: overwrite == Overwrite::Skip, "{s_main_overwrite_skip}" }
                    option { value: "overwrite", selected: overwrite == Overwrite::Overwrite, "{s_main_overwrite_overwrite}" }
                    option { value: "rename", selected: overwrite == Overwrite::Rename, "{s_main_overwrite_rename}" }
                }
            }
            // ⚠ CLAUDE: the panel is a brush. It never shows a row's choices and selecting rows never changes
            // it; Apply writes ALL of it onto the selected rows, and is off when none can take it.
            button {
                class: "btn converter-panel-apply",
                title: "{s_main_apply_tip}",
                "aria-label": "{s_main_apply_tip}",
                disabled: !can_apply,
                onclick: apply,
                Icon { width: 14, height: 14, icon: LdListChecks }
                " {s_main_apply}"
            }
            div { class: "converter-panel-run",
                div { class: "barh-grp",
                    select {
                        class: "select",
                        title: "{s_main_parallel}",
                        "aria-label": "{s_main_parallel}",
                        value: "{parallel}",
                        onchange: move |e| {
                            if let Ok(n) = e.value().parse::<u32>() {
                                settings.write().parallel = n.clamp(1, crate::settings::max_parallel());
                            }
                        },
                        for n in 1..=crate::settings::max_parallel() {
                            option { key: "{n}", value: "{n}", selected: n == parallel, "{n}" }
                        }
                    }
                    if busy {
                        button {
                            class: "btn icon",
                            title: "{s_main_stop}",
                            "aria-label": "{s_main_stop}",
                            onclick: stop,
                            Icon { width: 14, height: 14, icon: LdSquare }
                        }
                    }
                    button {
                        class: "btn icon",
                        title: "{s_main_convert}",
                        "aria-label": "{s_main_convert}",
                        disabled: ffmpeg_missing || !have_items || busy || trim_invalid,
                        onclick: convert,
                        Icon { width: 14, height: 14, icon: LdPlay }
                    }
                }
                button {
                    class: "btn icon",
                    title: "{s_main_open_output}",
                    "aria-label": "{s_main_open_output}",
                    disabled: open_target.is_none(),
                    onclick: move |_| {
                        if let Some(p) = &open_target {
                            let _ = open::that(p);
                        }
                    },
                    Icon { width: 14, height: 14, icon: LdFolderOpen }
                }
            }
        }
        }
        }
        div { class: "barh-footer",
            span { class: "converter-status", "{status_text}" }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::range_between;

    #[test]
    fn a_range_runs_in_list_order_whichever_end_came_first() {
        let ids = [10, 11, 12, 13, 14];
        assert_eq!(range_between(&ids, 11, 13), vec![11, 12, 13]);
        assert_eq!(range_between(&ids, 13, 11), vec![11, 12, 13]);
        assert_eq!(range_between(&ids, 12, 12), vec![12]);
        assert_eq!(range_between(&ids, 99, 12), vec![12], "an anchor no longer in the list selects just the clicked row");
    }
}
