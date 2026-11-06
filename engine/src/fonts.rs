//! The fonts the machine has, by family: what a text may be drawn in. A family is its regular face and, when the machine has them, the
//! bold, italic and bold italic ones (a missing one is the regular file, so a bold request never fails). The list comes from the system's own
//! register of installed fonts (Windows); other systems list none, and a text is then drawn in the default font.

use std::{
    collections::HashMap,
    sync::{Arc, Mutex, OnceLock},
};

/// One family and the file of each of its four faces.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Family {
    pub name: String,
    pub regular: String,
    pub bold: String,
    pub italic: String,
    pub bold_italic: String,
}

/// The families the register lists, from its text (`reg query` output: `    Arial Bold (TrueType)    REG_SZ    arialbd.ttf`), by name.
/// A file without a folder is in `folder`. Only plain font files (.ttf, .otf) count: a collection holds several faces in one file.
pub fn parse(listing: &str, folder: &str) -> Vec<Family> {
    // regular, bold, italic, bold italic
    let mut found: std::collections::BTreeMap<String, [Option<String>; 4]> = std::collections::BTreeMap::new();
    for line in listing.lines() {
        let Some((name, file)) = line.trim().split_once("    REG_SZ    ") else { continue };
        let file = file.trim();
        let lower = file.to_ascii_lowercase();
        if !(lower.ends_with(".ttf") || lower.ends_with(".otf")) {
            continue;
        }
        let name = name.trim();
        let name = name.strip_suffix(" (TrueType)").or_else(|| name.strip_suffix(" (OpenType)")).unwrap_or(name).trim();
        let (family, face) = if let Some(f) = name.strip_suffix(" Bold Italic") {
            (f, 3)
        } else if let Some(f) = name.strip_suffix(" Bold") {
            (f, 1)
        } else if let Some(f) = name.strip_suffix(" Italic").or_else(|| name.strip_suffix(" Oblique")) {
            (f, 2)
        } else {
            (name.strip_suffix(" Regular").unwrap_or(name), 0)
        };
        let path = if file.contains(':') || file.starts_with('/') { file.replace('\\', "/") } else { format!("{}/{}", folder.trim_end_matches('/'), file) };
        found.entry(family.to_owned()).or_default()[face] = Some(path);
    }
    found
        .into_iter()
        .filter_map(|(name, [regular, bold, italic, bold_italic])| {
            let regular = regular?;
            Some(Family { name, bold: bold.unwrap_or_else(|| regular.clone()), italic: italic.unwrap_or_else(|| regular.clone()), bold_italic: bold_italic.unwrap_or_else(|| regular.clone()), regular })
        })
        .collect()
}

/// The families installed on this machine, by name (read once).
pub fn installed() -> &'static [Family] {
    static LIST: OnceLock<Vec<Family>> = OnceLock::new();
    LIST.get_or_init(read_system)
}

/// The family of this name (the case does not matter), when the machine has it.
pub fn find(name: &str) -> Option<&'static Family> {
    installed().iter().find(|f| f.name.eq_ignore_ascii_case(name.trim()))
}

/// The file of a face of the default font (Arial), or of the family named: the regular face when the machine lacks the one asked for, and
/// empty when there is no such file at all (another system: ffmpeg then uses its own default).
pub fn face_file(family: &str, bold: bool, italic: bool) -> String {
    if let Some(f) = find(family) {
        return match (bold, italic) {
            (false, false) => f.regular.clone(),
            (true, false) => f.bold.clone(),
            (false, true) => f.italic.clone(),
            (true, true) => f.bold_italic.clone(),
        };
    }
    let suffix = match (bold, italic) {
        (false, false) => "",
        (true, false) => "bd",
        (false, true) => "i",
        (true, true) => "bi",
    };
    let regular = std::path::Path::new("C:/Windows/Fonts/arial.ttf");
    let file = format!("C:/Windows/Fonts/arial{suffix}.ttf");
    // (a variant the machine lacks is drawn in the regular face)
    let p = if std::path::Path::new(&file).is_file() { std::path::PathBuf::from(file) } else { regular.to_path_buf() };
    if cfg!(windows) && p.is_file() { p.to_string_lossy().into_owned() } else { String::new() }
}

