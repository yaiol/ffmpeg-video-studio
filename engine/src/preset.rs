//! A preset: what the video becomes. The ffmpeg arguments it produces are built in `ffmpeg::build_args`.
//!
//! Every field has one meaning in every container, and `normalized` repairs the combinations a
//! container cannot hold (VP9 in an MP4, Opus in a MOV, ...), so a hand-edited settings file can
//! never reach ffmpeg as a command it refuses.

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Container {
    Mkv,
    Mp4,
    Webm,
    Mov,
    Avi,
}

impl Container {
    pub const ALL: [Container; 5] = [Container::Mkv, Container::Mp4, Container::Webm, Container::Mov, Container::Avi];

    pub fn extension(self) -> &'static str {
        match self {
            Container::Mkv => "mkv",
            Container::Mp4 => "mp4",
            Container::Webm => "webm",
            Container::Mov => "mov",
            Container::Avi => "avi",
        }
    }

    /// The video codecs this container holds, the preferred one first (what `normalized` falls back to).
    pub fn video_codecs(self) -> &'static [VideoCodec] {
        match self {
            Container::Mkv => &[
                VideoCodec::H264,
                VideoCodec::H265,
                VideoCodec::H264Amd,
                VideoCodec::H265Amd,
                VideoCodec::H264Nvenc,
                VideoCodec::H265Nvenc,
                VideoCodec::H264Qsv,
                VideoCodec::H265Qsv,
                VideoCodec::Av1,
                VideoCodec::Vp9,
                VideoCodec::ProRes,
                VideoCodec::Raw,
                VideoCodec::Copy,
            ],
            Container::Mp4 => &[
                VideoCodec::H264,
                VideoCodec::H265,
                VideoCodec::H264Amd,
                VideoCodec::H265Amd,
                VideoCodec::H264Nvenc,
                VideoCodec::H265Nvenc,
                VideoCodec::H264Qsv,
                VideoCodec::H265Qsv,
                VideoCodec::Av1,
                VideoCodec::Copy,
            ],
            Container::Mov => &[
                VideoCodec::H264,
                VideoCodec::H265,
                VideoCodec::H264Amd,
                VideoCodec::H265Amd,
                VideoCodec::H264Nvenc,
                VideoCodec::H265Nvenc,
                VideoCodec::H264Qsv,
                VideoCodec::H265Qsv,
                VideoCodec::ProRes,
                VideoCodec::Av1,
                VideoCodec::Copy,
            ],
            Container::Webm => &[VideoCodec::Vp9, VideoCodec::Av1, VideoCodec::Copy],
            Container::Avi => &[VideoCodec::H264, VideoCodec::Raw, VideoCodec::Copy],
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum VideoCodec {
    /// libx264 - needs a GPL ffmpeg build.
    H264,
    /// libx265 - needs a GPL ffmpeg build.
    H265,
    /// `h264_amf`: the AMD graphics card's own H.264 encoder. Needs the card and its driver.
    H264Amd,
    /// `hevc_amf`: the AMD graphics card's own H.265 encoder.
    H265Amd,
    /// `h264_nvenc` / `hevc_nvenc`: the NVIDIA graphics card's own encoders.
    H264Nvenc,
    H265Nvenc,
    /// `h264_qsv` / `hevc_qsv`: the Intel graphics' Quick Sync encoders.
    H264Qsv,
    H265Qsv,
    /// SVT-AV1.
    Av1,
    Vp9,
    /// Uncompressed 24-bit RGB pictures (`rawvideo`): enormous files, no loss, nothing to decode.
    Raw,
    /// Apple ProRes through ffmpeg's own `prores_ks` - an editing format, not a delivery one: big
    /// files, cheap to decode. No GPL part, so any ffmpeg build has it.
    ProRes,
    /// The video stream is copied, not re-encoded.
    Copy,
}

impl VideoCodec {
    pub const ALL: [VideoCodec; 13] = [
        VideoCodec::H264,
        VideoCodec::H265,
        VideoCodec::H264Amd,
        VideoCodec::H265Amd,
        VideoCodec::H264Nvenc,
        VideoCodec::H265Nvenc,
        VideoCodec::H264Qsv,
        VideoCodec::H265Qsv,
        VideoCodec::Av1,
        VideoCodec::Vp9,
        VideoCodec::ProRes,
        VideoCodec::Raw,
        VideoCodec::Copy,
    ];

    /// Whether the codec can be steered by a target bitrate as well as by a quality number.
    pub fn has_rate_choice(self) -> bool {
        matches!(self, VideoCodec::H264 | VideoCodec::H265 | VideoCodec::Av1 | VideoCodec::Vp9)
    }

    /// The tunes the codec has. x264 has two more than x265 (`film`, `stillimage`).
    pub fn tunes(self) -> &'static [Tune] {
        match self {
            VideoCodec::H264 => &[Tune::Film, Tune::Animation, Tune::Grain, Tune::StillImage, Tune::FastDecode, Tune::ZeroLatency],
            VideoCodec::H265 => &[Tune::Animation, Tune::Grain, Tune::FastDecode, Tune::ZeroLatency],
            _ => &[],
        }
    }

    /// Whether the codec can write 10-bit pictures here. x264's depends on how the ffmpeg was built, so it
    /// is left out.
    pub fn supports_ten_bit(self) -> bool {
        matches!(self, VideoCodec::H265 | VideoCodec::Av1)
    }

    /// Whether the codec is steered by one quality number (a CRF; the AMD encoders take it as a
    /// constant quantizer). ProRes is steered by its profile, a copy by nothing.
    pub fn has_crf(self) -> bool {
        !matches!(self, VideoCodec::ProRes | VideoCodec::Raw | VideoCodec::Copy)
    }

    /// Whether the codec has an encoding speed to choose. The AMD encoders have a quality preset of
    /// their own whose names differ between ffmpeg versions, so none is set: the card's default stands.
    pub fn has_speed(self) -> bool {
        self.has_crf() && !self.is_amd()
    }

    /// The encoding speeds the codec offers, fastest first. Only x264 and x265 have an `ultrafast`.
    pub fn speeds(self) -> &'static [Speed] {
        match self {
            VideoCodec::H264 | VideoCodec::H265 => &[Speed::Ultrafast, Speed::Fast, Speed::Balanced, Speed::Slow],
            VideoCodec::Av1 | VideoCodec::Vp9 => &[Speed::Fast, Speed::Balanced, Speed::Slow],
            VideoCodec::H264Nvenc | VideoCodec::H265Nvenc | VideoCodec::H264Qsv | VideoCodec::H265Qsv => {
                &[Speed::Fast, Speed::Balanced, Speed::Slow]
            }
            VideoCodec::H264Amd | VideoCodec::H265Amd | VideoCodec::ProRes | VideoCodec::Raw | VideoCodec::Copy => &[],
        }
    }

    /// The encoders that run on a graphics card, whose maker the window names in a hint.
    pub fn graphics_maker(self) -> Option<&'static str> {
        match self {
            VideoCodec::H264Amd | VideoCodec::H265Amd => Some("amd"),
            VideoCodec::H264Nvenc | VideoCodec::H265Nvenc => Some("nvidia"),
            VideoCodec::H264Qsv | VideoCodec::H265Qsv => Some("intel"),
            _ => None,
        }
    }

    /// The codec as a family, whichever engine encodes it: a graphics-card encoder is its software codec's family (`h264_nvenc` is H.264).
    pub fn family(self) -> VideoCodec {
        match self {
            VideoCodec::H264Amd | VideoCodec::H264Nvenc | VideoCodec::H264Qsv => VideoCodec::H264,
            VideoCodec::H265Amd | VideoCodec::H265Nvenc | VideoCodec::H265Qsv => VideoCodec::H265,
            other => other,
        }
    }

    /// The hardware encoders of AMD graphics cards.
    pub fn is_amd(self) -> bool {
        matches!(self, VideoCodec::H264Amd | VideoCodec::H265Amd)
    }

    /// The quality scale `crf` is on: (lowest = best, highest = worst, a sensible default).
    pub fn crf_range(self) -> (u32, u32, u32) {
        match self {
            VideoCodec::H264 => (0, 51, 20),
            VideoCodec::H265 => (0, 51, 23),
            VideoCodec::H264Amd | VideoCodec::H265Amd => (0, 51, 20),
            // NVENC's constant quality and Quick Sync's global quality: 0 means "automatic" to both.
            VideoCodec::H264Nvenc | VideoCodec::H265Nvenc | VideoCodec::H264Qsv | VideoCodec::H265Qsv => (1, 51, 23),
            VideoCodec::Av1 => (0, 63, 30),
            VideoCodec::Vp9 => (0, 63, 31),
            VideoCodec::ProRes | VideoCodec::Raw | VideoCodec::Copy => (0, 0, 0),
        }
    }
}

