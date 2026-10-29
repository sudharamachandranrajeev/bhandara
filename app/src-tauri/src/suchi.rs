//! Suchi: the Files view. Holds one index at a time and answers the UI's
//! questions about it. The UI refers to folders and files by index id, never
//! by path, so it can only act on what the scan found.

use crate::{CommandResult, Storage};
use bhandara_core::index::{self, FileFilter, FileView, FolderView, Id, Index, Item, Summary, Totals, TypeStat};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_storage_access::StorageAccessExt;

/// Returned by `scan` when Android's all-files access is missing; the UI
/// then offers `request_storage_access`.
pub const ACCESS_NEEDED: &str = "ACCESS_NEEDED";

#[derive(Default)]
pub struct Suchi {
    index: Mutex<Option<Index>>,
    /// Stops a running scan, rescan or duplicate search.
    cancel: AtomicBool,
    busy: AtomicBool,
}

#[derive(Clone, Serialize)]
struct Progress {
    done: usize,
    total: usize,
}

#[derive(Serialize, Clone)]
pub struct ScanRoot {
    name: String,
    path: PathBuf,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DuplicateGroup {
    size: u64,
    /// Space freed by keeping one copy.
    wasted: u64,
    files: Vec<FileView>,
}

#[derive(Serialize, Deserialize, Default)]
struct Settings {
    #[serde(default)]
    excluded: Vec<PathBuf>,
}

/// Clears `busy` however the work ends.
struct BusyGuard<'a>(&'a AtomicBool);

impl Drop for BusyGuard<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}

impl Suchi {
    fn start(&self) -> CommandResult<BusyGuard<'_>> {
        if self.busy.swap(true, Ordering::AcqRel) {
            return Err("a scan is already running".into());
        }
        self.cancel.store(false, Ordering::Release);
        Ok(BusyGuard(&self.busy))
    }

    fn with<T>(&self, f: impl FnOnce(&Index) -> Option<T>) -> CommandResult<T> {
        let index = self.index.lock().unwrap();
        f(index.as_ref().ok_or("nothing scanned yet")?).ok_or_else(|| "no longer in the scan; rescan to refresh".into())
    }
}

/// Folders the user may scan: home (or Android's shared storage), then
/// every mounted volume.
#[tauri::command]
pub fn scan_roots(app: AppHandle, storage: State<'_, Storage>) -> Vec<ScanRoot> {
    let mut roots = Vec::new();
    if cfg!(target_os = "android") {
        roots.push(ScanRoot { name: "Internal storage".into(), path: "/storage/emulated/0".into() });
    } else if let Ok(home) = app.path().home_dir() {
        roots.push(ScanRoot { name: "Home".into(), path: home });
    }
    for volume in storage.volumes.lock().unwrap().iter() {
        // Android's /data is not readable by apps; its user files are under
        // shared storage, listed above.
        if cfg!(target_os = "android") && volume.mount == "/data" {
            continue;
        }
        let path = PathBuf::from(&volume.mount);
        if !roots.iter().any(|r| r.path == path) {
            roots.push(ScanRoot { name: volume.name.clone(), path });
        }
    }
    roots
}

