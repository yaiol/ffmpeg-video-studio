//! The job queue: N conversions at once, each with its own progress and its own cancel.
//!
//! std threads and a mutex - the window polls `snapshot()`, so the engine needs no async runtime
//! and no callback into the UI. One worker takes the next queued job, probes the file, resolves the
//! output path, runs ffmpeg and reads its progress.

use crate::{
    audio::{self, AudioEntry},
    destination::{self, Overwrite, Resolved},
    ffmpeg,
    lut::Lut,
    preset::Preset,
    effects::{EffectsEntry, Look},
    size::Size,
    trim::Trim,
};
use std::{
    io::{BufRead, BufReader},
    path::PathBuf,
    process::{Child, Stdio},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread,
};

#[derive(Clone, Debug)]
pub struct JobSpec {
    pub id: u64,
    pub input: PathBuf,
    /// The folder `input` was added through, if any - what the kept structure is relative to.
    pub source_root: Option<PathBuf>,
    pub preset: Preset,
    /// None = beside the source.
    pub folder: Option<PathBuf>,
    pub overwrite: Overwrite,
    /// The largest picture; the original size when it limits nothing.
    pub size: Size,
    /// What happens to the sound, decided per file against the preset's container.
    pub audio: AudioEntry,
    /// The colour LUT; `Lut::none()` applies none.
    pub lut: Lut,
    /// What else is done to the picture (a turn, a watermark, text); `EffectsEntry::none()` nothing.
    pub effects: EffectsEntry,
    /// The part of the file that is kept; `Trim::none()` all of it.
    pub trim: Trim,
}

#[derive(Clone, Debug, PartialEq)]
pub enum JobStatus {
    Queued,
    Probing,
    /// 0.0 ..= 100.0
    Running { percent: f32 },
    Done { output: PathBuf },
    Skipped { output: PathBuf },
    Failed { reason: String },
    Cancelled,
}

impl JobStatus {
    pub fn is_active(&self) -> bool {
        matches!(self, JobStatus::Queued | JobStatus::Probing | JobStatus::Running { .. })
    }
}

struct Slot {
    spec: JobSpec,
    status: JobStatus,
    cancel: Arc<AtomicBool>,
    child: Arc<Mutex<Option<Child>>>,
}

struct State {
    slots: Vec<Slot>,
    workers: usize,
    limit: usize,
}

struct Inner {
    /// Where ffmpeg is. It can appear while the app runs (the user downloads it), so it is set, not fixed.
    ffmpeg: Mutex<Option<PathBuf>>,
    state: Mutex<State>,
}

#[derive(Clone)]
pub struct Runner {
    inner: Arc<Inner>,
}

impl Default for Runner {
    fn default() -> Self {
        Runner::new()
    }
}

impl Runner {
    pub fn new() -> Runner {
        Runner {
            inner: Arc::new(Inner {
                ffmpeg: Mutex::new(None),
                state: Mutex::new(State { slots: Vec::new(), workers: 0, limit: 1 }),
            }),
        }
    }

    pub fn set_ffmpeg(&self, path: Option<PathBuf>) {
        *self.inner.ffmpeg.lock().unwrap() = path;
    }

    pub fn ffmpeg(&self) -> Option<PathBuf> {
        self.inner.ffmpeg.lock().unwrap().clone()
    }

    /// Queue jobs and make sure up to `parallel` workers are running. A job whose id is already
    /// known is replaced - unless it is still active, in which case it is left as it is.
    pub fn start(&self, jobs: Vec<JobSpec>, parallel: usize) {
        let mut spawn = 0;
        {
            let mut st = self.inner.state.lock().unwrap();
            st.limit = parallel.max(1);
            for spec in jobs {
                let slot = Slot {
                    spec: spec.clone(),
                    status: JobStatus::Queued,
                    cancel: Arc::new(AtomicBool::new(false)),
                    child: Arc::new(Mutex::new(None)),
                };
                match st.slots.iter().position(|s| s.spec.id == spec.id) {
                    Some(i) if st.slots[i].status.is_active() => {}
                    Some(i) => st.slots[i] = slot,
                    None => st.slots.push(slot),
                }
            }
            let queued = st.slots.iter().filter(|s| s.status == JobStatus::Queued).count();
            while st.workers < st.limit && spawn < queued {
                st.workers += 1;
                spawn += 1;
            }
        }
        for _ in 0..spawn {
            let inner = Arc::clone(&self.inner);
            thread::spawn(move || worker(inner));
        }
    }

    pub fn cancel(&self, id: u64) {
        let mut st = self.inner.state.lock().unwrap();
        if let Some(s) = st.slots.iter_mut().find(|s| s.spec.id == id) {
            cancel_slot(s);
        }
    }

    pub fn cancel_all(&self) {
        let mut st = self.inner.state.lock().unwrap();
        for s in st.slots.iter_mut() {
            cancel_slot(s);
        }
    }

    /// Forget a finished job (the window removed its row).
    pub fn remove(&self, id: u64) {
        let mut st = self.inner.state.lock().unwrap();
        st.slots.retain(|s| s.spec.id != id || s.status.is_active());
    }

    pub fn snapshot(&self) -> Vec<(u64, JobStatus)> {
        let st = self.inner.state.lock().unwrap();
        st.slots.iter().map(|s| (s.spec.id, s.status.clone())).collect()
    }

    pub fn busy(&self) -> bool {
        self.inner.state.lock().unwrap().slots.iter().any(|s| s.status.is_active())
    }
}

fn cancel_slot(s: &mut Slot) {
    match s.status {
        JobStatus::Queued => s.status = JobStatus::Cancelled,
        JobStatus::Probing | JobStatus::Running { .. } => {
            s.cancel.store(true, Ordering::SeqCst);
            if let Some(c) = s.child.lock().unwrap().as_mut() {
                let _ = c.kill();
            }
        }
        _ => {}
    }
}

fn set_status(inner: &Inner, id: u64, status: JobStatus) {
    let mut st = inner.state.lock().unwrap();
    if let Some(s) = st.slots.iter_mut().find(|s| s.spec.id == id) {
        s.status = status;
    }
}

