mod fs_ops;
mod pasteboard;
mod thumbs;
mod transfer;

use fs_ops::{Details, Listing, Sidebar, SortBy, TextPreview};
use notify::{RecommendedWatcher, RecursiveMode, Watcher};
use serde::Serialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_opener::OpenerExt;
use tokio::sync::Semaphore;

/// One folder watcher per pane; replacing an entry drops (stops) the old one.
#[derive(Default)]
struct Watchers(Mutex<HashMap<String, RecommendedWatcher>>);

/// Caps how many Quick Look processes run at once while a grid scrolls.
struct ThumbSlots(Semaphore);

#[derive(Serialize, Clone)]
struct DirChanged {
    pane: String,
    path: String,
}

/// Runs blocking filesystem work off the main thread and turns errors into
/// messages the UI can show.
async fn blocking<T, F>(f: F) -> Result<T, String>
where
    T: Send + 'static,
    F: FnOnce() -> std::io::Result<T> + Send + 'static,
{
    tauri::async_runtime::spawn_blocking(f)
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| describe(&e))
}

/// Plain words for the common failures; Patina's own messages pass through.
fn describe(e: &std::io::Error) -> String {
    use std::io::ErrorKind::*;
    match e.kind() {
        StorageFull => "There isn't enough space on the destination disk".into(),
        ReadOnlyFilesystem => "That disk is read-only".into(),
        PermissionDenied => "Permission denied: you may need to allow Patina in Privacy & Security, or change the folder's permissions".into(),
        NotFound if e.raw_os_error().is_some() => "The item couldn't be found; it may have been moved or deleted".into(),
        _ => e.to_string(),
    }
}

fn to_paths(paths: Vec<String>) -> Vec<PathBuf> {
    paths.into_iter().map(PathBuf::from).collect()
}

