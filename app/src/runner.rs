//! The one conversion runner of the process, the ffmpeg it drives, and the ffmpeg download.
//!
//! ffmpeg is looked up when the process starts (only in the folder every yaiol app shares, never on PATH)
//! and again after an install, and the effects catalogue installed with it is loaded at the same moments.
//! No ffmpeg is a real state the window reports instead of failing later at Convert.

use crate::{
    converter::notify_finished,
    ffmpeg_panel::{install, is_current},
    settings::Settings,
};
use dioxus::prelude::*;
use engine::{
    JobStatus, Runner,
    download::{Download, DownloadState, Latest, LatestState},
};
use std::{collections::HashMap, sync::LazyLock, time::Duration};

pub static RUNNER: LazyLock<Runner> = LazyLock::new(|| {
    let runner = Runner::new();
    refresh_ffmpeg(&runner);
    runner
});

pub static DOWNLOAD: LazyLock<Download> = LazyLock::new(Download::new);

/// Which ffmpeg yaiol says is the compatible one; asked at launch, and again from Settings only when that gave no answer.
pub static LATEST: LazyLock<Latest> = LazyLock::new(Latest::new);

/// Look for ffmpeg again; true when one was found.
pub fn refresh_ffmpeg(runner: &Runner) -> bool {
    let dir = crate::settings::ffmpeg_dir();
    let found = engine::ffmpeg::locate(dir.as_deref());
    // A catalogue that cannot be read is the same as none: the effects page says they are not installed.
    let _ = engine::effects::load_installed(dir.as_deref());
    let have = found.is_some();
    match &found {
        Some(p) => {
            engine::log::set_context("ffmpeg", &p.display().to_string());
            engine::log::set_context("ffmpeg version", &engine::ffmpeg::version_line(p));
        }
        None => engine::log::set_context("ffmpeg", "not installed"),
    }
    // Which graphics-card encoders this machine can run: asked of the encoders themselves, in the background.
    if let Some(p) = found.clone() {
        engine::hardware::load_or_probe_in_background(p);
    }
    runner.set_ffmpeg(found);
    have
}

/// What the app knows about the runs and about ffmpeg, whichever view is on screen: the status of every job, the download, and
/// whether ffmpeg is there. ONE copy, made in the app and handed to both views, which only show it. The things that start by
/// themselves (the ffmpeg check and install at launch, the update, the notice when a run ends) are done here, once per
/// launch - they were once written in each view and so ran again every time a view was mounted.
#[derive(Clone, Copy, PartialEq)]
pub struct RunState {
    pub statuses: Signal<HashMap<u64, JobStatus>>,
    pub download: Signal<DownloadState>,
    pub ffmpeg_ready: Signal<bool>,
    /// The popup the user closed (a failure, a newer version offered): it stays closed until an install starts again.
    pub dismissed: Signal<bool>,
}

impl RunState {
    /// A hook: call it once, from a component that outlives both views.
    pub fn new(settings: Signal<Settings>) -> RunState {
        let statuses = use_signal(HashMap::<u64, JobStatus>::new);
        let download = use_signal(|| DownloadState::Idle);
        let ffmpeg_ready = use_signal(|| RUNNER.ffmpeg().is_some());
        let dismissed = use_signal(|| false);
        let state = RunState { statuses, download, ffmpeg_ready, dismissed };
        use_hook(|| {
            // Which ffmpeg is the compatible one. Asked on every machine: the app cannot keep its ffmpeg right without
            // the answer, so this is not the optional request the dev-machine switch exists to silence.
            LATEST.check();
            // Nothing installed: install it now, there is nothing for the user to decide.
            if !*ffmpeg_ready.peek() {
                install();
            }
        });
        use_future(move || state.follow(settings));
        state
    }

    /// The window polls the runner and the download; the engine never calls back into it. Only a CHANGED snapshot is
    /// written, so an idle queue does not re-render the lists five times a second.
    async fn follow(self, settings: Signal<Settings>) {
        let RunState { mut statuses, mut download, mut ffmpeg_ready, mut dismissed } = self;
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
                }
                download.set(dl);
            }
            if let LatestState::Known(m) = LATEST.snapshot() {
                if !update_tried && *ffmpeg_ready.peek() && !is_current(&m) && !DOWNLOAD.snapshot().is_active() {
                    update_tried = true;
                    install();
                }
            }
        }
    }
}