/// Which engine encodes the codec families that have a graphics-card version (H.264 and H.265): the one Settings names, for every preset
/// that does not ask for software.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Encoder {
    /// x264 / x265: the same result on every machine.
    #[default]
    Software,
    /// The first graphics card that runs it (NVIDIA, then AMD, then Intel); software when none does.
    Auto,
    Nvidia,
    Amd,
    Intel,
}

impl Encoder {
    pub const ALL: [Encoder; 5] = [Encoder::Software, Encoder::Auto, Encoder::Nvidia, Encoder::Amd, Encoder::Intel];

    /// The makers whose cards this choice tries, in order (`VideoCodec::graphics_maker`'s names).
    pub fn makers(self) -> &'static [&'static str] {
        match self {
            Encoder::Software => &[],
            Encoder::Auto => &["nvidia", "amd", "intel"],
            Encoder::Nvidia => &["nvidia"],
            Encoder::Amd => &["amd"],
            Encoder::Intel => &["intel"],
        }
    }
}

/// What a preset says about the engine.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PresetEncoder {
    /// The engine Settings names.
    #[default]
    Default,
    /// Always software, whatever Settings names (archival work, where the quality per megabyte matters most).
    Software,
}

/// x264's and x265's `-tune`: what the encoder is told the pictures are like.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Tune {
    #[default]
    None,
    Film,
    Animation,
    Grain,
    StillImage,
    FastDecode,
    ZeroLatency,
}

