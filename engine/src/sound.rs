//! The sound of the preview: the same file, from the same moment, decoded by a second ffmpeg and played on the
//! machine's default output. The device pulls the samples at its own pace, so the sound never runs ahead of itself;
//! the picture is paced by the file's own clock, and both start together.

use crate::{
    ffmpeg::command,
    preview::{self, Spec},
};
use rodio::{ChannelCount, DeviceSinkBuilder, MixerDeviceSink, SampleRate, Source};
use std::{
    io::Read,
    path::Path,
    process::{Child, Stdio},
    sync::{
        Arc,
        atomic::{AtomicU32, Ordering},
        mpsc::{Receiver, RecvTimeoutError, sync_channel},
    },
    thread,
    time::Duration,
};

/// Samples decoded and not yet played, a few seconds at most: ffmpeg waits when the device is behind.
const QUEUED_CHUNKS: usize = 24;

/// The samples ffmpeg made, read by the device; the volume is read at every sample so a change is heard at once.
struct Pcm {
    chunks: Receiver<Vec<f32>>,
    now: std::vec::IntoIter<f32>,
    channels: ChannelCount,
    rate: SampleRate,
    volume: Arc<AtomicU32>,
}

impl Iterator for Pcm {
    type Item = f32;

    fn next(&mut self) -> Option<f32> {
        loop {
            if let Some(s) = self.now.next() {
                return Some(s * f32::from_bits(self.volume.load(Ordering::Relaxed)));
            }
            match self.chunks.recv_timeout(Duration::from_millis(20)) {
                Ok(next) => self.now = next.into_iter(),
                // ffmpeg is slower than the device for a moment: silence, not a stop.
                Err(RecvTimeoutError::Timeout) => return Some(0.0),
                Err(RecvTimeoutError::Disconnected) => return None,
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

/// A sound playing; stopped when dropped.
pub struct Sound {
    child: Child,
    volume: Arc<AtomicU32>,
    _device: MixerDeviceSink,
}

impl Sound {
    /// The sound of `spec` from its start, at `volume` (0 is silent, 1 is the file's own level).
    pub fn start(ffmpeg: &Path, spec: &Spec, volume: f32) -> Result<Sound, String> {
        let mut device = DeviceSinkBuilder::open_default_sink().map_err(|e| format!("no sound output: {e}"))?;
        device.log_on_drop(false);
        let (channels, rate) = (device.config().channel_count(), device.config().sample_rate());
        let mut child = command(ffmpeg)
            .args(preview::sound_args(spec, channels.get(), rate.get())?)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| format!("cannot run ffmpeg: {e}"))?;
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
        device.mixer().add(Pcm { chunks, now: Vec::new().into_iter(), channels, rate, volume: volume.clone() });
        Ok(Sound { child, volume, _device: device })
    }

    pub fn set_volume(&self, volume: f32) {
        self.volume.store(volume.max(0.0).to_bits(), Ordering::Relaxed);
    }
}

impl Drop for Sound {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
