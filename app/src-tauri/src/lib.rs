//! Tauri shell around bhandara-core: commands for the web UI and, on
//! desktop, a tray icon with a quick-view flyout.

use bhandara_core::{Rates, Throughput, Volume};
use std::sync::Mutex;
use tauri::State;
use tauri_plugin_storage_access::StorageAccessExt;

mod health;
mod suchi;
#[cfg(desktop)]
mod tray;

#[derive(Default)]
struct Storage {
    volumes: Mutex<Vec<Volume>>,
    throughput: Mutex<Throughput>,
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

/// Opens Android's "All files access" screen; a no-op on desktop.
#[tauri::command]
fn request_storage_access(app: tauri::AppHandle) -> CommandResult<()> {
    app.storage_access().request().map_err(|e| e.to_string())
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

#[tauri::command]
async fn health() -> health::HealthStatus {
    tauri::async_runtime::spawn_blocking(health::status).await.unwrap_or(health::HealthStatus {
        state: health::State::Missing,
        report: None,
        can_install: false,
    })
}

/// Windows: one UAC prompt to schedule the health helper.
#[tauri::command]
async fn install_health() -> CommandResult<()> {
    tauri::async_runtime::spawn_blocking(health::install).await.map_err(|e| e.to_string())?
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let builder = tauri::Builder::default();
    #[cfg(desktop)]
    let builder = builder.plugin(tauri_plugin_notification::init());
    builder
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_storage_access::init())
        .manage(Storage::default())
        .manage(suchi::Suchi::default())
        .invoke_handler(tauri::generate_handler![
            volumes,
            throughput,
            request_storage_access,
            eject,
            health,
            install_health,
            suchi::scan_roots,
            suchi::scan,
            suchi::stop_scan,
            suchi::scan_summary,
            suchi::folder,
            suchi::file_types,
            suchi::files,
            suchi::duplicates,
            suchi::export_duplicates,
            suchi::rescan,
            suchi::delete_item,
            suchi::open_item,
            suchi::open_root,
            suchi::exclusions,
            suchi::exclude,
            suchi::include,
        ])
        .setup(|_app| {
            #[cfg(desktop)]
            {
                tray::setup(_app)?;
                health::watch(_app.handle().clone());
            }
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running Bhandara");
}
