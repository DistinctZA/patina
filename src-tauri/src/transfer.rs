//! Copying and moving files, with progress and cancel for the progress panel.
//!
//! Within a disk, moves are renames and copies are APFS clones, so both are
//! near-instant. Big files going to another disk are copied in chunks, so the
//! progress bar moves and Cancel works part-way through a file.

use crate::fs_ops::{StagingDir, check_not_into_itself, file_name, rename_exclusive, unique_target};
use serde::Serialize;
use std::fs;
use std::io::{self, ErrorKind, Read, Write};
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

/// Files at least this big are copied in chunks when they go to another disk.
const CHUNKED_MIN: u64 = 32 << 20;
const CHUNK: usize = 4 << 20;
const REPORT_EVERY: Duration = Duration::from_millis(100);

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Mode {
    Copy,
    Move,
    /// Like Finder: move within a disk, copy to another one.
    Auto,
}

impl Mode {
    pub fn parse(s: &str) -> Self {
        match s {
            "copy" => Self::Copy,
            "move" => Self::Move,
            _ => Self::Auto,
        }
    }
}

#[derive(Serialize, Clone, Default, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Progress {
    /// "Copying" or "Moving", for the panel's label.
    pub verb: &'static str,
    pub done_bytes: u64,
    pub total_bytes: u64,
    pub done_items: u64,
    pub total_items: u64,
    /// The file being worked on.
    pub current: String,
}

/// Keeps the counts, reports them (at most every 100 ms) and stops the work
/// when the cancel flag is set.
pub struct Ticker<'a> {
    state: Progress,
    cancel: &'a AtomicBool,
    report: Box<dyn FnMut(&Progress) + 'a>,
    last: Option<Instant>,
    every: Duration,
}

impl<'a> Ticker<'a> {
    pub fn new(cancel: &'a AtomicBool, report: impl FnMut(&Progress) + 'a) -> Self {
        Self { state: Progress::default(), cancel, report: Box::new(report), last: None, every: REPORT_EVERY }
    }

    fn check(&self) -> io::Result<()> {
        if self.cancel.load(Ordering::Relaxed) {
            return Err(io::Error::new(ErrorKind::Interrupted, "Cancelled"));
        }
        Ok(())
    }

    fn working_on(&mut self, verb: &'static str, path: &Path) {
        self.state.verb = verb;
        self.state.current = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        self.emit(false);
    }

    fn add(&mut self, bytes: u64, items: u64) -> io::Result<()> {
        self.state.done_bytes += bytes;
        self.state.done_items += items;
        self.emit(false);
        self.check()
    }

    fn emit(&mut self, force: bool) {
        if force || self.last.is_none_or(|t| t.elapsed() >= self.every) {
            (self.report)(&self.state);
            self.last = Some(Instant::now());
        }
    }

    #[cfg(test)]
    pub fn progress(&self) -> &Progress {
        &self.state
    }
}

enum Action {
    /// Same disk: just rename.
    Rename,
    Copy,
    /// Another disk: copy, then remove the original.
    CopyThenDelete,
}

/// Copies or moves `sources` into `dest_dir`. Nothing is overwritten: name
/// clashes get "copy" names.
pub fn transfer(sources: &[PathBuf], dest_dir: &Path, mode: Mode, t: &mut Ticker) -> io::Result<()> {
    let dest_dev = fs::metadata(dest_dir)?.dev();
    let mut plan = Vec::new();
    for src in sources {
        let same_disk = fs::symlink_metadata(src)?.dev() == dest_dev;
        let action = match mode {
            Mode::Copy => Action::Copy,
            Mode::Move | Mode::Auto if same_disk => Action::Rename,
            Mode::Move => Action::CopyThenDelete,
            Mode::Auto => Action::Copy,
        };
        // Moving something into the folder it's already in does nothing.
        if matches!(action, Action::Rename | Action::CopyThenDelete) && src.parent() == Some(dest_dir) {
            continue;
        }
        plan.push((src.clone(), dest_dir.to_path_buf(), action));
    }
    run(plan, t)
}

/// Copies each item next to itself ("name copy", like Finder's Duplicate).
pub fn duplicate(sources: &[PathBuf], t: &mut Ticker) -> io::Result<()> {
    let plan = sources
        .iter()
        .map(|src| {
            let parent = src
                .parent()
                .ok_or_else(|| io::Error::new(ErrorKind::InvalidInput, "Can't duplicate the root folder"))?;
            Ok((src.clone(), parent.to_path_buf(), Action::Copy))
        })
        .collect::<io::Result<Vec<_>>>()?;
    run(plan, t)
}

