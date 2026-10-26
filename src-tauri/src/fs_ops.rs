//! Local filesystem operations behind the Tauri commands. Everything here is
//! blocking; callers run it on a blocking thread so the UI never stalls.

use serde::Serialize;
use std::cmp::Ordering;
use std::fs;
use std::io::{self, ErrorKind};
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Entry {
    pub name: String,
    pub path: String,
    pub is_dir: bool,
    pub is_symlink: bool,
    pub is_hidden: bool,
    pub size: u64,
    /// Milliseconds since the Unix epoch.
    pub modified: Option<u64>,
}

#[derive(Serialize)]
pub struct Listing {
    pub path: String,
    pub parent: Option<String>,
    pub entries: Vec<Entry>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum SortBy {
    Name,
    Size,
    Modified,
    Kind,
}

impl SortBy {
    pub fn parse(s: &str) -> Self {
        match s {
            "size" => Self::Size,
            "modified" => Self::Modified,
            "kind" => Self::Kind,
            _ => Self::Name,
        }
    }
}

pub fn list_dir(path: &Path, show_hidden: bool, sort_by: SortBy, desc: bool) -> io::Result<Listing> {
    let path = fs::canonicalize(path)?;
    let mut entries = Vec::new();

    for item in fs::read_dir(&path)? {
        let Ok(item) = item else { continue };
        let name = item.file_name().to_string_lossy().into_owned();
        let is_hidden = name.starts_with('.');
        if is_hidden && !show_hidden {
            continue;
        }

        let item_path = item.path();
        let is_symlink = item.file_type().map(|t| t.is_symlink()).unwrap_or(false);
        // Follow symlinks so a link to a folder behaves like a folder; fall back
        // to the link itself when the target is missing.
        let meta = fs::metadata(&item_path).or_else(|_| fs::symlink_metadata(&item_path));
        let (is_dir, size, modified) = match meta {
            Ok(m) => (
                m.is_dir(),
                if m.is_dir() { 0 } else { m.len() },
                m.modified()
                    .ok()
                    .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                    .map(|d| d.as_millis() as u64),
            ),
            Err(_) => (false, 0, None),
        };

        entries.push(Entry {
            name,
            path: item_path.to_string_lossy().into_owned(),
            is_dir,
            is_symlink,
            is_hidden,
            size,
            modified,
        });
    }

    sort_entries(&mut entries, sort_by, desc);

    Ok(Listing {
        parent: path.parent().map(|p| p.to_string_lossy().into_owned()),
        path: path.to_string_lossy().into_owned(),
        entries,
    })
}

/// Folders always come first; within each group the chosen key decides, with
/// the name as a tie-breaker.
pub fn sort_entries(entries: &mut [Entry], sort_by: SortBy, desc: bool) {
    entries.sort_by(|a, b| {
        let by_key = match sort_by {
            SortBy::Name => Ordering::Equal,
            SortBy::Size => a.size.cmp(&b.size),
            SortBy::Modified => a.modified.cmp(&b.modified),
            SortBy::Kind => natural_cmp(extension(&a.name), extension(&b.name)),
        };
        let ord = by_key.then_with(|| natural_cmp(&a.name, &b.name));
        let ord = if desc { ord.reverse() } else { ord };
        b.is_dir.cmp(&a.is_dir).then(ord)
    });
}

fn extension(name: &str) -> &str {
    match name.rfind('.') {
        Some(i) if i > 0 => &name[i + 1..],
        _ => "",
    }
}

/// Case-insensitive comparison that orders digit runs by value, so
/// "file 2" sorts before "file 10" like it does in Finder.
pub fn natural_cmp(a: &str, b: &str) -> Ordering {
    let a = a.to_lowercase();
    let b = b.to_lowercase();
    let (mut ai, mut bi) = (a.chars().peekable(), b.chars().peekable());

    loop {
        match (ai.peek().copied(), bi.peek().copied()) {
            (None, None) => return a.cmp(&b),
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            (Some(x), Some(y)) if x.is_ascii_digit() && y.is_ascii_digit() => {
                let na = take_digits(&mut ai);
                let nb = take_digits(&mut bi);
                let (ta, tb) = (na.trim_start_matches('0'), nb.trim_start_matches('0'));
                let ord = ta.len().cmp(&tb.len()).then_with(|| ta.cmp(tb));
                if ord != Ordering::Equal {
                    return ord;
                }
            }
            (Some(x), Some(y)) => {
                if x != y {
                    return x.cmp(&y);
                }
                ai.next();
                bi.next();
            }
        }
    }
}

fn take_digits(it: &mut std::iter::Peekable<std::str::Chars>) -> String {
    let mut s = String::new();
    while let Some(&c) = it.peek() {
        if !c.is_ascii_digit() {
            break;
        }
        s.push(c);
        it.next();
    }
    s
}

/// A path inside `dir` named `name` that doesn't exist yet, using Finder's
/// "name copy", "name copy 2" pattern. Nothing is ever overwritten.
pub fn unique_target(dir: &Path, name: &str, is_dir: bool) -> PathBuf {
    let first = dir.join(name);
    if !exists(&first) {
        return first;
    }
    let (stem, ext) = match name.rfind('.') {
        Some(i) if i > 0 && !is_dir => (&name[..i], &name[i..]),
        _ => (name, ""),
    };
    (1..)
        .map(|n| {
            let suffix = if n == 1 { " copy".to_string() } else { format!(" copy {n}") };
            dir.join(format!("{stem}{suffix}{ext}"))
        })
        .find(|p| !exists(p))
        .expect("unbounded range always finds a free name")
}

/// Like `Path::exists`, but also true for broken symlinks.
fn exists(p: &Path) -> bool {
    fs::symlink_metadata(p).is_ok()
}

fn file_name(src: &Path) -> io::Result<String> {
    src.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .ok_or_else(|| io::Error::new(ErrorKind::InvalidInput, "Can't use the root folder here"))
}

fn check_not_into_itself(src: &Path, dest_dir: &Path) -> io::Result<()> {
    let dest = fs::canonicalize(dest_dir)?;
    if let Ok(src) = fs::canonicalize(src) {
        if dest.starts_with(&src) {
            return Err(io::Error::new(
                ErrorKind::InvalidInput,
                format!("Can't put \"{}\" inside itself", file_name(&src)?),
            ));
        }
    }
    Ok(())
}

pub fn copy_into(sources: &[PathBuf], dest_dir: &Path) -> io::Result<()> {
    for src in sources {
        check_not_into_itself(src, dest_dir)?;
        let is_dir = fs::symlink_metadata(src)?.is_dir();
        let target = unique_target(dest_dir, &file_name(src)?, is_dir);
        copy_recursive(src, &target)?;
    }
    Ok(())
}

pub fn move_into(sources: &[PathBuf], dest_dir: &Path) -> io::Result<()> {
    for src in sources {
        if src.parent() == Some(dest_dir) {
            continue;
        }
        check_not_into_itself(src, dest_dir)?;
        let is_dir = fs::symlink_metadata(src)?.is_dir();
        let target = unique_target(dest_dir, &file_name(src)?, is_dir);
        match fs::rename(src, &target) {
            Ok(()) => {}
            // Different volume: copy, then remove the original.
            Err(e) if e.kind() == ErrorKind::CrossesDevices => {
                copy_recursive(src, &target)?;
                if is_dir {
                    fs::remove_dir_all(src)?;
                } else {
                    fs::remove_file(src)?;
                }
            }
            Err(e) => return Err(e),
        }
    }
    Ok(())
}

/// Copies files, folders and symlinks (as links). On APFS `fs::copy` clones
/// the file, so large copies on the same disk are instant.
fn copy_recursive(src: &Path, dest: &Path) -> io::Result<()> {
    let meta = fs::symlink_metadata(src)?;
    if meta.file_type().is_symlink() {
        std::os::unix::fs::symlink(fs::read_link(src)?, dest)
    } else if meta.is_dir() {
        fs::create_dir(dest)?;
        for item in fs::read_dir(src)? {
            let item = item?;
            copy_recursive(&item.path(), &dest.join(item.file_name()))?;
        }
        Ok(())
    } else {
        fs::copy(src, dest).map(|_| ())
    }
}

fn validate_name(name: &str) -> io::Result<&str> {
    let name = name.trim();
    if name.is_empty() || name == "." || name == ".." || name.contains('/') {
        return Err(io::Error::new(ErrorKind::InvalidInput, "That name isn't allowed"));
    }
    Ok(name)
}

pub fn rename(path: &Path, new_name: &str) -> io::Result<PathBuf> {
    let new_name = validate_name(new_name)?;
    let parent = path
        .parent()
        .ok_or_else(|| io::Error::new(ErrorKind::InvalidInput, "Can't rename the root folder"))?;
    let target = parent.join(new_name);
    // Allow changing only the capitalisation (same file on a case-insensitive disk).
    let same_file = fs::canonicalize(&target).ok() == fs::canonicalize(path).ok();
    if exists(&target) && !same_file {
        return Err(io::Error::new(
            ErrorKind::AlreadyExists,
            format!("\"{new_name}\" already exists"),
        ));
    }
    fs::rename(path, &target)?;
    Ok(target)
}

pub fn create_folder(dir: &Path, name: &str) -> io::Result<PathBuf> {
    let target = dir.join(validate_name(name)?);
    fs::create_dir(&target)?;
    Ok(target)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("patina-test-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn natural_order() {
        let mut names = vec!["file 10", "File 2", "file 1", "apple", "Banana"];
        names.sort_by(|a, b| natural_cmp(a, b));
        assert_eq!(names, ["apple", "Banana", "file 1", "File 2", "file 10"]);
    }

    #[test]
    fn copy_names_never_overwrite() {
        let dir = temp_dir("unique");
        fs::write(dir.join("a.txt"), "1").unwrap();
        assert_eq!(unique_target(&dir, "a.txt", false), dir.join("a copy.txt"));
        fs::write(dir.join("a copy.txt"), "2").unwrap();
        assert_eq!(unique_target(&dir, "a.txt", false), dir.join("a copy 2.txt"));
        assert_eq!(unique_target(&dir, "b.txt", false), dir.join("b.txt"));
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn copy_and_move_folders() {
        let root = temp_dir("ops");
        let (left, right) = (root.join("left"), root.join("right"));
        fs::create_dir_all(left.join("folder/inner")).unwrap();
        fs::write(left.join("folder/inner/x.txt"), "hello").unwrap();
        fs::create_dir(&right).unwrap();

        copy_into(&[left.join("folder")], &right).unwrap();
        assert_eq!(fs::read_to_string(right.join("folder/inner/x.txt")).unwrap(), "hello");

        // Copying again into the same place makes a "copy", it doesn't merge.
        copy_into(&[left.join("folder")], &right).unwrap();
        assert!(right.join("folder copy/inner/x.txt").exists());

        move_into(&[left.join("folder")], &right).unwrap();
        assert!(!left.join("folder").exists());
        assert!(right.join("folder copy 2/inner/x.txt").exists());

        assert!(copy_into(&[right.join("folder")], &right.join("folder/inner")).is_err());
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn listing_puts_folders_first() {
        let dir = temp_dir("list");
        fs::write(dir.join("b.txt"), "").unwrap();
        fs::write(dir.join(".hidden"), "").unwrap();
        fs::create_dir(dir.join("z-folder")).unwrap();

        let names = |l: Listing| l.entries.into_iter().map(|e| e.name).collect::<Vec<_>>();
        assert_eq!(names(list_dir(&dir, false, SortBy::Name, false).unwrap()), ["z-folder", "b.txt"]);
        assert_eq!(
            names(list_dir(&dir, true, SortBy::Name, false).unwrap()),
            ["z-folder", ".hidden", "b.txt"]
        );
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn rename_rejects_clashes() {
        let dir = temp_dir("rename");
        fs::write(dir.join("a"), "").unwrap();
        fs::write(dir.join("b"), "").unwrap();
        assert!(rename(&dir.join("a"), "b").is_err());
        assert!(rename(&dir.join("a"), "x/y").is_err());
        assert_eq!(rename(&dir.join("a"), "c").unwrap(), dir.join("c"));
        fs::remove_dir_all(&dir).unwrap();
    }
}
