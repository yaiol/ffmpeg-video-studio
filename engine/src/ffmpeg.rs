//! Everything that touches the ffmpeg executable: finding it, asking it about a file, building the
//! command line, and reading its progress. ffmpeg is a separate program, never linked, NOT shipped
//! with the app, and never the machine's own: the build yaiol approved, downloaded into the folder every
//! yaiol app shares.

use crate::{
    audio::{Audio, Resolved},
    effects::{self, Filter, Look},
    metadata::{Meta, Outputs, SourceTags, StreamKind, StreamTags},
    preset::{Container, Preset, RateMode, Speed, VideoCodec},
};
use std::{
    ffi::OsString,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

pub fn executable_name() -> &'static str {
    if cfg!(windows) { "ffmpeg.exe" } else { "ffmpeg" }
}

/// The ffmpeg to use: the one the app installed into `dir`, and no other. PATH is never
/// searched, and a program in `dir` that no install recorded (one put there by hand) is not a build we know.
pub fn locate(dir: Option<&Path>) -> Option<PathBuf> {
    let dir = dir?;
    crate::download::installed_version(dir)?;
    Some(dir.join(executable_name()))
}

/// A `Command` that does not flash a console window on Windows.
pub fn command(ffmpeg: &Path) -> Command {
    #[allow(unused_mut)]
    let mut c = Command::new(ffmpeg);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        c.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    c
}

/// What ffmpeg's `-version` banner says about the build: it may not be a non-free one (we neither
/// download nor knowingly run those). `None` when the program does not answer.
pub fn is_nonfree(ffmpeg: &Path) -> Option<bool> {
    let out = command(ffmpeg).arg("-version").stdin(Stdio::null()).output().ok()?;
    if !out.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&out.stdout).contains("--enable-nonfree"))
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Probe {
    pub duration_us: u64,
    /// The first (real) video stream's codec as ffmpeg names it (`h264`, `hevc`...); empty when none.
    pub video_codec: String,
    pub width: u32,
    pub height: u32,
    pub fps: Option<f32>,
    /// The first audio stream's codec; empty when none.
    pub audio_codec: String,
    /// The names of the tags the file carries, on the file and on each stream.
    pub tags: SourceTags,
}

fn parse_duration(s: &str) -> Option<u64> {
    // 00:03:41.23
    let mut it = s.trim().split(':');
    let h: u64 = it.next()?.parse().ok()?;
    let m: u64 = it.next()?.parse().ok()?;
    let sec: f64 = it.next()?.parse().ok()?;
    Some((h * 3600 + m * 60) * 1_000_000 + (sec * 1_000_000.0) as u64)
}

/// `1920x1080` out of a video stream line's comma-separated parts. A hex tag such as
/// `0x31637661` has a one-digit left side and is not a size.
fn parse_size(part: &str) -> Option<(u32, u32)> {
    let token = part.split_whitespace().next()?;
    let (w, h) = token.split_once('x')?;
    if w.len() < 2 || !w.bytes().all(|b| b.is_ascii_digit()) || !h.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    Some((w.parse().ok()?, h.parse().ok()?))
}

/// The names of the tags in the text `ffmpeg -i <file>` prints: under the file's own `Metadata:` (two spaces in) and under
/// each stream's (four). The tags of a chapter are not the file's and are skipped. A stream is counted among those of
/// its kind, in the file's order, which is how `-map 0:v:0` and `-metadata:s:a:1` address it.
pub fn parse_tags(stderr: &str) -> SourceTags {
    enum Scope {
        Nowhere,
        Container,
        /// A stream (index into `streams`; None for a kind that has no tags we address) before or after its tags.
        Stream(Option<usize>),
        StreamTags(usize),
        Chapters,
        Chapter(usize),
        ChapterTags(usize),
    }
    let mut tags = SourceTags::default();
    let mut counts = [0usize; 3];
    let mut scope = Scope::Nowhere;
    for raw in stderr.lines() {
        let indent = raw.len() - raw.trim_start().len();
        let line = raw.trim();
        // A tag line is `key : value`; a value that runs on has a line of its own that starts with the colon.
        let key = || line.split(':').next().map(str::trim).filter(|k| !k.is_empty());
        match indent {
            2 => {
                scope = if line == "Metadata:" {
                    Scope::Container
                } else if line == "Chapters:" {
                    Scope::Chapters
                } else if line.starts_with("Stream #") {
                    let kind = if line.contains(" Video: ") {
                        Some(StreamKind::Video)
                    } else if line.contains(" Audio: ") {
                        Some(StreamKind::Audio)
                    } else if line.contains(" Subtitle: ") {
                        Some(StreamKind::Subtitle)
                    } else {
                        None
                    };
                    Scope::Stream(kind.map(|kind| {
                        let slot = &mut counts[kind as usize];
                        tags.streams.push(StreamTags { kind, ordinal: *slot, keys: Vec::new() });
                        *slot += 1;
                        tags.streams.len() - 1
                    }))
                } else {
                    Scope::Nowhere
                };
            }
            4 => match scope {
                Scope::Container => {
                    if let Some(k) = key() {
                        tags.container.push(k.to_owned());
                    }
                }
                Scope::Stream(Some(i)) | Scope::StreamTags(i) if line == "Metadata:" => scope = Scope::StreamTags(i),
                Scope::StreamTags(i) if line.ends_with(':') && !line.contains(" : ") => scope = Scope::Stream(Some(i)),
                Scope::Chapters | Scope::Chapter(_) | Scope::ChapterTags(_) if line.starts_with("Chapter #") => {
                    tags.chapters.push(Vec::new());
                    scope = Scope::Chapter(tags.chapters.len() - 1);
                }
                _ => {}
            },
            6 => match scope {
                Scope::StreamTags(i) => {
                    if let Some(k) = key() {
                        tags.streams[i].keys.push(k.to_owned());
                    }
                }
                Scope::Chapter(i) | Scope::ChapterTags(i) if line == "Metadata:" => scope = Scope::ChapterTags(i),
                _ => {}
            },
            8 => {
                if let Scope::ChapterTags(i) = scope {
                    if let Some(k) = key() {
                        tags.chapters[i].push(k.to_owned());
                    }
                }
            }
            _ => {}
        }
    }
    tags
}

