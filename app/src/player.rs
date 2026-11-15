//! The player of the converted files: a row's play icon opens it on that file, and when a file ends the next converted one of the
//! same list follows. What is drawn is the catalog's `UisPlayer`; this file holds the playing (the sound, the picture, the position).
//! The same pane as Media Downloader's `player.rs`: a change to one is made to the other.

use crate::{
    catalog::{UisPlayer, UisPlayerLabels},
    i18n, settings,
};
use dioxus::prelude::*;
use engine::{
    picture::{self, Channel, Stream},
    player::Sound,
};
use std::{path::PathBuf, rc::Rc, time::Duration};

/// A converted file the player can play.
#[derive(Clone, Debug, PartialEq)]
pub struct Track {
    pub path: PathBuf,
    pub title: String,
}

impl Track {
    /// The converted file at `path`, shown by its file name.
    pub fn of(path: &std::path::Path) -> Track {
        Track { path: path.to_path_buf(), title: path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default() }
    }
}

/// What the rows and the pane share: the file on the player (none = no player), whether it is meant to be playing, and where it is
/// in seconds.
#[derive(Clone, Copy, PartialEq)]
pub struct Player {
    pub track: Signal<Option<Track>>,
    pub playing: Signal<bool>,
    pub position: Signal<f64>,
    /// Where the pictures of the playing video go, and the address the pane's image reads them from.
    pub channel: Signal<Rc<Channel>>,
}

impl Player {
    pub fn new() -> Self {
        Self { track: use_signal(|| None), playing: use_signal(|| false), position: use_signal(|| 0.0), channel: use_signal(|| Rc::new(Channel::new())) }
    }

    /// The play icon of a row: the file on the player is paused or resumed, any other file takes the player from its start.
    pub fn play(mut self, track: Track) {
        if self.track.peek().as_ref().is_some_and(|t| t.path == track.path) {
            let now = *self.playing.peek();
            self.playing.set(!now);
        } else {
            self.go_to(track);
        }
    }

    /// Put `track` on the player and start it from its beginning.
    fn go_to(mut self, track: Track) {
        self.position.set(0.0);
        self.track.set(Some(track));
        self.playing.set(true);
    }

    /// A file is on the player (the pane is open).
    pub fn is_open(&self) -> bool {
        self.track.read().is_some()
    }

    /// The address of the picture of the video on the player; None when it holds none (or the picture server could not start).
    fn picture(&self) -> Option<String> {
        self.track.read().as_ref()?;
        self.channel.read().url.clone().ok()
    }

    /// Is this file the one playing now?
    pub fn is_sounding(&self, path: &std::path::Path) -> bool {
        *self.playing.read() && self.track.read().as_ref().is_some_and(|t| t.path == path)
    }
}

/// The file after `current` in `queue`, and the one before it.
fn neighbours(queue: &[Track], current: Option<&Track>) -> (Option<Track>, Option<Track>) {
    let Some(i) = current.and_then(|c| queue.iter().position(|q| q.path == c.path)) else { return (None, None) };
    (i.checked_sub(1).and_then(|p| queue.get(p).cloned()), queue.get(i + 1).cloned())
}