impl Tune {
    /// The name ffmpeg's `-tune` takes; None for no tune.
    pub fn ffmpeg_name(self) -> Option<&'static str> {
        match self {
            Tune::None => Option::None,
            Tune::Film => Some("film"),
            Tune::Animation => Some("animation"),
            Tune::Grain => Some("grain"),
            Tune::StillImage => Some("stillimage"),
            Tune::FastDecode => Some("fastdecode"),
            Tune::ZeroLatency => Some("zerolatency"),
        }
    }
}

/// What steers the encoder: a quality number or a target bitrate.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RateMode {
    Crf,
    Bitrate,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Speed {
    /// x264 / x265's `ultrafast`: the largest files for the least work. Only those two have it.
    Ultrafast,
    Fast,
    Balanced,
    Slow,
}

/// ProRes profiles, lightest first. 422 Proxy .. 422 HQ are 4:2:2; 4444 is 4:4:4.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ProResProfile {
    Proxy,
    Lt,
    Standard,
    Hq,
    #[serde(rename = "4444")]
    Fourk,
}

impl ProResProfile {
    pub const ALL: [ProResProfile; 5] =
        [ProResProfile::Proxy, ProResProfile::Lt, ProResProfile::Standard, ProResProfile::Hq, ProResProfile::Fourk];

    /// The name ffmpeg's `-profile:v` takes.
    pub fn ffmpeg_name(self) -> &'static str {
        match self {
            ProResProfile::Proxy => "proxy",
            ProResProfile::Lt => "lt",
            ProResProfile::Standard => "standard",
            ProResProfile::Hq => "hq",
            ProResProfile::Fourk => "4444",
        }
    }

    /// The 10-bit pixel format the profile needs.
    pub fn pixel_format(self) -> &'static str {
        if self == ProResProfile::Fourk { "yuv444p10le" } else { "yuv422p10le" }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Preset {
    pub id: String,
    pub name: String,
    pub container: Container,
    pub video: VideoCodec,
    /// Which engine encodes it (only H.264 and H.265 have a choice).
    pub encoder: PresetEncoder,
    /// Constant rate factor; lower is better and larger. Unused for stream copy.
    pub crf: u32,
    /// Quality number or target bitrate, for the codecs that offer the choice (`has_rate_choice`).
    pub rate: RateMode,
    /// The target bitrate in kbit/s, when `rate` is `Bitrate`.
    pub bitrate_kbps: u32,
    /// 10-bit pictures, for the codecs that can (`supports_ten_bit`).
    pub ten_bit: bool,
    /// Extra x265 parameters, `key=value` pairs joined by colons, passed as they are. Only for H.265.
    pub x265_params: String,
    pub speed: Speed,
    /// Only for `VideoCodec::ProRes`.
    pub prores: ProResProfile,
    /// Only for x264 and x265 (`VideoCodec::tunes`).
    pub tune: Tune,
}

impl Default for Preset {
    fn default() -> Self {
        Preset {
            id: String::new(),
            name: String::new(),
            container: Container::Mkv,
            video: VideoCodec::H264,
            encoder: PresetEncoder::Default,
            crf: 20,
            rate: RateMode::Crf,
            bitrate_kbps: 2000,
            ten_bit: false,
            x265_params: String::new(),
            speed: Speed::Balanced,
            prores: ProResProfile::Standard,
            tune: Tune::None,
        }
    }
}