fn run(plan: Vec<(PathBuf, PathBuf, Action)>, t: &mut Ticker) -> io::Result<()> {
    // Count what will be copied first, so the bar has a total.
    for (src, _, action) in &plan {
        match action {
            Action::Rename => t.state.total_items += 1,
            Action::Copy | Action::CopyThenDelete => {
                let (bytes, items) = measure(src, t.cancel)?;
                t.state.total_bytes += bytes;
                t.state.total_items += items;
            }
        }
    }
    t.emit(true);

    for (src, dest_dir, action) in plan {
        t.check()?;
        check_not_into_itself(&src, &dest_dir)?;
        let dest_dev = fs::metadata(&dest_dir)?.dev();
        match action {
            Action::Rename => {
                t.working_on("Moving", &src);
                match publish(&src, &dest_dir, &file_name(&src)?) {
                    Ok(_) => t.add(0, 1)?,
                    // A different volume after all (e.g. a mount point): copy instead.
                    Err(e) if e.kind() == ErrorKind::CrossesDevices => {
                        let (bytes, items) = measure(&src, t.cancel)?;
                        t.state.total_bytes += bytes;
                        t.state.total_items += items.saturating_sub(1);
                        copy_cleanly(&src, &dest_dir, dest_dev, t, true)?;
                    }
                    Err(e) => return Err(e),
                }
            }
            Action::Copy => {
                t.working_on("Copying", &src);
                copy_cleanly(&src, &dest_dir, dest_dev, t, false)?;
            }
            Action::CopyThenDelete => {
                t.working_on("Moving", &src);
                copy_cleanly(&src, &dest_dir, dest_dev, t, true)?;
            }
        }
    }
    t.emit(true);
    Ok(())
}

/// Claim a free name atomically, retrying when another job got there first.
fn publish(src: &Path, dest_dir: &Path, name: &str) -> io::Result<PathBuf> {
    let is_dir = fs::symlink_metadata(src)?.is_dir();
    loop {
        let target = unique_target(dest_dir, name, is_dir);
        match rename_exclusive(src, &target) {
            Ok(()) => return Ok(target),
            Err(e) if e.kind() == ErrorKind::AlreadyExists => continue,
            Err(e) if e.raw_os_error() == Some(libc::ENOTSUP) || e.kind() == ErrorKind::Unsupported => {
                return Err(io::Error::new(ErrorKind::Unsupported, format!(
                    "The destination drive doesn't support the safe rename needed to place “{name}” here."
                )));
            }
            Err(e) => return Err(e),
        }
    }
}

struct CopiedEntry {
    source: PathBuf,
    before: fs::Metadata,
}

impl CopiedEntry {
    fn same_inode(&self, now: &fs::Metadata) -> bool {
        self.before.dev() == now.dev() && self.before.ino() == now.ino()
    }

    fn same_version(&self, now: &fs::Metadata, check_ctime: bool) -> bool {
        let before = &self.before;
        self.same_inode(now)
            && before.len() == now.len() && before.mode() == now.mode()
            && before.uid() == now.uid() && before.gid() == now.gid()
            && (before.mtime(), before.mtime_nsec()) == (now.mtime(), now.mtime_nsec())
            && (!check_ctime || (before.ctime(), before.ctime_nsec()) == (now.ctime(), now.ctime_nsec()))
    }

    fn changed(&self) -> io::Error {
        io::Error::other(format!("Move stopped because “{}” changed during the transfer.", self.source.display()))
    }

    fn metadata_at(&self, path: &Path) -> io::Result<fs::Metadata> {
        fs::symlink_metadata(path).map_err(|e| if e.kind() == ErrorKind::NotFound { self.changed() } else { e })
    }
}