/// Read the duration and the first video and audio streams out of the text `ffmpeg -i <file>` prints.
pub fn parse_probe(stderr: &str) -> Probe {
    let mut probe = Probe { tags: parse_tags(stderr), ..Probe::default() };
    for line in stderr.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("Duration:") {
            if let Some(d) = rest.split(',').next().and_then(parse_duration) {
                probe.duration_us = d;
            }
        } else if line.starts_with("Stream #") {
            if let Some((_, after)) = line.split_once(" Video: ") {
                // A cover picture is a video stream to ffmpeg; it is not the video.
                if probe.video_codec.is_empty() && !line.contains("(attached pic)") {
                    probe.video_codec = after.split([' ', ',']).next().unwrap_or("").to_owned();
                    // The parts split at commas that sit inside parentheses too (`yuv420p(tv, bt709)`),
                    // which is harmless: only a part that reads as a size or `N fps` is used.
                    for part in after.split(',').map(str::trim) {
                        if probe.width == 0 {
                            if let Some((w, h)) = parse_size(part) {
                                probe.width = w;
                                probe.height = h;
                            }
                        }
                        if let Some(n) = part.strip_suffix(" fps") {
                            probe.fps = n.trim().parse().ok();
                        }
                    }
                }
            } else if let Some((_, after)) = line.split_once(" Audio: ") {
                if probe.audio_codec.is_empty() {
                    probe.audio_codec = after.split([' ', ',']).next().unwrap_or("").to_owned();
                }
            }
        }
    }
    probe
}

/// Ask ffmpeg about a file (a run with no output exits non-zero by design; only the text matters).
pub fn probe(ffmpeg: &Path, input: &Path) -> Result<Probe, String> {
    let out = command(ffmpeg)
        .args(["-hide_banner", "-nostdin", "-i"])
        .arg(input)
        .stdin(Stdio::null())
        .output()
        .map_err(|e| format!("cannot run ffmpeg: {e}"))?;
    let text = String::from_utf8_lossy(&out.stderr).into_owned();
    let p = parse_probe(&text);
    if p.video_codec.is_empty() {
        let why = text
            .lines()
            .rev()
            .map(str::trim)
            .find(|l| l.contains("Invalid data") || l.contains("No such file") || l.contains("Error opening"))
            .unwrap_or("no video stream ffmpeg can read");
        return Err(why.to_owned());
    }
    Ok(p)
}

/// `out_time_us=` (microseconds written so far) from ffmpeg's `-progress` output.
pub fn parse_progress_line(line: &str) -> Option<u64> {
    line.trim().strip_prefix("out_time_us=")?.parse().ok()
}

