//! engine — everything that is not the window.
//!
//! No UI dependency: the app crate depends on this, never the reverse, so the toolkit stays
//! replaceable. Pure functions and data only.

pub mod audio;
pub mod destination;
pub mod download;
pub mod effects;
pub mod ffmpeg;
pub mod fonts;
pub mod ffmpeg_effects;
pub mod grade_pick;
pub mod hardware;
pub mod log;
pub mod measured;
pub mod metadata;
pub mod pictures;
pub mod preset;
pub mod preview;
pub mod queue;
pub mod size;
pub mod sound;
pub mod sound_plan;
pub mod trim;

pub use audio::AudioEntry;
pub use destination::{Destination, Overwrite};
pub use effects::EffectsEntry;
pub use preset::Preset;
pub use queue::{command_line, JobSpec, JobStatus, Runner};
pub use metadata::MetadataCleanup;
pub use size::Size;
pub use trim::Trim;

use std::cmp::Ordering;

/// What the file pickers and a dropped folder accept. Input is whatever ffmpeg decodes; this is the
/// list of video containers people actually have, so a folder walk does not pull in every file.
pub const VIDEO_EXTENSIONS: [&str; 18] = [
    "mkv", "mp4", "m4v", "mov", "avi", "wmv", "flv", "webm", "mpg", "mpeg", "mts", "m2ts", "ts", "3gp", "ogv", "vob", "divx", "asf",
];

pub fn is_video_file(name: &str) -> bool {
    name.rsplit_once('.').is_some_and(|(_, ext)| VIDEO_EXTENSIONS.iter().any(|e| e.eq_ignore_ascii_case(ext)))
}

/// Every video file under `path` (the file itself when it is one), in name order.
pub fn collect_video(path: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
    fn walk(path: &std::path::Path, out: &mut Vec<std::path::PathBuf>, depth: u32) {
        if path.is_dir() {
            if depth > 12 {
                return;
            }
            let Ok(rd) = std::fs::read_dir(path) else { return };
            let mut entries: Vec<std::path::PathBuf> = rd.filter_map(|e| e.ok().map(|e| e.path())).collect();
            entries.sort();
            for e in entries {
                walk(&e, out, depth + 1);
            }
        } else if path.file_name().is_some_and(|n| is_video_file(&n.to_string_lossy())) {
            out.push(path.to_path_buf());
        }
    }
    walk(path, out, 0);
}

/// How many of a run's jobs ended each way: (converted, skipped, failed).
pub fn run_counts<'a>(statuses: impl Iterator<Item = &'a JobStatus>) -> (usize, usize, usize) {
    let (mut done, mut skipped, mut failed) = (0, 0, 0);
    for s in statuses {
        match s {
            JobStatus::Done { .. } => done += 1,
            JobStatus::Skipped { .. } => skipped += 1,
            JobStatus::Failed { .. } => failed += 1,
            _ => {}
        }
    }
    (done, skipped, failed)
}

/// Languages whose text runs right to left, by workspace language code.
pub fn is_rtl(lang: &str) -> bool {
    matches!(lang, "ar" | "he" | "fa" | "ur")
}

/// Compare two dotted versions ("1.2.3" vs "1.2.10") number by number. Missing segments are 0 and
/// a non-numeric segment reads as 0, so a malformed beacon can never panic - it just compares low.
/// The twin of `cmpVersion` in the shared update-check.js; the two must agree.
pub fn compare_versions(a: &str, b: &str) -> Ordering {
    let parts = |v: &str| -> Vec<u64> {
        v.split('.').map(|p| p.trim().parse::<u64>().unwrap_or(0)).collect()
    };
    let (pa, pb) = (parts(a), parts(b));
    for i in 0..pa.len().max(pb.len()) {
        let o = pa.get(i).copied().unwrap_or(0).cmp(&pb.get(i).copied().unwrap_or(0));
        if o != Ordering::Equal {
            return o;
        }
    }
    Ordering::Equal
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn video_files_are_told_by_extension_in_any_case() {
        assert!(is_video_file("Holiday.MKV"));
        assert!(is_video_file("a.b.mp4"));
        assert!(!is_video_file("song.mp3"));
        assert!(!is_video_file("mkv"));
    }

    #[test]
    fn right_to_left_languages() {
        for code in ["ar", "he", "fa", "ur"] {
            assert!(is_rtl(code), "{code}");
        }
    }

    #[test]
    fn left_to_right_languages_and_variants() {
        for code in ["en", "fr", "zh_CN", "pt_BR"] {
            assert!(!is_rtl(code), "{code}");
        }
    }

    #[test]
    fn versions_compare_numerically_not_textually() {
        assert_eq!(compare_versions("1.2.10", "1.2.3"), Ordering::Greater);
        assert_eq!(compare_versions("1.2", "1.2.0"), Ordering::Equal);
        assert_eq!(compare_versions("0.9.9", "1.0.0"), Ordering::Less);
    }

    #[test]
    fn malformed_versions_never_panic() {
        assert_eq!(compare_versions("", "0.0.0"), Ordering::Equal);
        assert_eq!(compare_versions("abc", "1.0.0"), Ordering::Less);
    }


}