/// Claim the source name before cleanup so an editor's replacement at the old
/// path is never unlinked. On a conflict, put the surviving original back.
fn remove_copied_source(src: &Path, name: &str, copied: &mut [CopiedEntry]) -> io::Result<()> {
    let parent = src.parent().ok_or_else(|| io::Error::other("Can't move the root folder"))?;
    let stage = StagingDir::new(parent)?;
    let staged = stage.path.join(src.file_name().unwrap());
    // This slot belongs to our newly created directory. An ordinary rename
    // also works on source volumes that lack exclusive rename support.
    fs::rename(src, &staged)?;
    let staged_path = |entry: &CopiedEntry| stage.path.join(entry.source.strip_prefix(parent).unwrap());
    let cleanup = (|| {
        for entry in &mut *copied {
            let now = entry.metadata_at(&staged_path(entry))?;
            // Renaming the top item changes its ctime. Children keep theirs.
            if !entry.same_version(&now, entry.source != src) {
                return Err(entry.changed());
            }
            entry.before = now;
        }
        // Entries were recorded children first. Remove only known, unchanged
        // leaves and empty folders. A newly added child keeps its folder alive.
        for entry in copied {
            let path = staged_path(entry);
            let now = match fs::symlink_metadata(&path) {
                Ok(meta) => meta,
                // macOS can remove an AppleDouble sidecar with its data file.
                Err(e) if e.kind() == ErrorKind::NotFound => continue,
                Err(e) => return Err(e),
            };
            if entry.before.is_dir() {
                if !entry.same_inode(&now) {
                    return Err(entry.changed());
                }
                fs::remove_dir(path)?;
            } else {
                // Removing a sibling hard link changes ctime on the shared inode.
                if !entry.same_version(&now, entry.before.nlink() == now.nlink()) {
                    return Err(entry.changed());
                }
                fs::remove_file(path)?;
            }
        }
        Ok(())
    })();
    if let Err(e) = cleanup {
        let recovered = publish(&staged, parent, name).unwrap_or(staged);
        return Err(io::Error::other(format!("{e} The remaining original is at “{}”.", recovered.display())));
    }
    if fs::symlink_metadata(src).is_ok() {
        return Err(io::Error::other(format!("A new item appeared at “{}” during the move and was kept.", src.display())));
    }
    Ok(())
}

/// Build the copy in a private directory, then publish it atomically. Failure
/// cleanup can only touch this job's partial copy, never another job's files.
fn copy_cleanly(src: &Path, dest_dir: &Path, dest_dev: u64, t: &mut Ticker, moving: bool) -> io::Result<()> {
    let name = file_name(src)?;
    let staging = StagingDir::new(dest_dir)?;
    let partial = staging.path.join("item");
    let result = (|| {
        let mut copied = Vec::new();
        copy_tree(src, &partial, dest_dev, t, &mut |source, target, meta| {
            if moving {
                verify_move_metadata(source, target, &meta)?;
                copied.push(CopiedEntry { source: source.to_owned(), before: meta });
            }
            Ok(())
        })?;
        for entry in &copied {
            t.check()?;
            if !entry.same_version(&entry.metadata_at(&entry.source)?, true) {
                return Err(entry.changed());
            }
        }
        t.check()?;
        let target = publish(&partial, dest_dir, &name)?;
        if moving {
            remove_copied_source(src, &name, &mut copied)
                .map_err(|e| io::Error::other(format!("A copy is at “{}”. {e}", target.display())))?;
        }
        Ok(())
    })();
    if result.is_err() {
        let _ = remove_partial(&partial);
    }
    result
}

fn remove_partial(path: &Path) -> io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let meta = fs::symlink_metadata(path)?;
    if meta.is_dir() {
        // Completed subdirectories may already have read-only permissions.
        // This is only called inside a private copy staging directory.
        fs::set_permissions(path, fs::Permissions::from_mode(meta.permissions().mode() | 0o700))?;
        for child in fs::read_dir(path)? {
            remove_partial(&child?.path())?;
        }
        fs::remove_dir(path)
    } else {
        fs::remove_file(path)
    }
}

/// Total bytes and item count (files, folders, links) under a path. Counting
/// a huge folder can take a while, so it stops early on Cancel.
fn measure(path: &Path, cancel: &AtomicBool) -> io::Result<(u64, u64)> {
    if cancel.load(Ordering::Relaxed) {
        return Err(io::Error::new(ErrorKind::Interrupted, "Cancelled"));
    }
    let Ok(meta) = fs::symlink_metadata(path) else { return Ok((0, 1)) };
    if meta.is_dir() {
        let mut total = (0, 1);
        for item in fs::read_dir(path).into_iter().flatten().filter_map(Result::ok) {
            let (bytes, items) = measure(&item.path(), cancel)?;
            total = (total.0 + bytes, total.1 + items);
        }
        Ok(total)
    } else if meta.file_type().is_symlink() {
        Ok((0, 1))
    } else {
        Ok((meta.len(), 1))
    }
}