fn video_args(preset: &Preset, filter: &Filter, args: &mut Vec<OsString>) {
    let mut push = |items: &[&str]| args.extend(items.iter().map(OsString::from));
    let crf = preset.crf.to_string();
    // What steers the encoder: a quality number or a target bitrate.
    let (rate_flag, rate_value) = match preset.rate {
        RateMode::Crf => ("-crf", crf.clone()),
        RateMode::Bitrate => ("-b:v", format!("{}k", preset.bitrate_kbps)),
    };
    match preset.video {
        VideoCodec::Copy => push(&["-c:v", "copy"]),
        VideoCodec::Raw => push(&["-c:v", "rawvideo", "-pix_fmt", "rgb24"]),
        VideoCodec::ProRes => push(&[
            "-c:v",
            "prores_ks",
            "-profile:v",
            preset.prores.ffmpeg_name(),
            "-vendor:v",
            "ap10",
            "-pix_fmt",
            preset.prores.pixel_format(),
        ]),
        VideoCodec::H264 => {
            let speed = match preset.speed {
                Speed::Ultrafast => "ultrafast",
                Speed::Fast => "veryfast",
                Speed::Balanced => "medium",
                Speed::Slow => "slow",
            };
            // yuv420p: a 4:4:4 or 4:2:2 source would otherwise give a High 4:4:4 file nothing plays.
            push(&["-c:v", "libx264", rate_flag, &rate_value, "-preset", speed, "-pix_fmt", "yuv420p"]);
            if let Some(tune) = preset.tune.ffmpeg_name() {
                push(&["-tune", tune]);
            }
        }
        VideoCodec::H265 => {
            let speed = match preset.speed {
                Speed::Ultrafast => "ultrafast",
                Speed::Fast => "veryfast",
                Speed::Balanced => "medium",
                Speed::Slow => "slow",
            };
            push(&["-c:v", "libx265", rate_flag, &rate_value, "-preset", speed]);
            if let Some(tune) = preset.tune.ffmpeg_name() {
                push(&["-tune", tune]);
            }
            if preset.ten_bit {
                push(&["-profile:v", "main10", "-pix_fmt", "yuv420p10le"]);
            }
            if !preset.x265_params.is_empty() {
                push(&["-x265-params", &preset.x265_params]);
            }
            if preset.container == Container::Mp4 || preset.container == Container::Mov {
                push(&["-tag:v", "hvc1"]); // what Apple players and Windows' own player accept
            }
        }
        VideoCodec::H264Amd | VideoCodec::H265Amd => {
            // Constant quantizer, as the AMD presets of FFmpeg Batch have it. Only the H.264 encoder has a
            // B-frame quantizer (hevc_amf has none, and ffmpeg refuses an option it does not know).
            let h264 = preset.video == VideoCodec::H264Amd;
            push(&["-c:v", if h264 { "h264_amf" } else { "hevc_amf" }, "-rc", "cqp", "-qp_i", &crf, "-qp_p", &crf]);
            if h264 {
                push(&["-qp_b", &crf, "-pix_fmt", "yuv420p"]);
            } else if preset.container == Container::Mp4 || preset.container == Container::Mov {
                push(&["-tag:v", "hvc1"]);
            }
        }
        VideoCodec::H264Nvenc | VideoCodec::H265Nvenc => {
            // Constant quality in NVENC's variable-bitrate mode (`-b:v 0` lets the quality rule); p1 is the
            // fastest preset and p7 the slowest.
            let speed = match preset.speed {
                Speed::Fast | Speed::Ultrafast => "p2",
                Speed::Balanced => "p4",
                Speed::Slow => "p6",
            };
            let h264 = preset.video == VideoCodec::H264Nvenc;
            push(&["-c:v", if h264 { "h264_nvenc" } else { "hevc_nvenc" }, "-rc", "vbr", "-cq", &crf, "-b:v", "0", "-preset", speed]);
            if h264 {
                push(&["-pix_fmt", "yuv420p"]);
            } else if preset.container == Container::Mp4 || preset.container == Container::Mov {
                push(&["-tag:v", "hvc1"]);
            }
        }
        VideoCodec::H264Qsv | VideoCodec::H265Qsv => {
            let speed = match preset.speed {
                Speed::Fast | Speed::Ultrafast => "veryfast",
                Speed::Balanced => "medium",
                Speed::Slow => "slow",
            };
            let h264 = preset.video == VideoCodec::H264Qsv;
            push(&["-c:v", if h264 { "h264_qsv" } else { "hevc_qsv" }, "-global_quality", &crf, "-preset", speed, "-pix_fmt", "nv12"]);
            if !h264 && (preset.container == Container::Mp4 || preset.container == Container::Mov) {
                push(&["-tag:v", "hvc1"]);
            }
        }
        VideoCodec::Av1 => {
            let speed = match preset.speed {
                // `normalized` leaves Ultrafast only on x264 / x265.
                Speed::Fast | Speed::Ultrafast => "10",
                Speed::Balanced => "6",
                Speed::Slow => "4",
            };
            push(&["-c:v", "libsvtav1", rate_flag, &rate_value, "-preset", speed]);
            if preset.ten_bit {
                push(&["-pix_fmt", "yuv420p10le"]);
            }
        }
        VideoCodec::Vp9 => {
            let speed = match preset.speed {
                Speed::Fast | Speed::Ultrafast => "5",
                Speed::Balanced => "2",
                Speed::Slow => "1",
            };
            // A quality number needs the bitrate set to 0; a target bitrate is the bitrate.
            let mut v = vec!["-c:v", "libvpx-vp9", rate_flag, rate_value.as_str()];
            if preset.rate == RateMode::Crf {
                v.extend(["-b:v", "0"]);
            }
            v.extend(["-row-mt", "1", "-cpu-used", speed]);
            push(&v);
        }
    }
    // The colour, the turn, the size, then the overlays: one filter chain, or a filter graph when a
    // second input (a watermark) is laid over the picture. See `effects::plan`.
    match filter {
        Filter::None => {}
        Filter::Simple(chain) => push(&["-vf", chain]),
        Filter::Complex(graph) => push(&["-filter_complex", graph]),
    }
}

