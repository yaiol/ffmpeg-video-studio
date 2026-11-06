//! Metadata cleaning: which of a file's tags (and whether its chapters) are left out of the converted file.
//!
//! It is its own named setting and not a part of a preset or of the effects on purpose: a preset says how a file is
//! ENCODED, the effects say what is done to the PICTURE, and a cleaning says what the file CARRIES about itself - the
//! three are chosen independently, and a cleaning works the same on a copied video as on a re-encoded one. Audio
//! Converter's tag cleaning is the model.
//!
//! A tag is named the way ffmpeg names it (`title`, `location`, `creation_time`, `handler_name`...). A name in the
//! list applies wherever it appears: on the file as a whole, on every track and on every chapter alike (a chapter's
//! name is its `title`). The chapters switch removes the chapters themselves. ffmpeg's own `encoder` tag and
//! the container's technical fields (`major_brand`...) are not the user's data, are written again whatever is done,
//! and are never touched.

use serde::{Deserialize, Serialize};

/// The id of the entry a fresh install starts with: "Keep all tags", edited and deleted like any other.
pub const STANDARD_ID: &str = "default-keep";

/// The id of "nothing is removed" as a value, for code that has no entry to hand (tests).
pub const KEEP_ID: &str = "keep";

/// What happens to the tags that are NOT in the list.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    /// Keep every tag; the list names the ones to remove.
    #[default]
    Keep,
    /// Remove every tag; the list names the ones to keep.
    Delete,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct MetadataCleanup {
    pub id: String,
    pub name: String,
    pub mode: Mode,
    /// The names the mode is about: one per entry, case-insensitive, `*` is a wildcard. Stored as typed (a `;` also
    /// separates names); `normalized` tidies it for use.
    pub exceptions: Vec<String>,
    /// Leave the chapters out. A switch of its own: chapters are not tags.
    pub remove_chapters: bool,
}

impl Default for MetadataCleanup {
    fn default() -> Self {
        MetadataCleanup::keep()
    }
}

/// The fields that are the container's own and not the user's: ffmpeg writes them again whatever is asked, so they are
/// never named, dropped or offered. (`DURATION` and the `NUMBER_OF_*` / `BPS` / `_STATISTICS_*` family are what a
/// Matroska muxer writes about its own output.)
fn is_technical(key: &str) -> bool {
    const TECHNICAL: [&str; 11] = [
        "encoder",
        "major_brand",
        "minor_version",
        "compatible_brands",
        "duration",
        "number_of_frames",
        "number_of_bytes",
        "bps",
        "_statistics_writing_app",
        "_statistics_writing_date_utc",
        "_statistics_tags",
    ];
    TECHNICAL.iter().any(|t| t.eq_ignore_ascii_case(key))
}

/// Does `name` match `pattern`? Case-insensitive; `*` stands for any run of characters.
pub fn name_matches(pattern: &str, name: &str) -> bool {
    let p: Vec<char> = pattern.trim().to_lowercase().chars().collect();
    let n: Vec<char> = name.to_lowercase().chars().collect();
    // The classic wildcard walk with one backtrack point.
    let (mut pi, mut ni, mut star, mut mark) = (0usize, 0usize, None::<usize>, 0usize);
    while ni < n.len() {
        if pi < p.len() && p[pi] == '*' {
            star = Some(pi);
            mark = ni;
            pi += 1;
        } else if pi < p.len() && p[pi] == n[ni] {
            pi += 1;
            ni += 1;
        } else if let Some(s) = star {
            pi = s + 1;
            mark += 1;
            ni = mark;
        } else {
            return false;
        }
    }
    while pi < p.len() && p[pi] == '*' {
        pi += 1;
    }
    pi == p.len()
}

impl MetadataCleanup {
    /// The entry a fresh install starts with: keeps everything, an ordinary entry the user may edit.
    pub fn standard(name: &str) -> Self {
        MetadataCleanup { id: STANDARD_ID.to_owned(), ..MetadataCleanup::new(name) }
    }

    /// "Nothing is removed" as a value, for code that has no entry to hand.
    pub fn keep() -> Self {
        MetadataCleanup { id: KEEP_ID.to_owned(), ..MetadataCleanup::new("") }
    }

    pub fn new(name: &str) -> Self {
        MetadataCleanup { id: crate::preset::new_id(), name: name.to_owned(), mode: Mode::Keep, exceptions: Vec::new(), remove_chapters: false }
    }

    /// True when it would change nothing.
    pub fn removes_nothing(&self) -> bool {
        !self.remove_chapters && !self.edits_tags()
    }

    /// True when some tag is dropped: every one (`Delete`) or at least one named (`Keep`).
    pub fn edits_tags(&self) -> bool {
        self.mode == Mode::Delete || self.exceptions.iter().flat_map(|e| e.split(';')).any(|t| !t.trim().is_empty())
    }