/// Copies files, folders and symlinks (as links).
fn copy_tree(
    src: &Path, dest: &Path, dest_dev: u64, t: &mut Ticker,
    record: &mut impl FnMut(&Path, &Path, fs::Metadata) -> io::Result<()>,
) -> io::Result<()> {
    t.check()?;
    let meta = fs::symlink_metadata(src)?;
    if meta.file_type().is_symlink() {
        std::os::unix::fs::symlink(fs::read_link(src)?, dest)?;
        #[cfg(target_os = "macos")]
        copy_metadata(src, dest)?;
        t.add(0, 1)?;
    } else if meta.is_dir() {
        fs::create_dir(dest)?;
        t.add(0, 1)?;
        for item in fs::read_dir(src)? {
            let item = item?;
            copy_tree(&item.path(), &dest.join(item.file_name()), dest_dev, t, record)?;
        }
        // Apply metadata after the children, so copying still works for
        // read-only folders and doesn't change the preserved modification date.
        copy_metadata(src, dest)?;
    } else {
        t.working_on(t.state.verb, src);
        if meta.dev() != dest_dev && meta.len() >= CHUNKED_MIN {
            copy_chunked(src, dest, t)?;
        } else {
            // On APFS within a disk this is a clone: instant, and takes no space.
            fs::copy(src, dest)?;
            t.add(meta.len(), 0)?;
        }
        t.add(0, 1)?;
    }
    record(src, dest, meta)
}

/// Some filesystems report a successful native copy even when they silently
/// discard access rules or make a read-only file writable. Check before moving.
fn verify_move_metadata(src: &Path, dest: &Path, meta: &fs::Metadata) -> io::Result<()> {
    let loss = |what: &str| io::Error::other(format!(
        "The destination couldn't preserve {what} for “{}”. The original was kept.", src.display()
    ));
    if meta.permissions().readonly() && !fs::symlink_metadata(dest)?.permissions().readonly() {
        return Err(loss("read-only permissions"));
    }
    #[cfg(target_os = "macos")]
    {
        let flags = metadata_flags(src)?;
        if flags & libc::COPYFILE_ACL != 0 && metadata_flags(dest)? & libc::COPYFILE_ACL == 0 {
            return Err(loss("the access rules"));
        }
        // AppleDouble sidecars have no attributes of their own and reject
        // getxattr. Their contents are already included in the directory copy.
        if flags & libc::COPYFILE_XATTR == 0 {
            return Ok(());
        }
    }
    const TAGS: &str = "com.apple.metadata:_kMDItemUserTags";
    match xattr::get(src, TAGS) {
        Ok(Some(tags)) if xattr::get(dest, TAGS)?.as_ref() != Some(&tags) => Err(loss("the Finder tags")),
        Err(e) if e.kind() != ErrorKind::Unsupported && e.raw_os_error() != Some(libc::ENOTSUP) => Err(e),
        _ => Ok(()),
    }
}

#[cfg(target_os = "macos")]
fn metadata_flags(path: &Path) -> io::Result<libc::copyfile_flags_t> {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;
    let path = CString::new(path.as_os_str().as_bytes())?;
    let flags = libc::COPYFILE_CHECK | libc::COPYFILE_ACL | libc::COPYFILE_XATTR | libc::COPYFILE_NOFOLLOW;
    let result = unsafe { libc::copyfile(path.as_ptr(), std::ptr::null(), std::ptr::null_mut(), flags) };
    if result < 0 { Err(io::Error::last_os_error()) } else { Ok(result as libc::copyfile_flags_t) }
}

#[cfg(target_os = "macos")]
fn copy_metadata(src: &Path, dest: &Path) -> io::Result<()> {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;
    let src = CString::new(src.as_os_str().as_bytes())?;
    let dest = CString::new(dest.as_os_str().as_bytes())?;
    // Includes Finder tags, ACLs, permissions and dates, without copying data.
    let result = unsafe {
        libc::copyfile(src.as_ptr(), dest.as_ptr(), std::ptr::null_mut(), libc::COPYFILE_METADATA | libc::COPYFILE_NOFOLLOW)
    };
    if result == 0 { Ok(()) } else { Err(io::Error::last_os_error()) }
}

