//! Fetching ffmpeg for the user (Windows). The app does not ship ffmpeg: the builds with x264 and
//! x265 are GPL, and bundling one would put this app under the GPL. Downloading one at the user's
//! request, into the app's own folder, keeps the app what it is and the user's ffmpeg a normal one.
//!
//! The source is gyan.dev's "release essentials" build. Its published SHA-256 is fetched first and
//! the archive is refused if it does not match; a build that says `--enable-nonfree` is refused too.

use crate::ffmpeg;
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::Duration,
};

pub const URL: &str = "https://www.gyan.dev/ffmpeg/builds/ffmpeg-release-essentials.zip";

#[derive(Clone, Debug, PartialEq)]
pub enum DownloadState {
    Idle,
    Downloading { done: u64, total: Option<u64> },
    Verifying,
    Unpacking,
    Done(PathBuf),
    Failed(String),
}

impl DownloadState {
    pub fn is_active(&self) -> bool {
        matches!(self, DownloadState::Downloading { .. } | DownloadState::Verifying | DownloadState::Unpacking)
    }
}

#[derive(Clone)]
pub struct Download {
    state: Arc<Mutex<DownloadState>>,
}

impl Default for Download {
    fn default() -> Self {
        Download::new()
    }
}

impl Download {
    pub fn new() -> Download {
        Download { state: Arc::new(Mutex::new(DownloadState::Idle)) }
    }

    pub fn snapshot(&self) -> DownloadState {
        self.state.lock().unwrap().clone()
    }

    /// Start fetching into `dir`. Does nothing while a download is already running.
    pub fn start(&self, dir: PathBuf) {
        {
            let mut st = self.state.lock().unwrap();
            if st.is_active() {
                return;
            }
            *st = DownloadState::Downloading { done: 0, total: None };
        }
        let state = Arc::clone(&self.state);
        std::thread::spawn(move || {
            let set = |s: DownloadState| *state.lock().unwrap() = s;
            match run(&dir, &set) {
                Ok(path) => set(DownloadState::Done(path)),
                Err(e) => set(DownloadState::Failed(e)),
            }
        });
    }
}

fn agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_connect(Some(Duration::from_secs(20)))
        .timeout_recv_body(Some(Duration::from_secs(120)))
        .build()
        .into()
}

/// The first 64-hex-digit token of a `.sha256` file (`<hash>` or `<hash>  <name>`).
pub fn parse_checksum(text: &str) -> Option<String> {
    let token = text.split_whitespace().next()?.to_ascii_lowercase();
    (token.len() == 64 && token.bytes().all(|b| b.is_ascii_hexdigit())).then_some(token)
}

