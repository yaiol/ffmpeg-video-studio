//! The log file: what to send when something goes wrong.
//!
//! One plain-text file, `ffmpeg-video-studio.log`, in the app's data folder. Every launch writes a header (the version, the
//! system, the ffmpeg in use); every error writes an entry with what the engine knew at that moment - the command that was
//! run, what ffmpeg said, the job's settings - and the context set so far. When the file passes `MAX_BYTES` the previous
//! one is kept as `.1` and a new one starts, so it never grows without end. Nothing is ever sent anywhere: the user attaches
//! the file by hand.

use std::{
    ffi::OsString,
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    sync::{Mutex, OnceLock},
    time::{SystemTime, UNIX_EPOCH},
};

const MAX_BYTES: u64 = 1_000_000;

struct Log {
    file: PathBuf,
    /// Facts about this run that every error entry repeats (the ffmpeg in use, its version...).
    context: Vec<(String, String)>,
}

static LOG: OnceLock<Mutex<Log>> = OnceLock::new();

/// Start logging into `dir/<name>.log` and write the header of this run. Called once, first thing.
pub fn init(dir: &Path, name: &str, header: &[(&str, String)]) {
    let _ = fs::create_dir_all(dir);
    let file = dir.join(format!("{name}.log"));
    if fs::metadata(&file).map(|m| m.len() > MAX_BYTES).unwrap_or(false) {
        let _ = fs::rename(&file, dir.join(format!("{name}.log.1")));
    }
    let _ = LOG.set(Mutex::new(Log { file, context: Vec::new() }));
    let mut text = String::from("\n================ new run ================\n");
    for (key, value) in header {
        text.push_str(&format!("{key}: {value}\n"));
    }
    write(&text);
}

/// Where the log is, once `init` has run.
pub fn path() -> Option<PathBuf> {
    LOG.get().map(|l| l.lock().unwrap_or_else(|e| e.into_inner()).file.clone())
}

/// A fact every later error entry carries; setting it again replaces the earlier value.
pub fn set_context(key: &str, value: &str) {
    if let Some(l) = LOG.get() {
        let mut l = l.lock().unwrap_or_else(|e| e.into_inner());
        match l.context.iter_mut().find(|(k, _)| k == key) {
            Some(entry) => entry.1 = value.to_owned(),
            None => l.context.push((key.to_owned(), value.to_owned())),
        }
    }
}

/// A line worth keeping next to the errors (a launch, an install).
pub fn info(area: &str, message: &str) {
    write(&format!("[{}] info  {area}: {message}\n", now()));
}

/// An error, with the context of the run under it.
pub fn error(area: &str, message: &str) {
    let mut text = format!("[{}] ERROR {area}: {message}\n", now());
    if let Some(l) = LOG.get() {
        let l = l.lock().unwrap_or_else(|e| e.into_inner());
        for (key, value) in &l.context {
            text.push_str(&format!("    {key}: {value}\n"));
        }
    }
    write(&text);
}

/// `program arg arg ...` as one line to read, each argument quoted when it holds a space.
pub fn command_line(program: &Path, args: &[OsString]) -> String {
    let quote = |s: String| if s.is_empty() || s.contains([' ', '"']) { format!("\"{}\"", s.replace('"', "\\\"")) } else { s };
    let mut line = quote(program.display().to_string());
    for a in args {
        line.push(' ');
        line.push_str(&quote(a.to_string_lossy().into_owned()));
    }
    line
}

/// A panic anywhere (any thread) goes in the log, with where it happened and the stack, then the usual handling goes on.
pub fn install_panic_hook() {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let thread = std::thread::current();
        error("panic", &format!("thread {}: {info}\n{}", thread.name().unwrap_or("unnamed"), std::backtrace::Backtrace::force_capture()));
        previous(info);
    }));
}

fn write(text: &str) {
    let Some(l) = LOG.get() else { return };
    let l = l.lock().unwrap_or_else(|e| e.into_inner());
    if let Ok(mut f) = OpenOptions::new().create(true).append(true).open(&l.file) {
        let _ = f.write_all(text.as_bytes());
    }
}

/// UTC, `2026-10-28 14:03:09`.
fn now() -> String {
    let secs = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0) as i64;
    let (days, rest) = (secs.div_euclid(86_400), secs.rem_euclid(86_400));
    // Days since 1970-01-01 to a calendar date (Howard Hinnant's civil_from_days).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!("{year:04}-{month:02}-{day:02} {:02}:{:02}:{:02}", rest / 3600, rest / 60 % 60, rest % 60)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_clock_reads_as_a_utc_date() {
        let s = now();
        assert_eq!(s.len(), 19, "{s}");
        assert!(s.starts_with("20"), "{s}");
    }

    #[test]
    fn a_command_line_quotes_what_holds_a_space() {
        let line = command_line(Path::new("C:\\a b\\ffmpeg.exe"), &[OsString::from("-i"), OsString::from("my file.mp4")]);
        assert_eq!(line, "\"C:\\a b\\ffmpeg.exe\" -i \"my file.mp4\"");
    }
}