/// The ids of the presets a fresh install held, one per graphics-card encoder, before the engine became a setting. A settings file that
/// still has them loses them (they are the families' presets with a different engine, which Settings now chooses).
pub const RETIRED_HARDWARE_IDS: [&str; 6] =
    ["default-h264-amd", "default-h265-amd", "default-h264-nvenc", "default-h265-nvenc", "default-h264-qsv", "default-h265-qsv"];

/// A short unique id for a new preset: time plus a counter, no dependency for it.
pub fn new_id() -> String {
    use std::sync::atomic::{AtomicU32, Ordering};
    static N: AtomicU32 = AtomicU32::new(0);
    let ms = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_millis());
    format!("p{ms:x}{:x}", N.fetch_add(1, Ordering::Relaxed))
}

impl Preset {
    pub fn new(name: &str) -> Preset {
        Preset { id: new_id(), name: name.to_owned(), ..Preset::default() }
    }

    /// A preset saved before the engine was a setting named a graphics-card encoder as its codec; it is the codec's family now.
    pub fn into_family(mut self) -> Preset {
        self.video = self.video.family();
        self
    }

    /// The same preset with every field repaired to what its container accepts.
    pub fn normalized(mut self) -> Preset {
        if !self.container.video_codecs().contains(&self.video) {
            self.video = self.container.video_codecs()[0];
        }
        // A speed the codec does not have (ultrafast after a switch to AV1) becomes the nearest it has.
        if self.video.has_speed() && !self.video.speeds().contains(&self.speed) {
            self.speed = Speed::Fast;
        }
        let (lo, hi, _) = self.video.crf_range();
        self.crf = self.crf.clamp(lo, hi);
        if !self.video.has_rate_choice() {
            self.rate = RateMode::Crf;
        }
        self.bitrate_kbps = self.bitrate_kbps.clamp(50, 200_000);
        if !self.video.tunes().contains(&self.tune) {
            self.tune = Tune::None;
        }
        if !self.video.supports_ten_bit() {
            self.ten_bit = false;
        }
        self.x265_params = self.x265_params.trim().trim_matches('"').trim().to_owned();
        self
    }
}