/// The bytes of a font file, read once.
fn bytes_of(file: &str) -> Option<Arc<Vec<u8>>> {
    static CACHE: OnceLock<Mutex<HashMap<String, Option<Arc<Vec<u8>>>>>> = OnceLock::new();
    let mut cache = CACHE.get_or_init(Default::default).lock().ok()?;
    cache.entry(file.to_owned()).or_insert_with(|| std::fs::read(file).ok().map(Arc::new)).clone()
}

/// What the time tokens of a text are measured as (they are drawn as minutes and seconds).
fn measured(text: &str) -> String {
    text.replace("{elapsed}", "00:00").replace("{remaining}", "00:00")
}

/// How wide a line of text is, in em (the font's height unit, so the answer holds at any letter size), set in this font file. None when the
/// file cannot be read.
pub fn width_em(file: &str, text: &str) -> Option<f64> {
    let data = bytes_of(file)?;
    let face = ttf_parser::Face::parse(&data, 0).ok()?;
    let unit = f64::from(face.units_per_em().max(1));
    Some(measured(text).chars().map(|c| face.glyph_index(c).and_then(|g| face.glyph_hor_advance(g)).map_or(0.5, |a| f64::from(a) / unit)).sum())
}

/// The text broken into lines no wider than `max_em` (greedy, at spaces; a word wider than a line is cut where it overflows). Lines the writer
/// broke stay broken. A font that cannot be read leaves the text as it is.
pub fn wrap(file: &str, text: &str, max_em: f64) -> String {
    if max_em <= 0.0 || width_em(file, "").is_none() {
        return text.to_owned();
    }
    let width = |s: &str| width_em(file, s).unwrap_or(0.0);
    let mut lines: Vec<String> = Vec::new();
    for paragraph in text.split('\n') {
        let mut line = String::new();
        for word in paragraph.split(' ') {
            let joined = if line.is_empty() { word.to_owned() } else { format!("{line} {word}") };
            if width(&joined) <= max_em || line.is_empty() && word.is_empty() {
                line = joined;
                continue;
            }
            if !line.is_empty() {
                lines.push(std::mem::take(&mut line));
            }
            // a word wider than the zone is cut where it overflows
            let mut rest = word;
            while width(rest) > max_em && rest.chars().count() > 1 {
                let mut cut = rest.char_indices().map(|(i, _)| i).skip(1).take_while(|&i| width(&rest[..i]) <= max_em).last().unwrap_or_else(|| rest.char_indices().nth(1).map_or(rest.len(), |(i, _)| i));
                if cut == 0 {
                    cut = rest.len();
                }
                lines.push(rest[..cut].to_owned());
                rest = &rest[cut..];
            }
            line = rest.to_owned();
        }
        lines.push(line);
    }
    lines.join("\n")
}

#[cfg(windows)]
fn read_system() -> Vec<Family> {
    use std::os::windows::process::CommandExt;
    let mut listing = String::new();
    for hive in ["HKLM", "HKCU"] {
        let key = format!(r"{hive}\SOFTWARE\Microsoft\Windows NT\CurrentVersion\Fonts");
        let out = std::process::Command::new("reg").args(["query", &key]).creation_flags(0x0800_0000).output();
        if let Ok(out) = out {
            listing.push_str(&String::from_utf8_lossy(&out.stdout));
            listing.push('\n');
        }
    }
    let root = std::env::var("SystemRoot").unwrap_or_else(|_| r"C:\Windows".to_owned());
    parse(&listing, &format!("{}/Fonts", root.replace('\\', "/")))
}