    fn listed(&self, key: &str) -> bool {
        self.exceptions.iter().flat_map(|e| e.split(';')).map(str::trim).filter(|p| !p.is_empty()).any(|p| name_matches(p, key))
    }

    /// Is the tag `key` dropped? In `Keep` mode when the list names it, in `Delete` mode unless the list does. The
    /// container's own technical fields never are.
    pub fn drops(&self, key: &str) -> bool {
        if is_technical(key) {
            return false;
        }
        match self.mode {
            Mode::Keep => self.listed(key),
            Mode::Delete => !self.listed(key),
        }
    }

    /// The list as it is USED: trimmed, blanks and repeats dropped, the user's order and spelling kept. A `;`
    /// separates names the same way a line break does. It is applied when a job runs and never to what is stored, so
    /// the box shows again what was typed, `;` included.
    pub fn normalized(mut self) -> Self {
        let mut seen: Vec<String> = Vec::new();
        for t in std::mem::take(&mut self.exceptions).iter().flat_map(|e| e.split(';')) {
            let t = t.trim().to_owned();
            if !t.is_empty() && !seen.iter().any(|s| s.eq_ignore_ascii_case(&t)) {
                seen.push(t);
            }
        }
        self.exceptions = seen;
        self
    }

    /// The ffmpeg arguments that carry out the cleaning for one file. `source` is what the file carries (a tag is
    /// emptied by name, so what is there has to be known) and `out` which tracks the output has.
    ///
    /// Everything removed = `-map_metadata -1`; otherwise the tags are copied (`-map_metadata 0`) and each one that is
    /// dropped is emptied (`-metadata KEY=`, on a track `-metadata:s:v:0 KEY=`), which ffmpeg takes as "delete it".
    pub fn arguments(&self, source: &SourceTags, out: Outputs) -> Vec<String> {
        let c = self.clone().normalized();
        let mut args: Vec<String> = Vec::new();
        if c.mode == Mode::Delete && c.exceptions.is_empty() {
            args.extend(["-map_metadata".to_owned(), "-1".to_owned()]);
        } else {
            args.extend(["-map_metadata".to_owned(), "0".to_owned()]);
            if c.edits_tags() {
                for key in source.container.iter().filter(|k| c.drops(k)) {
                    args.extend(["-metadata".to_owned(), format!("{key}=")]);
                }
                for stream in &source.streams {
                    // Only the first video stream is mapped; the sound and the subtitles only when the output has them.
                    let mapped = match stream.kind {
                        StreamKind::Video => stream.ordinal == 0,
                        StreamKind::Audio => out.audio,
                        StreamKind::Subtitle => out.subtitles,
                    };
                    if !mapped {
                        continue;
                    }
                    for key in stream.keys.iter().filter(|k| c.drops(k)) {
                        args.extend([format!("-metadata:s:{}:{}", stream.kind.spec(), stream.ordinal), format!("{key}=")]);
                    }
                }
                if !c.remove_chapters {
                    for (i, keys) in source.chapters.iter().enumerate() {
                        for key in keys.iter().filter(|k| c.drops(k)) {
                            args.extend([format!("-metadata:c:{i}"), format!("{key}=")]);
                        }
                    }
                }
            }
        }
        if c.remove_chapters {
            args.extend(["-map_chapters".to_owned(), "-1".to_owned()]);
        }
        args
    }
}

/// The kinds of stream whose tags are known.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StreamKind {
    Video,
    Audio,
    Subtitle,
}

impl StreamKind {
    /// The letter ffmpeg's stream specifiers use.
    pub fn spec(self) -> char {
        match self {
            StreamKind::Video => 'v',
            StreamKind::Audio => 'a',
            StreamKind::Subtitle => 's',
        }
    }
}

/// The tag names of one stream of the source; `ordinal` counts the streams of that kind in the file's order, which is
/// what `-map 0:v:0` and `-metadata:s:a:1` address.
#[derive(Clone, Debug, PartialEq)]
pub struct StreamTags {
    pub kind: StreamKind,
    pub ordinal: usize,
    pub keys: Vec<String>,
}

/// The names of the tags a file carries: on the file as a whole, on each of its streams and on each chapter (by its
/// number). The values are not kept, only the names (a tag is removed by name).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SourceTags {
    pub container: Vec<String>,
    pub streams: Vec<StreamTags>,
    pub chapters: Vec<Vec<String>>,
}

/// Which tracks the output has, so a name is never given to a track that does not exist. The video is always there.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Outputs {
    pub audio: bool,
    pub subtitles: bool,
}