/// What a fresh install starts with, with stable ids so a saved selection survives. The user's own
/// list (the ones that proved themselves in FFmpeg Batch AV Converter) in the form this app holds.
pub fn defaults() -> Vec<Preset> {
    let mk = |id: &str, name: &str, f: &dyn Fn(&mut Preset)| {
        let mut p = Preset { id: id.to_owned(), name: name.to_owned(), ..Preset::default() };
        f(&mut p);
        p.normalized()
    };
    vec![
        mk("default-h264-hq", "H.264 high quality (MKV)", &|_| {}),
        mk("default-h264-ultrafast", "H.264 ultrafast (MKV)", &|p| {
            p.crf = 23;
            p.speed = Speed::Ultrafast;
        }),
        mk("default-h265-hq", "H.265 high quality (MKV)", &|p| {
            p.video = VideoCodec::H265;
            p.crf = 23;
        }),
        mk("default-h264-mp4", "H.264 (MP4)", &|p| {
            p.container = Container::Mp4;
            p.crf = 23;
        }),
        mk("default-h265-10bit", "H.265 10-bit, 2000 kbps (MKV)", &|p| {
            p.video = VideoCodec::H265;
            p.rate = RateMode::Bitrate;
            p.bitrate_kbps = 2000;
            p.ten_bit = true;
            p.x265_params = "min-keyint=23:keyint=250:bframes=8:b-adapt=2:b-pyramid:bframe-bias=0:rc-lookahead=80:lookahead-slices=4:scenecut=40".to_owned();
        }),
        mk("default-raw-avi", "Uncompressed RGB (AVI)", &|p| {
            p.container = Container::Avi;
            p.video = VideoCodec::Raw;
        }),
        mk("default-av1", "AV1 (MKV)", &|p| {
            p.video = VideoCodec::Av1;
            p.crf = VideoCodec::Av1.crf_range().2;
        }),
        mk("default-prores-422", "ProRes 422 (MOV)", &|p| {
            p.container = Container::Mov;
            p.video = VideoCodec::ProRes;
        }),
        mk("default-prores-proxy", "ProRes Proxy (MOV)", &|p| {
            p.container = Container::Mov;
            p.video = VideoCodec::ProRes;
            p.prores = ProResProfile::Proxy;
        }),
        mk("default-copy-mp4", "Stream copy to MP4", &|p| {
            p.container = Container::Mp4;
            p.video = VideoCodec::Copy;
        }),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn webm_gets_a_webm_codec() {
        let mut p = Preset::new("t");
        p.container = Container::Webm;
        p.video = VideoCodec::H264;
        assert_eq!(p.normalized().video, VideoCodec::Vp9);
    }

    #[test]
    fn mp4_gets_no_vp9_and_no_prores() {
        let mut p = Preset::new("t");
        p.container = Container::Mp4;
        p.video = VideoCodec::Vp9;
        assert_eq!(p.clone().normalized().video, VideoCodec::H264);
        p.video = VideoCodec::ProRes;
        assert_eq!(p.normalized().video, VideoCodec::H264);
    }

    #[test]
    fn every_offered_codec_survives_normalizing() {
        for c in Container::ALL {
            for v in c.video_codecs() {
                let n = Preset { container: c, video: *v, ..Preset::default() }.normalized();
                assert_eq!(n.video, *v, "{c:?}");
            }
        }
    }

    #[test]
    fn ultrafast_belongs_to_x264_and_x265_only() {
        let mut p = Preset { speed: Speed::Ultrafast, ..Preset::new("t") };
        assert_eq!(p.clone().normalized().speed, Speed::Ultrafast);
        p.video = VideoCodec::H265;
        assert_eq!(p.clone().normalized().speed, Speed::Ultrafast);
        p.video = VideoCodec::Av1;
        assert_eq!(p.clone().normalized().speed, Speed::Fast);
        p.video = VideoCodec::Vp9;
        assert_eq!(p.normalized().speed, Speed::Fast);
    }

    #[test]
    fn the_codecs_with_a_speed_list_it_and_the_others_list_none() {
        for c in VideoCodec::ALL {
            assert_eq!(c.has_speed(), !c.speeds().is_empty(), "{c:?}");
        }
    }

    #[test]
    fn a_bitrate_ten_bit_and_x265_parameters_belong_to_the_codecs_that_have_them() {
        let mut p = Preset { rate: RateMode::Bitrate, ten_bit: true, x265_params: " \"keyint=250\" ".into(), ..Preset::new("t") };
        p.video = VideoCodec::H265;
        let n = p.clone().normalized();
        assert_eq!((n.rate, n.ten_bit, n.x265_params.as_str()), (RateMode::Bitrate, true, "keyint=250"));
        // H.264 has the bitrate choice but no 10-bit here.
        p.video = VideoCodec::H264;
        let n = p.clone().normalized();
        assert_eq!((n.rate, n.ten_bit), (RateMode::Bitrate, false));
        // ProRes, a raw picture and the AMD encoders are steered by something else.
        for codec in [VideoCodec::ProRes, VideoCodec::Raw, VideoCodec::H264Amd] {
            p.video = codec;
            p.container = if codec == VideoCodec::Raw { Container::Avi } else { Container::Mov };
            assert_eq!(p.clone().normalized().rate, RateMode::Crf, "{codec:?}");
        }
        assert_eq!(Preset { bitrate_kbps: 1, ..Preset::new("t") }.normalized().bitrate_kbps, 50);
    }

    #[test]
    fn a_raw_picture_lives_in_avi_and_matroska_only() {
        for c in Container::ALL {
            assert_eq!(c.video_codecs().contains(&VideoCodec::Raw), matches!(c, Container::Avi | Container::Mkv), "{c:?}");
        }
        let n = Preset { container: Container::Mp4, video: VideoCodec::Raw, ..Preset::new("t") }.normalized();
        assert_eq!(n.video, VideoCodec::H264);
    }

    #[test]
    fn a_tune_belongs_to_the_codecs_that_have_it() {
        let mut p = Preset { tune: Tune::StillImage, ..Preset::new("t") };
        assert_eq!(p.clone().normalized().tune, Tune::StillImage);
        // x265 has no `stillimage`, and AV1 no tunes at all.
        p.video = VideoCodec::H265;
        assert_eq!(p.clone().normalized().tune, Tune::None);
        p.tune = Tune::Grain;
        assert_eq!(p.clone().normalized().tune, Tune::Grain);
        p.video = VideoCodec::Av1;
        assert_eq!(p.normalized().tune, Tune::None);
    }

    #[test]
    fn the_crf_is_clamped_to_its_scale() {
        let mut p = Preset::new("t");
        p.crf = 99;
        assert_eq!(p.normalized().crf, 51);
    }

    #[test]
    fn every_default_preset_is_already_normal() {
        for p in defaults() {
            assert_eq!(p.clone(), p.clone().normalized(), "{}", p.name);
        }
        assert_eq!(defaults().iter().map(|p| &p.id).collect::<std::collections::HashSet<_>>().len(), defaults().len());
    }
}
