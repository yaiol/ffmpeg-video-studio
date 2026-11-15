//! Playing a converted file (the player of the finished rows; a COPY of Media Downloader's `player.rs` without its length
//! reader - `picture::duration` asks ffmpeg here; a change to one is made to the other): ffmpeg decodes it to raw samples and the sound card takes them as it needs them (the same way the converters'
//! preview plays), so every format ffmpeg reads is heard (an opus file included) without a decoder of our own. A seek starts a new `Sound`
//! from the new place.

use crate::ffmpeg::command;
use rodio::{ChannelCount, DeviceSinkBuilder, MixerDeviceSink, SampleRate, Source};
use std::{
    io::Read,
    path::Path,
    process::{Child, Stdio},
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering},
        mpsc::{Receiver, RecvTimeoutError, sync_channel},
    },
    thread,
    time::Duration,
};

/// Samples decoded and not yet played, a few seconds at most: ffmpeg waits when the device is behind.
const QUEUED_CHUNKS: usize = 24;

/// A sound playing; stopped when dropped.
pub struct Sound {
    child: Child,
    volume: Arc<AtomicU32>,
    played: Arc<AtomicU64>,
    done: Arc<AtomicBool>,
    /// Where in the file it started, in seconds.
    from: f64,
    /// Samples a second, all channels together.
    per_second: f64,
    _device: MixerDeviceSink,
}

impl Sound {
    /// The file from `at` seconds, at `volume` (0 is silent, 1 is the file's own level).
    pub fn start(ffmpeg: &Path, file: &Path, at: f64, volume: f32) -> Result<Sound, String> {
        let mut device = DeviceSinkBuilder::open_default_sink().map_err(|e| format!("no sound output: {e}"))?;
        device.log_on_drop(false);
        let (channels, rate) = (device.config().channel_count(), device.config().sample_rate());
        let mut cmd = command(ffmpeg);
        cmd.args(["-hide_banner", "-nostdin", "-loglevel", "error"]);
        if at > 0.0 {
            cmd.args(["-ss", &format!("{at:.3}")]);
        }
        cmd.arg("-i").arg(file);
        cmd.args(["-map", "0:a:0", "-vn", "-f", "f32le", "-ac", &channels.get().to_string(), "-ar", &rate.get().to_string(), "-"]);
        let mut child = cmd.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::null()).spawn().map_err(|e| format!("cannot run ffmpeg: {e}"))?;
        let mut out = child.stdout.take().ok_or("no output pipe")?;
        let (tx, chunks) = sync_channel::<Vec<f32>>(QUEUED_CHUNKS);
        thread::spawn(move || {
            // 4-byte samples can be split across two reads: the loose bytes wait for the next one.
            let (mut buf, mut loose) = (vec![0u8; 16 * 1024], Vec::<u8>::new());
            while let Ok(n) = out.read(&mut buf) {
                if n == 0 {
                    break;
                }
                loose.extend_from_slice(&buf[..n]);
                let whole = loose.len() / 4 * 4;
                let samples: Vec<f32> = loose[..whole].chunks_exact(4).map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]])).collect();
                loose.drain(..whole);
                if tx.send(samples).is_err() {
                    break;
                }
            }
        });
        let volume = Arc::new(AtomicU32::new(volume.max(0.0).to_bits()));
        let (played, done) = (Arc::new(AtomicU64::new(0)), Arc::new(AtomicBool::new(false)));
        device.mixer().add(Pcm { chunks, now: Vec::new().into_iter(), channels, rate, volume: volume.clone(), played: played.clone(), done: done.clone() });
        Ok(Sound { child, volume, played, done, from: at, per_second: f64::from(channels.get()) * f64::from(rate.get()), _device: device })
    }

    pub fn set_volume(&self, volume: f32) {
        self.volume.store(volume.max(0.0).to_bits(), Ordering::Relaxed);
    }

    /// Where the playing has got to in the file, in seconds.
    pub fn position(&self) -> f64 {
        self.from + self.played.load(Ordering::Relaxed) as f64 / self.per_second
    }

    /// Everything has been played.
    pub fn finished(&self) -> bool {
        self.done.load(Ordering::Relaxed)
    }
}

impl Drop for Sound {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// The samples ffmpeg made, read by the device; the volume is read at every sample so a change is heard at once.
struct Pcm {
    chunks: Receiver<Vec<f32>>,
    now: std::vec::IntoIter<f32>,
    channels: ChannelCount,
    rate: SampleRate,
    volume: Arc<AtomicU32>,
    played: Arc<AtomicU64>,
    done: Arc<AtomicBool>,
}

impl Iterator for Pcm {
    type Item = f32;

    fn next(&mut self) -> Option<f32> {
        loop {
            if let Some(s) = self.now.next() {
                self.played.fetch_add(1, Ordering::Relaxed);
                return Some(s * f32::from_bits(self.volume.load(Ordering::Relaxed)));
            }
            match self.chunks.recv_timeout(Duration::from_millis(20)) {
                Ok(next) => self.now = next.into_iter(),
                // ffmpeg is slower than the device for a moment: silence, not a stop (and not counted as played).
                Err(RecvTimeoutError::Timeout) => return Some(0.0),
                Err(RecvTimeoutError::Disconnected) => {
                    self.done.store(true, Ordering::Relaxed);
                    return None;
                }
            }
        }
    }
}

impl Source for Pcm {
    fn current_span_len(&self) -> Option<usize> {
        None
    }
    fn channels(&self) -> ChannelCount {
        self.channels
    }
    fn sample_rate(&self) -> SampleRate {
        self.rate
    }
    fn total_duration(&self) -> Option<Duration> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Needs ffmpeg (`VIDS_FFMPEG_DIR`) and a sound output: a two-second tone is measured, played, and seeked into.
    #[test]
    #[ignore = "needs ffmpeg and a sound output"]
    fn a_file_is_measured_and_played_from_where_it_is_told() {
        let dir = std::env::var_os("VIDS_FFMPEG_DIR").map(std::path::PathBuf::from);
        let ff = crate::ffmpeg::locate(dir.as_deref()).expect("ffmpeg");
        let file = std::env::temp_dir().join(format!("vids-player-{}.mp3", std::process::id()));
        let made = command(&ff).args(["-y", "-hide_banner", "-loglevel", "error", "-f", "lavfi", "-i", "sine=f=440:d=2", "-c:a", "libmp3lame"]).arg(&file).status().unwrap();
        assert!(made.success());
        let long = crate::picture::duration(&ff, &file).expect("a length");
        assert!((long - 2.0).abs() < 0.2, "{long}");
        let sound = Sound::start(&ff, &file, 1.0, 0.0).unwrap();
        thread::sleep(Duration::from_millis(600));
        let at = sound.position();
        assert!(at > 1.2 && at < 2.2, "it started at 1 s and moved on: {at}");
        thread::sleep(Duration::from_millis(1500));
        assert!(sound.finished(), "the end is noticed");
        drop(sound);
        let _ = std::fs::remove_file(&file);
    }
}