/// A file's cleaning as `build_args` needs it: the setting, and what the source carries.
pub struct Meta<'a> {
    pub cleanup: &'a MetadataCleanup,
    pub source: &'a SourceTags,
}

/// The names the editor offers while a name is typed: what cameras, phones and editors write. A help for typing, not a
/// filter: any name may be entered, including a vendor's own.
const KNOWN_NAMES: &[&str] = &[
    "album", "album_artist", "artist", "author", "comment", "composer", "copyright", "creation_time", "date", "description", "encoded_by",
    "episode_id", "filename", "genre", "handler_name", "keywords", "language", "location", "make", "mimetype", "model", "network", "publisher",
    "rating", "reel_name", "season_number", "show", "software", "synopsis", "timecode", "title", "vendor_id", "year",
    "com.android.version", "com.apple.quicktime.creationdate", "com.apple.quicktime.location.ISO6709", "com.apple.quicktime.make",
    "com.apple.quicktime.model", "com.apple.quicktime.software",
];

/// Names to offer while `typed` is being written: those that start with it first, then those that merely contain it.
/// `taken` are the names already in the list, which are not offered again.
pub fn suggest_names(typed: &str, taken: &[String], limit: usize) -> Vec<&'static str> {
    let needle = typed.trim().to_lowercase();
    if needle.is_empty() || needle.contains('*') {
        return Vec::new();
    }
    let free = |n: &str| n != needle && !taken.iter().any(|t| t.trim().eq_ignore_ascii_case(n));
    let (mut starts, mut inside) = (Vec::new(), Vec::new());
    for n in KNOWN_NAMES.iter().copied().filter(|n| free(&n.to_lowercase())) {
        let lower = n.to_lowercase();
        if lower.starts_with(&needle) {
            starts.push(n);
        } else if lower.contains(&needle) {
            inside.push(n);
        }
    }
    starts.extend(inside);
    starts.truncate(limit);
    starts
}

