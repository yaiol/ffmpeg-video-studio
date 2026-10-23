//! Everything that touches the ffmpeg executable: finding it, asking it about a file, building the
//! command line, and reading its progress. ffmpeg is a separate program, never linked, and is NOT
//! shipped with the app: the user's own copy, or the one the app downloads into its own folder.

use crate::{
    audio::{Audio, Resolved},
    effects::{self, Filter, Look},
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

/// First `name` found in a PATH-style variable.
pub fn find_in_path(path_var: &std::ffi::OsStr, name: &str) -> Option<PathBuf> {
    std::env::split_paths(path_var).map(|d| d.join(name)).find(|p| p.is_file())
}

/// The ffmpeg to use: the one beside the executable, then the copy the app downloaded into `own_dir`,
/// then whatever is on PATH.
pub fn locate(own_dir: Option<&Path>) -> Option<PathBuf> {
    let name = executable_name();
    if let Some(dir) = std::env::current_exe().ok().and_then(|e| e.parent().map(Path::to_path_buf)) {
        let beside = dir.join(name);
        if beside.is_file() {
            return Some(beside);
        }
    }
    if let Some(own) = own_dir.map(|d| d.join(name)).filter(|p| p.is_file()) {
        return Some(own);
    }
    std::env::var_os("PATH").and_then(|p| find_in_path(&p, name))
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

/// Read the duration and the first video and audio streams out of the text `ffmpeg -i <file>` prints.
pub fn parse_probe(stderr: &str) -> Probe {
    let mut probe = Probe::default();
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
pub fn build_args(input: &Path, output: &Path, preset: &Preset, look: &Look, audio: Resolved) -> Vec<OsString> {
    let preset = preset.clone().normalized();
    // A copied stream can be neither recoloured, turned, rescaled nor overlaid.
    let copied = preset.video == VideoCodec::Copy;
    let plan = if copied { effects::Plan { inputs: Vec::new(), filter: Filter::None } } else { effects::plan(look, input) };
    // Burned-in subtitles replace the subtitle track: the output carries no other.
    let burned = !copied && look.effects.get(effects::Kind::Burn).is_some();
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
    // The file's tags and chapters are kept unless an effect leaves them out (which works on a copy too).
    if look.effects.strips_metadata() {
        push(&["-map_metadata", "-1", "-map_chapters", "-1"]);
    } else {
        push(&["-map_metadata", "0"]);
    }

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
        filters.extend(look.effects.audio_filter().filter(|_| !copied).map(str::to_owned));
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
    use crate::{effects::EffectsEntry, lut::Lut, size::Size};

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
        args_with_lut(preset, size, audio, &Lut::none())
    }

    fn args_with_lut(preset: &Preset, size: &Size, audio: Resolved, lut: &Lut) -> String {
        args_look(preset, size, audio, lut, &EffectsEntry::none())
    }

    fn args_look(preset: &Preset, size: &Size, audio: Resolved, lut: &Lut, effects: &EffectsEntry) -> String {
        let look = Look { size, lut, effects, duration_s: 100.0, trim: crate::trim::Trim::none() };
        build_args(Path::new("in.mkv"), Path::new("out.mp4"), preset, &look, audio)
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
        let lut = Lut { path: "D:\\Luts\\hlg.cube".into(), ..Lut::new("hlg") };
        let size = Size { max_height: Some(720), ..Size::new("s") };
        let audio = Resolved { mode: Audio::Aac, kbps: 128, channels: None, gain_db: 0, loudness: false };
        let a = args_with_lut(&p, &size, audio, &lut);
        let at = a.find("-vf lut3d=file='D\\:/Luts/hlg.cube',scale=").expect(&a);
        assert!(at > 0, "{a}");
        assert_eq!(a.matches("-vf").count(), 1, "{a}");
        // Alone, it is the whole chain; with no LUT and no size there is no filter at all.
        assert!(args_with_lut(&p, &Size::original(), audio, &lut).contains("-vf lut3d=file='D\\:/Luts/hlg.cube' "));
        assert!(!args_with_lut(&p, &Size::original(), audio, &Lut::none()).contains("-vf"));
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
        use crate::{effects::Effect, rotate::Rotation};
        let p = Preset::new("t");
        let audio = Resolved { mode: Audio::Aac, kbps: 128, channels: None, gain_db: 0, loudness: false };
        let turn = EffectsEntry { items: vec![Effect::Rotate { rotation: Rotation::Cw }], ..EffectsEntry::new("t") };
        let a = args_look(&p, &Size::original(), audio, &Lut::none(), &turn);
        assert!(a.contains("-vf transpose=1") && a.contains("-map 0:v:0") && !a.contains("filter_complex"), "{a}");
        let mut copy = Preset::new("c");
        copy.video = VideoCodec::Copy;
        assert!(!args_look(&copy, &Size::original(), audio, &Lut::none(), &turn).contains("transpose"));
    }

    #[test]
    fn a_watermark_adds_its_picture_as_a_second_input_and_maps_the_graph() {
        use crate::effects::{Corner, Effect, Watermark};
        let p = Preset::new("t");
        let audio = Resolved { mode: Audio::Aac, kbps: 128, channels: None, gain_db: 0, loudness: false };
        let wm = EffectsEntry {
            items: vec![Effect::Watermark(Watermark { path: "D:\\logo.png".into(), corner: Corner::BottomRight, ..Watermark::default() })],
            ..EffectsEntry::new("t")
        };
        let a = args_look(&p, &Size::original(), audio, &Lut::none(), &wm);
        assert!(a.contains("-i in.mkv -i D:\\logo.png "), "{a}");
        assert!(a.contains("-map [outv]") && !a.contains("-map 0:v:0"), "{a}");
        assert!(a.contains("-filter_complex [0:v][1:v]overlay=x=main_w-overlay_w-20:y=main_h-overlay_h-20[outv]"), "{a}");
        // The picture is not an input of a copied stream.
        let mut copy = Preset::new("c");
        copy.video = VideoCodec::Copy;
        assert!(!args_look(&copy, &Size::original(), audio, &Lut::none(), &wm).contains("logo.png"));
    }

    #[test]
    fn playing_backwards_filters_the_sound_too_and_burning_drops_the_subtitle_track() {
        use crate::effects::Effect;
        let mut p = Preset::new("t");
        p.container = Container::Mkv;
        let audio = Resolved { mode: Audio::Aac, kbps: 128, channels: None, gain_db: 0, loudness: false };
        let fx = |items| EffectsEntry { items, ..EffectsEntry::new("t") };
        let plain = args_look(&p, &Size::original(), audio, &Lut::none(), &EffectsEntry::none());
        assert!(plain.contains("-map 0:s? -c:s copy") && !plain.contains("-af"), "{plain}");
        let reversed = args_look(&p, &Size::original(), audio, &Lut::none(), &fx(vec![Effect::Reverse]));
        assert!(reversed.contains("-vf reverse") && reversed.contains("-af areverse"), "{reversed}");
        let burned = args_look(&p, &Size::original(), audio, &Lut::none(), &fx(vec![Effect::Burn]));
        assert!(!burned.contains("-map 0:s?") && burned.contains("-vf subtitles=filename="), "{burned}");
        // With the sound copied or absent there is nothing to filter.
        let copy_sound = Resolved { mode: Audio::Copy, kbps: 0, channels: None, gain_db: 0, loudness: false };
        assert!(!args_look(&p, &Size::original(), copy_sound, &Lut::none(), &fx(vec![Effect::Reverse])).contains("-af"));
    }

    fn args_trim(preset: &Preset, effects: &EffectsEntry, trim: crate::trim::Trim) -> Vec<String> {
        let size = Size::original();
        let look = Look { size: &size, lut: &Lut::none(), effects, duration_s: 100.0, trim };
        build_args(Path::new("in.mkv"), Path::new("out.mp4"), preset, &look, Resolved { mode: Audio::Aac, kbps: 128, channels: None, gain_db: 0, loudness: false })
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
        use crate::{effects::Effect, trim::Trim};
        let burn = EffectsEntry { items: vec![Effect::Burn], ..EffectsEntry::new("t") };
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

    #[test]
    fn removing_the_metadata_works_on_a_copied_stream_too() {
        use crate::effects::Effect;
        let strip = EffectsEntry { items: vec![Effect::Strip], ..EffectsEntry::new("t") };
        let audio = Resolved { mode: Audio::Copy, kbps: 0, channels: None, gain_db: 0, loudness: false };
        let mut copy = Preset::new("c");
        copy.video = VideoCodec::Copy;
        let a = args_look(&copy, &Size::original(), audio, &Lut::none(), &strip);
        assert!(a.contains("-map_metadata -1 -map_chapters -1") && !a.contains("-map_metadata 0"), "{a}");
        let a = args_look(&copy, &Size::original(), audio, &Lut::none(), &EffectsEntry::none());
        assert!(a.contains("-map_metadata 0") && !a.contains("-map_chapters"), "{a}");
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
        let lut = Lut { path: "x.cube".into(), ..Lut::new("l") };
        let a = args_with_lut(&p, &Size::original(), Resolved { mode: Audio::Aac, kbps: 128, channels: None, gain_db: 0, loudness: false }, &lut);
        assert!(!a.contains("lut3d"), "{a}");
    }

    #[test]
    fn the_level_and_the_loudness_become_audio_filters_before_the_reversal() {
        use crate::effects::Effect;
        let p = Preset::new("t");
        let loud = Resolved { mode: Audio::Aac, kbps: 128, channels: None, gain_db: -3, loudness: true };
        let a = args_with(&p, &Size::original(), loud);
        assert!(a.contains("-af volume=-3dB,loudnorm=I=-16:TP=-1.5:LRA=11"), "{a}");
        let back = EffectsEntry { items: vec![Effect::Reverse], ..EffectsEntry::new("t") };
        let a = args_look(&p, &Size::original(), loud, &Lut::none(), &back);
        assert!(a.contains("-af volume=-3dB,loudnorm=I=-16:TP=-1.5:LRA=11,areverse"), "{a}");
        // The sound's own filters work with a copied VIDEO, the reversal does not.
        let mut copy = Preset::new("c");
        copy.video = VideoCodec::Copy;
        let a = args_look(&copy, &Size::original(), loud, &Lut::none(), &back);
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