#[tauri::command]
pub async fn scan(app: AppHandle, storage: State<'_, Storage>, root: PathBuf) -> CommandResult<Summary> {
    if !scan_roots(app.clone(), storage).iter().any(|r| r.path == root) {
        return Err("not a scannable folder".into());
    }
    if !app.storage_access().granted().map_err(|e| e.to_string())? {
        return Err(ACCESS_NEEDED.into());
    }
    let excluded = settings(&app).excluded;
    let emitter = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let suchi = app.state::<Suchi>();
        let _busy = suchi.start()?;
        let index = index::scan(&root, &excluded, &suchi.cancel, |done, total| {
            let _ = emitter.emit("scan-progress", Progress { done, total });
        })
        .map_err(|e| e.to_string())?;
        let summary = index.summary();
        *suchi.index.lock().unwrap() = Some(index);
        Ok(summary)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub fn stop_scan(suchi: State<'_, Suchi>) {
    suchi.cancel.store(true, Ordering::Release);
}

#[tauri::command]
pub fn scan_summary(suchi: State<'_, Suchi>) -> Option<Summary> {
    suchi.index.lock().unwrap().as_ref().map(Index::summary)
}

#[tauri::command]
pub fn folder(suchi: State<'_, Suchi>, id: Id) -> CommandResult<FolderView> {
    suchi.with(|index| index.folder(id))
}

#[tauri::command]
pub fn file_types(suchi: State<'_, Suchi>) -> CommandResult<Vec<TypeStat>> {
    suchi.with(|index| Some(index.types()))
}

#[tauri::command]
pub fn files(suchi: State<'_, Suchi>, filter: FileFilter) -> CommandResult<Vec<FileView>> {
    suchi.with(|index| Some(index.files(&filter)))
}

#[tauri::command]
pub async fn duplicates(app: AppHandle) -> CommandResult<Vec<DuplicateGroup>> {
    tauri::async_runtime::spawn_blocking(move || {
        let suchi = app.state::<Suchi>();
        let _busy = suchi.start()?;
        // Hash without holding the lock, so browsing keeps working.
        let candidates = suchi.with(|index| Some(index.duplicate_candidates()))?;
        let groups = index::find_duplicates(candidates, &suchi.cancel, |done, total| {
            let _ = app.emit("duplicates-progress", Progress { done, total });
        });
        let mut groups: Vec<DuplicateGroup> = suchi.with(|index| {
            Some(
                groups
                    .into_iter()
                    .map(|ids| ids.into_iter().filter_map(|id| index.file(id)).collect::<Vec<_>>())
                    .filter(|files| files.len() > 1)
                    .map(|files| {
                        let size = files[0].size;
                        DuplicateGroup { size, wasted: size * (files.len() as u64 - 1), files }
                    })
                    .collect(),
            )
        })?;
        groups.sort_by_key(|g| std::cmp::Reverse(g.wasted));
        Ok(groups)
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Writes the duplicate groups as CSV to the Downloads folder and returns
/// the file's path.
#[tauri::command]
pub fn export_duplicates(app: AppHandle, groups: Vec<Vec<Id>>) -> CommandResult<PathBuf> {
    let suchi = app.state::<Suchi>();
    let mut csv = String::from("group,size,path\n");
    suchi.with(|index| {
        for (n, group) in groups.iter().enumerate() {
            for file in group.iter().filter_map(|&id| index.file(id)) {
                let path = file.path.display().to_string().replace('"', "\"\"");
                csv.push_str(&format!("{},{},\"{path}\"\n", n + 1, file.size));
            }
        }
        Some(())
    })?;
    let dir = app
        .path()
        .download_dir()
        .ok()
        .or_else(|| cfg!(target_os = "android").then(|| PathBuf::from("/storage/emulated/0/Download")))
        .ok_or("no Downloads folder")?;
    let path = dir.join(format!("bhandara-duplicates-{}.csv", bhandara_core::health::now()));
    std::fs::write(&path, csv).map_err(|e| e.to_string())?;
    Ok(path)
}

/// Scans one folder again; returns its new id.
#[tauri::command]
pub async fn rescan(app: AppHandle, id: Id) -> CommandResult<Id> {
    let excluded = settings(&app).excluded;
    tauri::async_runtime::spawn_blocking(move || {
        let suchi = app.state::<Suchi>();
        let _busy = suchi.start()?;
        let mut index = suchi.index.lock().unwrap();
        index.as_mut().ok_or("nothing scanned yet")?.rescan(id, &excluded, &suchi.cancel).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Move to Trash on desktop; permanent on Android.
#[tauri::command]
pub async fn delete_item(app: AppHandle, item: Item) -> CommandResult<Totals> {
    tauri::async_runtime::spawn_blocking(move || {
        let suchi = app.state::<Suchi>();
        let mut index = suchi.index.lock().unwrap();
        index.as_mut().ok_or("nothing scanned yet")?.delete(item).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Desktop: opens a folder, or shows a file in its folder.
#[tauri::command]
pub fn open_item(app: AppHandle, suchi: State<'_, Suchi>, item: Item) -> CommandResult<()> {
    use tauri_plugin_opener::OpenerExt;
    if cfg!(target_os = "android") {
        return Err("opening folders is not supported on Android".into());
    }
    let path = suchi.with(|index| index.path(item))?;
    match item {
        Item::Dir(_) => app.opener().open_path(path.to_string_lossy(), None::<&str>),
        Item::File(_) => app.opener().reveal_item_in_dir(&path),
    }
    .map_err(|e| e.to_string())
}

/// Opens a scan root (a drive or home) in the file manager.
#[tauri::command]
pub fn open_root(app: AppHandle, storage: State<'_, Storage>, path: PathBuf) -> CommandResult<()> {
    use tauri_plugin_opener::OpenerExt;
    if cfg!(target_os = "android") {
        return Err("opening folders is not supported on Android".into());
    }
    if !scan_roots(app.clone(), storage).iter().any(|r| r.path == path) {
        return Err("unknown path".into());
    }
    app.opener().open_path(path.to_string_lossy(), None::<&str>).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn exclusions(app: AppHandle) -> Vec<PathBuf> {
    settings(&app).excluded
}

/// Adds a scanned folder to the exclusions; takes effect on the next scan.
#[tauri::command]
pub fn exclude(app: AppHandle, suchi: State<'_, Suchi>, id: Id) -> CommandResult<Vec<PathBuf>> {
    let path = suchi.with(|index| index.path(Item::Dir(id)))?;
    let mut s = settings(&app);
    if !s.excluded.contains(&path) {
        s.excluded.push(path);
    }
    save_settings(&app, &s)?;
    Ok(s.excluded)
}

#[tauri::command]
pub fn include(app: AppHandle, path: PathBuf) -> CommandResult<Vec<PathBuf>> {
    let mut s = settings(&app);
    s.excluded.retain(|p| *p != path);
    save_settings(&app, &s)?;
    Ok(s.excluded)
}

fn settings_path(app: &AppHandle) -> Option<PathBuf> {
    Some(app.path().app_config_dir().ok()?.join("settings.json"))
}

fn settings(app: &AppHandle) -> Settings {
    settings_path(app)
        .and_then(|p| std::fs::read(p).ok())
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}

fn save_settings(app: &AppHandle, s: &Settings) -> CommandResult<()> {
    let path = settings_path(app).ok_or("no settings folder")?;
    std::fs::create_dir_all(path.parent().unwrap()).map_err(|e| e.to_string())?;
    std::fs::write(path, serde_json::to_vec_pretty(s).map_err(|e| e.to_string())?).map_err(|e| e.to_string())
}