/// What a fresh install starts with: one entry that keeps everything, like FFmpeg Audio Studio's tag cleaning.
pub fn defaults() -> Vec<MetadataCleanup> {
    vec![MetadataCleanup::standard("Keep all tags")]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cleanup(mode: Mode, names: &[&str]) -> MetadataCleanup {
        MetadataCleanup { mode, exceptions: names.iter().map(|s| (*s).to_owned()).collect(), ..MetadataCleanup::new("t") }
    }

    fn source() -> SourceTags {
        SourceTags {
            container: ["major_brand", "creation_time", "title", "artist", "encoder", "comment", "location", "location-eng"].map(String::from).to_vec(),
            streams: vec![
                StreamTags { kind: StreamKind::Video, ordinal: 0, keys: ["creation_time", "handler_name", "encoder"].map(String::from).to_vec() },
                StreamTags { kind: StreamKind::Audio, ordinal: 0, keys: ["creation_time", "handler_name", "language"].map(String::from).to_vec() },
                StreamTags { kind: StreamKind::Audio, ordinal: 1, keys: ["language", "title"].map(String::from).to_vec() },
                StreamTags { kind: StreamKind::Video, ordinal: 1, keys: ["comment"].map(String::from).to_vec() },
                StreamTags { kind: StreamKind::Subtitle, ordinal: 0, keys: ["language"].map(String::from).to_vec() },
            ],
            chapters: vec![vec!["title".to_owned()], Vec::new(), vec!["title".to_owned()]],
        }
    }

    const BOTH: Outputs = Outputs { audio: true, subtitles: true };

    #[test]
    fn wildcard_names() {
        assert!(name_matches("location*", "location-eng"));
        assert!(name_matches("*date", "CREATION_DATE"));
        assert!(name_matches("title", "TITLE"));
        assert!(!name_matches("title", "subtitle"));
        assert!(name_matches("*", "anything"));
    }

    #[test]
    fn the_list_is_tidied_and_a_semicolon_separates_like_a_line_break() {
        let c = cleanup(Mode::Keep, &["  TITLE; comment ", "", "Title", "location*"]);
        assert_eq!(c.normalized().exceptions, ["TITLE", "comment", "location*"]);
    }

    #[test]
    fn the_mode_decides_what_the_list_means() {
        let mut c = cleanup(Mode::Keep, &["comment"]);
        assert!(c.drops("COMMENT") && !c.drops("title"));
        c.mode = Mode::Delete;
        assert!(!c.drops("comment") && c.drops("title"));
    }

    #[test]
    fn the_containers_own_fields_are_never_dropped() {
        let c = cleanup(Mode::Delete, &[]);
        assert!(!c.drops("encoder") && !c.drops("major_brand") && !c.drops("DURATION") && !c.drops("_STATISTICS_TAGS"));
        assert!(c.drops("title") && c.drops("handler_name"));
    }

    #[test]
    fn keeping_everything_changes_nothing_and_asks_for_the_plain_copy() {
        let c = MetadataCleanup::standard("Keep all tags");
        assert!(c.removes_nothing());
        assert_eq!(c.arguments(&source(), BOTH), ["-map_metadata", "0"]);
    }

    #[test]
    fn removing_everything_is_one_switch_and_the_chapters_are_their_own() {
        let c = cleanup(Mode::Delete, &[]);
        assert_eq!(c.arguments(&source(), BOTH), ["-map_metadata", "-1"]);
        let chapters = MetadataCleanup { remove_chapters: true, ..MetadataCleanup::standard("c") };
        assert_eq!(chapters.arguments(&source(), BOTH), ["-map_metadata", "0", "-map_chapters", "-1"]);
        assert!(!chapters.removes_nothing());
    }

    #[test]
    fn a_name_in_keep_mode_is_emptied_on_the_file_and_on_every_track_that_has_it() {
        let a = cleanup(Mode::Keep, &["location*", "creation_time"]).arguments(&source(), BOTH);
        let text = a.join(" ");
        assert!(text.starts_with("-map_metadata 0 "), "{text}");
        for want in ["-metadata creation_time=", "-metadata location=", "-metadata location-eng=", "-metadata:s:v:0 creation_time=", "-metadata:s:a:0 creation_time="] {
            assert!(text.contains(want), "{want} in {text}");
        }
        assert!(!text.contains("title=") && !text.contains("handler_name"), "{text}");
    }

    #[test]
    fn delete_mode_empties_all_but_the_listed_and_skips_the_containers_own_fields() {
        let a = cleanup(Mode::Delete, &["title", "language"]).arguments(&source(), BOTH);
        let text = a.join(" ");
        for want in ["-metadata artist=", "-metadata comment=", "-metadata location=", "-metadata creation_time=", "-metadata:s:v:0 handler_name=", "-metadata:s:a:0 handler_name="] {
            assert!(text.contains(want), "{want} in {text}");
        }
        for not in ["-metadata title=", "language=", "encoder=", "major_brand="] {
            assert!(!text.contains(not), "{not} in {text}");
        }
    }

    #[test]
    fn a_name_applies_to_every_chapter_too_and_the_chapters_switch_overrides_it() {
        let text = cleanup(Mode::Keep, &["title"]).arguments(&source(), BOTH).join(" ");
        assert!(text.contains("-metadata:c:0 title=") && text.contains("-metadata:c:2 title=") && !text.contains("-metadata:c:1"), "{text}");
        // Kept in the other mode, and not addressed at all when the chapters go.
        assert!(!cleanup(Mode::Delete, &["title"]).arguments(&source(), BOTH).join(" ").contains("-metadata:c:"));
        let gone = MetadataCleanup { remove_chapters: true, ..cleanup(Mode::Keep, &["title"]) };
        let text = gone.arguments(&source(), BOTH).join(" ");
        assert!(text.contains("-map_chapters -1") && !text.contains("-metadata:c:"), "{text}");
    }

    #[test]
    fn a_track_the_output_does_not_have_is_not_addressed() {
        let c = cleanup(Mode::Delete, &["title"]);
        let none = c.arguments(&source(), Outputs { audio: false, subtitles: false }).join(" ");
        assert!(!none.contains(":s:a:") && !none.contains(":s:s:"), "{none}");
        // Only the first video stream is in the output.
        assert!(none.contains(":s:v:0") && !none.contains(":s:v:1"), "{none}");
        let with = c.arguments(&source(), BOTH).join(" ");
        assert!(with.contains("-metadata:s:a:0 ") && with.contains("-metadata:s:s:0 language="), "{with}");
    }

    #[test]
    fn a_saved_entry_is_read_with_defaults_for_what_it_lacks() {
        let c: MetadataCleanup = serde_json::from_str(r#"{"id":"x","name":"n","mode":"delete"}"#).unwrap();
        assert_eq!(c.mode, Mode::Delete);
        assert!(c.exceptions.is_empty() && !c.remove_chapters);
    }

    #[test]
    fn suggestions_start_with_what_was_typed_and_skip_what_is_taken() {
        let s = suggest_names("loc", &[], 5);
        assert_eq!(s.first().copied(), Some("location"));
        assert!(suggest_names("loc", &["location".to_owned()], 5).is_empty() || !suggest_names("loc", &["location".to_owned()], 5).contains(&"location"));
        assert!(suggest_names("", &[], 5).is_empty() && suggest_names("loc*", &[], 5).is_empty());
        assert!(suggest_names("quicktime", &[], 8).iter().all(|n| n.contains("quicktime")));
    }
}
