//! Where a converted file goes and what it is called. Pure functions: the queue asks
//! `resolve_output` and does the file-system work itself.

use serde::{Deserialize, Serialize};
use std::path::{Component, Path, PathBuf};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Overwrite {
    /// An output that already exists is left alone and the job is reported as skipped.
    Skip,
    Overwrite,
    /// `name (2).mp4`, `name (3).mp4`, ...
    Rename,
}

/// Id of the window-only choice that writes beside each source file. Not an entry of Settings; the window names it.
pub const SOURCE_ID: &str = "source";

/// Id of the window-only choice that opens the system's folder picker. Like `SOURCE_ID`, not an entry of Settings:
/// what it picks becomes an ordinary saved destination (`Destination::for_folder`).
pub const PICK_ID: &str = "pick";

/// A saved destination: a name and the folder it writes into. Empty = beside each file; absolute = one folder
/// for every file; relative (`convert`, `.\convert`, `..\convert`) = counted from each file's own folder.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Destination {
    pub id: String,
    pub name: String,
    pub folder: String,
}

impl Destination {
    /// The window-only "Source Folder": beside each file.
    pub fn source() -> Destination {
        Destination { id: SOURCE_ID.to_owned(), ..Destination::default() }
    }

    pub fn new(name: &str) -> Destination {
        Destination { id: crate::preset::new_id(), name: name.to_owned(), folder: String::new() }
    }

    /// The destination for a folder the user picked: named after the folder (its own name, or the whole path for a
    /// drive root, which has none), writing there.
    pub fn for_folder(dir: &Path) -> Destination {
        let name = dir.file_name().map_or_else(|| dir.display().to_string(), |n| n.to_string_lossy().into_owned());
        Destination { id: crate::preset::new_id(), name, folder: dir.display().to_string() }
    }

    /// Whether this destination writes into `dir`.
    pub fn writes_to(&self, dir: &Path) -> bool {
        self.folder_path().is_some_and(|f| same_file(&f, dir))
    }

    pub fn is_source(&self) -> bool {
        self.id == SOURCE_ID
    }

    /// The same destination with the folder as typed minus the spaces and quotes a paste brings along.
    pub fn normalized(mut self) -> Destination {
        self.folder = self.folder.trim().trim_matches('"').trim().to_owned();
        self
    }

    /// The folder to write into, None when the files stay beside their sources.
    pub fn folder_path(&self) -> Option<PathBuf> {
        Some(self.folder.trim()).filter(|f| !f.is_empty()).map(PathBuf::from)
    }
}

#[derive(Debug, PartialEq)]
pub enum Resolved {
    Skip(PathBuf),
    Write(PathBuf),
}

fn same_file(a: &Path, b: &Path) -> bool {
    if cfg!(windows) {
        a.to_string_lossy().eq_ignore_ascii_case(&b.to_string_lossy())
    } else {
        a == b
    }
}

/// Whether a destination folder is taken from each source file's own folder: anything that is not
/// absolute - `convert`, `.\convert`, `..\convert`. An absolute path (`D:\Out`, a share) is one place
/// for every file; so is a path with a root but no drive (`\Out`), which the system resolves itself.
pub fn is_relative_folder(folder: &Path) -> bool {
    !folder.as_os_str().is_empty() && !folder.is_absolute() && !folder.has_root()
}

/// The folder a destination names, for one source file. Empty: the source's own folder. Absolute: itself.
/// Relative: counted from the source file's folder - `.\convert` is a `convert` folder beside the file,
/// `..\convert` one beside its folder - so a single entry serves files in many folders. `.` and `..` are
/// folded away rather than left in the path, so what is shown and created is the plain folder.
pub fn base_folder(folder: &Path, input: &Path) -> PathBuf {
    let own = input.parent().map(Path::to_path_buf).unwrap_or_default();
    if folder.as_os_str().is_empty() {
        return own;
    }
    if !is_relative_folder(folder) {
        return folder.to_path_buf();
    }
    // `C:convert` (a drive but no root) is relative to that drive's current folder: not ours to fold.
    if matches!(folder.components().next(), Some(Component::Prefix(_))) {
        return folder.to_path_buf();
    }
    let mut out = own;
    for part in folder.components() {
        match part {
            Component::Normal(name) => out.push(name),
            Component::ParentDir => {
                if !out.pop() {
                    out.push("..");
                }
            }
            _ => {}
        }
    }
    out
}