/// The full argument list for one conversion. `normalized` is applied here, so a hand-edited
/// settings file can never produce a combination the container refuses.
pub fn build_args(input: &Path, output: &Path, preset: &Preset, look: &Look, audio: Resolved, meta: &Meta) -> Vec<OsString> {
    let preset = preset.clone().normalized();
    // A copied stream can be neither recoloured, turned, rescaled nor overlaid.
    let copied = preset.video == VideoCodec::Copy;
    let plan = if copied { effects::Plan::none() } else { effects::plan(look, input) };
    // Burned-in subtitles replace the subtitle track: the output carries no other.
    let burned = !copied && plan.burns;
    let mut args: Vec<OsString> = Vec::new();
    let mut push = |items: &[&str]| args.extend(items.iter().map(OsString::from));
    push(&["-hide_banner", "-nostdin", "-y", "-progress", "pipe:1", "-nostats"]);
    // The start of a cut is given before the input (a fast seek) - unless the subtitles are burned in:
    // their filter reads the file on the source's own timeline, so the cut is made after the input there.
    let start = look.trim.start_s.map(|s| format!("{s:.3}"));
    if let Some(start) = start.as_deref().filter(|_| !burned) {
        push(&["-ss", start]);
    }
    push(&["-i"]);
    args.push(input.into());
    for extra in &plan.inputs {
        args.push("-i".into());
        args.push(extra.into());
    }

    let mut push = |items: &[&str]| args.extend(items.iter().map(OsString::from));
    // The cut's start after the input (burned subtitles) and its length. A copied stream needs its
    // timestamps moved to zero, or the cut can start with a negative time and some players stall.
    if let Some(start) = start.as_deref().filter(|_| burned) {
        push(&["-ss", start]);
    }
    if let Some(length) = look.trim.length_s() {
        push(&["-t", &format!("{length:.3}")]);
    }
    if copied && !look.trim.is_none() {
        push(&["-avoid_negative_ts", "make_zero"]);
    }
    // The video is the first input's first video stream, or what the filter graph produces.
    match plan.filter {
        Filter::Complex(_) => push(&["-map", "[outv]"]),
        _ => push(&["-map", "0:v:0"]),
    }
    if audio.mode != Audio::None {
        push(&["-map", "0:a?"]);
    }
    // Text subtitles survive only in Matroska (an MP4 would need conversion, and a picture-based one
    // cannot be converted at all), so no other container is offered them.
    if preset.container == Container::Mkv && !burned {
        push(&["-map", "0:s?", "-c:s", "copy"]);
    }
    // The file's tags and chapters are carried over minus what the metadata setting removes (which is no filter, so
    // it works on a copied video too).
    let outputs = Outputs { audio: audio.mode != Audio::None, subtitles: preset.container == Container::Mkv && !burned };
    args.extend(meta.cleanup.arguments(meta.source, outputs).into_iter().map(OsString::from));

    video_args(&preset, &plan.filter, &mut args);

    let mut push = |items: &[&str]| args.extend(items.iter().map(OsString::from));
    match audio.mode {
        Audio::None => push(&["-an"]),
        Audio::Copy => push(&["-c:a", "copy"]),
        Audio::Aac => push(&["-c:a", "aac", "-b:a", &format!("{}k", audio.kbps)]),
        Audio::Opus => push(&["-c:a", "libopus", "-b:a", &format!("{}k", audio.kbps)]),
        Audio::Ac3 => push(&["-c:a", "ac3", "-b:a", &format!("{}k", audio.kbps)]),
        Audio::Pcm => push(&["-c:a", "pcm_s16le"]),
    }
    // Mixing down is asked of a mode that re-encodes: a copy cannot change the channels.
    if let Some(n) = audio.channels.filter(|_| audio.mode.encodes()) {
        push(&["-ac", &n.to_string()]);
    }
    // The sound's own filters: its level and loudness (of any mode that re-encodes), then the effect that
    // plays it backwards (which only applies when the video is not just copied). ffmpeg refuses a filter
    // with a copy, which is why the caller never leaves a copy where one is asked.
    if audio.mode.encodes() {
        let mut filters: Vec<String> = Vec::new();
        if audio.gain_db != 0 {
            filters.push(format!("volume={}dB", audio.gain_db));
        }
        if audio.loudness {
            filters.push("loudnorm=I=-16:TP=-1.5:LRA=11".to_owned());
        }
        filters.extend(plan.audio.clone());
        if !filters.is_empty() {
            push(&["-af", &filters.join(",")]);
        }
    }
    if preset.container == Container::Mp4 || preset.container == Container::Mov {
        push(&["-movflags", "+faststart"]);
    }
    args.push(output.into());
    args
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{effects::{EffectsEntry, Item}, metadata::MetadataCleanup, size::Size};

    fn entry(items: Vec<Item>) -> EffectsEntry {
        EffectsEntry { items, ..EffectsEntry::new("t") }
    }

    const SAMPLE: &str = "Input #0, matroska,webm, from 'a.mkv':
  Duration: 00:03:41.23, start: 0.000000, bitrate: 1234 kb/s
  Stream #0:0: Video: mjpeg (Baseline), yuvj420p(pc, bt470bg/unknown/unknown), 600x600 [SAR 1:1 DAR 1:1], 90k tbr, 90k tbn (attached pic)
  Stream #0:1(und): Video: h264 (High) (avc1 / 0x31637661), yuv420p(tv, bt709), 1920x1080 [SAR 1:1 DAR 16:9], 4500 kb/s, 29.97 fps, 29.97 tbr, 30k tbn (default)
  Stream #0:2(eng): Audio: aac (LC) (mp4a / 0x6134706D), 48000 Hz, stereo, fltp, 128 kb/s (default)
  Stream #0:3(eng): Subtitle: subrip";

    #[test]
    fn the_probe_reads_the_real_video_not_the_cover_picture() {
        let p = parse_probe(SAMPLE);
        assert_eq!(p.duration_us, 221_230_000);
        assert_eq!((p.video_codec.as_str(), p.width, p.height), ("h264", 1920, 1080));
        assert_eq!(p.fps, Some(29.97));
        assert_eq!(p.audio_codec, "aac");
    }

    #[test]
    fn a_hex_tag_is_not_a_size() {
        assert_eq!(parse_size("0x31637661"), None);
        assert_eq!(parse_size("1280x720 [SAR 1:1]"), Some((1280, 720)));
    }

    #[test]
    fn progress_is_read_in_microseconds() {
        assert_eq!(parse_progress_line("out_time_us=1500000"), Some(1_500_000));
        assert_eq!(parse_progress_line("out_time_us=N/A"), None);
        assert_eq!(parse_progress_line("frame=10"), None);
    }

    fn args_with(preset: &Preset, size: &Size, audio: Resolved) -> String {
        args_look(preset, size, audio, &EffectsEntry::none())
    }

    fn args_look(preset: &Preset, size: &Size, audio: Resolved, effects: &EffectsEntry) -> String {
        let look = Look { size, effects, duration_s: 100.0, trim: crate::trim::Trim::none() };
        let (cleanup, source) = (MetadataCleanup::keep(), SourceTags::default());
        build_args(Path::new("in.mkv"), Path::new("out.mp4"), preset, &look, audio, &Meta { cleanup: &cleanup, source: &source })
            .iter()
            .map(|a| a.to_string_lossy().into_owned())
            .collect::<Vec<_>>()
            .join(" ")
    }

    fn args_of(preset: &Preset, size: &Size) -> String {
        args_with(preset, size, Resolved { mode: Audio::Aac, kbps: 128, channels: None, gain_db: 0, loudness: false })
    }

    #[test]
    fn h264_in_mp4_takes_the_compatible_pixel_format_and_faststart() {
        let mut p = Preset::new("t");
        p.container = Container::Mp4;
        p.video = VideoCodec::H264;
        let a = args_of(&p, &Size::original());
        assert!(a.contains("-c:v libx264 -crf 20"), "{a}");
        assert!(a.contains("-pix_fmt yuv420p"), "{a}");
        assert!(a.contains("-movflags +faststart"), "{a}");
        assert!(!a.contains("-c:s"), "{a}");
    }

    #[test]
    fn a_size_adds_the_shrink_filter_and_the_original_adds_none() {
        let p = Preset::new("t");
        let small = Size { max_height: Some(720), ..Size::new("t") };
        assert!(args_of(&p, &small).contains("-vf scale=w=iw:h='min(ih,720)'"));
        assert!(!args_of(&p, &Size::original()).contains("scale="));
    }

    #[test]
    fn stream_copy_ignores_quality_and_scaling() {
        let mut p = Preset::new("t");
        p.video = VideoCodec::Copy;
        let a = args_of(&p, &Size { max_height: Some(720), ..Size::new("t") });
        assert!(a.contains("-c:v copy") && !a.contains("-crf") && !a.contains("scale="), "{a}");
    }

    #[test]
    fn prores_takes_its_profile_ten_bit_pixels_and_pcm_audio() {
        use crate::preset::ProResProfile;
        let mut p = Preset::new("t");
        p.container = Container::Mov;
        p.video = VideoCodec::ProRes;
        let a = args_with(&p, &Size::original(), Resolved { mode: Audio::Pcm, kbps: 0, channels: None, gain_db: 0, loudness: false });
        assert!(a.contains("-c:v prores_ks -profile:v standard -vendor:v ap10 -pix_fmt yuv422p10le"), "{a}");
        assert!(a.contains("-c:a pcm_s16le"), "{a}");
        assert!(!a.contains("-crf") && !a.contains("-preset"), "{a}");
        p.prores = ProResProfile::Fourk;
        let a = args_with(&p, &Size::original(), Resolved { mode: Audio::Pcm, kbps: 0, channels: None, gain_db: 0, loudness: false });
        assert!(a.contains("-profile:v 4444") && a.contains("yuv444p10le"), "{a}");
    }

    #[test]
    fn a_lut_comes_before_the_size_in_one_filter_chain() {
        let p = Preset::new("t");
        let lut = entry(vec![Item::with("try:lut3d", &[("file", "D:\\Luts\\hlg.cube")])]);
        let size = Size { max_height: Some(720), ..Size::new("s") };
        let audio = Resolved { mode: Audio::Aac, kbps: 128, channels: None, gain_db: 0, loudness: false };
        let a = args_look(&p, &size, audio, &lut);
        let at = a.find("-vf lut3d=").expect(&a);
        assert!(a[at..].contains("hlg.cube") && a[at..].contains(",scale="), "{a}");
        assert_eq!(a.matches("-vf").count(), 1, "{a}");
        // Alone, it is the whole chain; with no LUT and no size there is no filter at all.
        let alone = args_look(&p, &Size::original(), audio, &lut);
        assert!(alone.contains("-vf lut3d=") && !alone.contains("scale="), "{alone}");
        assert!(!args_with(&p, &Size::original(), audio).contains("-vf"));
    }

    #[test]
    fn a_target_bitrate_replaces_the_quality_number_for_every_codec_that_has_the_choice() {
        let size = Size::original();
        let mut p = Preset { rate: RateMode::Bitrate, bitrate_kbps: 2000, ..Preset::new("t") };
        let a = args_of(&p, &size);
        assert!(a.contains("-c:v libx264 -b:v 2000k -preset") && !a.contains("-crf"), "{a}");
        p.video = VideoCodec::Vp9;
        let a = args_of(&p, &size);
        assert!(a.contains("-c:v libvpx-vp9 -b:v 2000k -row-mt") && !a.contains("-b:v 0"), "{a}");
        p.rate = RateMode::Crf;
        assert!(args_of(&p, &size).contains("-crf 20 -b:v 0"));
    }

    #[test]
    fn ten_bit_h265_with_its_own_parameters_matches_the_owners_command() {
        let p = Preset {
            video: VideoCodec::H265,
            rate: RateMode::Bitrate,
            bitrate_kbps: 2000,
            ten_bit: true,
            x265_params: "keyint=250:bframes=8".into(),
            speed: Speed::Balanced,
            ..Preset::new("t")
        };
        let a = args_of(&p, &Size::original());
        assert!(
            a.contains("-c:v libx265 -b:v 2000k -preset medium -profile:v main10 -pix_fmt yuv420p10le -x265-params keyint=250:bframes=8"),
            "{a}"
        );
        // The parameters belong to x265 alone, and 10 bits to the codecs that have them.
        let a = args_of(&Preset { video: VideoCodec::H264, x265_params: "keyint=1".into(), ten_bit: true, ..Preset::new("t") }, &Size::original());
        assert!(!a.contains("x265") && !a.contains("10le"), "{a}");
    }

    #[test]
    fn a_tune_reaches_x264_and_x265_and_none_adds_nothing() {
        use crate::preset::Tune;
        let mut p = Preset { tune: Tune::Animation, ..Preset::new("t") };
        assert!(args_of(&p, &Size::original()).contains("-preset medium -pix_fmt yuv420p -tune animation"));
        p.video = VideoCodec::H265;
        assert!(args_of(&p, &Size::original()).contains("-tune animation"));
        p.tune = Tune::None;
        assert!(!args_of(&p, &Size::original()).contains("-tune"));
    }

    #[test]
    fn a_raw_picture_is_uncompressed_rgb() {
        let p = Preset { container: Container::Avi, video: VideoCodec::Raw, ..Preset::new("t") };
        let a = args_of(&p, &Size::original());
        assert!(a.contains("-c:v rawvideo -pix_fmt rgb24") && !a.contains("-crf") && !a.contains("-preset"), "{a}");
        assert!(!a.contains("-map 0:s?"), "{a}");
    }

    #[test]
    fn the_nvidia_and_intel_encoders_take_a_quality_and_a_speed() {
        let mut p = Preset::new("t");
        p.video = VideoCodec::H264Nvenc;
        p.crf = 23;
        let a = args_of(&p, &Size::original());
        assert!(a.contains("-c:v h264_nvenc -rc vbr -cq 23 -b:v 0 -preset p4 -pix_fmt yuv420p"), "{a}");
        p.speed = Speed::Slow;
        p.video = VideoCodec::H265Nvenc;
        p.container = Container::Mp4;
        let a = args_of(&p, &Size::original());
        assert!(a.contains("-c:v hevc_nvenc") && a.contains("-preset p6") && a.contains("-tag:v hvc1"), "{a}");
        let mut q = Preset::new("q");
        q.video = VideoCodec::H264Qsv;
        q.crf = 25;
        let a = args_of(&q, &Size::original());
        assert!(a.contains("-c:v h264_qsv -global_quality 25 -preset medium -pix_fmt nv12"), "{a}");
        q.video = VideoCodec::H265Qsv;
        q.container = Container::Mov;
        assert!(args_of(&q, &Size::original()).contains("-c:v hevc_qsv") && args_of(&q, &Size::original()).contains("-tag:v hvc1"));
    }

    #[test]
    fn the_amd_encoders_take_a_constant_quantizer_and_no_speed() {
        let mut p = Preset::new("t");
        p.video = VideoCodec::H264Amd;
        let a = args_of(&p, &Size::original());
        assert!(a.contains("-c:v h264_amf -rc cqp -qp_i 20 -qp_p 20 -qp_b 20 -pix_fmt yuv420p"), "{a}");
        assert!(!a.contains("-preset") && !a.contains("-quality"), "{a}");
        p.video = VideoCodec::H265Amd;
        p.container = Container::Mp4;
        let a = args_of(&p, &Size::original());
        assert!(a.contains("-c:v hevc_amf -rc cqp -qp_i 20 -qp_p 20"), "{a}");
        assert!(!a.contains("-qp_b") && a.contains("-tag:v hvc1"), "{a}");
    }

    #[test]
    fn a_turn_is_a_plain_filter_and_a_copied_stream_takes_none() {
        let p = Preset::new("t");
        let audio = Resolved { mode: Audio::Aac, kbps: 128, channels: None, gain_db: 0, loudness: false };
        let turn = entry(vec![Item::with("turn", &[("way", "cw")])]);
        let a = args_look(&p, &Size::original(), audio, &turn);
        assert!(a.contains("-vf transpose=dir=clock") && a.contains("-map 0:v:0") && !a.contains("filter_complex"), "{a}");
        let mut copy = Preset::new("c");
        copy.video = VideoCodec::Copy;
        assert!(!args_look(&copy, &Size::original(), audio, &turn).contains("transpose"));
    }

    #[test]
    fn a_watermark_adds_its_picture_as_a_second_input_and_maps_the_graph() {
        let p = Preset::new("t");
        let audio = Resolved { mode: Audio::Aac, kbps: 128, channels: None, gain_db: 0, loudness: false };
        let wm = entry(vec![Item::with("watermark", &[("file", "D:\\logo.png")])]);
        let a = args_look(&p, &Size::original(), audio, &wm);
        assert!(a.contains("-i in.mkv -i D:\\logo.png "), "{a}");
        assert!(a.contains("-map [outv]") && !a.contains("-map 0:v:0"), "{a}");
        assert!(a.contains("-filter_complex ") && a.contains("overlay") && a.contains("[1:v]"), "{a}");
        // The picture is not an input of a copied stream.
        let mut copy = Preset::new("c");
        copy.video = VideoCodec::Copy;
        assert!(!args_look(&copy, &Size::original(), audio, &wm).contains("logo.png"));
    }

    #[test]
    fn playing_backwards_filters_the_sound_too_and_burning_drops_the_subtitle_track() {
        let mut p = Preset::new("t");
        p.container = Container::Mkv;
        let audio = Resolved { mode: Audio::Aac, kbps: 128, channels: None, gain_db: 0, loudness: false };
        let fx = |key: &str| entry(vec![Item::new(key)]);
        let plain = args_look(&p, &Size::original(), audio, &EffectsEntry::none());
        assert!(plain.contains("-map 0:s? -c:s copy") && !plain.contains("-af"), "{plain}");
        let reversed = args_look(&p, &Size::original(), audio, &fx("play-backwards"));
        assert!(reversed.contains("-vf reverse") && reversed.contains("-af areverse"), "{reversed}");
        let burned = args_look(&p, &Size::original(), audio, &fx("burn-subtitles"));
        assert!(!burned.contains("-map 0:s?") && burned.contains("-vf subtitles=filename="), "{burned}");
        // With the sound copied or absent there is nothing to filter.
        let copy_sound = Resolved { mode: Audio::Copy, kbps: 0, channels: None, gain_db: 0, loudness: false };
        assert!(!args_look(&p, &Size::original(), copy_sound, &fx("play-backwards")).contains("-af"));
    }

    fn args_trim(preset: &Preset, effects: &EffectsEntry, trim: crate::trim::Trim) -> Vec<String> {
        let size = Size::original();
        let look = Look { size: &size, effects, duration_s: 100.0, trim };
        let (cleanup, source) = (MetadataCleanup::keep(), SourceTags::default());
        build_args(Path::new("in.mkv"), Path::new("out.mp4"), preset, &look, Resolved { mode: Audio::Aac, kbps: 128, channels: None, gain_db: 0, loudness: false }, &Meta { cleanup: &cleanup, source: &source })
            .iter()
            .map(|a| a.to_string_lossy().into_owned())
            .collect()
    }

    #[test]
    fn a_cut_seeks_before_the_input_and_asks_for_a_length() {
        use crate::trim::Trim;
        let p = Preset::new("t");
        let a = args_trim(&p, &EffectsEntry::none(), Trim::parse("10", "40").unwrap());
        let (ss, input, t) = (a.iter().position(|x| x == "-ss").unwrap(), a.iter().position(|x| x == "-i").unwrap(), a.iter().position(|x| x == "-t").unwrap());
        assert!(ss < input && input < t, "{a:?}");
        assert_eq!((a[ss + 1].as_str(), a[t + 1].as_str()), ("10.000", "30.000"));
        assert!(!a.iter().any(|x| x == "-avoid_negative_ts"));
        // No cut, no options; only an end is only a length.
        let a = args_trim(&p, &EffectsEntry::none(), Trim::none());
        assert!(!a.iter().any(|x| x == "-ss" || x == "-t"), "{a:?}");
        let a = args_trim(&p, &EffectsEntry::none(), Trim::parse("", "5").unwrap());
        assert!(!a.iter().any(|x| x == "-ss") && a.iter().any(|x| x == "-t"), "{a:?}");
    }

    #[test]
    fn a_cut_with_burned_subtitles_seeks_after_the_input() {
        use crate::trim::Trim;
        let burn = entry(vec![Item::new("burn-subtitles")]);
        let a = args_trim(&Preset::new("t"), &burn, Trim::parse("10", "").unwrap());
        let (ss, input) = (a.iter().position(|x| x == "-ss").unwrap(), a.iter().position(|x| x == "-i").unwrap());
        assert!(input < ss, "{a:?}");
    }

    #[test]
    fn a_copied_stream_that_is_cut_moves_its_timestamps_to_zero() {
        use crate::trim::Trim;
        let mut copy = Preset::new("c");
        copy.video = VideoCodec::Copy;
        let a = args_trim(&copy, &EffectsEntry::none(), Trim::parse("10", "20").unwrap());
        let at = a.iter().position(|x| x == "-avoid_negative_ts").unwrap();
        assert_eq!(a[at + 1], "make_zero");
        assert!(!args_trim(&copy, &EffectsEntry::none(), Trim::none()).iter().any(|x| x == "-avoid_negative_ts"));
    }

    fn args_meta(preset: &Preset, cleanup: &MetadataCleanup, source: &SourceTags) -> String {
        let (size, effects) = (Size::original(), EffectsEntry::none());
        let look = Look { size: &size, effects: &effects, duration_s: 100.0, trim: crate::trim::Trim::none() };
        let audio = Resolved { mode: Audio::Copy, kbps: 0, channels: None, gain_db: 0, loudness: false };
        build_args(Path::new("in.mkv"), Path::new("out.mp4"), preset, &look, audio, &Meta { cleanup, source })
            .iter()
            .map(|a| a.to_string_lossy().into_owned())
            .collect::<Vec<_>>()
            .join(" ")
    }

    #[test]
    fn the_metadata_setting_reaches_the_command_line_and_works_on_a_copied_stream_too() {
        use crate::metadata::Mode;
        let source = SourceTags { container: vec!["title".into(), "location".into()], ..SourceTags::default() };
        let mut copy = Preset::new("c");
        copy.video = VideoCodec::Copy;
        let keep = MetadataCleanup::keep();
        let a = args_meta(&copy, &keep, &source);
        assert!(a.contains("-map_metadata 0") && !a.contains("-map_chapters") && !a.contains("-metadata "), "{a}");
        let all = MetadataCleanup { mode: Mode::Delete, ..MetadataCleanup::new("x") };
        let a = args_meta(&copy, &all, &source);
        assert!(a.contains("-map_metadata -1") && !a.contains("-map_metadata 0"), "{a}");
        let some = MetadataCleanup { exceptions: vec!["location".into()], remove_chapters: true, ..MetadataCleanup::new("x") };
        let a = args_meta(&copy, &some, &source);
        assert!(a.contains("-map_metadata 0 -metadata location= ") && a.contains("-map_chapters -1") && !a.contains("title="), "{a}");
    }

    #[test]
    fn the_tags_of_a_file_are_read_by_name_on_the_file_and_on_each_stream() {
        // What `ffmpeg -i` printed for a phone-style MP4 (kept as it was, tag values included).
        let text = "Input #0, mov,mp4,m4a,3gp,3g2,mj2, from 'src.mp4':
  Metadata:
    major_brand     : isom
    compatible_brands: isomiso2mp41
    creation_time   : 2026-08-01T10:00:00.000000Z
    title           : My Holiday
    comment         : shot on a Pixel
                    : second line of the comment
    location-eng    : +48.8566+002.3522/
  Duration: 00:00:02.00, start: 0.000000, bitrate: 306 kb/s
  Chapters:
    Chapter #0:0: start 0.000000, end 1.000000
      Metadata:
        title           : Intro
  Stream #0:0[0x1](eng): Video: mpeg4 (Simple Profile) (mp4v / 0x7634706D), yuv420p, 160x120, 25 fps
    Metadata:
      creation_time   : 2026-08-01T10:00:00.000000Z
      handler_name    : VideoHandler Pixel
  Stream #0:1[0x2](und): Audio: aac (LC), 44100 Hz, mono, fltp, 69 kb/s
    Metadata:
      handler_name    : SoundHandler
      name            : Original track
  Stream #0:2[0x3](und): Audio: aac (LC), 44100 Hz, mono, fltp, 69 kb/s
    Metadata:
      language        : fra
  Stream #0:3[0x4](eng): Data: bin_data (text / 0x74786574), 0 kb/s
    Metadata:
      handler_name    : SubtitleHandler
";
        let tags = parse_tags(text);
        assert_eq!(tags.container, ["major_brand", "compatible_brands", "creation_time", "title", "comment", "location-eng"]);
        assert_eq!(tags.streams.len(), 3, "a data stream is not addressed: {:?}", tags.streams);
        assert_eq!((tags.streams[0].kind, tags.streams[0].ordinal), (StreamKind::Video, 0));
        assert_eq!(tags.streams[0].keys, ["creation_time", "handler_name"]);
        assert_eq!((tags.streams[1].kind, tags.streams[1].ordinal), (StreamKind::Audio, 0));
        assert_eq!(tags.streams[1].keys, ["handler_name", "name"]);
        assert_eq!((tags.streams[2].kind, tags.streams[2].ordinal), (StreamKind::Audio, 1));
        assert_eq!(tags.streams[2].keys, ["language"]);
        assert_eq!(tags.chapters, vec![vec!["title".to_owned()]], "the chapter's own tags, not the file's");
        assert_eq!(parse_probe(text).tags, tags);
    }

    #[test]
    fn ac3_and_the_channel_mix_reach_the_command_line() {
        let p = Preset::new("t");
        let a = args_with(&p, &Size::original(), Resolved { mode: Audio::Ac3, kbps: 256, channels: Some(2), gain_db: 0, loudness: false });
        assert!(a.contains("-c:a ac3 -b:a 256k") && a.contains("-ac 2"), "{a}");
        // A copy cannot be mixed down, so the count is not passed.
        let a = args_with(&p, &Size::original(), Resolved { mode: Audio::Copy, kbps: 0, channels: Some(2), gain_db: 0, loudness: false });
        assert!(!a.contains("-ac"), "{a}");
    }

    #[test]
    fn ultrafast_reaches_x264_and_x265() {
        let mut p = Preset { speed: Speed::Ultrafast, crf: 23, ..Preset::new("t") };
        let a = args_of(&p, &Size::original());
        assert!(a.contains("-c:v libx264 -crf 23 -preset ultrafast"), "{a}");
        p.video = VideoCodec::H265;
        assert!(args_of(&p, &Size::original()).contains("-c:v libx265 -crf 23 -preset ultrafast"));
    }

    #[test]
    fn a_copied_stream_takes_no_lut() {
        let mut p = Preset::new("t");
        p.video = VideoCodec::Copy;
        let lut = entry(vec![Item::with("try:lut3d", &[("file", "x.cube")])]);
        let a = args_look(&p, &Size::original(), Resolved { mode: Audio::Aac, kbps: 128, channels: None, gain_db: 0, loudness: false }, &lut);
        assert!(!a.contains("lut3d"), "{a}");
    }

    #[test]
    fn the_level_and_the_loudness_become_audio_filters_before_the_reversal() {
        let p = Preset::new("t");
        let loud = Resolved { mode: Audio::Aac, kbps: 128, channels: None, gain_db: -3, loudness: true };
        let a = args_with(&p, &Size::original(), loud);
        assert!(a.contains("-af volume=-3dB,loudnorm=I=-16:TP=-1.5:LRA=11"), "{a}");
        let back = entry(vec![Item::new("play-backwards")]);
        let a = args_look(&p, &Size::original(), loud, &back);
        assert!(a.contains("-af volume=-3dB,loudnorm=I=-16:TP=-1.5:LRA=11,areverse"), "{a}");
        // The sound's own filters work with a copied VIDEO, the reversal does not.
        let mut copy = Preset::new("c");
        copy.video = VideoCodec::Copy;
        let a = args_look(&copy, &Size::original(), loud, &back);
        assert!(a.contains("-af volume=-3dB,loudnorm=I=-16:TP=-1.5:LRA=11 ") && !a.contains("areverse"), "{a}");
        let flat = Resolved { mode: Audio::Aac, kbps: 128, channels: None, gain_db: 0, loudness: false };
        assert!(!args_with(&p, &Size::original(), flat).contains("-af"));
    }

    #[test]
    fn the_audio_decision_decides_the_audio_arguments() {
        let p = Preset::new("t");
        let a = args_with(&p, &Size::original(), Resolved { mode: Audio::Opus, kbps: 96, channels: None, gain_db: 0, loudness: false });
        assert!(a.contains("-map 0:a? ") && a.contains("-c:a libopus -b:a 96k"), "{a}");
        let a = args_with(&p, &Size::original(), Resolved { mode: Audio::Copy, kbps: 0, channels: None, gain_db: 0, loudness: false });
        assert!(a.contains("-c:a copy") && !a.contains("-b:a"), "{a}");
        let a = args_with(&p, &Size::original(), Resolved { mode: Audio::None, kbps: 0, channels: None, gain_db: 0, loudness: false });
        assert!(a.contains("-an") && !a.contains("0:a?"), "{a}");
    }

    #[test]
    fn matroska_keeps_the_subtitles() {
        let mut p = Preset::new("t");
        p.container = Container::Mkv;
        assert!(args_of(&p, &Size::original()).contains("-map 0:s? -c:s copy"));
    }
}