#[tauri::command]
fn home_dir(app: AppHandle) -> Result<String, String> {
    app.path()
        .home_dir()
        .map(|p| p.to_string_lossy().into_owned())
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn list_dir(path: String, show_hidden: bool, sort_by: String, desc: bool) -> Result<Listing, String> {
    blocking(move || fs_ops::list_dir(Path::new(&path), show_hidden, SortBy::parse(&sort_by), desc)).await
}

#[tauri::command]
async fn find_tagged(tag: String, show_hidden: bool, sort_by: String, desc: bool) -> Result<Listing, String> {
    blocking(move || fs_ops::find_tagged(&tag, show_hidden, SortBy::parse(&sort_by), desc)).await
}

#[tauri::command]
async fn sidebar(app: AppHandle, favorites: Option<Vec<String>>) -> Result<Sidebar, String> {
    let home = app.path().home_dir().map_err(|e| e.to_string())?;
    blocking(move || Ok(fs_ops::sidebar(&home, favorites.as_deref()))).await
}

fn settings_path(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(app.path().app_config_dir().map_err(|e| e.to_string())?.join("settings.json"))
}

/// The UI's saved settings (favourites, collapsed sidebar sections), or an
/// empty object on first run. Kept in ~/Library/Application Support/<id>.
#[tauri::command]
async fn read_settings(app: AppHandle) -> Result<serde_json::Value, String> {
    let path = settings_path(&app)?;
    blocking(move || match std::fs::read(&path) {
        Ok(bytes) => Ok(serde_json::from_slice(&bytes).unwrap_or_else(|_| serde_json::json!({}))),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(serde_json::json!({})),
        Err(e) => Err(e),
    })
    .await
}

#[tauri::command]
async fn write_settings(app: AppHandle, settings: serde_json::Value) -> Result<(), String> {
    let path = settings_path(&app)?;
    blocking(move || {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        // Write then rename, so a crash mid-save can't leave a half-written
        // file. Each save gets its own temporary name, so two saves can never
        // write into the same one.
        static SAVES: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let tmp = path.with_extension(format!("json.{}.tmp", SAVES.fetch_add(1, Ordering::Relaxed)));
        std::fs::write(&tmp, serde_json::to_vec_pretty(&settings)?)?;
        std::fs::rename(&tmp, &path)
    })
    .await
}

#[tauri::command]
async fn details(path: String) -> Result<Details, String> {
    blocking(move || fs_ops::details(Path::new(&path))).await
}

#[tauri::command]
async fn read_text(path: String) -> Result<Option<TextPreview>, String> {
    blocking(move || fs_ops::read_text(Path::new(&path), 64 * 1024)).await
}

/// Path to a cached PNG thumbnail, or `None` when Quick Look has no preview.
#[tauri::command]
async fn thumbnail(
    app: AppHandle,
    slots: State<'_, ThumbSlots>,
    path: String,
    size: u32,
) -> Result<Option<String>, String> {
    let cache = app.path().app_cache_dir().map_err(|e| e.to_string())?.join("thumbs");
    let _slot = slots.0.acquire().await.map_err(|e| e.to_string())?;
    blocking(move || thumbs::thumbnail(&cache, Path::new(&path), size))
        .await
        .map(|p| p.map(|p| p.to_string_lossy().into_owned()))
}

/// Running copies and moves, by the id the UI gave them, so they can be cancelled.
#[derive(Default)]
struct Jobs(Mutex<HashMap<String, Arc<AtomicBool>>>);

#[derive(Serialize, Clone)]
struct JobProgress {
    id: String,
    #[serde(flatten)]
    progress: transfer::Progress,
}

/// Runs a copy/move job on a blocking thread, emitting `transfer-progress`
/// as it goes. A cancelled job fails with "Cancelled".
async fn run_job(
    app: AppHandle,
    jobs: State<'_, Jobs>,
    id: String,
    work: impl FnOnce(&mut transfer::Ticker) -> std::io::Result<()> + Send + 'static,
) -> Result<(), String> {
    let cancel = Arc::new(AtomicBool::new(false));
    jobs.0.lock().unwrap().insert(id.clone(), cancel.clone());
    let job_id = id.clone();
    let result = blocking(move || {
        let mut ticker = transfer::Ticker::new(&cancel, |progress| {
            let _ = app.emit("transfer-progress", JobProgress { id: job_id.clone(), progress: progress.clone() });
        });
        work(&mut ticker)
    })
    .await;
    jobs.0.lock().unwrap().remove(&id);
    result
}

/// Copies or moves files into `dest`. `mode` is "copy", "move", or "auto"
/// (move within a disk, copy to another one, like dragging in Finder).
#[tauri::command]
async fn transfer(
    app: AppHandle,
    jobs: State<'_, Jobs>,
    id: String,
    sources: Vec<String>,
    dest: String,
    mode: String,
) -> Result<(), String> {
    let mode = transfer::Mode::parse(&mode);
    run_job(app, jobs, id, move |t| transfer::transfer(&to_paths(sources), Path::new(&dest), mode, t)).await
}

#[tauri::command]
async fn duplicate(app: AppHandle, jobs: State<'_, Jobs>, id: String, paths: Vec<String>) -> Result<(), String> {
    run_job(app, jobs, id, move |t| transfer::duplicate(&to_paths(paths), t)).await
}

#[tauri::command]
fn cancel_transfer(jobs: State<Jobs>, id: String) {
    if let Some(flag) = jobs.0.lock().unwrap().get(&id) {
        flag.store(true, Ordering::Relaxed);
    }
}

/// Renames several items at once: `renames` is a list of (path, new name).
#[tauri::command]
async fn rename_many(renames: Vec<(String, String)>) -> Result<(), String> {
    blocking(move || {
        let renames: Vec<(PathBuf, String)> = renames.into_iter().map(|(p, n)| (PathBuf::from(p), n)).collect();
        fs_ops::rename_many(&renames)
    })
    .await
}

// Clipboard commands are synchronous, so Tauri runs them on the main thread,
// where AppKit expects the pasteboard to be used.
#[tauri::command]
fn clipboard_copy(paths: Vec<String>) -> Result<(), String> {
    if pasteboard::copy_files(&paths) {
        Ok(())
    } else {
        Err("Couldn't put the files on the clipboard".into())
    }
}

#[tauri::command]
fn clipboard_paste() -> Vec<String> {
    pasteboard::pasted_files()
}

/// Hands a drag that left the window over to macOS, so the files can be
/// dropped into Finder, Mail, the desktop or any other app. Runs on the main
/// thread (sync command), as AppKit requires. Emits `drag-out-done` when the
/// drag ends, so panes can refresh if the files were moved.
#[cfg(target_os = "macos")]
#[tauri::command]
fn drag_out(app: AppHandle, window: tauri::Window, paths: Vec<String>, move_files: bool) -> Result<(), String> {
    let icon = paths
        .first()
        .and_then(|p| thumbs::system_icon_png(Path::new(p), 64))
        .ok_or("Couldn't make a drag image")?;
    let item = drag::DragItem::Files(to_paths(paths));
    let options = drag::Options {
        mode: if move_files { drag::DragMode::Move } else { drag::DragMode::Copy },
        ..Default::default()
    };
    drag::start_drag(
        &window,
        item,
        drag::Image::Raw(icon),
        move |result, _| {
            let _ = app.emit("drag-out-done", matches!(result, drag::DragResult::Dropped));
        },
        options,
    )
    .map_err(|e| e.to_string())
}

#[cfg(not(target_os = "macos"))]
#[tauri::command]
fn drag_out(_paths: Vec<String>, _move_files: bool) -> Result<(), String> {
    Err("Dragging out isn't supported on this system".into())
}

#[tauri::command]
async fn rename(path: String, new_name: String) -> Result<String, String> {
    blocking(move || fs_ops::rename(Path::new(&path), &new_name))
        .await
        .map(|p| p.to_string_lossy().into_owned())
}

#[tauri::command]
async fn create_folder(dir: String, name: String) -> Result<String, String> {
    blocking(move || fs_ops::create_folder(Path::new(&dir), &name))
        .await
        .map(|p| p.to_string_lossy().into_owned())
}

#[tauri::command]
async fn move_to_trash(paths: Vec<String>) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || trash::delete_all(paths))
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())
}

