//! The preview: the file as a picture, with the chosen effects on it, shown in a window of its own.
//!
//! ONE ffmpeg run per picture or per playback, built from the same plan as a conversion (`effects::try_plan`), so
//! what the preview shows is what the effects will do. Every picture - a still or a frame of a running stream - is
//! a JPEG published to the [`Hub`] of its pane ([`Channel`]); a small server on this machine hands the hub's latest picture to the window as a
//! motion-JPEG stream, which a web view draws in a plain image element with no script running per picture. Every
//! format ffmpeg reads shows, whatever the web view could play.

use crate::{
    effects::{self, EffectsEntry, Filter, Look},
    sound_plan::SoundPlan,
    ffmpeg::command,
    sound::Sound,
};
use std::{
    collections::HashMap,
    ffi::OsString,
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    path::{Path, PathBuf},
    process::{Child, Stdio},
    sync::{
        Arc, Condvar, Mutex, OnceLock,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    thread,
    time::Duration,
};

/// The latest picture anyone made, and the number of it (it only grows), with a way to wait for the next one.
pub struct Hub {
    latest: Mutex<(u64, Option<Arc<Vec<u8>>>)>,
    changed: Condvar,
    /// The producer whose pictures count: each stream and each still claims the hub when it starts, and a picture from a producer
    /// that is no longer the latest one is dropped (a stopped stream's last pictures, a still that took longer than the next playback).
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

    pub fn publish(&self, jpeg: Vec<u8>) {
        let mut g = self.latest.lock().unwrap_or_else(|e| e.into_inner());
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

/// What one picture or one playback is made from.
pub struct Spec<'a> {
    pub input: &'a Path,
    /// Where in the file it starts, in seconds.
    pub start_s: f64,
    /// How much to play (a stream); None plays to the end.
    pub length_s: Option<f64>,
    /// The size cap and the effects, exactly as a conversion would apply them.
    pub look: &'a Look<'a>,
    /// The picture is no wider than this (never enlarged).
    pub width: u32,
    /// The most pictures a second a stream makes.
    pub fps: f64,
    /// What the sound's effects make of the file once measured (`queue::plan_for`): the sound plays through its chain, from the start of the part
    /// they keep, and the picture is paced at the same speed. `None` = not measured: the effects' own chain stands.
    pub sound: Option<&'a SoundPlan>,
}

/// The ffmpeg arguments for a picture (`one_frame`) or a playback at the file's own pace.
// `-strict unofficial`: an effect can leave the picture as limited-range YUV (repeatfields does), which the mjpeg encoder refuses otherwise.
pub fn args(spec: &Spec, one_frame: bool) -> Result<Vec<OsString>, String> {
    build(spec, one_frame, "", &["-f", "image2pipe", "-c:v", "mjpeg", "-strict", "unofficial", "-q:v", "5", "pipe:1"])
}

/// The arguments with `after` appended to the picture's own chain (what the pane shows, scaled) and `output` as the way out.
fn build(spec: &Spec, one_frame: bool, after: &str, output: &[&str]) -> Result<Vec<OsString>, String> {
    let plan = effects::try_plan(spec.look, spec.input)?;
    let mut a: Vec<OsString> = Vec::new();
    let mut push = |items: &[&str]| a.extend(items.iter().map(OsString::from));
    push(&["-hide_banner", "-nostdin", "-loglevel", "error"]);
    // `-re` reads at the file's own pace, which is what makes a stream play in time.
    // (a tempo effect plays the sound faster or slower, so the picture is read at that speed too)
    if !one_frame {
        match spec.sound.map(|p| p.ratio).filter(|r| (r - 1.0).abs() > 1e-9) {
            Some(ratio) => push(&["-readrate", &format!("{ratio:.6}")]),
            None => push(&["-re"]),
        }
    }
    let start = format!("{:.3}", spec.start_s.max(0.0));
    // As in a conversion: burned subtitles are read from the file on its own timeline, so the start goes after the input.
    if !plan.burns {
        push(&["-ss", &start]);
    }
    a.push("-i".into());
    a.push(spec.input.into());
    for extra in &plan.inputs {
        a.push("-i".into());
        a.push(extra.into());
    }
    let mut push = |items: &[&str]| a.extend(items.iter().map(OsString::from));
    if plan.burns {
        push(&["-ss", &start]);
    }
    if let Some(len) = spec.length_s {
        push(&["-t", &format!("{len:.3}")]);
    }
    let scale = format!("scale='min({},iw)':-2{after}", spec.width);
    match &plan.filter {
        Filter::None => push(&["-vf", &scale]),
        Filter::Simple(chain) => push(&["-vf", &format!("{chain},{scale}")]),
        Filter::Complex(graph) => push(&["-filter_complex", &format!("{graph};[outv]{scale}[pv]"), "-map", "[pv]"]),
    }
    push(&["-an", "-sn"]);
    if one_frame {
        push(&["-frames:v", "1"]);
    } else {
        push(&["-fpsmax", &format!("{}", spec.fps.clamp(1.0, 60.0))]);
    }
    push(output);
    Ok(a)
}

/// The ffmpeg arguments for the sound of a playback: the same moment of the same file, the effects' audio chain on it,
/// as raw 32-bit samples in the output device's own channel count and rate.
pub fn sound_args(spec: &Spec, channels: u16, rate: u32) -> Result<Vec<OsString>, String> {
    let plan = effects::try_plan(spec.look, spec.input)?;
    // Measured: the sound is read from the start of the part the effects keep and the moment is counted on what they make (the fades and
    // the speed are placed on it), so the start is an output-side seek. Not measured: the moment is the file's own.
    let planned = spec.sound.filter(|p| p.chain.is_some());
    let ratio = planned.map_or(1.0, |p| p.ratio.max(1e-9));
    let base = planned.map_or(0.0, |p| p.trim(spec.look.trim).start_s.unwrap_or(0.0));
    let mut a: Vec<OsString> = Vec::new();
    let mut push = |items: &[&str]| a.extend(items.iter().map(OsString::from));
    push(&["-hide_banner", "-nostdin", "-loglevel", "error", "-ss", &format!("{:.3}", if planned.is_some() { base } else { spec.start_s.max(0.0) })]);
    a.push("-i".into());
    a.push(spec.input.into());
    let mut push = |items: &[&str]| a.extend(items.iter().map(OsString::from));
    if let Some(len) = spec.length_s {
        push(&["-t", &format!("{:.3}", len / ratio)]);
    }
    push(&["-vn", "-sn"]);
    if let Some(chain) = planned.and_then(|p| p.chain.as_deref()).or(plan.audio.as_deref()).filter(|c| !c.is_empty()) {
        push(&["-af", chain]);
    }
    if planned.is_some() {
        push(&["-ss", &format!("{:.3}", ((spec.start_s - base) / ratio).max(0.0))]);
    }
    push(&["-ac", &channels.to_string(), "-ar", &rate.to_string(), "-f", "f32le", "pipe:1"]);
    Ok(a)
}

/// What went wrong, from what ffmpeg said: the line that names the cause, not the closing lines ("Error opening output files") that every failure ends
/// with. A file without a subtitle track under a burn-in is said in plain words.
fn reason(text: &str) -> String {
    if text.contains("Unable to locate subtitle stream") {
        return "This file has no subtitle track to burn in".to_owned();
    }
    let closing = |l: &str| l.starts_with("Error opening output") || l.starts_with("Error initializing filters") || l.contains("received no packets");
    let line = text.lines().map(str::trim).find(|l| !l.is_empty() && !closing(l)).unwrap_or("no picture at that moment");
    // the filter's own tag ("[Parsed_drawtext_2 @ 000001bd6970df80]") says nothing to a reader
    match line.strip_prefix('[').and_then(|r| r.split_once("] ")) {
        Some((_, rest)) => rest.to_owned(),
        None => line.to_owned(),
    }
}

/// The colour of a patch of the picture the pane shows: the average of about 5 by 5 pixels around the point (`at` = the fractions of the
/// picture's width and height), as red, green and blue from 0 to 1. Made from the same plan as a still, so the effects in `spec` are what is on it.
pub fn sample(ffmpeg: &Path, spec: &Spec, at: (f64, f64)) -> Result<[f64; 3], String> {
    let (fx, fy) = (at.0.clamp(0.0, 1.0), at.1.clamp(0.0, 1.0));
    let after = format!(",crop=5:5:'min(iw-5,max(0,iw*{fx:.5}-2))':'min(ih-5,max(0,ih*{fy:.5}-2))',scale=1:1:flags=area,format=rgb24");
    let argv = build(spec, true, &after, &["-f", "rawvideo", "-pix_fmt", "rgb24", "pipe:1"])?;
    let out = command(ffmpeg).args(&argv).stdin(Stdio::null()).output().map_err(|e| format!("cannot run ffmpeg: {e}"))?;
    if out.stdout.len() < 3 {
        let text = String::from_utf8_lossy(&out.stderr);
        let why = reason(&text);
        crate::log::error("colour pick", &format!("{why}
command: {}", crate::log::command_line(ffmpeg, &argv)));
        return Err(why);
    }
    Ok([f64::from(out.stdout[0]) / 255.0, f64::from(out.stdout[1]) / 255.0, f64::from(out.stdout[2]) / 255.0])
}

/// Why a picture could not be made, and the filter ffmpeg blamed when it named one.
struct Failure {
    why: String,
    filter: Option<String>,
}

/// The filter ffmpeg names in its complaint (`[Parsed_drawvg_1 @ 0000` is `drawvg`).
fn blamed(text: &str) -> Option<String> {
    let tag = text.split("[Parsed_").nth(1)?.split(" @").next()?;
    Some(tag.rsplit_once('_')?.0.to_owned())
}

/// A picture and what had to be left out of it.
pub struct Picture {
    pub jpeg: Vec<u8>,
    /// One line per effect left out, each naming the effect and why: an effect ffmpeg refused (a setting that cannot work on one picture, a
    /// script not given yet) must not take the whole picture with it.
    pub skipped: Vec<String>,
}

/// One picture, made and published; an effect that ffmpeg refuses is left out of the picture and said in `skipped`, so the rest still shows.
pub fn still_noting(ffmpeg: &Path, spec: &Spec, hub: &Hub) -> Result<Picture, String> {
    settle(ffmpeg, spec, hub).map(|(jpeg, _, skipped)| Picture { jpeg, skipped })
}

/// The effects a playback can run: those of `spec` without the ones ffmpeg refuses on a picture at its start (a playback made with one would
/// end at once), and a line for each one left out.
pub fn playable(ffmpeg: &Path, spec: &Spec, hub: &Hub) -> Result<(EffectsEntry, Vec<String>), String> {
    settle(ffmpeg, spec, hub).map(|(_, entry, skipped)| (entry, skipped))
}

fn settle(ffmpeg: &Path, spec: &Spec, hub: &Hub) -> Result<(Vec<u8>, EffectsEntry, Vec<String>), String> {
    let mut entry = spec.look.effects.clone();
    let mut skipped: Vec<String> = Vec::new();
    loop {
        let look = Look { size: spec.look.size, effects: &entry, duration_s: spec.look.duration_s, trim: spec.look.trim };
        let attempt = Spec { look: &look, ..*spec };
        match still_try(ffmpeg, &attempt, hub) {
            Ok(jpeg) => return Ok((jpeg, entry, skipped)),
            Err(f) => {
                let culprit = f.filter.as_deref().and_then(|name| entry.items.iter().position(|i| i.uses_filter(name)));
                match culprit {
                    Some(at) if skipped.len() < 4 => {
                        skipped.push(format!("{}: {}", entry.items[at].title(), f.why));
                        entry.items.remove(at);
                    }
                    _ => return Err(f.why),
                }
            }
        }
    }
}

/// One picture, made and published; the bytes come back too.
pub fn still(ffmpeg: &Path, spec: &Spec, hub: &Hub) -> Result<Vec<u8>, String> {
    still_try(ffmpeg, spec, hub).map_err(|f| f.why)
}

fn still_try(ffmpeg: &Path, spec: &Spec, hub: &Hub) -> Result<Vec<u8>, Failure> {
    // A file's length is often a little longer than its last picture (the sound runs on), so a moment near the end
    // can have no picture: step back until one exists.
    let producer = hub.claim();
    let mut start = spec.start_s;
    let mut back = 0.25;
    loop {
        let argv = args(&Spec { start_s: start, ..*spec }, true).map_err(|why| Failure { why, filter: None })?;
        let out = command(ffmpeg).args(&argv).stdin(Stdio::null()).output().map_err(|e| {
            let why = format!("cannot run ffmpeg: {e}");
            crate::log::error("preview picture", &format!("{why}
command: {}", crate::log::command_line(ffmpeg, &argv)));
            Failure { why, filter: None }
        })?;
        if !out.stdout.is_empty() {
            hub.publish_from(producer, out.stdout.clone());
            return Ok(out.stdout);
        }
        if start <= 0.0 || back > 8.0 {
            let text = String::from_utf8_lossy(&out.stderr);
            let why = reason(&text);
            crate::log::error(
                "preview picture",
                &format!(
                    "{why}
asked for {:.3} s, tried back to {start:.3} s
input: {}
command: {}
what ffmpeg said:
{}",
                    spec.start_s,
                    spec.input.display(),
                    crate::log::command_line(ffmpeg, &argv),
                    text.trim()
                ),
            );
            return Err(Failure { filter: blamed(&text), why });
        }
        start = (start - back).max(0.0);
        back *= 2.0;
    }
}

/// A playback: ffmpeg running at the file's own pace, its pictures published as they come. Stopped when dropped.
pub struct Stream {
    child: Child,
    frames: Arc<AtomicU64>,
    done: Arc<AtomicBool>,
    /// None when the file has no sound or the machine has no output; the picture plays on without it.
    sound: Option<Sound>,
}

impl Stream {
    pub fn start(ffmpeg: &Path, spec: &Spec, hub: Arc<Hub>, volume: f32) -> Result<Stream, String> {
        let mut child = command(ffmpeg)
            .args(args(spec, false)?)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| {
                let why = format!("cannot run ffmpeg: {e}");
                crate::log::error("preview playback", &format!("{why}
input: {}", spec.input.display()));
                why
            })?;
        let mut out = child.stdout.take().ok_or("no output pipe")?;
        let (frames, done) = (Arc::new(AtomicU64::new(0)), Arc::new(AtomicBool::new(false)));
        let (n, end) = (frames.clone(), done.clone());
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
                    n.fetch_add(1, Ordering::Relaxed);
                    scanned = 0;
                }
                // The last byte may be the FF of an FF D9 split across two reads.
                scanned = buf.len().saturating_sub(1);
            }
            end.store(true, Ordering::Relaxed);
        });
        let sound = Sound::start(ffmpeg, spec, volume).ok();
        Ok(Stream { child, frames, done, sound })
    }

    /// Pictures made so far.
    pub fn frames(&self) -> u64 {
        self.frames.load(Ordering::Relaxed)
    }

    /// ffmpeg has stopped (the end of the file or of the part asked for) and everything it made was published.
    pub fn finished(&self) -> bool {
        self.done.load(Ordering::Relaxed)
    }

    /// The level of the sound from now on (0 is silent).
    pub fn set_volume(&self, volume: f32) {
        if let Some(sound) = &self.sound {
            sound.set_volume(volume);
        }
    }

    pub fn stop(&mut self) {
        self.sound = None;
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl Drop for Stream {
    fn drop(&mut self) {
        self.stop();
    }
}

/// Where the pictures of one pane go and where its window's image reads them: its own hub, so two panes (the work
/// view's and the preview window's) never show each other's pictures.
pub struct Channel {
    pub hub: Arc<Hub>,
    /// The address of the motion-JPEG stream of this hub, or why the server could not start.
    pub url: Result<String, String>,
}

struct Server {
    base: String,
    /// The path each hub is served under.
    hubs: Mutex<HashMap<String, Arc<Hub>>>,
    /// The files handed out under a name of their own (the pictures of the timeline).
    files: Mutex<HashMap<String, PathBuf>>,
    next: AtomicU64,
    token: String,
}

static SERVER: OnceLock<Result<Arc<Server>, String>> = OnceLock::new();

/// Started once, on this machine only, on a free port; the paths carry a name nobody guesses by chance.
fn server() -> Result<Arc<Server>, String> {
    SERVER
        .get_or_init(|| {
            let listener = TcpListener::bind("127.0.0.1:0").map_err(|e| format!("cannot start the preview: {e}"))?;
            let port = listener.local_addr().map_err(|e| e.to_string())?.port();
            let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(1);
            let token = format!("{:x}{:x}", nanos, std::process::id());
            let me = Arc::new(Server { base: format!("http://127.0.0.1:{port}"), hubs: Mutex::new(HashMap::new()), files: Mutex::new(HashMap::new()), next: AtomicU64::new(1), token });
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

/// The address a picture file is served at by the preview server (the pictures of the timeline); None when the server could not start.
pub fn serve_file(path: &Path) -> Option<String> {
    let s = server().ok()?;
    let name = format!("/{}/{}.jpg", s.token, s.next.fetch_add(1, Ordering::Relaxed));
    s.files.lock().unwrap_or_else(|e| e.into_inner()).insert(name.clone(), path.to_path_buf());
    Some(format!("{}{name}", s.base))
}

const WAVE_RATE: u32 = 4000;

/// The loudness of the first sound track along the file: `buckets` peaks from 0 to 1 (the loudest is 1), from one ffmpeg decode to mono samples.
/// Err when the file has no sound or no length.
pub fn waveform(ffmpeg: &Path, input: &Path, duration_s: f64, buckets: usize) -> Result<Vec<f32>, String> {
    if duration_s <= 0.0 || buckets == 0 {
        return Err("the file has no length".to_owned());
    }
    let mut child = command(ffmpeg)
        .args(["-hide_banner", "-nostdin", "-loglevel", "error", "-i"])
        .arg(input)
        .args(["-map", "0:a:0", "-vn", "-sn", "-ac", "1", "-ar", &WAVE_RATE.to_string(), "-f", "f32le", "-"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("cannot run ffmpeg: {e}"))?;
    let mut out = child.stdout.take().ok_or("no output pipe")?;
    let total = duration_s * f64::from(WAVE_RATE);
    let mut peaks = vec![0.0_f32; buckets];
    let (mut buf, mut loose, mut at) = (vec![0u8; 64 * 1024], Vec::<u8>::new(), 0_u64);
    while let Ok(n) = out.read(&mut buf) {
        if n == 0 {
            break;
        }
        loose.extend_from_slice(&buf[..n]);
        let whole = loose.len() / 4 * 4;
        for b in loose[..whole].chunks_exact(4) {
            let v = f32::from_le_bytes([b[0], b[1], b[2], b[3]]).abs();
            let i = ((at as f64 / total * buckets as f64) as usize).min(buckets - 1);
            if v > peaks[i] {
                peaks[i] = v;
            }
            at += 1;
        }
        loose.drain(..whole);
    }
    let _ = child.wait();
    if at == 0 {
        return Err("the file has no sound to show".to_owned());
    }
    let top = peaks.iter().copied().fold(0.0_f32, f32::max);
    Ok(peaks.into_iter().map(|p| if top > 0.0 { p / top } else { 0.0 }).collect())
}

/// The pictures of the timeline: `count` small frames spread over the file, each made by a seek (a keyframe, so it is quick on any length)
/// and kept in `dir` under a name that holds the file, its size and its date; a picture already there is reused. Four at a time.
/// A frame that cannot be made is None (its place is a gap in the timeline).
pub fn filmstrip(ffmpeg: &Path, input: &Path, duration_s: f64, count: usize, height: u32, dir: &Path) -> Vec<Option<PathBuf>> {
    if duration_s <= 0.0 || count == 0 || std::fs::create_dir_all(dir).is_err() {
        return Vec::new();
    }
    let meta = std::fs::metadata(input).ok();
    let key = {
        use sha2::{Digest, Sha256};
        let mut h = Sha256::new();
        h.update(input.to_string_lossy().as_bytes());
        h.update(meta.as_ref().map_or(0, |m| m.len()).to_le_bytes());
        h.update(meta.and_then(|m| m.modified().ok()).and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map_or(0, |d| d.as_secs()).to_le_bytes());
        h.update(count.to_le_bytes());
        h.update(height.to_le_bytes());
        h.finalize().iter().take(8).map(|b| format!("{b:02x}")).collect::<String>()
    };
    let next = Arc::new(AtomicU64::new(0));
    let out = Arc::new(Mutex::new(vec![None; count]));
    let workers: Vec<_> = (0..4)
        .map(|_| {
            let (next, out) = (next.clone(), out.clone());
            let (ffmpeg, input, dir, key) = (ffmpeg.to_path_buf(), input.to_path_buf(), dir.to_path_buf(), key.clone());
            thread::spawn(move || {
                loop {
                    let i = next.fetch_add(1, Ordering::Relaxed) as usize;
                    if i >= count {
                        return;
                    }
                    let file = dir.join(format!("{key}-{i}.jpg"));
                    if !file.exists() {
                        // the middle of each slot, so the first picture is not the black frame many files start with
                        let at = duration_s * (i as f64 + 0.5) / count as f64;
                        let done = command(&ffmpeg)
                            .args(["-hide_banner", "-nostdin", "-loglevel", "error", "-y", "-ss", &format!("{at:.3}"), "-i"])
                            .arg(&input)
                            .args(["-frames:v", "1", "-an", "-sn", "-vf", &format!("scale=-2:{height}"), "-q:v", "5"])
                            .arg(&file)
                            .stdin(Stdio::null())
                            .stdout(Stdio::null())
                            .stderr(Stdio::null())
                            .status()
                            .map(|s| s.success())
                            .unwrap_or(false);
                        if !done {
                            let _ = std::fs::remove_file(&file);
                            continue;
                        }
                    }
                    out.lock().unwrap_or_else(|e| e.into_inner())[i] = Some(file);
                }
            })
        })
        .collect();
    for w in workers {
        let _ = w.join();
    }
    let made = out.lock().unwrap_or_else(|e| e.into_inner()).clone();
    made
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
    let file = server.files.lock().unwrap_or_else(|e| e.into_inner()).get(&asked).cloned();
    if let Some(file) = file {
        let body = std::fs::read(&file).unwrap_or_default();
        let intro = format!("HTTP/1.1 200 OK\r\nContent-Type: image/jpeg\r\nContent-Length: {}\r\nCache-Control: max-age=3600\r\nConnection: close\r\n\r\n", body.len());
        let _ = conn.write_all(intro.as_bytes()).and_then(|_| conn.write_all(&body));
        return;
    }
    let hub = server.hubs.lock().unwrap_or_else(|e| e.into_inner()).get(&asked).cloned();
    let Some(hub) = hub else {
        let _ = conn.write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
        return;
    };
    let intro = "HTTP/1.1 200 OK\r\nContent-Type: multipart/x-mixed-replace; boundary=frame\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n";
    if conn.write_all(intro.as_bytes()).is_err() {
        return;
    }
    // Each picture is followed by the boundary that opens the next part: the web view shows a part only once it sees the boundary that ends it,
    // so a boundary written before the next picture would keep the latest one hidden (a still would show the one before it).
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

/// The width and height of a JPEG, read from its header (the picture the pane shows is never decoded here); None when it holds no frame header.
pub fn jpeg_size(jpeg: &[u8]) -> Option<(u32, u32)> {
    let mut at = 2;
    while at + 9 < jpeg.len() {
        if jpeg[at] != 0xFF {
            at += 1;
            continue;
        }
        let marker = jpeg[at + 1];
        // a start-of-frame marker (not the table markers that sit among them) holds the height and the width
        if (0xC0..=0xCF).contains(&marker) && !matches!(marker, 0xC4 | 0xC8 | 0xCC) {
            let h = u32::from(u16::from_be_bytes([jpeg[at + 5], jpeg[at + 6]]));
            let w = u32::from(u16::from_be_bytes([jpeg[at + 7], jpeg[at + 8]]));
            return Some((w, h));
        }
        if marker == 0xFF || marker == 0x00 || (0xD0..=0xD9).contains(&marker) {
            at += if marker == 0xFF { 1 } else { 2 };
            continue;
        }
        at += 2 + usize::from(u16::from_be_bytes([jpeg[at + 2], jpeg[at + 3]]));
    }
    None
}

/// `0:01:05.250`-style text a cut can be given in (`Trim::parse` reads it): hours only when there are some.
pub fn clock(seconds: f64) -> String {
    let ms = (seconds.max(0.0) * 1000.0).round() as u64;
    let (h, m, s, milli) = (ms / 3_600_000, ms / 60_000 % 60, ms / 1000 % 60, ms % 1000);
    if h > 0 { format!("{h}:{m:02}:{s:02}.{milli:03}") } else { format!("{m}:{s:02}.{milli:03}") }
}

/// The timecode a person reads on a player, `HH:MM:SS:FF`, for a time and the file's frame rate.
pub fn timecode(seconds: f64, fps: f64) -> String {
    let fps = if fps > 0.0 { fps } else { 25.0 };
    let frames_total = (seconds.max(0.0) * fps).round() as u64;
    let per_s = fps.round().max(1.0) as u64;
    let (s_total, f) = (frames_total / per_s, frames_total % per_s);
    format!("{:02}:{:02}:{:02}:{:02}", s_total / 3600, s_total / 60 % 60, s_total % 60, f)
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_filter_ffmpeg_blames_is_read_from_its_tag() {
        assert_eq!(super::blamed("[Parsed_shuffleframes_0 @ 000002cc9dfece80] Index 5 out of range").as_deref(), Some("shuffleframes"));
        assert_eq!(super::blamed("[Parsed_drawvg_1 @ 0000] Either source").as_deref(), Some("drawvg"));
        assert_eq!(super::blamed("Error opening output files"), None);
    }

    #[test]
    fn the_reason_is_the_line_that_names_the_cause_in_plain_words() {
        let burn = "[Parsed_subtitles_0 @ 0000024f46538d40] Unable to locate subtitle stream in C:/a.mp4
[AVFilterGraph @ 1] Error initializing filters
Error opening output file pipe:1.
Error opening output files: Operation not permitted";
        assert_eq!(super::reason(burn), "This file has no subtitle track to burn in");
        let text = "[Parsed_drawtext_2 @ 000001bd] Either text must be provided
[AVFilterGraph @ 2] Error initializing filters
Error opening output files: Invalid argument";
        assert_eq!(super::reason(text), "Either text must be provided");
        assert_eq!(super::reason(""), "no picture at that moment");
    }

    use super::*;
    use crate::{effects::EffectsEntry, ffmpeg, size::Size, trim::{Trim, parse_time}};
    use std::path::PathBuf;


    fn ffmpeg_for_tests() -> Option<PathBuf> {
        let name = ffmpeg::executable_name();
        std::env::split_paths(&std::env::var_os("PATH")?).map(|d| d.join(name)).find(|p| p.is_file())
    }

    fn clip(ff: &Path, tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("vids-preview-{tag}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let clip = dir.join("clip.mp4");
        assert!(ffmpeg::command(ff)
            .args(["-y", "-hide_banner", "-loglevel", "error", "-f", "lavfi", "-i", "testsrc=size=320x240:rate=25:duration=2", "-c:v", "mpeg4"])
            .arg(&clip)
            .status()
            .unwrap()
            .success());
        clip
    }

    fn text(args: &[OsString]) -> String {
        args.iter().map(|a| a.to_string_lossy().into_owned()).collect::<Vec<_>>().join(" ")
    }

    #[test]
    fn the_size_of_a_picture_is_read_from_its_header() {
        let Some(ff) = ffmpeg_for_tests() else { return };
        let clip = clip(&ff, "size");
        let (size, effects) = (Size::original(), EffectsEntry::none());
        let look = Look { size: &size, effects: &effects, duration_s: 2.0, trim: Trim::none() };
        let hub = Hub::new();
        let jpeg = still(&ff, &Spec { input: &clip, start_s: 0.5, length_s: None, look: &look, width: 160, fps: 25.0, sound: None }, &hub).unwrap();
        // the picture is never wider than 160 and keeps the shape of 320 x 240
        assert_eq!(jpeg_size(&jpeg), Some((160, 120)));
        assert_eq!(jpeg_size(b"not a picture"), None);
    }

    #[test]
    fn the_timeline_pictures_are_made_once_and_served_by_the_preview_server() {
        let Some(ff) = ffmpeg_for_tests() else { return };
        let clip = clip(&ff, "film");
        let dir = std::env::temp_dir().join(format!("vids-film-{}", std::process::id()));
        let first = filmstrip(&ff, &clip, 2.0, 6, 60, &dir);
        assert_eq!(first.len(), 6);
        assert!(first.iter().all(|f| f.as_ref().is_some_and(|p| p.exists())), "{first:?}");
        // a second call finds them already there
        assert_eq!(filmstrip(&ff, &clip, 2.0, 6, 60, &dir), first);
        let url = serve_file(first[2].as_ref().unwrap()).expect("the server starts");
        let rest = url.strip_prefix("http://").unwrap();
        let (host, path) = rest.split_at(rest.find('/').unwrap());
        let mut conn = TcpStream::connect(host).unwrap();
        conn.write_all(format!("GET {path} HTTP/1.1\r\nHost: {host}\r\n\r\n").as_bytes()).unwrap();
        conn.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
        let mut got = Vec::new();
        conn.read_to_end(&mut got).unwrap();
        let at = got.windows(4).position(|w| w == b"\r\n\r\n").unwrap() + 4;
        assert!(String::from_utf8_lossy(&got[..at]).contains("image/jpeg"));
        assert_eq!(jpeg_size(&got[at..]).map(|(_, h)| h), Some(60));
    }

    #[test]
    fn the_waveform_is_the_loudness_along_the_file_and_a_silent_file_has_none() {
        let Some(ff) = ffmpeg_for_tests() else { return };
        let dir = std::env::temp_dir().join(format!("vids-wave-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let tone = dir.join("tone.mp4");
        // one second of silence, then one of a tone
        assert!(ffmpeg::command(&ff)
            .args(["-y", "-hide_banner", "-loglevel", "error", "-f", "lavfi", "-i", "testsrc=size=64x64:rate=10:duration=2", "-f", "lavfi", "-i", "aevalsrc='if(gt(t,1),0.5*sin(2*PI*440*t),0)':d=2:s=8000", "-c:v", "mpeg4", "-c:a", "aac", "-shortest"])
            .arg(&tone)
            .status()
            .unwrap()
            .success());
        let peaks = waveform(&ff, &tone, 2.0, 10).unwrap();
        assert_eq!(peaks.len(), 10);
        assert!(peaks[2] < 0.1 && peaks[8] > 0.8, "{peaks:?}");
        assert!(waveform(&ff, &clip(&ff, "wave"), 2.0, 10).is_err());
    }

    #[test]
    fn a_playback_reads_at_the_files_pace_and_a_picture_does_not() {
        let (size, effects) = (Size::original(), EffectsEntry::none());
        let look = Look { size: &size, effects: &effects, duration_s: 10.0, trim: Trim::none() };
        let spec = Spec { input: Path::new("in.mkv"), start_s: 12.5, length_s: None, look: &look, width: 640, fps: 30.0, sound: None };
        let live = text(&args(&spec, false).unwrap());
        assert!(live.contains("-re -ss 12.500 -i in.mkv"), "{live}");
        assert!(live.contains("-fpsmax 30"), "{live}");
        assert!(live.contains("-vf scale='min(640,iw)':-2"), "{live}");
        let one = text(&args(&spec, true).unwrap());
        assert!(!one.contains("-re"), "{one}");
        assert!(one.contains("-frames:v 1"), "{one}");
    }

    #[test]
    fn the_sound_is_the_same_moment_decoded_for_the_output_device() {
        let (size, effects) = (Size::original(), EffectsEntry::none());
        let look = Look { size: &size, effects: &effects, duration_s: 10.0, trim: Trim::none() };
        let spec = Spec { input: Path::new("in.mkv"), start_s: 12.5, length_s: Some(4.0), look: &look, width: 640, fps: 30.0, sound: None };
        let a = text(&sound_args(&spec, 2, 48000).unwrap());
        assert!(a.contains("-ss 12.500 -i in.mkv -t 4.000 -vn -sn"), "{a}");
        assert!(a.ends_with("-ac 2 -ar 48000 -f f32le pipe:1"), "{a}");
    }

    #[test]
    fn the_sound_carries_the_effects_audio_chain() {
        crate::effects::set_catalog_text(include_str!("../testdata/effects.json")).ok();
        let size = Size::original();
        let effects = EffectsEntry { items: vec![crate::effects::Item::new("play-backwards")], ..EffectsEntry::new("t") };
        let look = Look { size: &size, effects: &effects, duration_s: 10.0, trim: Trim::none() };
        let spec = Spec { input: Path::new("in.mkv"), start_s: 0.0, length_s: None, look: &look, width: 640, fps: 30.0, sound: None };
        let a = text(&sound_args(&spec, 2, 44100).unwrap());
        assert!(a.contains("-af areverse"), "{a}");
    }

    #[test]
    fn the_chosen_effects_are_in_the_preview_exactly_as_in_a_conversion() {
        crate::effects::set_catalog_text(include_str!("../testdata/effects.json")).ok();
        let size = Size::original();
        let effects = EffectsEntry { items: vec![crate::effects::Item::with("turn", &[("way", "cw")])], ..EffectsEntry::new("t") };
        let look = Look { size: &size, effects: &effects, duration_s: 10.0, trim: Trim::none() };
        let spec = Spec { input: Path::new("in.mkv"), start_s: 0.0, length_s: Some(3.0), look: &look, width: 640, fps: 25.0, sound: None };
        let with = text(&args(&spec, false).unwrap());
        let plain = Look { effects: &EffectsEntry::none(), ..look };
        let without = text(&args(&Spec { look: &plain, ..spec }, false).unwrap());
        assert!(with.len() > without.len(), "{with} / {without}");
        assert!(with.contains("-t 3.000"), "{with}");
        assert!(with.contains("scale='min(640,iw)':-2"), "{with}");
    }

    #[test]
    fn a_still_is_a_jpeg_at_the_asked_moment_and_a_missing_file_says_why() {
        let Some(ff) = ffmpeg_for_tests() else { return };
        let file = clip(&ff, "still");
        let (size, effects) = (Size::original(), EffectsEntry::none());
        let look = Look { size: &size, effects: &effects, duration_s: 2.0, trim: Trim::none() };
        let hub = Hub::new();
        let at = |t: f64, input: &Path| still(&ff, &Spec { input, start_s: t, length_s: None, look: &look, width: 160, fps: 25.0, sound: None }, &hub);
        let (first, later) = (at(0.0, &file).unwrap(), at(1.5, &file).unwrap());
        assert_eq!(&first[..2], &[0xFF, 0xD8], "a JPEG starts with FFD8");
        assert_ne!(first, later, "another moment is another picture");
        assert!(at(0.0, &file.with_file_name("missing.mp4")).is_err());
        let _ = std::fs::remove_dir_all(file.parent().unwrap());
    }

    #[test]
    fn a_playback_publishes_every_picture_and_the_window_reads_them_from_the_server() {
        let Some(ff) = ffmpeg_for_tests() else { return };
        let file = clip(&ff, "stream");
        let (size, effects) = (Size::original(), EffectsEntry::none());
        let look = Look { size: &size, effects: &effects, duration_s: 2.0, trim: Trim::none() };

        // The window connects first, as it does when it opens.
        let (channel, other_pane) = (Channel::new(), Channel::new());
        let url = channel.url.clone().unwrap();
        let (hostport, path) = url.trim_start_matches("http://").split_once('/').map(|(h, p)| (h.to_owned(), format!("/{p}"))).unwrap();
        let mut conn = TcpStream::connect(&hostport).unwrap();
        conn.set_read_timeout(Some(Duration::from_secs(10))).unwrap();
        conn.write_all(format!("GET {path} HTTP/1.1\r\nHost: {hostport}\r\n\r\n").as_bytes()).unwrap();

        let stream = Stream::start(&ff, &Spec { input: &file, start_s: 0.0, length_s: Some(1.0), look: &look, width: 160, fps: 25.0, sound: None }, channel.hub.clone(), 1.0).unwrap();
        conn.set_read_timeout(Some(Duration::from_secs(5))).unwrap(); let mut got = Vec::new();
        let mut chunk = [0u8; 8192];
        while got.windows(10).filter(|w| *w == b"image/jpeg").count() < 3 {
            let n = conn.read(&mut chunk).unwrap();
            assert!(n > 0, "the server closed before three pictures came");
            got.extend_from_slice(&chunk[..n]);
        }
        let head = String::from_utf8_lossy(&got[..120.min(got.len())]).into_owned();
        assert!(head.contains("200 OK") && head.contains("multipart/x-mixed-replace"), "{head}");
        let mut waited = 0;
        while !stream.finished() && waited < 100 {
            thread::sleep(Duration::from_millis(100));
            waited += 1;
        }
        assert!(stream.frames() >= 20, "one second at 25 fps made {} pictures", stream.frames());
        drop(stream);
        // Another pane has its own hub: nothing of this playback reached it.
        assert!(other_pane.hub.wait_newer(0, Duration::from_millis(100)).is_none());

        // Another name is not served.
        let mut other = TcpStream::connect(&hostport).unwrap();
        other.write_all(b"GET /nope.mjpeg HTTP/1.1\r\n\r\n").unwrap();
        let mut reply = String::new();
        other.read_to_string(&mut reply).unwrap();
        assert!(reply.contains("404"), "{reply}");
        let _ = std::fs::remove_dir_all(file.parent().unwrap());
    }

    #[test]
    fn a_picture_is_closed_by_a_boundary_at_once_so_a_still_is_not_held_back() {
        let channel = Channel::new();
        let url = channel.url.clone().unwrap();
        let (hostport, path) = url.trim_start_matches("http://").split_once('/').map(|(h, p)| (h.to_owned(), format!("/{p}"))).unwrap();
        let mut conn = TcpStream::connect(&hostport).unwrap();
        conn.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
        conn.write_all(format!("GET {path} HTTP/1.1\r\nHost: {hostport}\r\n\r\n").as_bytes()).unwrap();
        channel.hub.publish(vec![0xFF, 0xD8, 1, 2, 3, 0xFF, 0xD9]);
        conn.set_read_timeout(Some(Duration::from_secs(5))).unwrap(); let mut got = Vec::new();
        let mut chunk = [0u8; 4096];
        // ONE picture only: its closing boundary must arrive without a second picture.
        while !got.ends_with(b"\xFF\xD9\r\n--frame\r\n") {
            let n = conn.read(&mut chunk).unwrap();
            assert!(n > 0, "the server closed");
            got.extend_from_slice(&chunk[..n]);
        }
    }

    #[test]
    fn a_sample_reads_the_colour_of_the_picture_at_a_point() {
        let Some(ff) = ffmpeg_for_tests() else { return };
        let dir = std::env::temp_dir().join(format!("vids-sample-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("solid.mp4");
        assert!(ffmpeg::command(&ff)
            .args(["-y", "-hide_banner", "-loglevel", "error", "-f", "lavfi", "-i", "color=c=0x806020:s=320x240:r=25:d=1", "-c:v", "libx264", "-pix_fmt", "yuv444p"])
            .arg(&file)
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
            || ffmpeg::command(&ff)
                .args(["-y", "-hide_banner", "-loglevel", "error", "-f", "lavfi", "-i", "color=c=0x806020:s=320x240:r=25:d=1", "-c:v", "mpeg4", "-q:v", "1"])
                .arg(&file)
                .status()
                .unwrap()
                .success());
        let (size, effects) = (Size::original(), EffectsEntry::none());
        let look = Look { size: &size, effects: &effects, duration_s: 1.0, trim: Trim::none() };
        let rgb = sample(&ff, &Spec { input: &file, start_s: 0.0, length_s: None, look: &look, width: 160, fps: 25.0, sound: None }, (0.5, 0.5)).unwrap();
        for (got, want) in rgb.iter().zip([0x80 as f64 / 255.0, 0x60 as f64 / 255.0, 0x20 as f64 / 255.0]) {
            assert!((got - want).abs() < 0.06, "{rgb:?}");
        }
        // a point on the very edge still has a patch
        assert!(sample(&ff, &Spec { input: &file, start_s: 0.0, length_s: None, look: &look, width: 160, fps: 25.0, sound: None }, (1.0, 1.0)).is_ok());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_clock_text_is_a_time_the_cut_reads_back() {
        assert_eq!(clock(65.25), "1:05.250");
        assert_eq!(clock(3725.5), "1:02:05.500");
        assert_eq!(parse_time(&clock(65.25)), Some(Some(65.25)));
        assert_eq!(parse_time(&clock(3725.5)), Some(Some(3725.5)));
    }

    #[test]
    fn the_timecode_counts_frames_within_the_second() {
        assert_eq!(timecode(0.0, 25.0), "00:00:00:00");
        assert_eq!(timecode(61.52, 25.0), "00:01:01:13");
    }
}
