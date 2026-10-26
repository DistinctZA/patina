mod fs_ops;

use fs_ops::{Listing, SortBy};
use notify::{RecommendedWatcher, RecursiveMode, Watcher};
use serde::Serialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_opener::OpenerExt;

/// One folder watcher per pane; replacing an entry drops (stops) the old one.
#[derive(Default)]
struct Watchers(Mutex<HashMap<String, RecommendedWatcher>>);

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
        .invoke_handler(tauri::generate_handler![
            home_dir,
            list_dir,
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