#[cfg(not(target_os = "macos"))]
fn copy_metadata(src: &Path, dest: &Path) -> io::Result<()> {
    let meta = fs::metadata(src)?;
    for name in xattr::list(src)? {
        if let Some(value) = xattr::get(src, &name)? {
            xattr::set(dest, &name, &value)?;
        }
    }
    let times = fs::FileTimes::new().set_modified(meta.modified()?).set_accessed(meta.accessed()?);
    fs::File::open(dest)?.set_times(times)?;
    fs::set_permissions(dest, meta.permissions())
}

/// Copies a big file piece by piece, reporting as it goes and stopping
/// promptly on Cancel. Keeps permissions, the modified date, and extended
/// attributes (Finder tags among them), as `fs::copy` does.
fn copy_chunked(src: &Path, dest: &Path, t: &mut Ticker) -> io::Result<()> {
    let mut reader = fs::File::open(src)?;
    let mut writer = fs::File::create_new(dest)?;
    let mut buf = vec![0u8; CHUNK];
    loop {
        let n = reader.read(&mut buf)?;
        if n == 0 {
            break;
        }
        writer.write_all(&buf[..n])?;
        t.add(n as u64, 0)?;
    }
    copy_metadata(src, dest)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("patina-transfer-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn quiet<R>(f: impl FnOnce(&mut Ticker) -> R) -> R {
        let cancel = AtomicBool::new(false);
        let mut t = Ticker::new(&cancel, |_| {});
        f(&mut t)
    }

    #[test]
    fn copy_and_move_folders() {
        let root = temp_dir("ops");
        let (left, right) = (root.join("left"), root.join("right"));
        fs::create_dir_all(left.join("folder/inner")).unwrap();
        fs::write(left.join("folder/inner/x.txt"), "hello").unwrap();
        fs::create_dir(&right).unwrap();

        quiet(|t| transfer(&[left.join("folder")], &right, Mode::Copy, t)).unwrap();
        assert_eq!(fs::read_to_string(right.join("folder/inner/x.txt")).unwrap(), "hello");

        // Copying again into the same place makes a "copy", it doesn't merge.
        quiet(|t| transfer(&[left.join("folder")], &right, Mode::Copy, t)).unwrap();
        assert!(right.join("folder copy/inner/x.txt").exists());

        quiet(|t| transfer(&[left.join("folder")], &right, Mode::Move, t)).unwrap();
        assert!(!left.join("folder").exists());
        assert!(right.join("folder copy 2/inner/x.txt").exists());

        assert!(quiet(|t| transfer(&[right.join("folder")], &right.join("folder/inner"), Mode::Copy, t)).is_err());
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn auto_moves_within_a_disk() {
        let root = temp_dir("auto");
        let (a, b) = (root.join("a"), root.join("b"));
        fs::create_dir_all(&a).unwrap();
        fs::create_dir_all(&b).unwrap();
        fs::write(a.join("one.txt"), "1").unwrap();
        fs::write(a.join("two.txt"), "2").unwrap();

        quiet(|t| transfer(&[a.join("one.txt")], &b, Mode::Auto, t)).unwrap();
        assert!(!a.join("one.txt").exists() && b.join("one.txt").exists());
        quiet(|t| transfer(&[a.join("two.txt")], &b, Mode::Copy, t)).unwrap();
        assert!(a.join("two.txt").exists() && b.join("two.txt").exists());
        quiet(|t| transfer(&[a.join("two.txt")], &b, Mode::Move, t)).unwrap();
        assert!(!a.join("two.txt").exists() && b.join("two copy.txt").exists());

        assert_eq!(Mode::parse("copy"), Mode::Copy);
        assert_eq!(Mode::parse("anything"), Mode::Auto);
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn duplicate_makes_a_copy_alongside() {
        let dir = temp_dir("duplicate");
        fs::write(dir.join("report.pdf"), "x").unwrap();
        fs::create_dir(dir.join("folder")).unwrap();
        quiet(|t| duplicate(&[dir.join("report.pdf"), dir.join("folder")], t)).unwrap();
        assert!(dir.join("report copy.pdf").exists());
        assert!(dir.join("folder copy").is_dir());
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn progress_counts_bytes_and_items() {
        let root = temp_dir("progress");
        fs::create_dir_all(root.join("src/sub")).unwrap();
        fs::write(root.join("src/a.bin"), vec![1u8; 1000]).unwrap();
        fs::write(root.join("src/sub/b.bin"), vec![2u8; 500]).unwrap();
        fs::create_dir(root.join("dest")).unwrap();

        let cancel = AtomicBool::new(false);
        let reports = RefCell::new(Vec::new());
        let mut t = Ticker::new(&cancel, |p| reports.borrow_mut().push(p.clone()));
        transfer(&[root.join("src")], &root.join("dest"), Mode::Copy, &mut t).unwrap();
        let last = t.progress().clone();
        drop(t);

        // src, sub, a.bin, b.bin
        assert_eq!((last.total_items, last.done_items), (4, 4));
        assert_eq!((last.total_bytes, last.done_bytes), (1500, 1500));
        assert_eq!(last.verb, "Copying");
        assert!(!reports.borrow().is_empty());
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn cancel_stops_and_leaves_no_partial_copy() {
        let root = temp_dir("cancel");
        fs::create_dir_all(root.join("src")).unwrap();
        for i in 0..20 {
            fs::write(root.join(format!("src/f{i}.txt")), "x").unwrap();
        }
        fs::create_dir(root.join("dest")).unwrap();

        // Cancel as soon as the first file is done.
        let cancel = AtomicBool::new(false);
        let mut t = Ticker::new(&cancel, |p| {
            if p.done_items >= 2 {
                cancel.store(true, Ordering::Relaxed);
            }
        });
        // Reports are throttled; report every step for this test.
        t.every = Duration::ZERO;
        let err = transfer(&[root.join("src")], &root.join("dest"), Mode::Copy, &mut t).unwrap_err();
        assert_eq!(err.kind(), ErrorKind::Interrupted);
        assert!(!root.join("dest/src").exists(), "half-made copy should be removed");
        assert!(root.join("src/f0.txt").exists(), "originals untouched");
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn chunked_copy_keeps_contents_permissions_and_tags() {
        use std::os::unix::fs::PermissionsExt;
        let root = temp_dir("chunked");
        let src = root.join("big.bin");
        let data: Vec<u8> = (0..(CHUNK * 2 + 123)).map(|i| (i % 251) as u8).collect();
        fs::write(&src, &data).unwrap();
        fs::set_permissions(&src, fs::Permissions::from_mode(0o640)).unwrap();
        xattr::set(&src, "com.example.patina-test", b"tag").unwrap();

        let cancel = AtomicBool::new(false);
        let mut t = Ticker::new(&cancel, |_| {});
        copy_chunked(&src, &root.join("copy.bin"), &mut t).unwrap();

        assert_eq!(fs::read(root.join("copy.bin")).unwrap(), data);
        assert_eq!(fs::metadata(root.join("copy.bin")).unwrap().permissions().mode() & 0o777, 0o640);
        assert_eq!(xattr::get(root.join("copy.bin"), "com.example.patina-test").unwrap().as_deref(), Some(&b"tag"[..]));
        assert_eq!(t.progress().done_bytes, data.len() as u64);
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn cross_volume_move_keeps_edits_replacements_and_new_children() {
        for change in ["edit", "replace", "add"] {
            let root = temp_dir(&format!("changed-{change}"));
            let folder = root.join("folder");
            let dest = root.join("dest");
            fs::create_dir(&folder).unwrap();
            fs::create_dir(&dest).unwrap();
            let file = folder.join("notes");
            fs::write(&file, "old").unwrap();
            let cancel = AtomicBool::new(false);
            let mut t = Ticker::new(&cancel, |p| {
                if p.done_items != 2 { return; }
                match change {
                    "edit" => fs::write(&file, "new").unwrap(),
                    "replace" => {
                        let replacement = root.join("new-save");
                        fs::write(&replacement, "new").unwrap();
                        fs::rename(replacement, &file).unwrap();
                    }
                    _ => fs::write(folder.join("new notes"), "also keep me").unwrap(),
                }
            });
            t.every = Duration::ZERO;
            let device = fs::metadata(&dest).unwrap().dev();
            let error = copy_cleanly(&folder, &dest, device, &mut t, true).unwrap_err();
            drop(t);
            assert!(error.to_string().contains("changed"), "{error}");
            if change == "add" {
                assert_eq!(fs::read_to_string(folder.join("new notes")).unwrap(), "also keep me");
            } else {
                assert_eq!(fs::read_to_string(file).unwrap(), "new");
            }
            assert_eq!(fs::read_dir(dest).unwrap().count(), 0);
            fs::remove_dir_all(root).unwrap();
        }
    }

    #[test]
    fn cleanup_restores_a_replacement_and_never_removes_unlisted_children() {
        let root = temp_dir("cleanup-conflict");
        let file = root.join("notes");
        fs::write(&file, "old").unwrap();
        let mut copied = [CopiedEntry { before: fs::symlink_metadata(&file).unwrap(), source: file.clone() }];
        fs::write(root.join("replacement"), "new").unwrap();
        fs::rename(root.join("replacement"), &file).unwrap();
        assert!(remove_copied_source(&file, "notes", &mut copied).is_err());
        assert_eq!(fs::read_to_string(&file).unwrap(), "new");

        let folder = root.join("folder");
        fs::create_dir(&folder).unwrap();
        fs::write(folder.join("added"), "mine").unwrap();
        let mut copied = [CopiedEntry { before: fs::symlink_metadata(&folder).unwrap(), source: folder.clone() }];
        assert!(remove_copied_source(&folder, "folder", &mut copied).is_err());
        assert_eq!(fs::read_to_string(folder.join("added")).unwrap(), "mine");
        assert_eq!(fs::read_dir(&root).unwrap().count(), 2);
        fs::remove_dir_all(root).unwrap();
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn chunked_move_preserves_read_only_files_finder_tags_and_acl() {
        use std::os::unix::fs::PermissionsExt;
        use std::process::Command;
        let root = temp_dir("chunked-metadata");
        let src = root.join("record");
        let dest = root.join("dest");
        fs::create_dir(&dest).unwrap();
        fs::File::create(&src).unwrap().set_len(CHUNKED_MIN).unwrap();
        let mut tag = Vec::new();
        plist::to_writer_binary(&mut tag, &["Work\n6"]).unwrap();
        let key = "com.apple.metadata:_kMDItemUserTags";
        xattr::set(&src, key, &tag).unwrap();
        assert!(Command::new("/bin/chmod").args(["+a", "everyone allow readattr"]).arg(&src).status().unwrap().success());
        fs::set_permissions(&src, fs::Permissions::from_mode(0o444)).unwrap();
        let different_device = fs::metadata(&src).unwrap().dev().wrapping_add(1);
        quiet(|t| copy_cleanly(&src, &dest, different_device, t, true)).unwrap();
        let moved = dest.join("record");
        assert!(!src.exists());
        let meta = fs::metadata(&moved).unwrap();
        assert_eq!(meta.len(), CHUNKED_MIN);
        assert_eq!(meta.permissions().mode() & 0o777, 0o444);
        assert_eq!(xattr::get(&moved, key).unwrap(), Some(tag));
        let acl = Command::new("/bin/ls").arg("-lde").arg(&moved).output().unwrap();
        assert!(String::from_utf8_lossy(&acl.stdout).contains("everyone allow readattr"));
        fs::remove_dir_all(root).unwrap();
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn moves_detect_metadata_a_destination_silently_dropped() {
        use std::os::unix::fs::PermissionsExt;
        use std::process::Command;
        let root = temp_dir("missing-metadata");
        let src = root.join("source");
        let dest = root.join("copy");
        fs::write(&src, "bytes").unwrap();
        fs::write(&dest, "bytes").unwrap();
        let verify = || verify_move_metadata(&src, &dest, &fs::symlink_metadata(&src).unwrap());
        let tags = "com.apple.metadata:_kMDItemUserTags";
        let mut value = Vec::new();
        plist::to_writer_binary(&mut value, &["Work\n6"]).unwrap();
        xattr::set(&src, tags, &value).unwrap();
        assert!(verify().unwrap_err().to_string().contains("Finder tags"));
        xattr::set(&dest, tags, &value).unwrap();
        fs::set_permissions(&src, fs::Permissions::from_mode(0o444)).unwrap();
        assert!(verify().unwrap_err().to_string().contains("read-only"));
        fs::set_permissions(&dest, fs::Permissions::from_mode(0o444)).unwrap();
        assert!(Command::new("/bin/chmod").args(["+a", "everyone allow readattr"]).arg(&src).status().unwrap().success());
        assert!(verify().unwrap_err().to_string().contains("access rules"));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn concurrent_transfers_preserve_every_source() {
        use std::collections::HashSet;
        use std::sync::Barrier;
        for mode in [Mode::Copy, Mode::Move] {
            let root = temp_dir(&format!("concurrent-{mode:?}"));
            let dest = root.join("dest");
            fs::create_dir(&dest).unwrap();
            let barrier = Barrier::new(16);
            let sources: Vec<_> = (0..16).map(|i| {
                let dir = root.join(format!("source-{i}"));
                fs::create_dir(&dir).unwrap();
                let file = dir.join("report.txt");
                fs::write(&file, format!("contents-{i}")).unwrap();
                file
            }).collect();
            std::thread::scope(|scope| {
                for src in &sources {
                    let dest = &dest;
                    let barrier = &barrier;
                    scope.spawn(move || {
                        let cancel = AtomicBool::new(false);
                        let mut ticker = Ticker::new(&cancel, |p| {
                            // Release every job just before destination allocation.
                            if p.current.is_empty() { barrier.wait(); }
                        });
                        transfer(&[src.clone()], dest, mode, &mut ticker).unwrap();
                    });
                }
            });
            let actual: HashSet<_> = fs::read_dir(&dest).unwrap()
                .map(|e| fs::read_to_string(e.unwrap().path()).unwrap()).collect();
            let expected: HashSet<_> = (0..16).map(|i| format!("contents-{i}")).collect();
            assert_eq!(actual, expected);
            assert!(sources.iter().all(|p| p.exists() == (mode == Mode::Copy)));
            fs::remove_dir_all(root).unwrap();
        }
    }

    #[test]
    fn cancelling_a_copy_never_removes_another_jobs_destination() {
        let root = temp_dir("cancel-collision");
        fs::create_dir(root.join("src")).unwrap();
        fs::write(root.join("src/a"), "A").unwrap();
        fs::create_dir(root.join("dest")).unwrap();
        let cancel = AtomicBool::new(false);
        let mut ticker = Ticker::new(&cancel, |p| {
            if p.done_items == 2 {
                fs::create_dir(root.join("dest/src")).unwrap();
                fs::write(root.join("dest/src/keep"), "another job").unwrap();
                cancel.store(true, Ordering::Relaxed);
            }
        });
        ticker.every = Duration::ZERO;
        let err = transfer(&[root.join("src")], &root.join("dest"), Mode::Copy, &mut ticker).unwrap_err();
        assert_eq!(err.kind(), ErrorKind::Interrupted);
        assert_eq!(fs::read_to_string(root.join("dest/src/keep")).unwrap(), "another job");
        assert_eq!(fs::read_dir(root.join("dest")).unwrap().count(), 1);
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn copied_and_moved_directories_keep_metadata() {
        use std::os::unix::fs::PermissionsExt;
        use std::time::UNIX_EPOCH;
        let root = temp_dir("directory-metadata");
        let src = root.join("private");
        fs::create_dir(&src).unwrap();
        fs::write(src.join("child"), "contents").unwrap();
        fs::hard_link(src.join("child"), src.join("another name")).unwrap();
        std::os::unix::fs::symlink("missing", src.join("broken link")).unwrap();
        fs::set_permissions(&src, fs::Permissions::from_mode(0o700)).unwrap();
        let modified = UNIX_EPOCH + Duration::from_secs(1_000_000);
        fs::File::open(&src).unwrap().set_times(fs::FileTimes::new().set_modified(modified)).unwrap();
        let mut tags = Vec::new();
        plist::to_writer_binary(&mut tags, &vec!["Work\n4"]).unwrap();
        xattr::set(&src, "com.apple.metadata:_kMDItemUserTags", &tags).unwrap();
        for moving in [false, true] {
            let dest = root.join(if moving { "moved" } else { "copied" });
            fs::create_dir(&dest).unwrap();
            let device = fs::metadata(&dest).unwrap().dev();
            if moving {
                // Exercise the cross-volume move path without needing a mounted disk.
                quiet(|t| copy_cleanly(&src, &dest, device, t, true)).unwrap();
                assert!(!src.exists());
            } else {
                quiet(|t| transfer(&[src.clone()], &dest, Mode::Copy, t)).unwrap();
            }
            let copy = dest.join("private");
            assert_eq!(fs::metadata(&copy).unwrap().permissions().mode() & 0o777, 0o700);
            assert_eq!(fs::metadata(&copy).unwrap().modified().unwrap(), modified);
            assert_eq!(xattr::get(&copy, "com.apple.metadata:_kMDItemUserTags").unwrap(), Some(tags.clone()));
            assert_eq!(fs::read_to_string(copy.join("child")).unwrap(), "contents");
            assert_eq!(fs::read_link(copy.join("broken link")).unwrap(), Path::new("missing"));
            assert_eq!(fs::read_dir(&dest).unwrap().count(), 1);
        }
        fs::remove_dir_all(root).unwrap();
    }
}
