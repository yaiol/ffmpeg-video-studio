//! The picture of a converted video, for the player of the finished files: ffmpeg decodes the file at its own pace and every frame is a JPEG handed to the
//! window as a motion-JPEG stream by a small server on this machine, which a web view draws in a plain image element. Every format
//! ffmpeg reads shows (an MKV, an AV1), whatever the web view could play by itself. The sound is `player::Sound`, started at the same
//! moment; the two are not locked to one clock.
//!
//! A COPY of Media Downloader's `picture.rs` (itself the playback part of this app's `preview.rs`, without the effects): a change to
//! one is made to the other. The preview keeps its own machinery because its pictures carry the effects.

use crate::ffmpeg::command;
use std::{
    collections::HashMap,
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    path::Path,
    process::{Child, Stdio},
    sync::{
        Arc, Condvar, Mutex, OnceLock,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    thread,
    time::Duration,
};

/// The picture is no wider than this (never enlarged): the pane is small, and a JPEG per frame of a 4K video would be heavy for nothing.
const MAX_WIDTH: u32 = 960;
/// The most pictures a second.
const MAX_FPS: u32 = 30;

/// The latest picture anyone made, and the number of it (it only grows), with a way to wait for the next one.
pub struct Hub {
    latest: Mutex<(u64, Option<Arc<Vec<u8>>>)>,
    changed: Condvar,
    /// The producer whose pictures count: each playback and each still claims the hub when it starts, and a picture from a producer
    /// that is no longer the latest one is dropped (a stopped playback's last pictures, a still that took longer than the next playback).
    producer: AtomicU64,
}

impl Hub {
    pub const fn new() -> Hub {
        Hub { latest: Mutex::new((0, None)), changed: Condvar::new(), producer: AtomicU64::new(0) }
    }

    /// A new producer takes over the hub; its number goes with every picture it publishes.
    pub fn claim(&self) -> u64 {
        self.producer.fetch_add(1, Ordering::SeqCst) + 1
    }

    /// A picture from `producer`, kept only while that producer is the latest one.
    pub fn publish_from(&self, producer: u64, jpeg: Vec<u8>) {
        let mut g = self.latest.lock().unwrap_or_else(|e| e.into_inner());
        if self.producer.load(Ordering::SeqCst) != producer {
            return;
        }
        g.0 += 1;
        g.1 = Some(Arc::new(jpeg));
        self.changed.notify_all();
    }

    /// The picture numbered after `seen`, waiting up to `timeout` for it; None when none came.
    pub fn wait_newer(&self, seen: u64, timeout: Duration) -> Option<(u64, Arc<Vec<u8>>)> {
        let mut g = self.latest.lock().unwrap_or_else(|e| e.into_inner());
        if g.0 <= seen {
            g = self.changed.wait_timeout_while(g, timeout, |g| g.0 <= seen).unwrap_or_else(|e| e.into_inner()).0;
        }
        match (&g.1, g.0 > seen) {
            (Some(p), true) => Some((g.0, p.clone())),
            _ => None,
        }
    }
}

impl Default for Hub {
    fn default() -> Hub {
        Hub::new()
    }
}

/// The ffmpeg arguments for one picture (`one_frame`) or a playback at the file's own pace, from `at` seconds.
fn args(file: &Path, at: f64, one_frame: bool) -> Vec<std::ffi::OsString> {
    let mut a: Vec<std::ffi::OsString> = Vec::new();
    let mut push = |items: &[&str]| a.extend(items.iter().map(std::ffi::OsString::from));
    push(&["-hide_banner", "-nostdin", "-loglevel", "error"]);
    // `-re` reads at the file's own pace, which is what makes a playback run in time.
    if !one_frame {
        push(&["-re"]);
    }
    push(&["-ss", &format!("{:.3}", at.max(0.0))]);
    a.push("-i".into());
    a.push(file.into());
    let mut push = |items: &[&str]| a.extend(items.iter().map(std::ffi::OsString::from));
    push(&["-map", "0:v:0", "-vf", &format!("scale='min({MAX_WIDTH},iw)':-2"), "-an", "-sn"]);
    if one_frame {
        push(&["-frames:v", "1"]);
    } else {
        push(&["-fpsmax", &MAX_FPS.to_string()]);
    }
    // `-strict unofficial`: a file can hold limited-range YUV, which the mjpeg encoder refuses otherwise.
    push(&["-f", "image2pipe", "-c:v", "mjpeg", "-strict", "unofficial", "-q:v", "5", "pipe:1"]);
    a
}

/// A playback: ffmpeg running at the file's own pace, its pictures published as they come. Stopped when dropped.
pub struct Stream {
    child: Child,
    done: Arc<AtomicBool>,
}

impl Stream {
    pub fn start(ffmpeg: &Path, file: &Path, at: f64, hub: Arc<Hub>) -> Result<Stream, String> {
        let mut child = command(ffmpeg).args(args(file, at, false)).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::null()).spawn().map_err(|e| {
            let why = format!("cannot run ffmpeg: {e}");
            crate::log::error("player picture", &format!("{why}\ninput: {}", file.display()));
            why
        })?;
        let mut out = child.stdout.take().ok_or("no output pipe")?;
        let done = Arc::new(AtomicBool::new(false));
        let end = done.clone();
        let producer = hub.claim();
        thread::spawn(move || {
            // The pictures arrive back to back; a JPEG ends with FF D9 (a literal FF inside the data is always followed by 00).
            let (mut buf, mut chunk, mut scanned) = (Vec::<u8>::new(), vec![0u8; 64 * 1024], 0usize);
            while let Ok(read) = out.read(&mut chunk) {
                if read == 0 {
                    break;
                }
                buf.extend_from_slice(&chunk[..read]);
                while let Some(at) = buf[scanned..].windows(2).position(|w| w == [0xFF, 0xD9]).map(|p| p + scanned) {
                    let rest = buf.split_off(at + 2);
                    hub.publish_from(producer, std::mem::replace(&mut buf, rest));
                    scanned = 0;
                }
                // The last byte may be the FF of an FF D9 split across two reads.
                scanned = buf.len().saturating_sub(1);
            }
            end.store(true, Ordering::Relaxed);
        });
        Ok(Stream { child, done })
    }

    /// ffmpeg has stopped (the end of the file) and everything it made was published.
    pub fn finished(&self) -> bool {
        self.done.load(Ordering::Relaxed)
    }
}

