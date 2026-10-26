//! Local filesystem operations behind the Tauri commands. Everything here is
//! blocking; callers run it on a blocking thread so the UI never stalls.

use serde::Serialize;
use std::cmp::Ordering;
use std::ffi::{CStr, CString};
use std::fs;
use std::io::{self, ErrorKind, Read};
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

/// A Finder tag. `color` uses Finder's numbering: 0 none, 1 grey, 2 green,
/// 3 purple, 4 blue, 5 yellow, 6 red, 7 orange.
#[derive(Serialize, Clone)]
pub struct Tag {
    pub name: String,
    pub color: u8,
}

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
    pub tags: Vec<Tag>,
}

#[derive(Serialize)]
pub struct Listing {
    pub path: String,
    pub parent: Option<String>,
    pub entries: Vec<Entry>,
    /// Free space on the volume, in bytes.
    pub available: Option<u64>,
    /// Which disk the folder is on, so the UI can tell a drag will copy
    /// (another disk) rather than move.
    pub device: Option<u64>,
}

fn millis(t: io::Result<SystemTime>) -> Option<u64> {
    t.ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as u64)
}

fn entry_for(path: PathBuf, name: String) -> Entry {
    let link_meta = fs::symlink_metadata(&path).ok();
    let is_symlink = link_meta.as_ref().is_some_and(|m| m.file_type().is_symlink());
    let finder_hidden = link_meta.as_ref().is_some_and(has_hidden_flag);
    // Follow symlinks so a link to a folder behaves like a folder; fall back
    // to the link itself when the target is missing.
    let meta = fs::metadata(&path).ok().or(link_meta);
    let (is_dir, size, modified) = match meta {
        Some(m) => (m.is_dir(), if m.is_dir() { 0 } else { m.len() }, millis(m.modified())),
        None => (false, 0, None),
    };
    Entry {
        is_hidden: finder_hidden || name.starts_with('.'),
        tags: read_tags(&path),
        name,
        path: path.to_string_lossy().into_owned(),
        is_dir,
        is_symlink,
        size,
        modified,
    }
}

/// Finder also hides items with the "hidden" file flag set, such as ~/Library.
#[cfg(target_os = "macos")]
fn has_hidden_flag(meta: &fs::Metadata) -> bool {
    use std::os::macos::fs::MetadataExt as _;
    meta.st_flags() & libc::UF_HIDDEN != 0
}

#[cfg(not(target_os = "macos"))]
fn has_hidden_flag(_meta: &fs::Metadata) -> bool {
    false
}

/// Finder stores tags as a binary plist of "Name\nColour" strings.
pub fn read_tags(path: &Path) -> Vec<Tag> {
    let Ok(Some(bytes)) = xattr::get(path, "com.apple.metadata:_kMDItemUserTags") else {
        return Vec::new();
    };
    let Ok(raw) = plist::from_bytes::<Vec<String>>(&bytes) else {
        return Vec::new();
    };
    raw.into_iter()
        .map(|t| match t.split_once('\n') {
            Some((name, color)) => Tag { name: name.to_string(), color: color.parse().unwrap_or(0) },
            None => Tag { name: t, color: 0 },
        })
        .collect()
}

fn statvfs(path: &Path) -> Option<libc::statvfs> {
    let c = CString::new(path.as_os_str().as_bytes()).ok()?;
    let mut s: libc::statvfs = unsafe { std::mem::zeroed() };
    (unsafe { libc::statvfs(c.as_ptr(), &mut s) } == 0).then_some(s)
}

pub fn available_space(path: &Path) -> Option<u64> {
    statvfs(path).map(|s| s.f_bavail as u64 * s.f_frsize as u64)
}

fn total_space(path: &Path) -> Option<u64> {
    statvfs(path).map(|s| s.f_blocks as u64 * s.f_frsize as u64)
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
        // Skip dotfiles before touching the disk; flag-hidden items are only
        // known after the stat in `entry_for`.
        if name.starts_with('.') && !show_hidden {
            continue;
        }
        let entry = entry_for(item.path(), name);
        if !entry.is_hidden || show_hidden {
            entries.push(entry);
        }
    }

    sort_entries(&mut entries, sort_by, desc);

    Ok(Listing {
        parent: path.parent().map(|p| p.to_string_lossy().into_owned()),
        available: available_space(&path),
        device: fs::metadata(&path).ok().map(|m| m.dev()),
        path: path.to_string_lossy().into_owned(),
        entries,
    })
}

