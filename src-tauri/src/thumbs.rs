//! Thumbnails made by macOS itself: Quick Look (the same previews Finder
//! shows) for files, and the Finder icon for `.app` bundles, where Quick Look
//! can hang. Results are cached on disk, keyed by path, size and mtime.

use std::collections::hash_map::DefaultHasher;
use std::fs;
use std::hash::{Hash, Hasher};
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

const TIMEOUT: Duration = Duration::from_secs(10);

pub fn thumbnail(cache_dir: &Path, path: &Path, size: u32) -> io::Result<Option<PathBuf>> {
    let meta = fs::metadata(path)?;
    let mut h = DefaultHasher::new();
    path.hash(&mut h);
    meta.len().hash(&mut h);
    meta.modified().ok().hash(&mut h);
    size.hash(&mut h);
    let key = format!("{:016x}", h.finish());

    fs::create_dir_all(cache_dir)?;
    let png = cache_dir.join(format!("{key}.png"));
    if png.exists() {
        return Ok(Some(png));
    }
    // Remember files that have no preview so we don't ask again.
    let miss = cache_dir.join(format!("{key}.none"));
    if miss.exists() {
        return Ok(None);
    }

    let work = cache_dir.join(format!("tmp-{key}"));
    let _ = fs::remove_dir_all(&work);
    fs::create_dir_all(&work)?;

    let made = if meta.is_dir() {
        match system_icon_png(path, size) {
            Some(bytes) => {
                let out = work.join("icon.png");
                fs::write(&out, bytes)?;
                Some(out)
            }
            None => None,
        }
    } else {
        let mut cmd = Command::new("/usr/bin/qlmanage");
        cmd.args(["-t", "-s", &size.to_string(), "-o"]).arg(&work).arg(path);
        run(cmd)?;
        // qlmanage names its output after the input file, plus ".png".
        fs::read_dir(&work)?
            .filter_map(Result::ok)
            .map(|e| e.path())
            .find(|p| p.extension().is_some_and(|e| e == "png"))
    };

    let result = match made.filter(|p| p.exists()) {
        Some(p) => {
            fs::rename(&p, &png)?;
            Some(png)
        }
        None => {
            fs::write(&miss, b"")?;
            None
        }
    };
    let _ = fs::remove_dir_all(&work);
    Ok(result)
}

/// The icon Finder shows for `path`, as PNG bytes. Asking AppKit (rather than
/// reading the bundle's .icns) also covers apps whose real icon lives only in
/// an asset catalog, such as Books.
#[cfg(target_os = "macos")]
fn system_icon_png(path: &Path, size: u32) -> Option<Vec<u8>> {
    use objc2::AnyThread as _;
    use objc2_app_kit::{NSBitmapImageFileType, NSBitmapImageRep, NSWorkspace};
    use objc2_foundation::{NSDictionary, NSPoint, NSRect, NSSize, NSString};

    let image = NSWorkspace::sharedWorkspace().iconForFile(&NSString::from_str(path.to_str()?));
    let side = f64::from(size);
    let mut rect = NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(side, side));
    let cg = unsafe { image.CGImageForProposedRect_context_hints(&mut rect, None, None) }?;
    let rep = NSBitmapImageRep::initWithCGImage(NSBitmapImageRep::alloc(), &cg);
    let png = unsafe { rep.representationUsingType_properties(NSBitmapImageFileType::PNG, &NSDictionary::new()) }?;
    Some(png.to_vec())
}

#[cfg(not(target_os = "macos"))]
fn system_icon_png(_path: &Path, _size: u32) -> Option<Vec<u8>> {
    None
}

/// Runs a command quietly, killing it if it takes longer than `TIMEOUT`.
fn run(mut cmd: Command) -> io::Result<()> {
    let mut child = cmd.stdout(Stdio::null()).stderr(Stdio::null()).spawn()?;
    let started = Instant::now();
    while child.try_wait()?.is_none() {
        if started.elapsed() > TIMEOUT {
            let _ = child.kill();
            break;
        }
        thread::sleep(Duration::from_millis(15));
    }
    // Reap the process so it doesn't linger as a zombie.
    let _ = child.wait();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_icons_are_made_and_cached() {
        let cache = std::env::temp_dir().join(format!("patina-thumbs-{}", std::process::id()));
        let app = Path::new("/System/Applications/Calculator.app");
        if !app.exists() {
            return;
        }
        let png = thumbnail(&cache, app, 64).unwrap().expect("Calculator has an icon");
        assert!(fs::metadata(&png).unwrap().len() > 0);
        // A second call is served from the cache.
        assert_eq!(thumbnail(&cache, app, 64).unwrap(), Some(png));
        fs::remove_dir_all(&cache).unwrap();
    }

    #[test]
    fn asset_catalog_icons_are_not_blank() {
        // Books ships a blank placeholder .icns; its real icon is in Assets.car.
        let app = Path::new("/System/Applications/Books.app");
        if !app.exists() {
            return;
        }
        let png = system_icon_png(app, 128).expect("Books has an icon");
        // A blank 128px PNG compresses to ~1 KB; a real icon is far larger.
        assert!(png.len() > 8_000, "icon looks blank ({} bytes)", png.len());
    }
}