impl Drop for Stream {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// The one picture at `at` seconds, published to `hub` (what a paused player shows). Blocks until ffmpeg has made it.
pub fn still(ffmpeg: &Path, file: &Path, at: f64, hub: &Hub) -> Result<(), String> {
    let producer = hub.claim();
    let out = command(ffmpeg).args(args(file, at, true)).stdin(Stdio::null()).stderr(Stdio::null()).output().map_err(|e| format!("cannot run ffmpeg: {e}"))?;
    if out.stdout.is_empty() {
        return Err("no picture".to_owned());
    }
    hub.publish_from(producer, out.stdout);
    Ok(())
}

/// How long the file plays, in seconds, as ffmpeg reads it (for the files whose header the tag reader does not know: MKV, WebM).
pub fn duration(ffmpeg: &Path, file: &Path) -> Option<f64> {
    let out = command(ffmpeg).args(["-hide_banner", "-nostdin", "-i"]).arg(file).stdin(Stdio::null()).stdout(Stdio::null()).output().ok()?;
    parse_duration(&String::from_utf8_lossy(&out.stderr))
}

/// `Duration: 00:03:07.52` in what ffmpeg says about a file.
fn parse_duration(text: &str) -> Option<f64> {
    let rest = text.split("Duration: ").nth(1)?;
    let clock = rest.split([',', ' ', '\n']).next()?;
    let mut parts = clock.split(':');
    let (h, m, s) = (parts.next()?.parse::<f64>().ok()?, parts.next()?.parse::<f64>().ok()?, parts.next()?.parse::<f64>().ok()?);
    let d = h * 3600.0 + m * 60.0 + s;
    (d > 0.0).then_some(d)
}

/// Where the player's pictures go and where the window's image reads them.
pub struct Channel {
    pub hub: Arc<Hub>,
    /// The address of the motion-JPEG stream of this hub, or why the server could not start.
    pub url: Result<String, String>,
}

struct Server {
    base: String,
    /// The path each hub is served under.
    hubs: Mutex<HashMap<String, Arc<Hub>>>,
    next: AtomicU64,
    token: String,
}

static SERVER: OnceLock<Result<Arc<Server>, String>> = OnceLock::new();

/// Started once, on this machine only, on a free port; the paths carry a name nobody guesses by chance.
fn server() -> Result<Arc<Server>, String> {
    SERVER
        .get_or_init(|| {
            let listener = TcpListener::bind("127.0.0.1:0").map_err(|e| format!("cannot start the picture: {e}"))?;
            let port = listener.local_addr().map_err(|e| e.to_string())?.port();
            let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(1);
            let token = format!("{:x}{:x}", nanos, std::process::id());
            let me = Arc::new(Server { base: format!("http://127.0.0.1:{port}"), hubs: Mutex::new(HashMap::new()), next: AtomicU64::new(1), token });
            let serving = me.clone();
            thread::spawn(move || {
                for conn in listener.incoming().flatten() {
                    let serving = serving.clone();
                    thread::spawn(move || serve(conn, &serving));
                }
            });
            Ok(me)
        })
        .clone()
}

impl Channel {
    pub fn new() -> Channel {
        let hub = Arc::new(Hub::new());
        let url = server().map(|s| {
            let path = format!("/{}/{}.mjpeg", s.token, s.next.fetch_add(1, Ordering::Relaxed));
            s.hubs.lock().unwrap_or_else(|e| e.into_inner()).insert(path.clone(), hub.clone());
            format!("{}{path}", s.base)
        });
        Channel { hub, url }
    }
}

impl Default for Channel {
    fn default() -> Channel {
        Channel::new()
    }
}

fn serve(mut conn: TcpStream, server: &Server) {
    let mut head = Vec::new();
    let mut byte = [0u8; 1];
    // The request line and headers, up to the blank line.
    while !head.ends_with(b"\r\n\r\n") && head.len() < 8192 {
        match conn.read(&mut byte) {
            Ok(1) => head.push(byte[0]),
            _ => return,
        }
    }
    let head = String::from_utf8_lossy(&head);
    let asked = head.split_whitespace().nth(1).unwrap_or("").to_owned();
    let hub = server.hubs.lock().unwrap_or_else(|e| e.into_inner()).get(&asked).cloned();
    let Some(hub) = hub else {
        let _ = conn.write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
        return;
    };
    let intro = "HTTP/1.1 200 OK\r\nContent-Type: multipart/x-mixed-replace; boundary=frame\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n";
    if conn.write_all(intro.as_bytes()).is_err() {
        return;
    }
    // Each picture is followed by the boundary that opens the next part: the web view shows a part only once it sees the boundary that
    // ends it, so a boundary written before the next picture would keep the latest one hidden (a still would show the one before it).
    if conn.write_all(b"--frame\r\n").is_err() {
        return;
    }
    let mut seen = 0;
    loop {
        // A picture was already there when the window connected: it is shown at once.
        let Some((n, jpeg)) = hub.wait_newer(seen, Duration::from_secs(5)) else { continue };
        seen = n;
        let part = format!("Content-Type: image/jpeg\r\nContent-Length: {}\r\n\r\n", jpeg.len());
        if conn.write_all(part.as_bytes()).and_then(|_| conn.write_all(&jpeg)).and_then(|_| conn.write_all(b"\r\n--frame\r\n")).is_err() {
            return;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_length_is_read_from_what_ffmpeg_says() {
        let said = "Input #0, matroska,webm, from 'a.mkv':\n  Metadata:\n    ENCODER         : Lavf61\n  Duration: 00:03:07.52, start: 0.000000, bitrate: 1234 kb/s\n";
        assert!((parse_duration(said).unwrap() - 187.52).abs() < 1e-6);
        assert!((parse_duration("  Duration: 01:02:09.00, start").unwrap() - 3729.0).abs() < 1e-6);
        assert_eq!(parse_duration("  Duration: N/A, start: 0.000000"), None);
        assert_eq!(parse_duration("no such file"), None);
    }

    #[test]
    fn a_playback_reads_at_the_files_pace_and_a_still_is_one_frame() {
        let text = |a: Vec<std::ffi::OsString>| a.iter().map(|s| s.to_string_lossy().into_owned()).collect::<Vec<_>>().join(" ");
        let play = text(args(Path::new("a.mkv"), 12.5, false));
        assert!(play.contains("-re -ss 12.500 -i a.mkv") && play.contains("-fpsmax 30") && !play.contains("-frames:v"), "{play}");
        let one = text(args(Path::new("a.mkv"), 0.0, true));
        assert!(one.contains("-ss 0.000 -i a.mkv") && one.contains("-frames:v 1") && !one.contains("-re"), "{one}");
    }

    #[test]
    fn a_picture_from_a_producer_that_was_replaced_is_dropped() {
        let hub = Hub::new();
        let old = hub.claim();
        let new = hub.claim();
        hub.publish_from(old, vec![1]);
        assert!(hub.wait_newer(0, Duration::from_millis(10)).is_none());
        hub.publish_from(new, vec![2]);
        assert_eq!(*hub.wait_newer(0, Duration::from_millis(10)).unwrap().1, vec![2]);
    }

    /// Needs ffmpeg (`VIDS_FFMPEG_DIR`): a two-second test video is measured, one still is made, and a playback is read back through the server.
    #[test]
    #[ignore = "needs ffmpeg"]
    fn a_video_is_measured_shown_still_and_streamed_through_the_server() {
        let dir = std::env::var_os("VIDS_FFMPEG_DIR").map(std::path::PathBuf::from);
        let ff = crate::ffmpeg::locate(dir.as_deref()).expect("ffmpeg");
        let file = std::env::temp_dir().join(format!("vids-picture-{}.mkv", std::process::id()));
        let made = command(&ff).args(["-y", "-hide_banner", "-loglevel", "error", "-f", "lavfi", "-i", "testsrc=d=2:s=320x240:r=25", "-c:v", "mpeg4"]).arg(&file).status().unwrap();
        assert!(made.success());
        let long = duration(&ff, &file).expect("a length");
        assert!((long - 2.0).abs() < 0.2, "{long}");

        let channel = Channel::new();
        let url = channel.url.clone().expect("the server");
        still(&ff, &file, 1.0, &channel.hub).unwrap();
        let (n, jpeg) = channel.hub.wait_newer(0, Duration::from_millis(10)).expect("the still");
        assert!(jpeg.starts_with(&[0xFF, 0xD8]) && jpeg.ends_with(&[0xFF, 0xD9]));

        let stream = Stream::start(&ff, &file, 0.0, channel.hub.clone()).unwrap();
        // The window's side: ask the server for the stream and count the pictures that come.
        let rest = url.strip_prefix("http://").unwrap();
        let (host, path) = rest.split_once('/').unwrap();
        let mut conn = TcpStream::connect(host).unwrap();
        conn.set_read_timeout(Some(Duration::from_millis(300))).unwrap();
        write!(conn, "GET /{path} HTTP/1.1\r\nHost: {host}\r\n\r\n").unwrap();
        let (mut got, mut buf) = (Vec::new(), [0u8; 16384]);
        let until = std::time::Instant::now() + Duration::from_secs(4);
        while std::time::Instant::now() < until && !(stream.finished() && got.len() > 1000) {
            if let Ok(read) = conn.read(&mut buf) {
                got.extend_from_slice(&buf[..read]);
            }
        }
        let text = String::from_utf8_lossy(&got);
        assert!(text.starts_with("HTTP/1.1 200 OK") && text.contains("multipart/x-mixed-replace"));
        let parts = got.windows(7).filter(|w| w == b"--frame").count();
        assert!(parts > 20, "about 50 pictures were expected, {parts} came");
        assert!(stream.finished(), "the end of the file is noticed");
        assert!(channel.hub.wait_newer(n, Duration::from_millis(10)).is_some());
        drop(stream);
        let _ = std::fs::remove_file(&file);
    }
}
