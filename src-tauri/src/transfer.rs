//! Copying and moving files, with progress and cancel for the progress panel.
//!
//! Within a disk, moves are renames and copies are APFS clones, so both are
//! near-instant. Big files going to another disk are copied in chunks, so the
//! progress bar moves and Cancel works part-way through a file.

use crate::fs_ops::{check_not_into_itself, file_name, unique_target};
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
                let (bytes, items) = measure(src);
                t.state.total_bytes += bytes;
                t.state.total_items += items;
            }
        }
    }
    t.emit(true);

    for (src, dest_dir, action) in plan {
        t.check()?;
        check_not_into_itself(&src, &dest_dir)?;
        let is_dir = fs::symlink_metadata(&src)?.is_dir();
        let target = unique_target(&dest_dir, &file_name(&src)?, is_dir);
        let dest_dev = fs::metadata(&dest_dir)?.dev();
        match action {
            Action::Rename => {
                t.working_on("Moving", &src);
                match fs::rename(&src, &target) {
                    Ok(()) => t.add(0, 1)?,
                    // A different volume after all (e.g. a mount point): copy instead.
                    Err(e) if e.kind() == ErrorKind::CrossesDevices => {
                        let (bytes, items) = measure(&src);
                        t.state.total_bytes += bytes;
                        t.state.total_items += items.saturating_sub(1);
                        copy_then_delete(&src, &target, dest_dev, t)?;
                    }
                    Err(e) => return Err(e),
                }
            }
            Action::Copy => {
                t.working_on("Copying", &src);
                copy_cleanly(&src, &target, dest_dev, t)?;
            }
            Action::CopyThenDelete => {
                t.working_on("Moving", &src);
                copy_then_delete(&src, &target, dest_dev, t)?;
            }
        }
    }
    t.emit(true);
    Ok(())
}

fn copy_then_delete(src: &Path, target: &Path, dest_dev: u64, t: &mut Ticker) -> io::Result<()> {
    copy_cleanly(src, target, dest_dev, t)?;
    if fs::symlink_metadata(src)?.is_dir() {
        fs::remove_dir_all(src)
    } else {
        fs::remove_file(src)
    }
}

/// Copies an item; if that fails or is cancelled part-way, removes the
/// half-made copy so no partial files are left behind.
fn copy_cleanly(src: &Path, target: &Path, dest_dev: u64, t: &mut Ticker) -> io::Result<()> {
    let result = copy_tree(src, target, dest_dev, t);
    if result.is_err() {
        let _ = match fs::symlink_metadata(target) {
            Ok(m) if m.is_dir() => fs::remove_dir_all(target),
            Ok(_) => fs::remove_file(target),
            Err(_) => Ok(()),
        };
    }
    result
}

/// Total bytes and item count (files, folders, links) under a path.
fn measure(path: &Path) -> (u64, u64) {
    let Ok(meta) = fs::symlink_metadata(path) else { return (0, 1) };
    if meta.is_dir() {
        fs::read_dir(path)
            .into_iter()
            .flatten()
            .filter_map(Result::ok)
            .fold((0, 1), |(b, n), e| {
                let (eb, en) = measure(&e.path());
                (b + eb, n + en)
            })
    } else if meta.file_type().is_symlink() {
        (0, 1)
    } else {
        (meta.len(), 1)
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
        Ok(())
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
}
