//! The work view: ONE file at a time, with its own list of effects within reach (`work_effects.rs`). A copy of the batch view
//! (`converter.rs`) with a compact panel and a button that opens the preview window (`work_preview.rs`) on the
//! selected file; what else it will do differently is not decided yet.
//!
//! ⚠ CLAUDE: NO HARDCODED UI STRINGS. Every user-visible string goes through the t() lookup with the key
//! a string literal at the call site.

use crate::{
    catalog::{UisAppHeader, UisFldLabelled, LbsDropZone},
    converter::{Item, NEXT_ID, RunTag, TagKind, overall_percent, percent_text, range_between, run_summary},
    i18n, manager,
    ffmpeg_panel::{FfmpegPopup, Popup, download_note},
    runner::{RUNNER, RunState},
    preview_session::{Drawer, Mark, State, TextEdit, WidgetEdit},
    settings::{Panel, RunChoice, Settings},
    work_effects::{FxView, WorkEffects},
    work_channels::ChannelsDrawer,
    work_colour::{ColourDrawer, Pick},
    work_compressor::CompressorDrawer,
    work_level::LevelDrawer,
    work_curves::CurvesDrawer,
    work_tone::ToneDrawer,
    work_preview::PreviewPane,
};
use crate::work_command::CommandPanel;
use dioxus::prelude::*;
use dioxus_free_icons::{
    Icon,
    icons::ld_icons::{LdEraser, LdFilePlus, LdFolder, LdFolderOpen, LdFilm, LdPlay, LdScaling, LdVideo, LdSquare, LdVolume2, LdX},
};
use engine::{effects::EffectsEntry, JobSpec, JobStatus, Overwrite, Trim, download::DownloadState};
use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
    sync::atomic::Ordering,
    time::Duration,
};

/// A time written in a cut box, when it holds one.
fn written_time(text: &str) -> Option<f64> {
    engine::trim::parse_time(text).flatten()
}

/// What a mark of the preview does to the panel's two cut boxes: a start that is not before the end clears the end
/// (and the other way round), so the boxes never hold a cut Convert would refuse.
/// What a widget drawn over the picture did: the effect takes the values (or leaves the list when they are the defaults).
fn apply_widget(mut work_fx: Signal<EffectsEntry>, edit: WidgetEdit) {
    let mut e = work_fx.peek().clone();
    if edit.keep {
        e.set_item(&edit.key, edit.values.as_ref());
    } else {
        e.set_widget(&edit.key, edit.values.as_ref());
    }
    work_fx.set(e.normalized().arranged(&crate::work_effects::standard()));
}

/// A text or a watermark added, changed or removed: `key` is the effect it belongs to.
fn apply_texts(mut work_fx: Signal<EffectsEntry>, (key, edit): (String, TextEdit)) {
    if crate::work_text::layer_widget(&key).is_none() {
        return;
    }
    let mut e = work_fx.peek().clone();
    match edit {
        TextEdit::Add(values) => e.add_item(&key, &values),
        TextEdit::Set(n, values) => e.set_nth(&key, n, &values),
        TextEdit::Remove(n) => e.remove_nth(&key, n),
    }
    work_fx.set(e.normalized().arranged(&crate::work_effects::standard()));
}

fn apply_mark(mut trim_start: Signal<String>, mut trim_end: Signal<String>, mark: Mark) {
    match mark {
        Mark::Start(t) => {
            let at = written_time(&t).unwrap_or(0.0);
            if written_time(&trim_end.peek()).is_some_and(|e| e <= at) {
                trim_end.set(String::new());
            }
            trim_start.set(t);
        }
        Mark::End(t) => {
            let at = written_time(&t).unwrap_or(0.0);
            if written_time(&trim_start.peek()).is_some_and(|s| s >= at) {
                trim_start.set(String::new());
            }
            trim_end.set(t);
        }
        Mark::Clear => {
            trim_start.set(String::new());
            trim_end.set(String::new());
        }
    }
}

/// What the preview is told: the first selected file in list order, the size cap and effects of the panel, the cut
/// boxes, and the language and theme.
fn chosen_preview(settings: Signal<Settings>, work_fx: Signal<EffectsEntry>, items: Signal<Vec<Item>>, selected: Signal<HashSet<u64>>, trim_start: Signal<String>, trim_end: Signal<String>) -> State {
    let s = settings.peek();
    let sel = selected.peek();
    let source = items.peek().iter().find(|i| sel.contains(&i.id)).map(|i| i.path.clone());
    let state = State {
        source,
        size: s.size_by(&s.work.size_id),
        effects: work_fx.peek().clone(),
        cut_start: trim_start.peek().clone(),
        cut_end: trim_end.peek().clone(),
        lang: s.lang.clone(),
        theme: s.theme.clone(),
    };
    state
}

/// The panel's choices as one file's run settings (the effects are the work list, as they are now).
fn panel_run(settings: Signal<Settings>, work_fx: Signal<EffectsEntry>, trim_start: Signal<String>, trim_end: Signal<String>, effects_name: &str) -> RunChoice {
    let s = settings.peek();
    let mut run = s.choice_of(&s.work, &trim_start.peek(), &trim_end.peek());
    let mut fx = work_fx.peek().clone();
    fx.name = effects_name.to_owned();
    run.effects_inline = Some(fx);
    run
}

/// What the panel was last known to hold, and for which selection: the point the next change is measured from.
#[derive(Default)]
struct PanelSeen {
    selection: HashSet<u64>,
    run: Option<RunChoice>,
}

/// Where the selected files do not agree: the panel shows those blank, until a touch makes them agree.
#[derive(Clone, Default, PartialEq)]
pub struct Mixed {
    /// The panel's fields (`preset`, `size`, `audio`, `tags`, `destination`, `overwrite`, `cut_start`, `cut_end`) that differ.
    pub fields: HashSet<&'static str>,
    /// Effects some of the files have and others do not.
    pub partial: HashSet<String>,
    /// Effects every file has, with values that are not the same.
    pub differing: HashSet<String>,
    /// The controls of those (`<effect>.<control>`) the files do not agree on.
    pub controls: HashSet<String>,
}