#[cfg(not(windows))]
fn read_system() -> Vec<Family> {
    Vec::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    const LISTING: &str = "HKEY_LOCAL_MACHINE\\SOFTWARE\\Microsoft\\Windows NT\\CurrentVersion\\Fonts
    Arial (TrueType)    REG_SZ    arial.ttf
    Arial Bold (TrueType)    REG_SZ    arialbd.ttf
    Arial Italic (TrueType)    REG_SZ    ariali.ttf
    Arial Bold Italic (TrueType)    REG_SZ    arialbi.ttf
    Cambria & Cambria Math (TrueType)    REG_SZ    cambria.ttc
    Impact (TrueType)    REG_SZ    impact.ttf
    Roboto Bold (TrueType)    REG_SZ    D:\\Users\\me\\Fonts\\Roboto-Bold.ttf
    Segoe UI Light (TrueType)    REG_SZ    segoeuil.ttf
";

    #[test]
    fn a_family_is_its_regular_face_and_the_three_others_when_the_machine_has_them() {
        let list = parse(LISTING, "C:/Windows/Fonts");
        let arial = list.iter().find(|f| f.name == "Arial").unwrap();
        assert_eq!((arial.regular.as_str(), arial.bold.as_str(), arial.italic.as_str(), arial.bold_italic.as_str()), ("C:/Windows/Fonts/arial.ttf", "C:/Windows/Fonts/arialbd.ttf", "C:/Windows/Fonts/ariali.ttf", "C:/Windows/Fonts/arialbi.ttf"));
    }

    #[test]
    fn a_missing_face_is_the_regular_file_and_a_family_without_a_regular_face_is_not_listed() {
        let list = parse(LISTING, "C:/Windows/Fonts");
        let impact = list.iter().find(|f| f.name == "Impact").unwrap();
        assert_eq!((impact.bold.as_str(), impact.italic.as_str()), (impact.regular.as_str(), impact.regular.as_str()));
        assert!(list.iter().all(|f| f.name != "Roboto"), "only a bold face: no regular one to fall back to");
    }

    #[test]
    fn a_collection_is_left_out_and_a_light_face_is_a_family_of_its_own() {
        let list = parse(LISTING, "C:/Windows/Fonts");
        assert!(list.iter().all(|f| !f.name.contains("Cambria")));
        assert!(list.iter().any(|f| f.name == "Segoe UI Light"));
    }

    fn arial() -> Option<String> {
        let f = face_file("", false, false);
        (!f.is_empty()).then_some(f)
    }

    #[test]
    fn a_line_is_as_wide_as_its_letters_and_the_time_tokens_are_five_digits_wide() {
        let Some(f) = arial() else { return };
        let (a, b) = (width_em(&f, "ab").unwrap(), width_em(&f, "abab").unwrap());
        assert!((b - 2.0 * a).abs() < 1e-9);
        assert_eq!(width_em(&f, "{elapsed}"), width_em(&f, "00:00"));
        assert!(width_em("C:/nowhere/none.ttf", "x").is_none());
    }

    #[test]
    fn a_text_is_broken_at_spaces_to_fit_and_a_broken_line_stays_broken() {
        let Some(f) = arial() else { return };
        let one = width_em(&f, "hello world").unwrap();
        let fits = wrap(&f, "hello world", one + 0.01);
        assert_eq!(fits, "hello world");
        let narrow = wrap(&f, "hello world again", one * 0.7);
        assert!(narrow.contains('\n') && narrow.split('\n').all(|l| width_em(&f, l).unwrap() <= one * 0.7 + 1e-9), "{narrow}");
        assert_eq!(wrap(&f, "a\nb", 50.0), "a\nb");
        assert_eq!(wrap(&f, "hello", 0.0), "hello", "no width, no breaking");
    }

    #[test]
    fn a_word_wider_than_the_zone_is_cut_and_nothing_is_lost() {
        let Some(f) = arial() else { return };
        let out = wrap(&f, "abcdefghijklmnop", width_em(&f, "abcd").unwrap());
        assert!(out.contains('\n'));
        assert_eq!(out.replace('\n', ""), "abcdefghijklmnop");
    }

    #[test]
    fn a_file_with_its_own_folder_keeps_it() {
        let list = parse("    Roboto (TrueType)    REG_SZ    D:\\Users\\me\\Fonts\\Roboto.ttf\n", "C:/Windows/Fonts");
        assert_eq!(list[0].regular, "D:/Users/me/Fonts/Roboto.ttf");
    }
}