/// Every file carrying a Finder tag, found through Spotlight. The listing's
/// path is `tag:<name>` so the UI can tell it apart from a real folder.
pub fn find_tagged(tag: &str, show_hidden: bool, sort_by: SortBy, desc: bool) -> io::Result<Listing> {
    let escaped = tag.replace('\\', "\\\\").replace('\'', "\\'");
    let out = Command::new("/usr/bin/mdfind")
        .arg(format!("kMDItemUserTags == '{escaped}'"))
        .output()?;
    let mut entries: Vec<Entry> = String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter(|l| !l.is_empty())
        .map(PathBuf::from)
        .filter_map(|p| {
            let name = p.file_name()?.to_string_lossy().into_owned();
            Some(entry_for(p, name))
        })
        .filter(|e| show_hidden || !e.is_hidden)
        .collect();
    sort_entries(&mut entries, sort_by, desc);
    Ok(Listing { path: format!("tag:{tag}"), parent: None, entries, available: None, device: None })
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Details {
    pub size: u64,
    pub created: Option<u64>,
    pub modified: Option<u64>,
    pub accessed: Option<u64>,
    /// Permission bits only, e.g. 0o644.
    pub mode: u32,
    pub owner: String,
    pub group: String,
    /// Number of items inside, for folders.
    pub items: Option<usize>,
    pub link_target: Option<String>,
}

pub fn details(path: &Path) -> io::Result<Details> {
    let meta = fs::metadata(path).or_else(|_| fs::symlink_metadata(path))?;
    let items = meta
        .is_dir()
        .then(|| fs::read_dir(path).map(|d| d.count()).ok())
        .flatten();
    Ok(Details {
        size: meta.len(),
        created: millis(meta.created()),
        modified: millis(meta.modified()),
        accessed: millis(meta.accessed()),
        mode: meta.permissions().mode() & 0o777,
        owner: user_name(meta.uid()),
        group: group_name(meta.gid()),
        items,
        link_target: fs::read_link(path).ok().map(|p| p.to_string_lossy().into_owned()),
    })
}

fn user_name(uid: u32) -> String {
    let pw = unsafe { libc::getpwuid(uid) };
    if pw.is_null() {
        return uid.to_string();
    }
    unsafe { CStr::from_ptr((*pw).pw_name) }.to_string_lossy().into_owned()
}

fn group_name(gid: u32) -> String {
    let gr = unsafe { libc::getgrgid(gid) };
    if gr.is_null() {
        return gid.to_string();
    }
    unsafe { CStr::from_ptr((*gr).gr_name) }.to_string_lossy().into_owned()
}

#[derive(Serialize)]
pub struct TextPreview {
    pub text: String,
    pub truncated: bool,
}

/// The start of a file as text, or `None` if it looks binary.
pub fn read_text(path: &Path, max_bytes: usize) -> io::Result<Option<TextPreview>> {
    // Opening a named pipe or device would block until something writes to it.
    if !fs::metadata(path)?.is_file() {
        return Ok(None);
    }
    let file = fs::File::open(path)?;
    let len = file.metadata()?.len();
    let mut buf = Vec::with_capacity(max_bytes.min(len as usize));
    file.take(max_bytes as u64).read_to_end(&mut buf)?;
    if buf[..buf.len().min(8192)].contains(&0) {
        return Ok(None);
    }
    let text = String::from_utf8_lossy(&buf);
    let bad = text.chars().filter(|&c| c == char::REPLACEMENT_CHARACTER).count();
    if bad > 8 && bad * 100 > text.len() {
        return Ok(None);
    }
    Ok(Some(TextPreview { truncated: len > buf.len() as u64, text: text.into_owned() }))
}

#[derive(Serialize)]
pub struct Place {
    pub name: String,
    pub path: String,
    pub icon: &'static str,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Volume {
    pub name: String,
    pub path: String,
    pub available: Option<u64>,
    pub total: Option<u64>,
    pub is_root: bool,
}

#[derive(Serialize)]
pub struct Sidebar {
    pub favorites: Vec<Place>,
    pub volumes: Vec<Volume>,
}

/// Well-known folders, with the name and icon the sidebar shows for them.
fn known_places(home: &Path) -> [(String, PathBuf, &'static str); 9] {
    let user = home.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or("Home".into());
    [
        (user, home.to_path_buf(), "home"),
        ("Desktop".into(), home.join("Desktop"), "desktop"),
        ("Documents".into(), home.join("Documents"), "documents"),
        ("Downloads".into(), home.join("Downloads"), "downloads"),
        ("Applications".into(), "/Applications".into(), "applications"),
        ("Movies".into(), home.join("Movies"), "movies"),
        ("Music".into(), home.join("Music"), "music"),
        ("Pictures".into(), home.join("Pictures"), "pictures"),
        ("iCloud Drive".into(), home.join("Library/Mobile Documents/com~apple~CloudDocs"), "icloud"),
    ]
}

/// The sidebar's contents. `custom` is the user's own Favourites list (in
/// order), or `None` for the default set.
pub fn sidebar(home: &Path, custom: Option<&[String]>) -> Sidebar {
    let known = known_places(home);
    let favorites = match custom {
        None => known
            .into_iter()
            .filter(|(_, p, _)| p.is_dir())
            .map(|(name, p, icon)| Place { name, path: p.to_string_lossy().into_owned(), icon })
            .collect(),
        // Folders that no longer exist are skipped, not forgotten, so a
        // drive that's unplugged comes back when it's plugged in again.
        Some(paths) => paths
            .iter()
            .map(PathBuf::from)
            .filter(|p| p.is_dir())
            .map(|p| match known.iter().find(|(_, k, _)| *k == p) {
                Some((name, _, icon)) => Place { name: name.clone(), path: p.to_string_lossy().into_owned(), icon },
                None => Place {
                    name: p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "/".into()),
                    path: p.to_string_lossy().into_owned(),
                    icon: "folder",
                },
            })
            .collect(),
    };

    // Each mounted volume appears in /Volumes; the startup disk is a symlink to "/".
    let mut volumes: Vec<Volume> = fs::read_dir("/Volumes")
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .filter(|e| !e.file_name().to_string_lossy().starts_with('.'))
        .filter_map(|e| {
            let path = fs::canonicalize(e.path()).ok()?;
            Some(Volume {
                name: e.file_name().to_string_lossy().into_owned(),
                available: available_space(&path),
                total: total_space(&path),
                is_root: path == Path::new("/"),
                path: path.to_string_lossy().into_owned(),
            })
        })
        .collect();
    if !volumes.iter().any(|v| v.is_root) {
        volumes.push(Volume {
            name: "Macintosh HD".into(),
            path: "/".into(),
            available: available_space(Path::new("/")),
            total: total_space(Path::new("/")),
            is_root: true,
        });
    }
    volumes.sort_by(|a, b| b.is_root.cmp(&a.is_root).then_with(|| natural_cmp(&a.name, &b.name)));

    Sidebar { favorites, volumes }
}

/// Folders always come first; within each group the chosen key decides, with
/// the name as a tie-breaker.
pub fn sort_entries(entries: &mut Vec<Entry>, sort_by: SortBy, desc: bool) {
    // Lowercase each name once, not on every comparison: big folders sort
    // several times faster.
    let mut keyed: Vec<(String, Entry)> = entries.drain(..).map(|e| (e.name.to_lowercase(), e)).collect();
    keyed.sort_by(|(ka, a), (kb, b)| {
        let by_key = match sort_by {
            SortBy::Name => Ordering::Equal,
            SortBy::Size => a.size.cmp(&b.size),
            SortBy::Modified => a.modified.cmp(&b.modified),
            SortBy::Kind => natural_cmp_lower(extension(ka), extension(kb)),
        };
        let ord = by_key
            .then_with(|| natural_cmp_lower(ka, kb))
            .then_with(|| a.name.cmp(&b.name));
        let ord = if desc { ord.reverse() } else { ord };
        b.is_dir.cmp(&a.is_dir).then(ord)
    });
    entries.extend(keyed.into_iter().map(|(_, e)| e));
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
    natural_cmp_lower(&a.to_lowercase(), &b.to_lowercase())
}

/// `natural_cmp` for strings that are already lowercase.
fn natural_cmp_lower(a: &str, b: &str) -> Ordering {
    let (mut ai, mut bi) = (a.chars().peekable(), b.chars().peekable());

    loop {
        match (ai.peek().copied(), bi.peek().copied()) {
            (None, None) => return a.cmp(b),
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

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Transfer {
    Copy,
    Move,
    /// Like Finder: move within a disk, copy to another one.
    Auto,
}

impl Transfer {
    pub fn parse(s: &str) -> Self {
        match s {
            "copy" => Self::Copy,
            "move" => Self::Move,
            _ => Self::Auto,
        }
    }
}

pub fn transfer(sources: &[PathBuf], dest_dir: &Path, mode: Transfer) -> io::Result<()> {
    match mode {
        Transfer::Copy => copy_into(sources, dest_dir),
        Transfer::Move => move_into(sources, dest_dir),
        Transfer::Auto => {
            let dest_dev = fs::metadata(dest_dir)?.dev();
            let (same_disk, other_disk): (Vec<PathBuf>, Vec<PathBuf>) = sources
                .iter()
                .cloned()
                .partition(|s| fs::symlink_metadata(s).is_ok_and(|m| m.dev() == dest_dev));
            move_into(&same_disk, dest_dir)?;
            copy_into(&other_disk, dest_dir)
        }
    }
}

/// Copies each item next to itself ("name copy", like Finder's Duplicate).
pub fn duplicate(sources: &[PathBuf]) -> io::Result<()> {
    for src in sources {
        let parent = src
            .parent()
            .ok_or_else(|| io::Error::new(ErrorKind::InvalidInput, "Can't duplicate the root folder"))?;
        copy_into(std::slice::from_ref(src), parent)?;
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
    fn reads_finder_tags() {
        let dir = temp_dir("tags");
        let file = dir.join("tagged.txt");
        fs::write(&file, "").unwrap();
        let mut bytes = Vec::new();
        plist::to_writer_binary(&mut bytes, &vec!["Red\n6", "Work"]).unwrap();
        xattr::set(&file, "com.apple.metadata:_kMDItemUserTags", &bytes).unwrap();

        let tags = read_tags(&file);
        assert_eq!(tags.len(), 2);
        assert_eq!((tags[0].name.as_str(), tags[0].color), ("Red", 6));
        assert_eq!((tags[1].name.as_str(), tags[1].color), ("Work", 0));
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn details_and_permissions() {
        let dir = temp_dir("details");
        let file = dir.join("a.sh");
        fs::write(&file, "echo hi").unwrap();
        fs::set_permissions(&file, fs::Permissions::from_mode(0o750)).unwrap();
        let d = details(&file).unwrap();
        assert_eq!(d.mode, 0o750);
        assert_eq!(d.size, 7);
        assert!(!d.owner.is_empty());
        assert_eq!(details(&dir).unwrap().items, Some(1));
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn text_preview_skips_binary() {
        let dir = temp_dir("text");
        fs::write(dir.join("a.txt"), "hello\nworld").unwrap();
        fs::write(dir.join("b.bin"), [0u8, 159, 146, 150, 0, 1]).unwrap();
        fs::write(dir.join("big.txt"), "x".repeat(100)).unwrap();
        assert_eq!(read_text(&dir.join("a.txt"), 1024).unwrap().unwrap().text, "hello\nworld");
        assert!(read_text(&dir.join("b.bin"), 1024).unwrap().is_none());
        assert!(read_text(&dir.join("big.txt"), 10).unwrap().unwrap().truncated);
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

    #[test]
    fn finder_hidden_flag_hides_items() {
        let dir = temp_dir("uf-hidden");
        fs::create_dir(dir.join("Library")).unwrap();
        fs::write(dir.join("visible.txt"), "").unwrap();
        let status = Command::new("/usr/bin/chflags").arg("hidden").arg(dir.join("Library")).status().unwrap();
        assert!(status.success());

        let names = |l: Listing| l.entries.into_iter().map(|e| e.name).collect::<Vec<_>>();
        assert_eq!(names(list_dir(&dir, false, SortBy::Name, false).unwrap()), ["visible.txt"]);
        assert_eq!(names(list_dir(&dir, true, SortBy::Name, false).unwrap()), ["Library", "visible.txt"]);
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn text_preview_ignores_pipes() {
        let dir = temp_dir("fifo");
        let fifo = dir.join("pipe");
        assert!(Command::new("/usr/bin/mkfifo").arg(&fifo).status().unwrap().success());
        // Would block forever if it tried to open the pipe.
        assert!(read_text(&fifo, 1024).unwrap().is_none());
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn sort_by_size_and_kind() {
        let dir = temp_dir("sort");
        fs::write(dir.join("b.txt"), "xx").unwrap();
        fs::write(dir.join("a.zip"), "xxxxx").unwrap();
        fs::write(dir.join("C.md"), "x").unwrap();
        let names = |by, desc| {
            list_dir(&dir, false, by, desc).unwrap().entries.into_iter().map(|e| e.name).collect::<Vec<_>>()
        };
        assert_eq!(names(SortBy::Size, false), ["C.md", "b.txt", "a.zip"]);
        assert_eq!(names(SortBy::Size, true), ["a.zip", "b.txt", "C.md"]);
        assert_eq!(names(SortBy::Kind, false), ["C.md", "b.txt", "a.zip"]);
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn transfer_auto_moves_within_a_disk() {
        let root = temp_dir("transfer");
        let (a, b) = (root.join("a"), root.join("b"));
        fs::create_dir_all(&a).unwrap();
        fs::create_dir_all(&b).unwrap();
        fs::write(a.join("one.txt"), "1").unwrap();
        fs::write(a.join("two.txt"), "2").unwrap();

        // Same disk: Auto moves.
        transfer(&[a.join("one.txt")], &b, Transfer::Auto).unwrap();
        assert!(!a.join("one.txt").exists() && b.join("one.txt").exists());
        // Copy keeps the original; Move removes it.
        transfer(&[a.join("two.txt")], &b, Transfer::Copy).unwrap();
        assert!(a.join("two.txt").exists() && b.join("two.txt").exists());
        transfer(&[a.join("two.txt")], &b, Transfer::Move).unwrap();
        assert!(!a.join("two.txt").exists() && b.join("two copy.txt").exists());

        assert_eq!(Transfer::parse("copy"), Transfer::Copy);
        assert_eq!(Transfer::parse("anything"), Transfer::Auto);
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn duplicate_makes_a_copy_alongside() {
        let dir = temp_dir("duplicate");
        fs::write(dir.join("report.pdf"), "x").unwrap();
        fs::create_dir(dir.join("folder")).unwrap();
        duplicate(&[dir.join("report.pdf"), dir.join("folder")]).unwrap();
        assert!(dir.join("report copy.pdf").exists());
        assert!(dir.join("folder copy").is_dir());
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn listing_reports_its_disk() {
        let dir = temp_dir("device");
        let listing = list_dir(&dir, false, SortBy::Name, false).unwrap();
        assert_eq!(listing.device, Some(fs::metadata(&dir).unwrap().dev()));
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn rename_can_change_only_capitalisation() {
        let dir = temp_dir("rename-case");
        fs::write(dir.join("notes.txt"), "").unwrap();
        assert_eq!(rename(&dir.join("notes.txt"), "Notes.txt").unwrap(), dir.join("Notes.txt"));
        let names: Vec<_> = fs::read_dir(&dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names, ["Notes.txt"]);
        fs::remove_dir_all(&dir).unwrap();
    }
}