/// Opens a file in its default app, like double-clicking it in Finder.
#[tauri::command]
fn open_path(app: AppHandle, path: String) -> Result<(), String> {
    app.opener().open_path(path, None::<&str>).map_err(|e| e.to_string())
}

#[tauri::command]
fn reveal_in_finder(app: AppHandle, path: String) -> Result<(), String> {
    app.opener().reveal_item_in_dir(path).map_err(|e| e.to_string())
}

/// Watches the folder a pane shows and emits `dir-changed` when anything in it
/// changes, so the pane refreshes without the user pressing anything.
#[tauri::command]
fn watch_dir(app: AppHandle, watchers: State<Watchers>, pane: String, path: String) -> Result<(), String> {
    let payload = DirChanged { pane: pane.clone(), path: path.clone() };
    let mut watcher = notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
        if res.is_ok() {
            let _ = app.emit("dir-changed", payload.clone());
        }
    })
    .map_err(|e| e.to_string())?;
    watcher
        .watch(Path::new(&path), RecursiveMode::NonRecursive)
        .map_err(|e| e.to_string())?;
    watchers.0.lock().unwrap().insert(pane, watcher);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::describe;
    use std::io::{Error, ErrorKind};

    #[test]
    fn common_errors_read_plainly() {
        // ENOSPC, as a full disk reports it.
        assert_eq!(describe(&Error::from_raw_os_error(28)), "There isn't enough space on the destination disk");
        assert!(describe(&Error::from_raw_os_error(2)).starts_with("The item couldn't be found"));
        // Patina's own messages are kept.
        assert_eq!(describe(&Error::new(ErrorKind::AlreadyExists, "\"a\" already exists")), "\"a\" already exists");
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        // Updates: download, check the signature against the key in
        // tauri.conf.json, install, then restart.
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .manage(Watchers::default())
        .manage(Jobs::default())
        .manage(ThumbSlots(Semaphore::new(4)))
        .setup(|app| {
            // Tell the sidebar when a drive is plugged in or ejected.
            let handle = app.handle().clone();
            let mut watcher = notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
                if res.is_ok() {
                    let _ = handle.emit("volumes-changed", ());
                }
            })?;
            watcher.watch(Path::new("/Volumes"), RecursiveMode::NonRecursive)?;
            app.state::<Watchers>().0.lock().unwrap().insert("volumes".into(), watcher);

            // Keep the thumbnail cache from growing forever.
            if let Ok(cache) = app.path().app_cache_dir() {
                std::thread::spawn(move || thumbs::prune(&cache.join("thumbs")));
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            home_dir,
            list_dir,
            find_tagged,
            sidebar,
            read_settings,
            write_settings,
            details,
            read_text,
            thumbnail,
            transfer,
            duplicate,
            clipboard_copy,
            clipboard_paste,
            drag_out,
            cancel_transfer,
            rename_many,
            rename,
            create_folder,
            move_to_trash,
            open_path,
            reveal_in_finder,
            watch_dir,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
