//! Trim: keep only part of each file, from a start time to an end time on the SOURCE's own timeline.
//! A choice of the run, not of a preset - the same cut goes with any codec - and it is never saved: a cut
//! left over from yesterday would quietly shorten every file converted tomorrow.
//!
//! The start is given to ffmpeg before the input, which seeks fast (to the keyframe before it, then drops
//! what is not wanted when the video is re-encoded; a stream copy starts on that keyframe). The end becomes a
//! length. With burned-in subtitles the start is given after the input instead: the subtitles are read from
//! the file by a filter of their own, on the source's timeline, and only that keeps them in step.

/// What is kept of each file. A side left empty is not cut.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Trim {
    pub start_s: Option<f64>,
    pub end_s: Option<f64>,
}

/// A time as the user writes it: seconds (`90`, `12.5`), minutes and seconds (`1:30`), or hours (`1:02:03.5`).
/// Empty is no time; anything else that is not a time is None too, so the caller can tell the two apart.
pub fn parse_time(text: &str) -> Option<Option<f64>> {
    let text = text.trim();
    if text.is_empty() {
        return Some(None);
    }
    let parts: Vec<&str> = text.split(':').collect();
    if parts.len() > 3 {
        return None;
    }
    let mut seconds = 0.0;
    for (i, part) in parts.iter().enumerate() {
        let last = i == parts.len() - 1;
        let value: f64 = if last {
            part.trim().parse().ok().filter(|v: &f64| v.is_finite())?
        } else {
            f64::from(part.trim().parse::<u32>().ok()?)
        };
        if value < 0.0 {
            return None;
        }
        seconds = seconds * 60.0 + value;
    }
    Some(Some(seconds))
}

impl Trim {
    pub fn none() -> Trim {
        Trim::default()
    }

    pub fn is_none(&self) -> bool {
        self.start_s.is_none() && self.end_s.is_none()
    }

    /// The trim the two texts name, or None when either is not a time or the end is not after the start.
    pub fn parse(start: &str, end: &str) -> Option<Trim> {
        let (start_s, end_s) = (parse_time(start)?, parse_time(end)?);
        if let (Some(s), Some(e)) = (start_s, end_s) {
            if e <= s {
                return None;
            }
        }
        Some(Trim { start_s, end_s })
    }

    /// How long the output is, given the source's length (0 when unknown): the part between the two times.
    pub fn kept_s(&self, duration_s: f64) -> f64 {
        let start = self.start_s.unwrap_or(0.0);
        let end = match (self.end_s, duration_s > 0.0) {
            (Some(e), true) => e.min(duration_s),
            (Some(e), false) => e,
            (None, _) => duration_s,
        };
        (end - start).max(0.0)
    }

    /// The length to ask of ffmpeg (`-t`), when the end is cut.
    pub fn length_s(&self) -> Option<f64> {
        self.end_s.map(|e| (e - self.start_s.unwrap_or(0.0)).max(0.001))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn times_are_read_the_way_people_write_them() {
        assert_eq!(parse_time("90"), Some(Some(90.0)));
        assert_eq!(parse_time(" 1:30 "), Some(Some(90.0)));
        assert_eq!(parse_time("01:02:03.5"), Some(Some(3723.5)));
        assert_eq!(parse_time("12.5"), Some(Some(12.5)));
        assert_eq!(parse_time(""), Some(None));
        for bad in ["abc", "1:2:3:4", "-5", "1:-2", "1.5:30", "nan", "1::2"] {
            assert_eq!(parse_time(bad), None, "{bad}");
        }
    }

    #[test]
    fn an_end_must_come_after_the_start() {
        assert!(Trim::parse("10", "20").is_some());
        assert!(Trim::parse("20", "10").is_none());
        assert!(Trim::parse("10", "10").is_none());
        assert_eq!(Trim::parse("", "").unwrap(), Trim::none());
        assert!(Trim::parse("x", "").is_none());
    }

    #[test]
    fn the_kept_length_is_what_lies_between_the_two_times() {
        let t = Trim::parse("10", "40").unwrap();
        assert_eq!(t.kept_s(100.0), 30.0);
        assert_eq!(t.length_s(), Some(30.0));
        // An end past the file's end is the end.
        assert_eq!(Trim::parse("", "500").unwrap().kept_s(100.0), 100.0);
        assert_eq!(Trim::parse("30", "").unwrap().kept_s(100.0), 70.0);
        assert_eq!(Trim::none().kept_s(100.0), 100.0);
        assert_eq!(Trim::parse("30", "").unwrap().length_s(), None);
        // A start past the end leaves nothing, and an unknown length is not guessed.
        assert_eq!(Trim::parse("200", "").unwrap().kept_s(100.0), 0.0);
        assert_eq!(Trim::parse("5", "").unwrap().kept_s(0.0), 0.0);
    }
}