fn run(dir: &Path, set: &dyn Fn(DownloadState)) -> Result<PathBuf, String> {
    fs::create_dir_all(dir).map_err(|e| format!("cannot create {}: {e}", dir.display()))?;
    let agent = agent();
    let expected = {
        let mut r = agent.get(format!("{URL}.sha256")).call().map_err(|e| format!("checksum: {e}"))?;
        let text = r.body_mut().read_to_string().map_err(|e| format!("checksum: {e}"))?;
        parse_checksum(&text).ok_or_else(|| "the published checksum could not be read".to_owned())?
    };

    let zip_path = dir.join("ffmpeg-download.zip");
    let digest = {
        let mut resp = agent.get(URL).call().map_err(|e| format!("download: {e}"))?;
        let total = resp.headers().get("content-length").and_then(|v| v.to_str().ok()).and_then(|v| v.parse::<u64>().ok());
        let mut reader = resp.body_mut().with_config().limit(1 << 31).reader();
        let mut file = fs::File::create(&zip_path).map_err(|e| e.to_string())?;
        let mut hasher = Sha256::new();
        let mut buf = vec![0u8; 256 * 1024];
        let mut done = 0u64;
        let mut last = 0u64;
        loop {
            let n = reader.read(&mut buf).map_err(|e| format!("download: {e}"))?;
            if n == 0 {
                break;
            }
            hasher.update(&buf[..n]);
            file.write_all(&buf[..n]).map_err(|e| e.to_string())?;
            done += n as u64;
            if done - last >= 512 * 1024 {
                last = done;
                set(DownloadState::Downloading { done, total });
            }
        }
        hasher.finalize()
    };
    set(DownloadState::Verifying);
    let actual = hex(&digest);
    if actual != expected {
        let _ = fs::remove_file(&zip_path);
        return Err("the download does not match its published checksum".to_owned());
    }
    set(DownloadState::Unpacking);
    let exe = unpack(&zip_path, dir)?;
    let _ = fs::remove_file(&zip_path);
    if ffmpeg::is_nonfree(&exe) != Some(false) {
        let _ = fs::remove_file(&exe);
        return Err("the downloaded ffmpeg did not run, or is a non-free build".to_owned());
    }
    Ok(exe)
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Take `…/bin/ffmpeg.exe` out of the archive into `dir`, through a `.part` file so a failed copy
/// never leaves a half-written ffmpeg that looks like a real one.
pub fn unpack(zip_path: &Path, dir: &Path) -> Result<PathBuf, String> {
    let file = fs::File::open(zip_path).map_err(|e| e.to_string())?;
    let mut archive = zip::ZipArchive::new(file).map_err(|e| format!("not a zip archive: {e}"))?;
    let name = ffmpeg::executable_name();
    let wanted = format!("/bin/{name}");
    let mut found = None;
    for i in 0..archive.len() {
        let entry = archive.by_index(i).map_err(|e| e.to_string())?;
        if entry.name().is_ok_and(|n| n.ends_with(&wanted)) {
            found = Some(i);
            break;
        }
    }
    let i = found.ok_or_else(|| format!("{name} is not in the archive"))?;
    let mut entry = archive.by_index(i).map_err(|e| e.to_string())?;
    let target = dir.join(name);
    let part = dir.join(format!("{name}.part"));
    {
        let mut out = fs::File::create(&part).map_err(|e| e.to_string())?;
        std::io::copy(&mut entry, &mut out).map_err(|e| e.to_string())?;
    }
    fs::rename(&part, &target).map_err(|e| e.to_string())?;
    Ok(target)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_checksum_file_is_read_with_or_without_a_name() {
        let h = "ab".repeat(32);
        assert_eq!(parse_checksum(&h), Some(h.clone()));
        assert_eq!(parse_checksum(&format!("{}  ffmpeg.zip\n", h.to_uppercase())), Some(h));
        assert_eq!(parse_checksum("<html>not found</html>"), None);
        assert_eq!(parse_checksum(""), None);
    }

    #[test]
    fn ffmpeg_is_taken_out_of_the_archive_and_nothing_else() {
        let dir = std::env::temp_dir().join(format!("yaiol-vdcv-unpack-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let zip_path = dir.join("a.zip");
        {
            let mut w = zip::ZipWriter::new(fs::File::create(&zip_path).unwrap());
            let o = zip::write::SimpleFileOptions::default();
            w.start_file("ffmpeg-8.0-essentials_build/bin/ffprobe.exe", o).unwrap();
            w.write_all(b"probe").unwrap();
            w.start_file(format!("ffmpeg-8.0-essentials_build/bin/{}", ffmpeg::executable_name()), o).unwrap();
            w.write_all(b"the program").unwrap();
            w.finish().unwrap();
        }
        let exe = unpack(&zip_path, &dir).unwrap();
        assert_eq!(fs::read(&exe).unwrap(), b"the program");
        assert!(!dir.join(format!("{}.part", ffmpeg::executable_name())).exists());
        assert!(!dir.join("ffprobe.exe").exists());
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn an_archive_without_ffmpeg_is_refused() {
        let dir = std::env::temp_dir().join(format!("yaiol-vdcv-unpack2-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let zip_path = dir.join("a.zip");
        {
            let mut w = zip::ZipWriter::new(fs::File::create(&zip_path).unwrap());
            w.start_file("readme.txt", zip::write::SimpleFileOptions::default()).unwrap();
            w.write_all(b"x").unwrap();
            w.finish().unwrap();
        }
        assert!(unpack(&zip_path, &dir).is_err());
        let _ = fs::remove_dir_all(dir);
    }
}
