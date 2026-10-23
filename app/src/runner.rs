//! The one conversion runner of the process, the ffmpeg it drives, and the ffmpeg download.
//!
//! ffmpeg is looked up when the process starts (beside the executable, in the app's own folder,
//! then on PATH) and again after a download. No ffmpeg is a real state the window reports instead of
//! failing later at Convert.

use engine::{Runner, download::Download};
use std::sync::LazyLock;

pub static RUNNER: LazyLock<Runner> = LazyLock::new(|| {
    let runner = Runner::new();
    refresh_ffmpeg(&runner);
    runner
});

pub static DOWNLOAD: LazyLock<Download> = LazyLock::new(Download::new);

/// Look for ffmpeg again; true when one was found.
pub fn refresh_ffmpeg(runner: &Runner) -> bool {
    let found = engine::ffmpeg::locate(crate::settings::ffmpeg_dir().as_deref());
    let have = found.is_some();
    runner.set_ffmpeg(found);
    have
}