fn worker(inner: Arc<Inner>) {
    loop {
        let job = {
            let mut st = inner.state.lock().unwrap();
            // A shrunk limit lets surplus workers retire as soon as they finish a job.
            let idx = if st.workers > st.limit {
                None
            } else {
                st.slots.iter().position(|s| s.status == JobStatus::Queued)
            };
            match idx {
                Some(i) => {
                    let s = &mut st.slots[i];
                    s.status = JobStatus::Probing;
                    Some((s.spec.clone(), Arc::clone(&s.cancel), Arc::clone(&s.child)))
                }
                None => {
                    st.workers -= 1;
                    None
                }
            }
        };
        let Some((spec, cancel, child)) = job else { return };
        let status = run_job(&inner, &spec, &cancel, &child);
        set_status(&inner, spec.id, status);
    }
}

fn run_job(inner: &Inner, spec: &JobSpec, cancel: &AtomicBool, child_slot: &Mutex<Option<Child>>) -> JobStatus {
    let Some(ffmpeg_path) = inner.ffmpeg.lock().unwrap().clone() else {
        return JobStatus::Failed { reason: "ffmpeg not found".to_owned() };
    };
    let probe = match ffmpeg::probe(&ffmpeg_path, &spec.input) {
        Ok(p) => p,
        Err(reason) => return JobStatus::Failed { reason },
    };
    if cancel.load(Ordering::SeqCst) {
        return JobStatus::Cancelled;
    }

    let preset = spec.preset.clone().normalized();
    // The effects as they will be used (a path pasted with quotes is cleaned). The picture a watermark lays
    // over the video must be there, or ffmpeg stops with a message about a filter graph.
    let effects = spec.effects.clone().normalized();
    if preset.video != crate::preset::VideoCodec::Copy {
        if let Some(w) = effects.watermark().filter(|w| !w.path.is_empty() && !w.file_exists()) {
            return JobStatus::Failed { reason: format!("Watermark image not found: {}", w.path) };
        }
    }
    // A LUT that is chosen but not there would make ffmpeg stop with a message about a filter graph.
    if !spec.lut.is_none() && preset.video != crate::preset::VideoCodec::Copy && !spec.lut.file_exists() {
        return JobStatus::Failed { reason: format!("LUT file not found: {}", spec.lut.path) };
    }
    let output = match destination::resolve_output(
        spec.folder.as_deref(),
        &spec.input,
        spec.source_root.as_deref(),
        preset.container.extension(),
        spec.overwrite,
    ) {
        Resolved::Skip(output) => return JobStatus::Skipped { output },
        Resolved::Write(p) => p,
    };
    if let Some(dir) = output.parent() {
        if let Err(e) = std::fs::create_dir_all(dir) {
            return JobStatus::Failed { reason: format!("cannot create {}: {e}", dir.display()) };
        }
    }

    // What is written is what the cut leaves, so that is what progress and the countdown are measured against.
    let kept_s = spec.trim.kept_s(probe.duration_us as f64 / 1_000_000.0);
    let look = Look { size: &spec.size, lut: &spec.lut, effects: &effects, duration_s: kept_s, trim: spec.trim };
    let mut sound = audio::resolve(&spec.audio, &preset, &probe.audio_codec);
    // A sound an effect has to process cannot be copied.
    if preset.video != crate::preset::VideoCodec::Copy && effects.audio_filter().is_some() {
        sound = audio::reencoded(sound, &preset, &probe.audio_codec);
    }
    let args = ffmpeg::build_args(&spec.input, &output, &preset, &look, sound);
    set_status(inner, spec.id, JobStatus::Running { percent: 0.0 });
    let mut child = match ffmpeg::command(&ffmpeg_path)
        .args(&args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
    {
        Ok(c) => c,
        Err(e) => return JobStatus::Failed { reason: format!("cannot run ffmpeg: {e}") },
    };
    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    *child_slot.lock().unwrap() = Some(child);
    // Cancelled between the check above and the spawn: the kill found no child, so do it now.
    if cancel.load(Ordering::SeqCst) {
        if let Some(c) = child_slot.lock().unwrap().as_mut() {
            let _ = c.kill();
        }
    }

    // stderr must be drained or ffmpeg blocks on a full pipe; keep its tail for the error message.
    let errors = thread::spawn(move || {
        let mut tail: Vec<String> = Vec::new();
        if let Some(e) = stderr {
            for line in BufReader::new(e).lines().map_while(Result::ok) {
                let line = line.trim().to_owned();
                if !line.is_empty() {
                    tail.push(line);
                    if tail.len() > 12 {
                        tail.remove(0);
                    }
                }
            }
        }
        tail
    });

    if let Some(out) = stdout {
        for line in BufReader::new(out).lines().map_while(Result::ok) {
            if let Some(us) = ffmpeg::parse_progress_line(&line) {
                if kept_s > 0.0 {
                    let pct = (us as f64 / (kept_s * 1_000_000.0) * 100.0).clamp(0.0, 99.9) as f32;
                    set_status(inner, spec.id, JobStatus::Running { percent: pct });
                }
            }
        }
    }

    let exit = child_slot.lock().unwrap().take().map(|mut c| c.wait());
    let tail = errors.join().unwrap_or_default();

    if cancel.load(Ordering::SeqCst) {
        let _ = std::fs::remove_file(&output);
        return JobStatus::Cancelled;
    }
    match exit {
        Some(Ok(status)) if status.success() => JobStatus::Done { output },
        _ => {
            let _ = std::fs::remove_file(&output);
            let reason = tail.last().cloned().unwrap_or_else(|| "ffmpeg failed".to_owned());
            JobStatus::Failed { reason }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        audio::Audio,
        effects::{BlurFit, Corner, Crop, Effect, FrameRate, TextOverlay, Watermark},
        preset::{Container, VideoCodec},
        rotate::Rotation,
    };
    use std::{fs, path::Path, time::Duration};

    /// Tests that need a real ffmpeg run only when one is on PATH, and skip quietly otherwise.
    fn ffmpeg_for_tests() -> Option<PathBuf> {
        ffmpeg::locate(None)
    }

    /// A 2-second test clip made by ffmpeg itself.
    fn make_clip(ff: &Path, path: &Path) {
        let ok = ffmpeg::command(ff)
            .args(["-y", "-hide_banner", "-loglevel", "error", "-f", "lavfi", "-i", "testsrc=size=320x240:rate=25:duration=2"])
            .args(["-f", "lavfi", "-i", "sine=frequency=440:duration=2", "-c:v", "mpeg4", "-c:a", "aac", "-shortest"])
            .arg(path)
            .status()
            .unwrap()
            .success();
        assert!(ok, "could not make the test clip");
    }

    fn wait(runner: &Runner) -> Vec<(u64, JobStatus)> {
        for _ in 0..600 {
            if !runner.busy() {
                break;
            }
            thread::sleep(Duration::from_millis(100));
        }
        runner.snapshot()
    }

    fn scratch(name: &str) -> PathBuf {
        let p = std::env::temp_dir().join(format!("yaiol-vdcv-queue-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&p);
        fs::create_dir_all(&p).unwrap();
        p
    }

    /// An encoder this ffmpeg lacks (the LGPL builds have no libx264) makes the test skip, not fail.
    fn has_encoder(ff: &Path, name: &str) -> bool {
        let out = ffmpeg::command(ff).args(["-hide_banner", "-encoders"]).output().unwrap();
        String::from_utf8_lossy(&out.stdout).contains(name)
    }

    #[test]
    fn a_clip_is_converted_with_progress_and_a_playable_result() {
        let Some(ff) = ffmpeg_for_tests() else { return };
        let dir = scratch("convert");
        let input = dir.join("clip.avi");
        make_clip(&ff, &input);
        let mut preset = Preset::new("t");
        preset.container = Container::Mp4;
        preset.video = if has_encoder(&ff, "libx264") { VideoCodec::H264 } else { VideoCodec::Av1 };
        if preset.video == VideoCodec::Av1 && !has_encoder(&ff, "libsvtav1") {
            return;
        }
        let aac = AudioEntry { mode: Audio::Aac, kbps: 128, ..AudioEntry::new("aac") };
        let runner = Runner::new();
        runner.set_ffmpeg(Some(ff.clone()));
        runner.start(
            vec![JobSpec { id: 1, input: input.clone(), source_root: None, preset, folder: None, overwrite: Overwrite::Overwrite, size: Size::original(), audio: aac, lut: Lut::none(), effects: EffectsEntry::none(), trim: Trim::none() }],
            1,
        );
        let snap = wait(&runner);
        let JobStatus::Done { output } = &snap[0].1 else { panic!("{:?}", snap) };
        let p = ffmpeg::probe(&ff, output).unwrap();
        assert!(p.duration_us > 1_500_000, "{p:?}");
        assert_eq!(p.audio_codec, "aac");
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn prores_in_a_mov_with_pcm_audio_is_written_and_read_back() {
        let Some(ff) = ffmpeg_for_tests() else { return };
        if !has_encoder(&ff, "prores_ks") {
            return;
        }
        let dir = scratch("prores");
        let input = dir.join("clip.avi");
        make_clip(&ff, &input);
        let mut preset = Preset::new("t");
        preset.container = Container::Mov;
        preset.video = VideoCodec::ProRes;
        let runner = Runner::new();
        runner.set_ffmpeg(Some(ff.clone()));
        runner.start(
            vec![JobSpec { id: 1, input, source_root: None, preset, folder: None, overwrite: Overwrite::Overwrite, size: Size::original(), audio: AudioEntry::original(), lut: Lut::none(), effects: EffectsEntry::none(), trim: Trim::none() }],
            1,
        );
        let snap = wait(&runner);
        let JobStatus::Done { output } = &snap[0].1 else { panic!("{:?}", snap) };
        let p = ffmpeg::probe(&ff, output).unwrap();
        assert_eq!(p.video_codec, "prores");
        assert!(p.audio_codec.starts_with("pcm_s16le"), "{p:?}");
        let _ = fs::remove_dir_all(dir);
    }

    /// There is no way to run an AMD encoder without the card, so this checks what can be checked
    /// anywhere: that every option the arguments name is one this ffmpeg's encoder has.
    #[test]
    fn every_option_the_amd_arguments_name_exists_in_this_ffmpeg() {
        let Some(ff) = ffmpeg_for_tests() else { return };
        for (codec, name) in [
            (VideoCodec::H264Amd, "h264_amf"),
            (VideoCodec::H265Amd, "hevc_amf"),
            (VideoCodec::H264Nvenc, "h264_nvenc"),
            (VideoCodec::H265Nvenc, "hevc_nvenc"),
            (VideoCodec::H264Qsv, "h264_qsv"),
            (VideoCodec::H265Qsv, "hevc_qsv"),
        ] {
            if !has_encoder(&ff, name) {
                continue;
            }
            let help = ffmpeg::command(&ff).args(["-hide_banner", "-h", &format!("encoder={name}")]).output().unwrap();
            let help = String::from_utf8_lossy(&help.stdout).into_owned();
            let preset = Preset { video: codec, ..Preset::new("t") };
            let (size, lut, effects) = (Size::original(), Lut::none(), EffectsEntry::none());
            let look = Look { size: &size, lut: &lut, effects: &effects, duration_s: 1.0, trim: Trim::none() };
            let args = ffmpeg::build_args(Path::new("in.mkv"), Path::new("out.mkv"), &preset, &look, crate::audio::Resolved { mode: Audio::None, kbps: 0, channels: None, gain_db: 0, loudness: false });
            let args: Vec<String> = args.iter().map(|a| a.to_string_lossy().into_owned()).collect();
            let from = args.iter().position(|a| a == name).unwrap();
            // Every flag after the encoder's name up to the output, except generic ones the muxer owns.
            for flag in args[from + 1..].iter().filter(|a| a.starts_with('-') && !matches!(a.as_str(), "-pix_fmt" | "-tag:v" | "-an" | "-map_metadata" | "-b:v" | "-global_quality")) {
                assert!(help.contains(&format!("  {flag} ")), "{name} has no option {flag}");
            }
        }
    }

    #[test]
    fn a_size_shrinks_the_picture_and_never_enlarges_it() {
        let Some(ff) = ffmpeg_for_tests() else { return };
        let dir = scratch("size");
        let input = dir.join("clip.avi"); // 320 x 240
        make_clip(&ff, &input);
        let mut preset = Preset::new("t");
        preset.video = VideoCodec::Av1;
        if !has_encoder(&ff, "libsvtav1") {
            preset.video = VideoCodec::H264;
            if !has_encoder(&ff, "libx264") {
                return;
            }
        }
        let runner = Runner::new();
        runner.set_ffmpeg(Some(ff.clone()));
        let spec = |id: u64, size: Size, folder: &str| JobSpec {
            id,
            input: input.clone(),
            source_root: None,
            preset: preset.clone(),
            folder: Some(dir.join(folder)),
            overwrite: Overwrite::Overwrite,
            size,
            audio: AudioEntry::original(),
            lut: Lut::none(),
            effects: EffectsEntry::none(),
            trim: Trim::none(),
        };
        let shrink = Size { max_height: Some(120), ..Size::new("small") };
        let huge = Size { max_width: Some(1920), max_height: Some(1080), ..Size::new("huge") };
        runner.start(vec![spec(1, shrink, "a"), spec(2, huge, "b")], 2);
        let snap = wait(&runner);
        let dims = |i: usize| {
            let JobStatus::Done { output } = &snap[i].1 else { panic!("{:?}", snap) };
            let p = ffmpeg::probe(&ff, output).unwrap();
            (p.width, p.height)
        };
        assert_eq!(dims(0), (160, 120));
        assert_eq!(dims(1), (320, 240));
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn a_lut_file_is_applied_and_a_missing_one_fails_the_job_plainly() {
        let Some(ff) = ffmpeg_for_tests() else { return };
        let dir = scratch("lut folder"); // a space in the path, on purpose
        let input = dir.join("clip.avi");
        make_clip(&ff, &input);
        // The identity table: it changes no colour, so the test is about ffmpeg accepting the path.
        let cube = dir.join("identity.cube");
        fs::write(&cube, "LUT_3D_SIZE 2\n0 0 0\n1 0 0\n0 1 0\n1 1 0\n0 0 1\n1 0 1\n0 1 1\n1 1 1\n").unwrap();
        let mut preset = Preset::new("t");
        preset.container = Container::Mov;
        preset.video = VideoCodec::ProRes;
        let runner = Runner::new();
        runner.set_ffmpeg(Some(ff.clone()));
        let spec = |id: u64, path: &Path| JobSpec {
            id,
            input: input.clone(),
            source_root: None,
            preset: preset.clone(),
            folder: Some(dir.join(format!("out{id}"))),
            overwrite: Overwrite::Overwrite,
            size: Size::original(),
            audio: AudioEntry::original(),
            lut: Lut { path: path.to_string_lossy().into_owned(), ..Lut::new("l") },
            effects: EffectsEntry::none(),
            trim: Trim::none(),
        };
        runner.start(vec![spec(1, &cube), spec(2, &dir.join("gone.cube"))], 2);
        let snap = wait(&runner);
        assert!(matches!(snap[0].1, JobStatus::Done { .. }), "{snap:?}");
        let JobStatus::Failed { reason } = &snap[1].1 else { panic!("{snap:?}") };
        assert!(reason.contains("LUT file not found"), "{reason}");
        let _ = fs::remove_dir_all(dir);
    }

    fn effects_spec(input: &Path, dir: &Path, id: u64, size: Size, effects: EffectsEntry) -> JobSpec {
        let mut preset = Preset::new("t");
        preset.container = Container::Mov;
        preset.video = VideoCodec::ProRes;
        JobSpec {
            id,
            input: input.to_path_buf(),
            source_root: None,
            preset,
            folder: Some(dir.join(format!("out{id}"))),
            overwrite: Overwrite::Overwrite,
            size,
            audio: AudioEntry::original(),
            lut: Lut::none(),
            effects,
            trim: Trim::none(),
        }
    }

    fn entry_of(items: Vec<Effect>) -> EffectsEntry {
        EffectsEntry { items, ..EffectsEntry::new("e") }
    }

    #[test]
    fn a_quarter_turn_swaps_the_sides_and_the_size_applies_to_the_turned_picture() {
        let Some(ff) = ffmpeg_for_tests() else { return };
        let dir = scratch("turn");
        let input = dir.join("clip.avi"); // 320 x 240
        make_clip(&ff, &input);
        let turn = |r: Rotation| entry_of(vec![Effect::Rotate { rotation: r }]);
        // 320 x 240 turned is 240 x 320; a height limit of 160 then halves it to 120 x 160.
        let limit = Size { max_height: Some(160), ..Size::new("s") };
        let runner = Runner::new();
        runner.set_ffmpeg(Some(ff.clone()));
        runner.start(
            vec![
                effects_spec(&input, &dir, 1, Size::original(), turn(Rotation::Cw)),
                effects_spec(&input, &dir, 2, Size::original(), turn(Rotation::Half)),
                effects_spec(&input, &dir, 3, limit, turn(Rotation::Ccw)),
            ],
            3,
        );
        let snap = wait(&runner);
        let dims = |i: usize| {
            let JobStatus::Done { output } = &snap[i].1 else { panic!("{:?}", snap) };
            let p = ffmpeg::probe(&ff, output).unwrap();
            (p.width, p.height)
        };
        assert_eq!(dims(0), (240, 320));
        assert_eq!(dims(1), (320, 240));
        assert_eq!(dims(2), (120, 160));
        let _ = fs::remove_dir_all(dir);
    }

    /// The mean colour of an 8 x 8 square of the first frame, as 0-255 red, green, blue.
    fn square_colour(ff: &Path, video: &Path, x: u32, y: u32) -> (u32, u32, u32) {
        let out = ffmpeg::command(ff)
            .args(["-v", "error", "-i"])
            .arg(video)
            .args(["-vf", &format!("crop=8:8:{x}:{y}"), "-frames:v", "1", "-f", "rawvideo", "-pix_fmt", "rgb24", "-"])
            .output()
            .unwrap();
        assert_eq!(out.stdout.len(), 8 * 8 * 3, "{}", String::from_utf8_lossy(&out.stderr));
        let mut sum = (0u32, 0u32, 0u32);
        for px in out.stdout.chunks(3) {
            sum = (sum.0 + u32::from(px[0]), sum.1 + u32::from(px[1]), sum.2 + u32::from(px[2]));
        }
        (sum.0 / 64, sum.1 / 64, sum.2 / 64)
    }

    #[test]
    fn a_watermark_is_laid_in_its_corner_and_a_missing_picture_fails_the_job() {
        let Some(ff) = ffmpeg_for_tests() else { return };
        let dir = scratch("watermark dir"); // a space in the path
        let input = dir.join("clip.avi"); // 320 x 240
        make_clip(&ff, &input);
        let logo = dir.join("logo.png");
        let ok = ffmpeg::command(&ff)
            .args(["-y", "-v", "error", "-f", "lavfi", "-i", "color=c=red:s=64x64", "-frames:v", "1"])
            .arg(&logo)
            .status()
            .unwrap()
            .success();
        assert!(ok);
        let mark = |path: &Path, corner: Corner| {
            entry_of(vec![Effect::Watermark(Watermark { path: path.to_string_lossy().into_owned(), corner, ..Watermark::default() })])
        };
        let runner = Runner::new();
        runner.set_ffmpeg(Some(ff.clone()));
        runner.start(
            vec![
                effects_spec(&input, &dir, 1, Size::original(), mark(&logo, Corner::BottomRight)),
                effects_spec(&input, &dir, 2, Size::original(), mark(&logo, Corner::TopLeft)),
                effects_spec(&input, &dir, 3, Size::original(), mark(&dir.join("gone.png"), Corner::TopLeft)),
            ],
            3,
        );
        let snap = wait(&runner);
        let out = |i: usize| {
            let JobStatus::Done { output } = &snap[i].1 else { panic!("{:?}", snap) };
            output.clone()
        };
        // 64 px red at a 20 px margin: bottom right covers x 236..300, y 156..220; top left 20..84.
        let near = |c: (u32, u32, u32)| c.0 > 180 && c.1 < 80 && c.2 < 80;
        assert!(near(square_colour(&ff, &out(0), 250, 170)), "{:?}", square_colour(&ff, &out(0), 250, 170));
        assert!(!near(square_colour(&ff, &out(0), 30, 30)));
        assert!(near(square_colour(&ff, &out(1), 30, 30)));
        assert_eq!(ffmpeg::probe(&ff, &out(0)).unwrap().width, 320);
        let JobStatus::Failed { reason } = &snap[2].1 else { panic!("{snap:?}") };
        assert!(reason.contains("Watermark image not found"), "{reason}");
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn text_with_the_name_and_the_countdown_is_accepted_by_ffmpeg_with_a_watermark_too() {
        let Some(ff) = ffmpeg_for_tests() else { return };
        let has_drawtext = ffmpeg::command(&ff).args(["-hide_banner", "-filters"]).output().map(|o| String::from_utf8_lossy(&o.stdout).contains("drawtext")).unwrap_or(false);
        if !has_drawtext {
            return;
        }
        let dir = scratch("text");
        let input = dir.join("it's a 50% clip.avi"); // every character the text has to survive
        make_clip(&ff, &input);
        let logo = dir.join("logo.png");
        assert!(ffmpeg::command(&ff).args(["-y", "-v", "error", "-f", "lavfi", "-i", "color=c=red:s=32x32", "-frames:v", "1"]).arg(&logo).status().unwrap().success());
        let text = Effect::Text(TextOverlay { text: "{name} {elapsed} / {remaining}".into(), corner: Corner::BottomLeft, ..TextOverlay::default() });
        let mark = Effect::Watermark(Watermark { path: logo.to_string_lossy().into_owned(), ..Watermark::default() });
        let runner = Runner::new();
        runner.set_ffmpeg(Some(ff.clone()));
        runner.start(
            vec![
                effects_spec(&input, &dir, 1, Size::original(), entry_of(vec![text.clone()])),
                effects_spec(&input, &dir, 2, Size::original(), entry_of(vec![mark, text])),
            ],
            2,
        );
        let snap = wait(&runner);
        for (id, s) in &snap {
            assert!(matches!(s, JobStatus::Done { .. }), "job {id}: {s:?}");
        }
        let _ = fs::remove_dir_all(dir);
    }

    /// The first frame of `video` (after the given input options) as 24-bit RGB, optionally through a filter.
    fn first_frame(ff: &Path, video: &Path, before_input: &[&str], vf: Option<&str>) -> Vec<u8> {
        let mut cmd = ffmpeg::command(ff);
        cmd.args(["-v", "error"]).args(before_input).arg("-i").arg(video);
        if let Some(f) = vf {
            cmd.args(["-vf", f]);
        }
        let out = cmd.args(["-frames:v", "1", "-f", "rawvideo", "-pix_fmt", "rgb24", "-"]).output().unwrap();
        assert!(!out.stdout.is_empty(), "{}", String::from_utf8_lossy(&out.stderr));
        out.stdout
    }

    /// The mean absolute difference between two frames of the same size, 0-255.
    fn difference(a: &[u8], b: &[u8]) -> f64 {
        assert_eq!(a.len(), b.len());
        a.iter().zip(b).map(|(x, y)| f64::from(x.abs_diff(*y))).sum::<f64>() / a.len() as f64
    }

    fn has_filter(ff: &Path, name: &str) -> bool {
        let out = ffmpeg::command(ff).args(["-hide_banner", "-filters"]).output().unwrap();
        String::from_utf8_lossy(&out.stdout).contains(&format!(" {name} "))
    }

    #[test]
    fn a_vertical_video_is_centred_on_a_blurred_copy_in_a_16_9_picture() {
        let Some(ff) = ffmpeg_for_tests() else { return };
        let dir = scratch("blurfit");
        let input = dir.join("portrait.avi"); // 180 x 320
        let made = ffmpeg::command(&ff)
            .args(["-y", "-v", "error", "-f", "lavfi", "-i", "testsrc=size=180x320:rate=25:duration=2", "-c:v", "mpeg4"])
            .arg(&input)
            .status()
            .unwrap()
            .success();
        assert!(made);
        let fit = entry_of(vec![Effect::BlurFit(BlurFit { height: 180, blur: 5 })]);
        let runner = Runner::new();
        runner.set_ffmpeg(Some(ff.clone()));
        runner.start(vec![effects_spec(&input, &dir, 1, Size::original(), fit)], 1);
        let snap = wait(&runner);
        let JobStatus::Done { output } = &snap[0].1 else { panic!("{:?}", snap) };
        let p = ffmpeg::probe(&ff, output).unwrap();
        assert_eq!((p.width, p.height), (320, 180));
        // The strip at the left is the blurred copy, not black bars: it has colour in it.
        let strip = first_frame(&ff, output, &[], Some("crop=8:180:0:0"));
        assert!(strip.iter().map(|b| u32::from(*b)).sum::<u32>() / strip.len() as u32 > 30, "the sides are black");
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn stereoscopic_puts_the_two_eyes_side_by_side() {
        let Some(ff) = ffmpeg_for_tests() else { return };
        let dir = scratch("stereo");
        let input = dir.join("clip.avi"); // 320 x 240
        make_clip(&ff, &input);
        let runner = Runner::new();
        runner.set_ffmpeg(Some(ff.clone()));
        runner.start(vec![effects_spec(&input, &dir, 1, Size::original(), entry_of(vec![Effect::Stereo]))], 1);
        let snap = wait(&runner);
        let JobStatus::Done { output } = &snap[0].1 else { panic!("{:?}", snap) };
        let p = ffmpeg::probe(&ff, output).unwrap();
        assert_eq!((p.width, p.height), (640, 240));
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn playing_backwards_starts_on_the_last_frame_and_its_sound_cannot_be_copied() {
        let Some(ff) = ffmpeg_for_tests() else { return };
        let dir = scratch("reverse");
        let input = dir.join("clip.avi");
        make_clip(&ff, &input);
        let back = entry_of(vec![Effect::Reverse]);
        let runner = Runner::new();
        runner.set_ffmpeg(Some(ff.clone()));
        // A ProRes MOV, and an H.264 / AV1 MP4 whose sound would have been copied (the source's AAC fits).
        let mut specs = vec![effects_spec(&input, &dir, 1, Size::original(), back.clone())];
        let mp4_codec = if has_encoder(&ff, "libx264") { Some(VideoCodec::H264) } else if has_encoder(&ff, "libsvtav1") { Some(VideoCodec::Av1) } else { None };
        if let Some(codec) = mp4_codec {
            let mut spec = effects_spec(&input, &dir, 2, Size::original(), back);
            spec.preset.container = Container::Mp4;
            spec.preset.video = codec;
            specs.push(spec);
        }
        runner.start(specs, 2);
        let snap = wait(&runner);
        for (id, status) in &snap {
            assert!(matches!(status, JobStatus::Done { .. }), "job {id}: {status:?}");
        }
        let JobStatus::Done { output } = &snap[0].1 else { unreachable!() };
        let first = first_frame(&ff, output, &[], None);
        let last_of_source = first_frame(&ff, &input, &["-sseof", "-0.05"], None);
        let first_of_source = first_frame(&ff, &input, &[], None);
        assert!(difference(&first, &last_of_source) < 8.0, "{}", difference(&first, &last_of_source));
        assert!(difference(&first, &first_of_source) > difference(&first, &last_of_source));
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn burning_the_subtitles_draws_them_into_the_picture() {
        let Some(ff) = ffmpeg_for_tests() else { return };
        if !has_filter(&ff, "subtitles") {
            return;
        }
        let dir = scratch("burn dir"); // a space in the path, on purpose
        let clip = dir.join("clip.avi");
        make_clip(&ff, &clip);
        let srt = dir.join("subs.srt");
        fs::write(&srt, "1\n00:00:00,000 --> 00:00:02,000\nHELLO WORLD\n").unwrap();
        let input = dir.join("with subs.mkv");
        let muxed = ffmpeg::command(&ff)
            .args(["-y", "-v", "error", "-i"])
            .arg(&clip)
            .arg("-i")
            .arg(&srt)
            .args(["-map", "0", "-map", "1", "-c", "copy", "-c:s", "srt"])
            .arg(&input)
            .status()
            .unwrap()
            .success();
        assert!(muxed);
        let runner = Runner::new();
        runner.set_ffmpeg(Some(ff.clone()));
        runner.start(
            vec![
                effects_spec(&input, &dir, 1, Size::original(), entry_of(vec![Effect::Burn])),
                effects_spec(&input, &dir, 2, Size::original(), EffectsEntry::none()),
            ],
            2,
        );
        let snap = wait(&runner);
        let out = |i: usize| {
            let JobStatus::Done { output } = &snap[i].1 else { panic!("{:?}", snap) };
            output.clone()
        };
        // The bottom of the picture, where subtitles sit: identical without the effect, different with it.
        let region = Some("crop=240:70:40:160");
        let burned = first_frame(&ff, &out(0), &[], region);
        let plain = first_frame(&ff, &out(1), &[], region);
        assert!(difference(&burned, &plain) > 0.3, "{}", difference(&burned, &plain));
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn crop_frame_rate_deinterlace_and_stripped_metadata_work_on_a_real_clip() {
        let Some(ff) = ffmpeg_for_tests() else { return };
        let dir = scratch("basics");
        let input = dir.join("clip.avi"); // 320 x 240, 25 fps
        make_clip(&ff, &input);
        let tagged = dir.join("tagged.avi");
        assert!(ffmpeg::command(&ff).args(["-y", "-v", "error", "-i"]).arg(&input).args(["-c", "copy", "-metadata", "title=SecretTitle"]).arg(&tagged).status().unwrap().success());
        let jobs = vec![
            effects_spec(&input, &dir, 1, Size::original(), entry_of(vec![Effect::Crop(Crop { left: 20, right: 20, top: 11, bottom: 11 })])),
            effects_spec(&input, &dir, 2, Size::original(), entry_of(vec![Effect::Fps(FrameRate { value: "15".into() })])),
            effects_spec(&input, &dir, 3, Size::original(), entry_of(vec![Effect::Deinterlace])),
            effects_spec(&tagged, &dir, 4, Size::original(), entry_of(vec![Effect::Strip])),
            effects_spec(&tagged, &dir, 5, Size::original(), EffectsEntry::none()),
        ];
        let runner = Runner::new();
        runner.set_ffmpeg(Some(ff.clone()));
        runner.start(jobs, 5);
        let snap = wait(&runner);
        let out = |i: usize| {
            let JobStatus::Done { output } = &snap[i].1 else { panic!("{:?}", snap) };
            output.clone()
        };
        // 320 - 40 = 280 wide; 240 - 22 = 218 high (already even).
        let p = ffmpeg::probe(&ff, &out(0)).unwrap();
        assert_eq!((p.width, p.height), (280, 218));
        assert_eq!(ffmpeg::probe(&ff, &out(1)).unwrap().fps.map(|f| f.round() as u32), Some(15));
        assert_eq!(ffmpeg::probe(&ff, &out(2)).unwrap().width, 320);
        // The title the source carries survives an ordinary conversion and not one that leaves the metadata out.
        assert!(info_text(&ff, &out(4)).contains("SecretTitle"), "the title was lost without the effect");
        assert!(!info_text(&ff, &out(3)).contains("SecretTitle"), "the title is still there");
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn a_cut_keeps_the_part_between_the_two_times_and_progress_follows_it() {
        let Some(ff) = ffmpeg_for_tests() else { return };
        let dir = scratch("cut");
        let input = dir.join("clip.avi"); // two seconds
        make_clip(&ff, &input);
        let cut = |id: u64, start: &str, end: &str| {
            let mut spec = effects_spec(&input, &dir, id, Size::original(), EffectsEntry::none());
            spec.trim = Trim::parse(start, end).unwrap();
            spec
        };
        let mut copy = cut(4, "0.5", "1.5");
        copy.preset.container = Container::Mkv;
        copy.preset.video = VideoCodec::Copy;
        let runner = Runner::new();
        runner.set_ffmpeg(Some(ff.clone()));
        runner.start(vec![cut(1, "0.5", "1.5"), cut(2, "1", ""), cut(3, "", "0.5"), copy], 4);
        let snap = wait(&runner);
        let seconds = |i: usize| {
            let JobStatus::Done { output } = &snap[i].1 else { panic!("{:?}", snap) };
            ffmpeg::probe(&ff, output).unwrap().duration_us as f64 / 1_000_000.0
        };
        assert!((seconds(0) - 1.0).abs() < 0.15, "{}", seconds(0));
        assert!((seconds(1) - 1.0).abs() < 0.15, "{}", seconds(1));
        assert!((seconds(2) - 0.5).abs() < 0.15, "{}", seconds(2));
        // A stream copy starts on a keyframe, so it is only as exact as the keyframes; it must still be a cut.
        assert!(seconds(3) > 0.5 && seconds(3) < 2.0, "{}", seconds(3));
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn a_level_change_loudness_and_a_tune_are_accepted_by_a_real_ffmpeg() {
        let Some(ff) = ffmpeg_for_tests() else { return };
        if !has_filter(&ff, "loudnorm") {
            return;
        }
        let dir = scratch("level");
        let input = dir.join("clip.avi");
        make_clip(&ff, &input);
        // A louder sound, brought to a standard loudness, mixed down - and the video copied, which the
        // sound's own filters must work with.
        let mut spec = effects_spec(&input, &dir, 1, Size::original(), EffectsEntry::none());
        spec.preset.container = Container::Mkv;
        spec.preset.video = VideoCodec::Copy;
        spec.audio = AudioEntry { mode: Audio::Aac, kbps: 128, channels: Some(2), gain_db: 6, loudness: true, ..AudioEntry::new("loud") };
        let mut specs = vec![spec];
        if has_encoder(&ff, "libx264") {
            let mut tuned = effects_spec(&input, &dir, 2, Size::original(), EffectsEntry::none());
            tuned.preset.container = Container::Mkv;
            tuned.preset.video = VideoCodec::H264;
            tuned.preset.tune = crate::preset::Tune::StillImage;
            specs.push(tuned);
        }
        let runner = Runner::new();
        runner.set_ffmpeg(Some(ff.clone()));
        runner.start(specs, 2);
        let snap = wait(&runner);
        for (id, status) in &snap {
            assert!(matches!(status, JobStatus::Done { .. }), "job {id}: {status:?}");
        }
        let JobStatus::Done { output } = &snap[0].1 else { unreachable!() };
        let text = info_text(&ff, output);
        assert!(text.contains("Audio: aac") && text.contains("stereo"), "{text}");
        let _ = fs::remove_dir_all(dir);
    }

    fn info_text(ff: &Path, video: &Path) -> String {
        let out = ffmpeg::command(ff).args(["-hide_banner", "-i"]).arg(video).output().unwrap();
        String::from_utf8_lossy(&out.stderr).into_owned()
    }

    /// Whether the encoder is not only in this ffmpeg but works on this machine (it needs the card).
    fn encoder_works(ff: &Path, name: &str) -> bool {
        ffmpeg::command(ff)
            .args(["-v", "error", "-f", "lavfi", "-i", "testsrc=size=640x360:rate=25:duration=0.5", "-c:v", name, "-f", "null", "-"])
            .output()
            .map(|o| o.status.success() && o.stderr.is_empty())
            .unwrap_or(false)
    }

    #[test]
    fn the_encoders_of_the_graphics_card_convert_a_real_clip_where_the_card_has_them() {
        let Some(ff) = ffmpeg_for_tests() else { return };
        let dir = scratch("hardware");
        let input = dir.join("clip.avi");
        make_clip(&ff, &input);
        let mut specs = Vec::new();
        let mut names = Vec::new();
        for (codec, name, expected) in [
            (VideoCodec::H264Amd, "h264_amf", "h264"),
            (VideoCodec::H264Nvenc, "h264_nvenc", "h264"),
            (VideoCodec::H265Nvenc, "hevc_nvenc", "hevc"),
            (VideoCodec::H264Qsv, "h264_qsv", "h264"),
        ] {
            if !encoder_works(&ff, name) {
                continue;
            }
            let mut spec = effects_spec(&input, &dir, specs.len() as u64 + 1, Size::original(), EffectsEntry::none());
            spec.preset.container = Container::Mkv;
            spec.preset.video = codec;
            specs.push(spec);
            names.push((name, expected));
        }
        if specs.is_empty() {
            return;
        }
        let runner = Runner::new();
        runner.set_ffmpeg(Some(ff.clone()));
        runner.start(specs, 1);
        let snap = wait(&runner);
        for (i, (name, expected)) in names.iter().enumerate() {
            let JobStatus::Done { output } = &snap[i].1 else { panic!("{name}: {:?}", snap[i]) };
            assert_eq!(ffmpeg::probe(&ff, output).unwrap().video_codec, *expected, "{name}");
        }
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn an_uncompressed_rgb_avi_is_written_with_a_sound_avi_can_hold() {
        let Some(ff) = ffmpeg_for_tests() else { return };
        let dir = scratch("rawavi");
        let input = dir.join("clip.avi"); // mpeg4 + AAC
        make_clip(&ff, &input);
        let mut spec = effects_spec(&input, &dir, 1, Size::original(), EffectsEntry::none());
        spec.preset.container = Container::Avi;
        spec.preset.video = VideoCodec::Raw;
        let runner = Runner::new();
        runner.set_ffmpeg(Some(ff.clone()));
        runner.start(vec![spec], 1);
        let snap = wait(&runner);
        let JobStatus::Done { output } = &snap[0].1 else { panic!("{:?}", snap) };
        assert_eq!(output.extension().unwrap(), "avi");
        let p = ffmpeg::probe(&ff, output).unwrap();
        assert_eq!((p.video_codec.as_str(), p.audio_codec.as_str()), ("rawvideo", "ac3"));
        let text = info_text(&ff, output);
        // AVI stores uncompressed 24-bit RGB in the byte order of a bitmap, which ffmpeg names bgr24.
        assert!(text.contains("rgb24") || text.contains("bgr24"), "{text}");
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn h265_at_a_target_bitrate_in_ten_bit_with_the_owners_x265_parameters_runs() {
        let Some(ff) = ffmpeg_for_tests() else { return };
        if !has_encoder(&ff, "libx265") {
            return;
        }
        let dir = scratch("x265");
        let input = dir.join("clip.avi");
        make_clip(&ff, &input);
        let mut spec = effects_spec(&input, &dir, 1, Size::original(), EffectsEntry::none());
        spec.preset.container = Container::Mkv;
        spec.preset.video = VideoCodec::H265;
        spec.preset.rate = crate::preset::RateMode::Bitrate;
        spec.preset.bitrate_kbps = 500;
        spec.preset.ten_bit = true;
        spec.preset.x265_params = "min-keyint=23:keyint=250:bframes=8:b-adapt=2:b-pyramid:bframe-bias=0:rc-lookahead=80:lookahead-slices=4:scenecut=40".into();
        let runner = Runner::new();
        runner.set_ffmpeg(Some(ff.clone()));
        runner.start(vec![spec], 1);
        let snap = wait(&runner);
        let JobStatus::Done { output } = &snap[0].1 else { panic!("{:?}", snap) };
        let text = info_text(&ff, output);
        assert!(text.contains("hevc (Main 10)") && text.contains("yuv420p10le"), "{text}");
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn ac3_is_written_and_mixed_down_to_two_channels() {
        let Some(ff) = ffmpeg_for_tests() else { return };
        let dir = scratch("ac3");
        let input = dir.join("clip.avi"); // mono sound
        make_clip(&ff, &input);
        let mut preset = Preset::new("t");
        preset.video = VideoCodec::Copy;
        let ac3 = AudioEntry { mode: Audio::Ac3, kbps: 256, channels: Some(2), ..AudioEntry::new("ac3") };
        let runner = Runner::new();
        runner.set_ffmpeg(Some(ff.clone()));
        runner.start(
            vec![JobSpec {
                id: 1,
                input,
                source_root: None,
                preset,
                folder: None,
                overwrite: Overwrite::Overwrite,
                size: Size::original(),
                audio: ac3,
                lut: Lut::none(),
                effects: EffectsEntry::none(),
                trim: Trim::none(),
            }],
            1,
        );
        let snap = wait(&runner);
        let JobStatus::Done { output } = &snap[0].1 else { panic!("{:?}", snap) };
        assert_eq!(ffmpeg::probe(&ff, output).unwrap().audio_codec, "ac3");
        let info = ffmpeg::command(&ff).args(["-hide_banner", "-i"]).arg(output).output().unwrap();
        let text = String::from_utf8_lossy(&info.stderr).into_owned();
        assert!(text.contains("Audio: ac3") && text.contains("stereo") && text.contains("256 kb/s"), "{text}");
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn stream_copy_changes_the_container_only() {
        let Some(ff) = ffmpeg_for_tests() else { return };
        let dir = scratch("copy");
        let input = dir.join("clip.mp4");
        make_clip(&ff, &input);
        let mut preset = Preset::new("t");
        preset.container = Container::Mkv;
        preset.video = VideoCodec::Copy;
        let runner = Runner::new();
        runner.set_ffmpeg(Some(ff.clone()));
        runner.start(
            vec![JobSpec { id: 1, input, source_root: None, preset, folder: None, overwrite: Overwrite::Skip, size: Size::original(), audio: AudioEntry::original(), lut: Lut::none(), effects: EffectsEntry::none(), trim: Trim::none() }],
            1,
        );
        let snap = wait(&runner);
        let JobStatus::Done { output } = &snap[0].1 else { panic!("{:?}", snap) };
        assert_eq!(ffmpeg::probe(&ff, output).unwrap().video_codec, "mpeg4");
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn an_existing_output_is_skipped_and_a_bad_file_fails() {
        let Some(ff) = ffmpeg_for_tests() else { return };
        let dir = scratch("skip");
        let input = dir.join("clip.avi");
        make_clip(&ff, &input);
        fs::write(dir.join("clip.mkv"), b"keep me").unwrap();
        let bad = dir.join("bad.avi");
        fs::write(&bad, b"this is not a video").unwrap();
        let mut preset = Preset::new("t");
        preset.video = VideoCodec::Copy;
        let runner = Runner::new();
        runner.set_ffmpeg(Some(ff));
        let spec = |id, input: PathBuf| JobSpec {
            id,
            input,
            source_root: None,
            preset: preset.clone(),
            folder: None,
            overwrite: Overwrite::Skip,
            size: Size::original(),
            audio: AudioEntry::original(),
            lut: Lut::none(),
            effects: EffectsEntry::none(),
            trim: Trim::none(),
        };
        runner.start(vec![spec(1, input), spec(2, bad)], 2);
        let snap = wait(&runner);
        assert!(matches!(snap[0].1, JobStatus::Skipped { .. }), "{snap:?}");
        assert!(matches!(snap[1].1, JobStatus::Failed { .. }), "{snap:?}");
        assert_eq!(fs::read(dir.join("clip.mkv")).unwrap(), b"keep me");
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn a_queue_without_ffmpeg_fails_its_jobs_plainly() {
        let runner = Runner::new();
        let spec = JobSpec {
            id: 1,
            input: PathBuf::from("x.avi"),
            source_root: None,
            preset: Preset::new("t"),
            folder: None,
            overwrite: Overwrite::Skip,
            size: Size::original(),
            audio: AudioEntry::original(),
            lut: Lut::none(),
            effects: EffectsEntry::none(),
            trim: Trim::none(),
        };
        runner.start(vec![spec], 1);
        let snap = wait(&runner);
        assert!(matches!(snap[0].1, JobStatus::Failed { .. }), "{snap:?}");
    }
}