fn mixed_of(runs: &[&RunChoice]) -> Mixed {
    let mut m = Mixed::default();
    let Some((first, rest)) = runs.split_first() else { return m };
    if rest.is_empty() {
        return m;
    }
    let differ = |f: &dyn Fn(&RunChoice) -> String| rest.iter().any(|r| f(r) != f(first));
    let checks: [(&'static str, &dyn Fn(&RunChoice) -> String); 8] = [
        ("preset", &|r| r.preset_id.clone()),
        ("size", &|r| r.size_id.clone()),
        ("audio", &|r| r.audio_id.clone()),
        ("tags", &|r| r.metadata_id.clone()),
        ("destination", &|r| r.destination_id.clone()),
        ("overwrite", &|r| format!("{:?}", r.overwrite)),
        ("cut_start", &|r| r.trim_start.clone()),
        ("cut_end", &|r| r.trim_end.clone()),
    ];
    for (name, f) in checks {
        if differ(f) {
            m.fields.insert(name);
        }
    }
    let items_of = |r: &RunChoice| r.effects_inline.as_ref().map(|e| e.items.clone()).unwrap_or_default();
    let all: Vec<Vec<engine::effects::Item>> = runs.iter().map(|r| items_of(r)).collect();
    let keys: HashSet<&String> = all.iter().flatten().map(|i| &i.key).collect();
    for key in keys {
        let found: Vec<Option<&engine::effects::Item>> = all.iter().map(|l| l.iter().find(|i| &i.key == key)).collect();
        if found.iter().any(Option::is_none) {
            m.partial.insert(key.clone());
        } else if found.iter().any(|i| i != &found[0]) {
            m.differing.insert(key.clone());
            let values: Vec<_> = found.iter().flatten().map(|i| i.all_values()).collect();
            let ids: HashSet<&String> = values.iter().flat_map(|v| v.keys()).collect();
            for id in ids {
                if values.iter().any(|v| v.get(id) != values[0].get(id)) {
                    m.controls.insert(format!("{key}.{id}"));
                }
            }
        }
    }
    m
}

/// Write one change into every selected file that is not queued or running: a touch of a control that the files disagree on
/// reaches them all, even when it picks what the panel already held.
fn write_selected(mut items: Signal<Vec<Item>>, selected: Signal<HashSet<u64>>, statuses: Signal<HashMap<u64, JobStatus>>, change: impl Fn(&mut RunChoice)) {
    let sel = selected.peek().clone();
    let st = statuses.peek();
    for i in items.write().iter_mut() {
        if sel.contains(&i.id) && !st.get(&i.id).is_some_and(JobStatus::is_active) {
            change(&mut i.run);
        }
    }
}

/// The controls touched in the panel, copied from the panel's effects whatever they held before.
fn write_forced(run: &mut RunChoice, panel: &RunChoice, forced: &HashSet<String>) {
    let (Some(from), Some(own)) = (panel.effects_inline.as_ref(), run.effects_inline.as_mut()) else { return };
    for f in forced {
        let Some((key, id)) = f.split_once('.') else { continue };
        // what the panel holds for it: its own value, or the control's default when it holds none
        let value = from.items.iter().find(|i| i.key == key).and_then(|i| i.values.get(id).cloned().or_else(|| i.all_values().get(id).map(|v| v.to_json())));
        if let (Some(v), Some(item)) = (value, own.items.iter_mut().find(|i| i.key == key)) {
            item.values.insert(id.to_owned(), v);
        }
    }
}

/// What the panel changed since `base`, written into `run`: every field it touched, and nothing else - so a curve moved for
/// several files leaves each file's own cut, crop and other effects alone.
fn write_change(run: &mut RunChoice, base: &RunChoice, panel: &RunChoice) {
    if panel.preset_id != base.preset_id {
        run.preset_id = panel.preset_id.clone();
    }
    if panel.size_id != base.size_id {
        run.size_id = panel.size_id.clone();
    }
    if panel.audio_id != base.audio_id {
        run.audio_id = panel.audio_id.clone();
    }
    if panel.metadata_id != base.metadata_id {
        run.metadata_id = panel.metadata_id.clone();
    }
    if panel.destination_id != base.destination_id {
        run.destination_id = panel.destination_id.clone();
    }
    if panel.overwrite != base.overwrite {
        run.overwrite = panel.overwrite;
    }
    if panel.trim_start != base.trim_start {
        run.trim_start = panel.trim_start.clone();
    }
    if panel.trim_end != base.trim_end {
        run.trim_end = panel.trim_end.clone();
    }
    let none: Vec<engine::effects::Item> = Vec::new();
    let before = base.effects_inline.as_ref().map_or(&none, |e| &e.items);
    let after = panel.effects_inline.as_ref().map_or(&none, |e| &e.items);
    if before == after {
        return;
    }
    let mut fx = run.effects_inline.clone().unwrap_or_else(|| EffectsEntry::new(""));
    for old in before {
        if !after.iter().any(|n| n.key == old.key) {
            fx.items.retain(|i| i.key != old.key);
        }
    }
    for new in after {
        let old = before.iter().find(|o| o.key == new.key);
        if old == Some(new) {
            continue;
        }
        match (fx.items.iter_mut().find(|i| i.key == new.key), old) {
            // the file has it too: only the values the panel changed
            (Some(own), Some(old)) => {
                for (k, v) in &new.values {
                    if old.values.get(k) != Some(v) {
                        own.values.insert(k.clone(), v.clone());
                    }
                }
                own.values.retain(|k, _| old.values.get(k).is_none() || new.values.contains_key(k));
            }
            (Some(own), None) => *own = new.clone(),
            (None, _) => fx.items.push(new.clone()),
        }
    }
    run.effects_inline = Some(fx.normalized().arranged(&crate::work_effects::standard()));
}

/// The panel and the selected files are ONE setting. A new selection loads its first file's choices into the panel; after that, whatever the panel
/// changed is written, control by control, to every selected file that is not queued or running (each file otherwise keeps its own). Run every tick
/// by the view's loop.
fn sync_panel(
    mut settings: Signal<Settings>,
    mut work_fx: Signal<EffectsEntry>,
    mut items: Signal<Vec<Item>>,
    selected: Signal<HashSet<u64>>,
    mut trim_start: Signal<String>,
    mut trim_end: Signal<String>,
    statuses: Signal<HashMap<u64, JobStatus>>,
    mut mixed: Signal<Mixed>,
    mut touched: Signal<HashSet<String>>,
    seen: &mut PanelSeen,
) {
    let sel = selected.peek().clone();
    let now = {
        let list = items.peek();
        let runs: Vec<&RunChoice> = list.iter().filter(|i| sel.contains(&i.id)).map(|i| &i.run).collect();
        mixed_of(&runs)
    };
    if *mixed.peek() != now {
        mixed.set(now);
    }
    let Some(first) = items.peek().iter().find(|i| sel.contains(&i.id)).map(|i| i.run.clone()) else {
        seen.selection = sel;
        seen.run = None;
        return;
    };
    if sel != seen.selection || seen.run.is_none() {
        // a new selection: the panel shows the first file's choices
        seen.selection = sel;
        let p = settings.peek().work.clone();
        let loaded = Panel { preset_id: first.preset_id.clone(), size_id: first.size_id.clone(), audio_id: first.audio_id.clone(), metadata_id: first.metadata_id.clone(), destination_id: first.destination_id.clone(), overwrite: first.overwrite, ..p.clone() };
        if loaded != p {
            settings.write().work = loaded;
        }
        if *trim_start.peek() != first.trim_start {
            trim_start.set(first.trim_start.clone());
        }
        if *trim_end.peek() != first.trim_end {
            trim_end.set(first.trim_end.clone());
        }
        let fx = settings.peek().effects_of(&first);
        if work_fx.peek().items != fx.items {
            work_fx.set(fx);
        }
        let name = first.effects_inline.as_ref().map(|e| e.name.clone()).unwrap_or_default();
        seen.run = Some(panel_run(settings, work_fx, trim_start, trim_end, &name));
        return;
    }
    let name = first.effects_inline.as_ref().map(|e| e.name.clone()).unwrap_or_default();
    let panel = panel_run(settings, work_fx, trim_start, trim_end, &name);
    let Some(base) = seen.run.clone() else { return };
    let forced: HashSet<String> = std::mem::take(&mut *touched.write());
    if panel == base && forced.is_empty() {
        return;
    }
    let st = statuses.peek();
    for i in items.write().iter_mut() {
        if sel.contains(&i.id) && !st.get(&i.id).is_some_and(JobStatus::is_active) {
            write_change(&mut i.run, &base, &panel);
            write_forced(&mut i.run, &panel, &forced);
        }
    }
    seen.run = Some(panel);
}

/// What the work view keeps when it is not on screen: the list, the selection, the cut and the effects. The app creates it
/// once, so going to the batch view and back finds everything as it was.
#[derive(Clone, Copy, PartialEq)]
pub struct WorkState {
    items: Signal<Vec<Item>>,
    selected: Signal<HashSet<u64>>,
    anchor: Signal<Option<u64>>,
    trim_start: Signal<String>,
    trim_end: Signal<String>,
    work_fx: Signal<EffectsEntry>,
    /// Where the selected files disagree.
    mixed: Signal<Mixed>,
    /// The controls (`<effect>.<control>`) touched in the panel since the last tick: a touch reaches every selected file even when it
    /// sets what the panel already held.
    touched: Signal<HashSet<String>>,
    /// Where the preview was, in seconds.
    pos: Signal<f64>,
    /// What the pane is told, and the channel its picture travels on: kept, so the last picture is still there when the view comes back.
    pane_state: Signal<State>,
    channel: Signal<std::sync::Arc<engine::preview::Channel>>,
    /// The effects list's open rows and add menu.
    fx_view: FxView,
    /// The same for the block of sound effects.
    fx_view_audio: FxView,
}

impl WorkState {
    /// A hook: call it from a component that outlives both views.
    pub fn new() -> WorkState {
        WorkState {
            items: use_signal(Vec::new),
            selected: use_signal(HashSet::new),
            anchor: use_signal(|| None),
            trim_start: use_signal(String::new),
            trim_end: use_signal(String::new),
            work_fx: use_signal(|| EffectsEntry::new("work")),
            mixed: use_signal(Mixed::default),
            touched: use_signal(HashSet::new),
            pos: use_signal(|| 0.0),
            pane_state: use_signal(State::default),
            channel: use_signal(|| std::sync::Arc::new(engine::preview::Channel::new())),
            fx_view: FxView::new(),
            fx_view_audio: FxView::new(),
        }
    }
}

#[component]
pub fn Work(
    /// What survives a visit to the batch view.
    state: WorkState,
    /// True while this view is the one on screen: both stay mounted, the other is only hidden.
    active: bool,
    /// What the app knows about the runs and ffmpeg.
    run: RunState,
    settings: Signal<Settings>,
    app_name: String,
    app_version: String,
    /// The header's first group (batch / work) - the app owns it.
    header_start: Element,
    /// The header's right-hand group (GitHub, help, settings) - the app owns those.
    on_manage: EventHandler<String>,
    children: Element,
) -> Element {
    let WorkState { mut items, mut selected, mut anchor, mut trim_start, mut trim_end, work_fx, mixed, touched, pos, pane_state, channel, fx_view, fx_view_audio } = state;
    let drawer = use_signal(Drawer::default);
    // the panel under the preview folded away, so the picture takes its room
    let folded = use_signal(|| false);
    let pick = use_signal(|| None::<Pick>);
    let RunState { statuses, download, ffmpeg_ready, mut dismissed } = run;
    let mut pick_nonce = use_signal(|| 0u32);
    // What the preview pane in this window is told.
    let mut pane_state = pane_state;
    use_future(move || async move {
        let mut seen = PanelSeen::default();
        loop {
            tokio::time::sleep(Duration::from_millis(200)).await;
            sync_panel(settings, work_fx, items, selected, trim_start, trim_end, statuses, mixed, touched, &mut seen);
            let chosen = chosen_preview(settings, work_fx, items, selected, trim_start, trim_end);
            if *pane_state.peek() != chosen {
                pane_state.set(chosen);
            }
        }
    });

    // The window may not be shorter than the layout needs: measured whenever this view comes on screen (both views stay
    // mounted, and a hidden one has no layout to measure).
    use_effect(use_reactive!(|active| {
        if active {
            spawn(crate::fit_window_height());
        }
    }));

    let lang = settings.read().lang.clone();
    let t = |key: &str| i18n::t(&lang, key);

    let mut add_paths = move |paths: Vec<PathBuf>| {
        // Each file holds its own settings: an added file starts with no effects and no cut, and takes the preset, size, audio,
        // tags and destination of the file before it (the panel's own, for the first one). It becomes the selection.
        let mut run = match items.peek().last() {
            Some(prev) => RunChoice { trim_start: String::new(), trim_end: String::new(), ..prev.run.clone() },
            None => settings.read().choice_of(&settings.read().work, "", ""),
        };
        run.effects_id = engine::effects::NONE_ID.to_owned();
        let mut fx = EffectsEntry::new("");
        fx.name = format!("{} (0)", i18n::t(&settings.peek().lang, "lblWorkEffects"));
        run.effects_inline = Some(fx);
        let mut fresh = Vec::new();
        let mut list = items.write();
        for p in &paths {
            // Work takes files, never a folder: a dropped folder is ignored.
            if p.is_dir() {
                continue;
            }
            let mut found = Vec::new();
            engine::collect_video(p, &mut found);
            for path in found {
                if list.iter().any(|i| i.path == path) {
                    continue;
                }
                let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
                fresh.push(id);
                list.push(Item { id, name, path, root: None, run: run.clone() });
            }
        }
        if let Some(last) = fresh.last().copied() {
            selected.set(fresh.into_iter().collect());
            anchor.set(Some(last));
        }
    };

    // Files named when the app was started (another program opening a file in this one) are added once, when the view first appears.
    let mut opened = use_signal(|| false);
    use_effect(move || {
        if !*opened.peek() {
            opened.set(true);
            let given: Vec<PathBuf> = std::env::args_os().skip(1).map(PathBuf::from).filter(|p| p.is_file()).collect();
            if !given.is_empty() {
                add_paths(given);
            }
        }
    });

    let pick_files = move |_| {
        spawn(async move {
            let exts: Vec<&str> = engine::VIDEO_EXTENSIONS.to_vec();
            if let Some(files) = rfd::AsyncFileDialog::new().add_filter("video", &exts).pick_files().await {
                add_paths(files.into_iter().map(|f| f.path().to_path_buf()).collect());
            }
        });
    };

    // The command the first selected file would run with, the panel's choices and the effects as they are now (probed off the UI thread).
    let command = use_resource(move || {
        let s = settings.read();
        let sel = selected.read();
        let spec = items.read().iter().find(|i| sel.contains(&i.id)).and_then(|i| {
            let mut run = s.choice_of(&s.work, &trim_start.read(), &trim_end.read());
            run.effects_inline = Some(work_fx.read().clone());
            Some(JobSpec {
                id: i.id,
                input: i.path.clone(),
                source_root: i.root.clone(),
                preset: s.run_preset(&run.preset_id),
                folder: s.destination_by(&run.destination_id).folder_path(),
                overwrite: Overwrite::Overwrite,
                size: s.size_by(&run.size_id),
                audio: s.audio_by(&run.audio_id),
                trim: Trim::parse(&run.trim_start, &run.trim_end)?,
                effects: s.effects_of(&run),
                metadata: s.metadata_by(&run.metadata_id),
            })
        });
        async move {
            let spec = spec?;
            tokio::task::spawn_blocking(move || {
                let dir = crate::settings::ffmpeg_dir();
                let ffmpeg = engine::ffmpeg::locate(dir.as_deref()).ok_or_else(|| "ffmpeg not found".to_owned())?;
                engine::command_line(&ffmpeg, &spec)
            })
            .await
            .ok()
        }
    });
    let (command_text, command_failed) = match &*command.read() {
        Some(Some(Ok(line))) => (line.clone(), false),
        Some(Some(Err(reason))) => (reason.clone(), true),
        _ => (String::new(), false),
    };

    let ffmpeg_missing = !ffmpeg_ready();
    let busy = statuses.read().values().any(JobStatus::is_active);
    // A cut that does not parse stops Convert: the panel writes through, so it is in the row that would run.
    let trim_invalid = {
        let st = statuses.read();
        items.read().iter().any(|i| !st.get(&i.id).is_some_and(JobStatus::is_active) && Trim::parse(&i.run.trim_start, &i.run.trim_end).is_none())
    };

    // Every file runs with ITS OWN choices; only the number at once belongs to the run.
    let convert = move |_| {
        let s = settings.read();
        let parallel = s.work.parallel as usize;
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
                    preset: s.run_preset(&run.preset_id),
                    folder: s.destination_by(&run.destination_id).folder_path(),
                    overwrite: run.overwrite,
                    size: s.size_by(&run.size_id),
                    audio: s.audio_by(&run.audio_id),
                    trim: Trim::parse(&run.trim_start, &run.trim_end)?,
                    effects: s.effects_of(run),
                    metadata: s.metadata_by(&run.metadata_id),
                })
            })
            .collect();
        drop(st);
        drop(s);
        RUNNER.start(specs, parallel);
    };
    let stop = move |_| RUNNER.cancel_all();
    let empty_list = items.read().is_empty();
    let drop_text = t("msgWorkDrop");
    let have_items = !items.read().is_empty();
    let mixed_now = mixed.read().clone();
    let is_mixed = |f: &str| mixed_now.fields.contains(f);

    let s = settings.read();
    let preset_options: Vec<(String, String)> = s.presets.iter().map(|p| (p.id.clone(), p.name.clone())).collect();
    let preset_id = s.work.preset_id.clone();
    // "(Original)" comes first and is not an entry of Settings.
    let mut size_options: Vec<(String, String)> = vec![(engine::size::ORIGINAL_ID.to_owned(), t("lblMainSizeOriginal"))];
    size_options.extend(s.sizes.iter().map(|z| (z.id.clone(), z.name.clone())));
    let size_id = s.work.size_id.clone();
    // The audio on offer depends on what the selected preset's container can hold; a selection that
    // the preset cannot take is shown as the original.
    let sel_preset = s.preset_by(&s.work.preset_id);
    let mut audio_options: Vec<(String, String)> = vec![(engine::audio::ORIGINAL_ID.to_owned(), t("lblMainAudioOriginal"))];
    audio_options.extend(s.audios.iter().filter(|a| a.available_for(&sel_preset)).map(|a| (a.id.clone(), a.name.clone())));
    let metadata_options: Vec<(String, String)> = s.metadata.iter().map(|m| (m.id.clone(), m.name.clone())).collect();
    let metadata_id = s.work.metadata_id.clone();
    let audio_id = s.effective_audio_id_of(&s.work);
    let mut dest_options: Vec<(String, String)> = vec![(engine::destination::SOURCE_ID.to_owned(), t("lblMainDestSource"))];
    dest_options.extend(s.destinations.iter().map(|d| (d.id.clone(), d.name.clone())));
    dest_options.push((engine::destination::PICK_ID.to_owned(), t("lblMainDestPick")));
    let destination_id = s.work.destination_id.clone();
    let saved_folder = s.destination_by(&s.work.destination_id).folder_path();
    let parallel = s.work.parallel;
    let overwrite = s.work.overwrite;
    drop(s);
    // Where the selected files disagree the box shows nothing chosen: the value the panel holds is only the first file's.
    let (mx_preset, mx_size, mx_audio, mx_tags, mx_dest, mx_over, mx_cut_start, mx_cut_end) = (is_mixed("preset"), is_mixed("size"), is_mixed("audio"), is_mixed("tags"), is_mixed("destination"), is_mixed("overwrite"), is_mixed("cut_start"), is_mixed("cut_end"));
    let shown = |mixed: bool, id: &str| if mixed { String::new() } else { id.to_owned() };
    let (shown_preset, shown_size, shown_audio, shown_tags, shown_dest) = (shown(mx_preset, &preset_id), shown(mx_size, &size_id), shown(mx_audio, &audio_id), shown(mx_tags, &metadata_id), shown(mx_dest, &destination_id));

    let s_main_add_files = t("btnMainAddFiles");
    let s_main_stop = t("btnMainStop");
    let s_main_convert = t("btnMainConvert");
    let s_main_preset = t("lblMainVideo");
    let s_lbl_preset = s_main_preset.clone();
    let s_lbl_size = t("lblWorkSize");
    let s_lbl_audio = t("lblWorkAudio");
    let s_lbl_tags = t("lblWorkTags");
    let s_lbl_effects = t("lblWorkEffects");
    let s_lbl_effects_audio = t("lblWorkEffectsAudio");
    let s_main_effects_audio = t("tipMainEffectsAudio");
    let s_lbl_trim = t("lblWorkTrim");
    let s_lbl_destination = t("lblWorkDestination");
    let s_lbl_overwrite = t("lblWorkOverwrite");
    let s_main_edit_presets = t("tipMainEditPresets");
    let s_main_size = t("tipMainSize");
    let s_main_audio = t("tipMainAudio");
    let s_main_edit_audio = t("tipMainEditAudio");
    let s_main_trim = t("tipMainTrim");
    let s_main_trim_start = t("lblMainTrimStart");
    let s_main_trim_end = t("lblMainTrimEnd");
    let s_main_effects = t("tipMainEffects");
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
    let sel_now = selected.read().clone();
    // The names tags are made of, once per render: a row only looks its choices up.
    let cfg = settings.read().clone();
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
            format!("{} · {}", t("msgMainProgress").replace("{done}", &finished.to_string()).replace("{total}", &touched.to_string()), percent_text(overall_percent(st.values())))
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
        UisAppHeader { app_name, app_version, bare: true,
            {header_start}
            div { class: "uis-barh-grp",
                button {
                    class: "uis-btn ico",
                    title: "{s_main_add_files}",
                    "aria-label": "{s_main_add_files}",
                    onclick: pick_files,
                    Icon { width: 14, height: 14, icon: LdFilePlus }
                }
            }
        }
        LbsDropZone {
            class: "uis-app-main",
            label: drop_label,
            on_files: move |dropped: Vec<(String, String)>| {
                add_paths(dropped.into_iter().map(|(_, p)| PathBuf::from(p)).collect());
            },
            {children}
        div { class: "converter-body",
        div { class: match (empty_list, folded()) {
            (true, _) => "converter-main is-empty",
            (false, true) => "converter-main is-folded",
            (false, false) => "converter-main",
        },
        if let Some(popup) = popup {
            FfmpegPopup { lang: lang.clone(), popup, on_close: move |_| dismissed.set(true) }
        }
        // ⚠ CLAUDE: tabindex makes the list the thing a click focuses, which is what lets Ctrl+A reach it;
        // a click on the empty part clears the selection (a row's own click stops here).
        if empty_list {
            div { class: "work-empty", p { class: "wv-failure", "{drop_text}" } }
        }
        PreviewPane { state: pane_state, pos, channel, visible: active, on_mark: move |mark: Mark| apply_mark(trim_start, trim_end, mark), on_widget: move |edit: WidgetEdit| apply_widget(work_fx, edit), on_texts: move |edit: (String, TextEdit)| apply_texts(work_fx, edit), drawer, folded, pick }
        if drawer() == Drawer::Colour {
            ColourDrawer { list: work_fx, lang: lang.clone(), pick, on_edit: move |edit: WidgetEdit| apply_widget(work_fx, edit), mixed: mixed_now.differing.contains("colour") || mixed_now.partial.contains("colour") }
        } else if drawer() == Drawer::Tone {
            ToneDrawer { list: work_fx, lang: lang.clone(), on_edit: move |edit: WidgetEdit| apply_widget(work_fx, edit), mixed: mixed_now.differing.contains("equalizer") || mixed_now.partial.contains("equalizer") }
        } else if drawer() == Drawer::Compressor {
            CompressorDrawer { list: work_fx, lang: lang.clone(), on_edit: move |edit: WidgetEdit| apply_widget(work_fx, edit), mixed: mixed_now.differing.contains("compressor") || mixed_now.partial.contains("compressor") }
        } else if drawer() == Drawer::Level {
            LevelDrawer { list: work_fx, lang: lang.clone(), on_edit: move |edit: WidgetEdit| apply_widget(work_fx, edit), mixed: crate::work_level::MODES.iter().any(|k| mixed_now.differing.contains(*k) || mixed_now.partial.contains(*k)) }
        } else if drawer() == Drawer::Channels {
            ChannelsDrawer { list: work_fx, lang: lang.clone(), on_edit: move |edit: WidgetEdit| apply_widget(work_fx, edit), mixed: mixed_now.differing.contains("channels") || mixed_now.partial.contains("channels") }
        } else if drawer() == Drawer::Curves {
            CurvesDrawer { list: work_fx, lang: lang.clone(), on_edit: move |edit: WidgetEdit| apply_widget(work_fx, edit), mixed: mixed_now.differing.contains("curves") || mixed_now.partial.contains("curves") }
        } else {
        // the file list and the command share the height every panel of the strip has
        div { class: "work-strip",
        div {
            class: "uis-lvw-scroll converter-list work-list",
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
            for item in items.read().iter().cloned() {
                {
                    let status = statuses.read().get(&item.id).cloned();
                    let active = status.as_ref().is_some_and(JobStatus::is_active);
                    let (badge, detail, state) = match &status {
                        None => (String::new(), String::new(), "idle"),
                        Some(JobStatus::Queued) => (t("msgMainStatusQueued"), String::new(), "idle"),
                        Some(JobStatus::Probing) => (t("msgMainStatusReading"), String::new(), "idle"),
                        Some(JobStatus::Analysing { percent }) => (format!("{} {}", t("msgMainStatusAnalysing"), percent_text(*percent)), String::new(), "run"),
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
                    let fx = cfg.effects_of(r);
                    if !fx.is_none() && !(r.effects_inline.is_some() && fx.items.is_empty()) {
                        tags.push((TagKind::Effects, fx.name, s_main_effects.clone()));
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
                    let row_class = if sel_now.contains(&id) { "uis-lvw-item converter-row active" } else { "uis-lvw-item converter-row" };
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
                            div { class: "uis-lvw-item-icon converter-cover-box",
                                Icon { width: 18, height: 18, icon: LdFilm }
                            }
                            div { class: "uis-lvw-item-body",
                                div { class: "uis-lvw-item-name", "{item.name}" }
                                div { class: "uis-lvw-item-sub", "{path}" }
                                div { class: "converter-tags",
                                    for (n, (kind, text, title)) in tags.into_iter().enumerate() {
                                        RunTag { key: "{n}", kind, text, title }
                                    }
                                }
                            }
                            if !badge.is_empty() {
                                span { class: "uis-lvw-item-badge converter-state converter-state-{state}", title: "{detail}", "{badge}" }
                            }
                            button {
                                class: "uis-btn ico subtle",
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
        CommandPanel { text: command_text.clone(), failed: command_failed, lang: lang.clone() }
        }
        }
        }
        // The run's choices, one column, every row the same width. Tooltips name them (no labels).
        aside { class: "uis-pnl converter-panel work-panel uis-cmp-panel uis-cmp-side",
            UisFldLabelled { compact: true, label: s_lbl_preset.clone(), title: s_main_preset.clone(),
                select {
                    class: "uis-ddn-std",
                    title: "{s_main_preset}",
                    "aria-label": "{s_main_preset}",
                    value: "{shown_preset}",
                    onchange: move |e| {
                        let v = e.value();
                        settings.write().work.preset_id = v.clone();
                        write_selected(items, selected, statuses, |r| r.preset_id = v.clone());
                    },
                    if mx_preset { option { value: "", selected: true, disabled: true, hidden: true } }
                    for (id, name) in preset_options {
                        option { key: "{id}", value: "{id}", selected: id == shown_preset, "{name}" }
                    }
                }
                button {
                    class: "uis-btn ico",
                    title: "{s_main_edit_presets}",
                    "aria-label": "{s_main_edit_presets}",
                    onclick: move |_| on_manage.call(manager::PRESETS.to_owned()),
                    Icon { width: 14, height: 14, icon: LdVideo }
                }
            }
            UisFldLabelled { compact: true, label: s_lbl_size.clone(), title: s_main_size.clone(),
                select {
                    class: "uis-ddn-std",
                    title: "{s_main_size}",
                    "aria-label": "{s_main_size}",
                    value: "{shown_size}",
                    onchange: move |e| {
                        let v = e.value();
                        settings.write().work.size_id = v.clone();
                        write_selected(items, selected, statuses, |r| r.size_id = v.clone());
                    },
                    if mx_size { option { value: "", selected: true, disabled: true, hidden: true } }
                    for (id, name) in size_options {
                        option { key: "{id}", value: "{id}", selected: id == shown_size, "{name}" }
                    }
                }
                button {
                    class: "uis-btn ico",
                    title: "{s_main_edit_sizes}",
                    "aria-label": "{s_main_edit_sizes}",
                    onclick: move |_| on_manage.call(manager::SIZES.to_owned()),
                    Icon { width: 14, height: 14, icon: LdScaling }
                }
            }
            UisFldLabelled { compact: true, label: s_lbl_audio.clone(), title: s_main_audio.clone(),
                select {
                    class: "uis-ddn-std",
                    title: "{s_main_audio}",
                    "aria-label": "{s_main_audio}",
                    value: "{shown_audio}",
                    onchange: move |e| {
                        let v = e.value();
                        settings.write().work.audio_id = v.clone();
                        write_selected(items, selected, statuses, |r| r.audio_id = v.clone());
                    },
                    if mx_audio { option { value: "", selected: true, disabled: true, hidden: true } }
                    for (id, name) in audio_options {
                        option { key: "{id}", value: "{id}", selected: id == shown_audio, "{name}" }
                    }
                }
                button {
                    class: "uis-btn ico",
                    title: "{s_main_edit_audio}",
                    "aria-label": "{s_main_edit_audio}",
                    onclick: move |_| on_manage.call(manager::AUDIO.to_owned()),
                    Icon { width: 14, height: 14, icon: LdVolume2 }
                }
            }
            UisFldLabelled { compact: true, label: s_lbl_tags.clone(), title: s_main_metadata.clone(),
                select {
                    class: "uis-ddn-std",
                    title: "{s_main_metadata}",
                    "aria-label": "{s_main_metadata}",
                    value: "{shown_tags}",
                    onchange: move |e| {
                        let v = e.value();
                        settings.write().work.metadata_id = v.clone();
                        write_selected(items, selected, statuses, |r| r.metadata_id = v.clone());
                    },
                    if mx_tags { option { value: "", selected: true, disabled: true, hidden: true } }
                    for (id, name) in metadata_options {
                        option { key: "{id}", value: "{id}", selected: id == shown_tags, "{name}" }
                    }
                }
                button {
                    class: "uis-btn ico",
                    title: "{s_main_edit_metadata}",
                    "aria-label": "{s_main_edit_metadata}",
                    onclick: move |_| on_manage.call(manager::METADATA.to_owned()),
                    Icon { width: 14, height: 14, icon: LdEraser }
                }
            }
            div { class: "work-fx",
                div { class: "uis-fld-labelled-label wfx-label", title: "{s_main_effects}", "{s_lbl_effects}" }
                WorkEffects { list: work_fx, view: fx_view, lang: lang.clone(), mixed: mixed_now.clone(), touched }
            }
            div { class: "work-fx",
                div { class: "uis-fld-labelled-label wfx-label", title: "{s_main_effects_audio}", "{s_lbl_effects_audio}" }
                WorkEffects { list: work_fx, view: fx_view_audio, lang: lang.clone(), mixed: mixed_now.clone(), touched, sound: true }
            }
            UisFldLabelled { compact: true, label: s_lbl_trim.clone(), title: s_main_trim.clone(),
                input {
                    class: "uis-txt",
                    title: "{s_main_trim}",
                    "aria-label": "{s_main_trim_start}",
                    placeholder: "{s_main_trim_start}",
                    value: if mx_cut_start { String::new() } else { trim_start.read().clone() },
                    oninput: move |e| {
                        let v = e.value();
                        trim_start.set(v.clone());
                        write_selected(items, selected, statuses, |r| r.trim_start = v.clone());
                    },
                }
                input {
                    class: "uis-txt",
                    title: "{s_main_trim}",
                    "aria-label": "{s_main_trim_end}",
                    placeholder: "{s_main_trim_end}",
                    value: if mx_cut_end { String::new() } else { trim_end.read().clone() },
                    oninput: move |e| {
                        let v = e.value();
                        trim_end.set(v.clone());
                        write_selected(items, selected, statuses, |r| r.trim_end = v.clone());
                    },
                }
            }
            UisFldLabelled { compact: true, label: s_lbl_destination.clone(), title: s_main_destination.clone(),
                select {
                    class: "uis-ddn-std",
                    title: "{s_main_destination}",
                    "aria-label": "{s_main_destination}",
                    // A new key rebuilds the box after the folder picker closes: cancelled, it must not go on
                    // showing the pick entry as if it were the choice.
                    key: "{pick_nonce}",
                    value: "{shown_dest}",
                    onchange: move |e| {
                        let chosen = e.value();
                        if chosen == engine::destination::PICK_ID {
                            // Owned by the app's window: opened from the drop-down's own pop-up, a dialog without
                            // an owner came up BEHIND the window.
                            let window = dioxus::desktop::window();
                            spawn(async move {
                                if let Some(dir) = rfd::AsyncFileDialog::new().set_parent(&window.window).pick_folder().await {
                                    let id = settings.write().destination_for_folder(dir.path());
                                    settings.write().work.destination_id = id.clone();
                                    write_selected(items, selected, statuses, |r| r.destination_id = id.clone());
                                }
                                pick_nonce += 1;
                            });
                        } else {
                            settings.write().work.destination_id = chosen.clone();
                            write_selected(items, selected, statuses, |r| r.destination_id = chosen.clone());
                        }
                    },
                    if mx_dest { option { value: "", selected: true, disabled: true, hidden: true } }
                    for (id, name) in dest_options {
                        option { key: "{id}", value: "{id}", selected: id == shown_dest, "{name}" }
                    }
                }
                button {
                    class: "uis-btn ico",
                    title: "{s_main_edit_destinations}",
                    "aria-label": "{s_main_edit_destinations}",
                    onclick: move |_| on_manage.call(manager::DESTINATIONS.to_owned()),
                    Icon { width: 14, height: 14, icon: LdFolder }
                }
            }
            UisFldLabelled { compact: true, label: s_lbl_overwrite.clone(), title: s_main_overwrite.clone(),
                select {
                    class: "uis-ddn-std",
                    title: "{s_main_overwrite}",
                    "aria-label": "{s_main_overwrite}",
                    value: if mx_over { "" } else { match overwrite { Overwrite::Skip => "skip", Overwrite::Overwrite => "overwrite", Overwrite::Rename => "rename" } },
                    onchange: move |e| {
                        let chosen = match e.value().as_str() {
                            "overwrite" => Overwrite::Overwrite,
                            "rename" => Overwrite::Rename,
                            _ => Overwrite::Skip,
                        };
                        settings.write().work.overwrite = chosen;
                        write_selected(items, selected, statuses, |r| r.overwrite = chosen);
                    },
                    if mx_over { option { value: "", selected: true, disabled: true, hidden: true } }
                    option { value: "skip", selected: !mx_over && overwrite == Overwrite::Skip, "{s_main_overwrite_skip}" }
                    option { value: "overwrite", selected: !mx_over && overwrite == Overwrite::Overwrite, "{s_main_overwrite_overwrite}" }
                    option { value: "rename", selected: !mx_over && overwrite == Overwrite::Rename, "{s_main_overwrite_rename}" }
                }
            }
            // ⚠ RULE: the panel writes through. Selecting a file loads its choices into the panel; changing anything in the panel changes
            // the selected files at once (`sync_panel`), so there is no Apply and what the panel shows is what Convert runs.
            div { class: "converter-panel-run uis-cmp-run",
                div { class: "uis-barh-grp",
                    select {
                        class: "uis-ddn-std",
                        title: "{s_main_parallel}",
                        "aria-label": "{s_main_parallel}",
                        value: "{parallel}",
                        onchange: move |e| {
                            if let Ok(n) = e.value().parse::<u32>() {
                                settings.write().work.parallel = n.clamp(1, crate::settings::max_parallel());
                            }
                        },
                        for n in 1..=crate::settings::max_parallel() {
                            option { key: "{n}", value: "{n}", selected: n == parallel, "{n}" }
                        }
                    }
                    if busy {
                        button {
                            class: "uis-btn ico",
                            title: "{s_main_stop}",
                            "aria-label": "{s_main_stop}",
                            onclick: stop,
                            Icon { width: 14, height: 14, icon: LdSquare }
                        }
                    }
                    button {
                        class: "uis-btn ico",
                        title: "{s_main_convert}",
                        "aria-label": "{s_main_convert}",
                        disabled: ffmpeg_missing || !have_items || busy || trim_invalid,
                        onclick: convert,
                        Icon { width: 14, height: 14, icon: LdPlay }
                    }
                }
                button {
                    class: "uis-btn ico",
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
        div { class: "uis-barh-footer",
            span { class: "converter-status", "{status_text}" }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use engine::effects::Item as Fx;

    fn run_with(effects: Vec<Fx>, cut: &str) -> RunChoice {
        let mut fx = EffectsEntry::new("");
        fx.items = effects;
        RunChoice { preset_id: String::new(), size_id: String::new(), audio_id: String::new(), effects_id: String::new(), audio_effects_id: String::new(), effects_inline: Some(fx), metadata_id: String::new(), trim_start: cut.to_owned(), trim_end: String::new(), destination_id: String::new(), overwrite: Overwrite::Skip }
    }

    #[test]
    fn a_panel_change_reaches_a_file_as_that_control_only() {
        let base = run_with(vec![], "");
        let panel = run_with(vec![Fx::new("curves")], "");
        let mut own = run_with(vec![Fx::new("vignette")], "0:10");
        write_change(&mut own, &base, &panel);
        let keys: Vec<String> = own.effects_inline.unwrap().items.iter().map(|i| i.key.clone()).collect();
        assert!(keys.contains(&"curves".to_owned()) && keys.contains(&"vignette".to_owned()));
        assert_eq!(own.trim_start, "0:10");
    }

    #[test]
    fn the_files_disagree_only_where_they_differ() {
        let one = run_with(vec![Fx::new("curves"), Fx::new("vignette")], "");
        let two = run_with(vec![Fx::new("curves")], "0:10");
        let m = mixed_of(&[&one, &two]);
        assert!(m.fields.contains("cut_start") && !m.fields.contains("preset"));
        assert!(m.partial.contains("vignette") && !m.partial.contains("curves"));
        assert!(mixed_of(&[&one]).fields.is_empty());
    }

    #[test]
    fn a_touched_control_reaches_the_files_even_when_it_sets_the_same_value() {
        let panel = run_with(vec![Fx::with("vignette", &[("angle", "0.5")])], "");
        let mut own = run_with(vec![Fx::with("vignette", &[("angle", "0.9")])], "");
        let forced: HashSet<String> = ["vignette.angle".to_owned()].into();
        write_forced(&mut own, &panel, &forced);
        assert_eq!(own.effects_inline.unwrap().items[0].values.get("angle").and_then(|v| v.as_str()), Some("0.5"));
    }

    #[test]
    fn a_removed_effect_leaves_only_the_files_that_had_it() {
        let base = run_with(vec![Fx::new("curves")], "");
        let panel = run_with(vec![], "");
        let mut own = run_with(vec![Fx::new("curves"), Fx::new("vignette")], "");
        write_change(&mut own, &base, &panel);
        let keys: Vec<String> = own.effects_inline.unwrap().items.iter().map(|i| i.key.clone()).collect();
        assert_eq!(keys, vec!["vignette".to_owned()]);
    }
}
