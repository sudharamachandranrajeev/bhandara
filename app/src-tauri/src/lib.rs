//! Tauri shell around bhandara-core: commands for the web UI and, on
//! desktop, a tray icon with a quick-view flyout.

use bhandara_core::{FolderUsage, Rates, Throughput, Volume};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Manager, State};
use tauri_plugin_opener::OpenerExt;

#[cfg(desktop)]
mod tray;

const FOLDER_LIMIT: usize = 5;
const FOLDER_TTL: Duration = Duration::from_secs(5 * 60);

#[derive(Default)]
struct Storage {
    volumes: Mutex<Vec<Volume>>,
    throughput: Mutex<Throughput>,
    folders: Mutex<Option<(FolderUsage, Instant)>>,
}

type CommandResult<T> = Result<T, String>;

#[tauri::command]
async fn volumes(storage: State<'_, Storage>) -> CommandResult<Vec<Volume>> {
    let volumes = tauri::async_runtime::spawn_blocking(bhandara_core::volumes)
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())?;
    *storage.volumes.lock().unwrap() = volumes.clone();
    Ok(volumes)
}

#[tauri::command]
fn throughput(storage: State<'_, Storage>) -> Option<Rates> {
    let knames: Vec<String> = storage.volumes.lock().unwrap().iter().map(|v| v.kname.clone()).collect();
    storage.throughput.lock().unwrap().poll(&knames)
}

/// The user's home on desktop; the shared storage root on Android.
fn scan_root(app: &AppHandle) -> Option<PathBuf> {
    if cfg!(target_os = "android") {
        Some(PathBuf::from("/storage/emulated/0"))
    } else {
        app.path().home_dir().ok()
    }
}

/// Largest folders in the scan root, cached for five minutes unless `force`.
#[tauri::command]
async fn largest_folders(app: AppHandle, storage: State<'_, Storage>, force: bool) -> CommandResult<FolderUsage> {
    if !force && let Some((usage, at)) = storage.folders.lock().unwrap().as_ref() && at.elapsed() < FOLDER_TTL {
        return Ok(usage.clone());
    }
    let root = scan_root(&app).ok_or("no home directory")?;
    let usage = tauri::async_runtime::spawn_blocking(move || bhandara_core::largest_children(&root, FOLDER_LIMIT))
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())?;
    *storage.folders.lock().unwrap() = Some((usage.clone(), Instant::now()));
    Ok(usage)
}

/// The web UI may only open what it was shown: a known mount point or a
/// listed folder.
fn is_known_path(storage: &Storage, path: &Path) -> bool {
    let mounted = storage.volumes.lock().unwrap().iter().any(|v| v.mounts.iter().any(|m| Path::new(m) == path));
    let listed = storage.folders.lock().unwrap().as_ref().is_some_and(|(usage, _)| usage.folders.iter().any(|f| f.path == path));
    mounted || listed
}

#[tauri::command]
fn open_path(app: AppHandle, storage: State<'_, Storage>, path: PathBuf) -> CommandResult<()> {
    if !is_known_path(&storage, &path) {
        return Err("unknown path".into());
    }
    app.opener().open_path(path.to_string_lossy(), None::<&str>).map_err(|e| e.to_string())
}

#[tauri::command]
async fn eject(storage: State<'_, Storage>, mount: String) -> CommandResult<()> {
    let removable = storage.volumes.lock().unwrap().iter().any(|v| v.removable && v.mount == mount);
    if !removable {
        return Err("not a removable volume".into());
    }
    tauri::async_runtime::spawn_blocking(move || bhandara_core::eject(&mount))
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(Storage::default())
        .invoke_handler(tauri::generate_handler![volumes, throughput, largest_folders, open_path, eject])
        .setup(|_app| {
            #[cfg(desktop)]
            tray::setup(_app)?;
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running Bhandara");
}