/// `away`: the view shown is not the one the file belongs to. The pane is then not drawn and the file is paused, and it comes back
/// where it was when its own view is shown again. `fill`: the pane takes all the room of its place (the work view, where it stands
/// in the preview's place) instead of a share of the list's.
#[component]
pub fn PlayerPane(player: Player, queue: ReadSignal<Vec<Track>>, lang: String, away: bool, #[props(default)] fill: bool) -> Element {
    let t = |key: &str| i18n::t(&lang, key);
    let Player { mut track, mut playing, mut position, channel } = player;
    let mut sound = use_signal(|| None::<Sound>);
    let mut stream = use_signal(|| None::<Stream>);
    let mut length = use_signal(|| 0.0_f64);
    let mut volume = use_signal(|| 1.0_f32);
    // Bumped by a seek, so the playing starts again from the new place.
    let mut seeked = use_signal(|| 0u32);
    let mut failure = use_signal(String::new);

    // The other view is shown: the file waits, paused, until its own view is back.
    use_effect(use_reactive!(|away| {
        if away && *playing.peek() {
            playing.set(false);
        }
    }));

    // The file changed: its length, as ffmpeg reads it.
    use_effect(move || {
        let t = track();
        let long = t.and_then(|t| engine::ffmpeg::locate(settings::ffmpeg_dir().as_deref()).and_then(|ff| picture::duration(&ff, &t.path)));
        length.set(long.unwrap_or(0.0));
    });

    // What is meant to play against what does: start it from where it is, or stop it where it got to.
    let lang_fx = lang.clone();
    use_effect(move || {
        let (t, want, _) = (track(), playing(), seeked());
        sound.write().take();
        stream.write().take();
        let Some(t) = t else { return };
        let ffmpeg = engine::ffmpeg::locate(settings::ffmpeg_dir().as_deref());
        let at = *position.peek();
        if !want {
            // Paused or stopped: the picture of the moment it is at.
            if let Some(ff) = ffmpeg {
                let hub = channel.peek().hub.clone();
                std::thread::spawn(move || {
                    if let Err(e) = picture::still(&ff, &t.path, at, &hub) {
                        engine::log::error("player picture", &format!("{e}\ninput: {}", t.path.display()));
                    }
                });
            }
            return;
        }
        let Some(ff) = ffmpeg else {
            failure.set(i18n::t(&lang_fx, "msgStgFfmpegNone"));
            playing.set(false);
            return;
        };
        // The picture first, then the sound: both read the file from the same moment. A video with no sound plays its picture alone.
        let pictures = Stream::start(&ff, &t.path, at, channel.peek().hub.clone()).ok();
        match Sound::start(&ff, &t.path, at, *volume.peek()) {
            Ok(s) => {
                failure.set(String::new());
                sound.set(Some(s));
                stream.set(pictures);
            }
            Err(_) if pictures.is_some() => {
                failure.set(String::new());
                stream.set(pictures);
            }
            Err(e) => {
                engine::log::error("player", &format!("{e}\ninput: {}", t.path.display()));
                failure.set(e);
                playing.set(false);
            }
        }
    });

    // Follow the playing: the position, and the end of the file (which hands the player to the next one).
    use_future(move || async move {
        loop {
            tokio::time::sleep(Duration::from_millis(200)).await;
            // A video is over when its picture is; its sound may be shorter, or missing.
            let pictures = stream.peek().as_ref().map(Stream::finished);
            let state = match (sound.peek().as_ref().map(|s| (s.position(), s.finished())), pictures) {
                (Some((p, ended)), Some(over)) => Some((p, over && ended)),
                (None, Some(over)) => Some((*position.peek(), over)),
                (heard, None) => heard,
            };
            match state {
                Some((p, false)) => position.set(p),
                Some((_, true)) => {
                    let (_, next) = neighbours(&queue.peek(), track.peek().as_ref());
                    match next {
                        Some(n) => player.go_to(n),
                        None => {
                            position.set(0.0);
                            playing.set(false);
                        }
                    }
                }
                None => {}
            }
        }
    });

    let (prev, next) = neighbours(&queue.read(), track.read().as_ref());
    let labels = UisPlayerLabels {
        play: t("tipWorkPlay"),
        pause: t("tipWorkPause"),
        stop: t("tipMainStop"),
        previous: t("tipMainPrevious"),
        next: t("tipMainNext"),
        volume: t("tipWorkVolume"),
        close: t("tipMainPlayerClose"),
    };
    let title = track.read().as_ref().map(|t| t.title.clone()).unwrap_or_default();
    let (has_prev, has_next) = (prev.is_some(), next.is_some());
    let picture = player.picture();
    let box_class = match (away || !player.is_open(), fill) {
        (true, _) => "out-player out-player-away",
        (false, true) => "out-player out-player-fill",
        (false, false) => "out-player out-player-video",
    };

    // The box is the app's (how tall, its padding and border); what is in it is the catalog's player.
    rsx! {
        div { class: box_class,
            UisPlayer {
                title,
                error: failure(),
                picture,
                position: position(),
                length: length(),
                volume: volume(),
                playing: playing(),
                has_prev,
                has_next,
                labels,
                on_seek: move |v: f64| {
                    position.set(v);
                    seeked += 1;
                },
                on_toggle: move |_| {
                    if let Some(s) = sound.peek().as_ref() {
                        position.set(s.position());
                    }
                    let now = *playing.peek();
                    playing.set(!now);
                },
                on_stop: move |_| {
                    position.set(0.0);
                    playing.set(false);
                },
                on_prev: move |_| {
                    if let Some(p) = prev.clone() {
                        player.go_to(p);
                    }
                },
                on_next: move |_| {
                    if let Some(n) = next.clone() {
                        player.go_to(n);
                    }
                },
                on_volume: move |v: f32| {
                    volume.set(v);
                    if let Some(s) = sound.peek().as_ref() {
                        s.set_volume(v);
                    }
                },
                on_close: move |_| {
                    playing.set(false);
                    track.set(None);
                },
            }
        }
    }
}