/// The output path for `input`.
///
/// - `folder` None: beside the source. Some: inside that folder, and when the file came in through an
///   added folder (`root`) its sub-folders are kept, the added folder's own name included. A RELATIVE
///   folder (`convert`, `.\convert`, `..\convert`) is counted from each file's own folder instead, and
///   keeps no structure: the folders the files sit in already are the structure.
/// - An output is never the input itself: the same name in the same place becomes `name (converted)`.
pub fn resolve_output(folder: Option<&Path>, input: &Path, root: Option<&Path>, ext: &str, overwrite: Overwrite) -> Resolved {
    let stem = input.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| "video".to_owned());
    let dir = match folder {
        None => input.parent().map(Path::to_path_buf).unwrap_or_default(),
        Some(f) if is_relative_folder(f) => base_folder(f, input),
        Some(f) => {
            let mut d = f.to_path_buf();
            if let Some(root) = root {
                if let Some(name) = root.file_name() {
                    d.push(name);
                }
                if let Some(rel) = input.parent().and_then(|p| p.strip_prefix(root).ok()) {
                    d.push(rel);
                }
            }
            d
        }
    };
    let mut path = dir.join(format!("{stem}.{ext}"));
    if same_file(&path, input) {
        path = dir.join(format!("{stem} (converted).{ext}"));
    }
    if !path.exists() {
        return Resolved::Write(path);
    }
    match overwrite {
        Overwrite::Skip => Resolved::Skip(path),
        Overwrite::Overwrite => Resolved::Write(path),
        Overwrite::Rename => {
            let base = path.with_extension("");
            let base = base.to_string_lossy().into_owned();
            let mut n = 2;
            loop {
                let candidate = PathBuf::from(format!("{base} ({n}).{ext}"));
                if !candidate.exists() {
                    return Resolved::Write(candidate);
                }
                n += 1;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn a_picked_folder_gives_a_destination_named_after_it() {
        let d = Destination::for_folder(Path::new(r"D:\Out\Reels"));
        assert_eq!(d.name, if cfg!(windows) { "Reels" } else { r"D:\Out\Reels" });
        assert!(d.writes_to(Path::new(r"D:\Out\Reels")));
        assert!(!d.writes_to(Path::new(r"D:\Out")));
        assert!(!Destination::source().writes_to(Path::new(r"D:\Out")), "beside each file writes into no folder");
    }

    fn scratch(name: &str) -> PathBuf {
        let p = std::env::temp_dir().join(format!("yaiol-vdcv-dest-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&p);
        fs::create_dir_all(&p).unwrap();
        p
    }

    #[test]
    fn beside_the_source_by_default() {
        let d = scratch("beside");
        let out = resolve_output(None, &d.join("a.avi"), None, "mkv", Overwrite::Skip);
        assert_eq!(out, Resolved::Write(d.join("a.mkv")));
        let _ = fs::remove_dir_all(d);
    }

    #[test]
    fn a_relative_folder_is_counted_from_each_files_own_folder() {
        let d = scratch("relative");
        let a = d.join("a").join("clip.avi");
        let b = d.join("b").join("deep").join("clip.avi");
        let beside = Path::new(".\\convert");
        assert_eq!(resolve_output(Some(beside), &a, None, "mp4", Overwrite::Skip), Resolved::Write(d.join("a").join("convert").join("clip.mp4")));
        assert_eq!(resolve_output(Some(beside), &b, None, "mp4", Overwrite::Skip), Resolved::Write(d.join("b").join("deep").join("convert").join("clip.mp4")));
        let up = Path::new("..\\convert");
        assert_eq!(resolve_output(Some(up), &b, None, "mp4", Overwrite::Skip), Resolved::Write(d.join("b").join("convert").join("clip.mp4")));
        let plain = Path::new("convert");
        assert_eq!(resolve_output(Some(plain), &a, None, "mp4", Overwrite::Skip), Resolved::Write(d.join("a").join("convert").join("clip.mp4")));
        let _ = fs::remove_dir_all(d);
    }

    #[test]
    fn a_relative_folder_keeps_no_added_folder_structure() {
        let d = scratch("relative-root");
        let root = d.join("Films");
        let input = root.join("2020").join("a.avi");
        let out = resolve_output(Some(Path::new("convert")), &input, Some(&root), "mp4", Overwrite::Skip);
        assert_eq!(out, Resolved::Write(root.join("2020").join("convert").join("a.mp4")));
        let _ = fs::remove_dir_all(d);
    }

    #[test]
    fn parent_references_are_folded_and_cannot_climb_past_the_root() {
        let base = std::env::temp_dir();
        let file = base.join("a").join("b").join("f.avi");
        assert_eq!(base_folder(Path::new("../../x"), &file), base.join("x"));
        // Far more levels up than exist stops at the root instead of failing.
        let far = base_folder(Path::new(&format!("{}x", "../".repeat(60))), &file);
        assert!(far.ends_with("x"), "{far:?}");
        assert!(!is_relative_folder(Path::new("")));
        assert!(!is_relative_folder(&base));
    }

    #[test]
    fn an_output_is_never_the_input() {
        let d = scratch("never");
        let out = resolve_output(None, &d.join("a.mkv"), None, "mkv", Overwrite::Overwrite);
        assert_eq!(out, Resolved::Write(d.join("a (converted).mkv")));
        let _ = fs::remove_dir_all(d);
    }

    #[test]
    fn an_added_folder_keeps_its_name_and_structure() {
        let d = scratch("tree");
        let out_dir = d.join("out");
        let root = d.join("Films");
        let input = root.join("2020").join("a.avi");
        let out = resolve_output(Some(&out_dir), &input, Some(&root), "mp4", Overwrite::Skip);
        assert_eq!(out, Resolved::Write(out_dir.join("Films").join("2020").join("a.mp4")));
        let single = resolve_output(Some(&out_dir), &input, None, "mp4", Overwrite::Skip);
        assert_eq!(single, Resolved::Write(out_dir.join("a.mp4")));
        let _ = fs::remove_dir_all(d);
    }

    #[test]
    fn the_three_policies_for_an_existing_output() {
        let d = scratch("policy");
        fs::write(d.join("a.mp4"), b"x").unwrap();
        fs::write(d.join("a (2).mp4"), b"x").unwrap();
        let input = d.join("a.avi");
        assert_eq!(resolve_output(None, &input, None, "mp4", Overwrite::Skip), Resolved::Skip(d.join("a.mp4")));
        assert_eq!(resolve_output(None, &input, None, "mp4", Overwrite::Overwrite), Resolved::Write(d.join("a.mp4")));
        assert_eq!(resolve_output(None, &input, None, "mp4", Overwrite::Rename), Resolved::Write(d.join("a (3).mp4")));
        let _ = fs::remove_dir_all(d);
    }
}
