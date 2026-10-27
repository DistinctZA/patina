mod fs_ops;
mod pasteboard;
mod thumbs;

use fs_ops::{Details, Listing, Sidebar, SortBy, TextPreview, Transfer};
use notify::{RecommendedWatcher, RecursiveMode, Watcher};
use serde::Serialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
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
        .map_err(|e| e.to_string())
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
        // Write then rename, so a crash mid-save can't leave a half-written file.
        let tmp = path.with_extension("json.tmp");
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

/// Copies or moves files into `dest`. `mode` is "copy", "move", or "auto"
/// (move within a disk, copy to another one, like dragging in Finder).
#[tauri::command]
async fn transfer(sources: Vec<String>, dest: String, mode: String) -> Result<(), String> {
    blocking(move || fs_ops::transfer(&to_paths(sources), Path::new(&dest), Transfer::parse(&mode))).await
}

#[tauri::command]
async fn duplicate(paths: Vec<String>) -> Result<(), String> {
    blocking(move || fs_ops::duplicate(&to_paths(paths))).await
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
async fn copy_into(sources: Vec<String>, dest: String) -> Result<(), String> {
    blocking(move || fs_ops::copy_into(&to_paths(sources), Path::new(&dest))).await
}

#[tauri::command]
async fn move_into(sources: Vec<String>, dest: String) -> Result<(), String> {
    blocking(move || fs_ops::move_into(&to_paths(sources), Path::new(&dest))).await
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

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(Watchers::default())
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
            copy_into,
            move_into,
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
