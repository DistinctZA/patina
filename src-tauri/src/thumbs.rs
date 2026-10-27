//! Thumbnails made by macOS itself: Quick Look's thumbnail service (the same
//! previews Finder shows) for files, and the Finder icon for `.app` bundles.
//! Results are cached on disk, keyed by path, size and mtime.

use std::collections::hash_map::DefaultHasher;
use std::fs;
use std::hash::{Hash, Hasher};
use std::io;
use std::path::{Path, PathBuf};
use std::time::Duration;

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
        let out = work.join("thumb.png");
        quick_look_png(&preview_source(path), size, &out).then_some(out)
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
pub fn system_icon_png(path: &Path, size: u32) -> Option<Vec<u8>> {
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
pub fn system_icon_png(_path: &Path, _size: u32) -> Option<Vec<u8>> {
    None
}

const KEEP_FOR: Duration = Duration::from_secs(60 * 24 * 60 * 60);
const MAX_CACHE_BYTES: u64 = 400 << 20;
const PRUNED_CACHE_BYTES: u64 = 300 << 20;

/// Trims the cache: leftover work folders, entries older than 60 days, then
/// the oldest entries while it's over 400 MB. Runs in the background at launch.
pub fn prune(cache_dir: &Path) {
    let Ok(items) = fs::read_dir(cache_dir) else { return };
    let now = std::time::SystemTime::now();
    let age = |m: &fs::Metadata| m.modified().ok().and_then(|t| now.duration_since(t).ok()).unwrap_or_default();

    let mut kept: Vec<(PathBuf, u64, Duration)> = Vec::new();
    for item in items.filter_map(Result::ok) {
        let path = item.path();
        let Ok(meta) = item.metadata() else { continue };
        let name = item.file_name().to_string_lossy().into_owned();
        if meta.is_dir() {
            // An interrupted thumbnail's work folder.
            if name.starts_with("tmp-") && age(&meta) > Duration::from_secs(3600) {
                let _ = fs::remove_dir_all(&path);
            }
        } else if age(&meta) > KEEP_FOR {
            let _ = fs::remove_file(&path);
        } else {
            kept.push((path, meta.len(), age(&meta)));
        }
    }

    let mut total: u64 = kept.iter().map(|(_, size, _)| size).sum();
    if total > MAX_CACHE_BYTES {
        kept.sort_by(|a, b| b.2.cmp(&a.2)); // oldest first
        for (path, size, _) in kept {
            if total <= PRUNED_CACHE_BYTES {
                break;
            }
            if fs::remove_file(&path).is_ok() {
                total -= size;
            }
        }
    }
}

/// The file whose picture represents `path`. Wallpapers in
/// /System/Library/Desktop Pictures are small `.madesktop` descriptors that
/// Quick Look can't draw; each names a ready-made image to use instead.
fn preview_source(path: &Path) -> PathBuf {
    if path.extension().is_some_and(|e| e == "madesktop") {
        let named = plist::Value::from_file(path).ok().and_then(|v| {
            v.as_dictionary()?
                .get("thumbnailPath")?
                .as_string()
                .map(PathBuf::from)
        });
        if let Some(image) = named.filter(|p| p.is_file()) {
            return image;
        }
    }
    path.to_path_buf()
}

/// Asks Quick Look's thumbnail service (as Finder does) to write a PNG
/// thumbnail of `path` to `out`. Unlike the `qlmanage` debugging tool it
/// replaces, it needs no extra process and fails fast instead of hanging on
/// files it can't draw. Returns false when there's no thumbnail (the UI then
/// shows a file-type icon).
#[cfg(target_os = "macos")]
fn quick_look_png(path: &Path, size: u32, out: &Path) -> bool {
    use block2::RcBlock;
    use objc2::AnyThread as _;
    use objc2_core_foundation::CGSize;
    use objc2_foundation::{NSError, NSString, NSURL};
    use objc2_quick_look_thumbnailing::{
        QLThumbnailGenerationRequest, QLThumbnailGenerationRequestRepresentationTypes as Kind, QLThumbnailGenerator,
    };
    use std::sync::mpsc;

    let (Some(src), Some(dst)) = (path.to_str(), out.to_str()) else {
        return false;
    };
    let src = NSURL::fileURLWithPath(&NSString::from_str(src));
    let dst = NSURL::fileURLWithPath(&NSString::from_str(dst));
    let side = f64::from(size);
    let request = unsafe {
        QLThumbnailGenerationRequest::initWithFileAtURL_size_scale_representationTypes(
            QLThumbnailGenerationRequest::alloc(),
            &src,
            CGSize::new(side, side),
            1.0,
            // A real preview only: files without one fall back to the UI's
            // file-type icons rather than the system's generic ones.
            Kind::Thumbnail,
        )
    };

    let (tx, rx) = mpsc::channel();
    let done = RcBlock::new(move |err: *mut NSError| {
        let _ = tx.send(err.is_null());
    });
    let generator = unsafe { QLThumbnailGenerator::sharedGenerator() };
    // The replacement (`asContentType`) differs only in taking a UTType.
    #[allow(deprecated)]
    unsafe {
        generator.saveBestRepresentationForRequest_toFileAtURL_withContentType_completionHandler(
            &request,
            &dst,
            &NSString::from_str("public.png"),
            &done,
        );
    }
    match rx.recv_timeout(TIMEOUT) {
        Ok(saved) => saved && out.exists(),
        Err(_) => {
            unsafe { generator.cancelRequest(&request) };
            false
        }
    }
}

#[cfg(not(target_os = "macos"))]
fn quick_look_png(_path: &Path, _size: u32, _out: &Path) -> bool {
    false
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
    fn wallpapers_and_images_get_thumbnails() {
        let cache = std::env::temp_dir().join(format!("patina-ql-{}", std::process::id()));
        let wallpaper = Path::new("/System/Library/Desktop Pictures/Big Sur Aerial.madesktop");
        if wallpaper.exists() {
            let png = thumbnail(&cache, wallpaper, 128).unwrap().expect("wallpaper thumbnail");
            assert!(fs::metadata(&png).unwrap().len() > 2_000);
        }
        // A plain text file: Quick Look renders its first page.
        let text = cache.join("note.txt");
        fs::create_dir_all(&cache).unwrap();
        fs::write(&text, "Patina thumbnail test\n".repeat(40)).unwrap();
        assert!(thumbnail(&cache, &text, 128).unwrap().is_some());
        fs::remove_dir_all(&cache).unwrap();
    }

    #[test]
    fn prune_removes_old_entries_and_leftovers() {
        let cache = std::env::temp_dir().join(format!("patina-prune-{}", std::process::id()));
        let _ = fs::remove_dir_all(&cache);
        fs::create_dir_all(cache.join("tmp-stale")).unwrap();
        fs::write(cache.join("fresh.png"), b"x").unwrap();
        fs::write(cache.join("old.png"), b"x").unwrap();
        let long_ago = std::time::SystemTime::now() - Duration::from_secs(90 * 24 * 60 * 60);
        fs::File::options().write(true).open(cache.join("old.png")).unwrap().set_modified(long_ago).unwrap();
        fs::File::open(cache.join("tmp-stale")).unwrap().set_modified(long_ago).unwrap();

        prune(&cache);
        assert!(cache.join("fresh.png").exists());
        assert!(!cache.join("old.png").exists());
        assert!(!cache.join("tmp-stale").exists());
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
