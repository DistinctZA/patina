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
                        copy_then_delete(&src, &dest_dir, dest_dev, t)?;
                    }
                    Err(e) => return Err(e),
                }
            }
            Action::Copy => {
                t.working_on("Copying", &src);
                copy_cleanly(&src, &dest_dir, dest_dev, t)?;
            }
            Action::CopyThenDelete => {
                t.working_on("Moving", &src);
                copy_then_delete(&src, &dest_dir, dest_dev, t)?;
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
            Err(e) => return Err(e),
        }
    }
}

fn copy_then_delete(src: &Path, dest_dir: &Path, dest_dev: u64, t: &mut Ticker) -> io::Result<()> {
    copy_cleanly(src, dest_dir, dest_dev, t)?;
    if fs::symlink_metadata(src)?.is_dir() {
        fs::remove_dir_all(src)
    } else {
        fs::remove_file(src)
    }
}

/// Build the copy in a private directory, then publish it atomically. Failure
/// cleanup can only touch this job's partial copy, never another job's files.
fn copy_cleanly(src: &Path, dest_dir: &Path, dest_dev: u64, t: &mut Ticker) -> io::Result<()> {
    let staging = StagingDir::new(dest_dir)?;
    let partial = staging.path.join("item");
    let result = (|| {
        copy_tree(src, &partial, dest_dev, t)?;
        t.check()?;
        publish(&partial, dest_dir, &file_name(src)?)?;
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
fn copy_tree(src: &Path, dest: &Path, dest_dev: u64, t: &mut Ticker) -> io::Result<()> {
    t.check()?;
    let meta = fs::symlink_metadata(src)?;
    if meta.file_type().is_symlink() {
        std::os::unix::fs::symlink(fs::read_link(src)?, dest)?;
        t.add(0, 1)
    } else if meta.is_dir() {
        fs::create_dir(dest)?;
        t.add(0, 1)?;
        for item in fs::read_dir(src)? {
            let item = item?;
            copy_tree(&item.path(), &dest.join(item.file_name()), dest_dev, t)?;
        }
        // Apply metadata after the children, so copying still works for
        // read-only folders and doesn't change the preserved modification date.
        copy_directory_metadata(src, dest)
    } else {
        t.working_on(t.state.verb, src);
        if meta.dev() != dest_dev && meta.len() >= CHUNKED_MIN {
            copy_chunked(src, dest, &meta, t)?;
        } else {
            // On APFS within a disk this is a clone: instant, and takes no space.
            fs::copy(src, dest)?;
            t.add(meta.len(), 0)?;
        }
        t.add(0, 1)
    }
}

#[cfg(target_os = "macos")]
fn copy_directory_metadata(src: &Path, dest: &Path) -> io::Result<()> {
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
fn copy_directory_metadata(src: &Path, dest: &Path) -> io::Result<()> {
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
fn copy_chunked(src: &Path, dest: &Path, meta: &fs::Metadata, t: &mut Ticker) -> io::Result<()> {
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
    writer.set_permissions(meta.permissions())?;
    if let Ok(modified) = meta.modified() {
        writer.set_modified(modified)?;
    }
    if let Ok(names) = xattr::list(src) {
        for name in names {
            if let Ok(Some(value)) = xattr::get(src, &name) {
                let _ = xattr::set(dest, &name, &value);
            }
        }
    }
    Ok(())
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
        let meta = fs::metadata(&src).unwrap();
        copy_chunked(&src, &root.join("copy.bin"), &meta, &mut t).unwrap();

        assert_eq!(fs::read(root.join("copy.bin")).unwrap(), data);
        assert_eq!(fs::metadata(root.join("copy.bin")).unwrap().permissions().mode() & 0o777, 0o640);
        assert_eq!(xattr::get(root.join("copy.bin"), "com.example.patina-test").unwrap().as_deref(), Some(&b"tag"[..]));
        assert_eq!(t.progress().done_bytes, data.len() as u64);
        fs::remove_dir_all(&root).unwrap();
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
                quiet(|t| copy_then_delete(&src, &dest, device, t)).unwrap();
                assert!(!src.exists());
            } else {
                quiet(|t| transfer(&[src.clone()], &dest, Mode::Copy, t)).unwrap();
            }
            let copy = dest.join("private");
            assert_eq!(fs::metadata(&copy).unwrap().permissions().mode() & 0o777, 0o700);
            assert_eq!(fs::metadata(&copy).unwrap().modified().unwrap(), modified);
            assert_eq!(xattr::get(&copy, "com.apple.metadata:_kMDItemUserTags").unwrap(), Some(tags.clone()));
            assert_eq!(fs::read_to_string(copy.join("child")).unwrap(), "contents");
            assert_eq!(fs::read_dir(&dest).unwrap().count(), 1);
        }
        fs::remove_dir_all(root).unwrap();
    }
}
