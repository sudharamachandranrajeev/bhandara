//! Shodhana: the Clean and Watch views. Like Suchi, the UI refers to build
//! folders by scan id, never by path, so it can only remove what the scan
//! found and the guards allowed.

use crate::{CommandResult, Storage};
use bhandara_core::caches::{self, Cache};
use bhandara_core::clean::{self, Candidate, Mode, Receipt, Rule, Scan};
use bhandara_core::watch::{self, Growth, Paths, Settings, SnapshotNotice};
use serde::Serialize;
use std::path::PathBuf;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use tauri::{AppHandle, Emitter, Manager, State};

#[derive(Default)]
pub struct Shodhana {
    scan: Mutex<Option<Scan>>,
    cancel: AtomicBool,
    busy: AtomicBool,
}

struct BusyGuard<'a>(&'a AtomicBool);

impl Drop for BusyGuard<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}

impl Shodhana {
    fn start(&self) -> CommandResult<BusyGuard<'_>> {
        if self.busy.swap(true, Ordering::AcqRel) {
            return Err("a clean scan is already running".into());
        }
        self.cancel.store(false, Ordering::Release);
        Ok(BusyGuard(&self.busy))
    }
}

pub fn paths(app: &AppHandle) -> CommandResult<Paths> {
    let config = app.path().app_config_dir().map_err(|e| e.to_string())?;
    let data = app.path().app_data_dir().map_err(|e| e.to_string())?;
    Ok(Paths::new(&config, &data))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuleInfo {
    rule: Rule,
    label: &'static str,
    why_safe: &'static str,
    comes_back: &'static str,
}

#[tauri::command]
pub fn clean_rules() -> Vec<RuleInfo> {
    Rule::ALL.iter().map(|&rule| RuleInfo { rule, label: rule.label(), why_safe: rule.why_safe(), comes_back: rule.comes_back() }).collect()
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ScanView {
    root: PathBuf,
    candidates: Vec<Candidate>,
    partial: bool,
    reclaimable: u64,
    scanned_at: u64,
}

impl From<&Scan> for ScanView {
    fn from(s: &Scan) -> Self {
        ScanView { root: s.root.clone(), candidates: s.candidates.clone(), partial: s.partial, reclaimable: s.reclaimable(), scanned_at: s.scanned_at }
    }
}

#[derive(Clone, Serialize)]
struct Progress {
    done: usize,
    total: usize,
}

/// Looks for build output in a scan root or any folder inside one. Emits
/// `clean-progress` while sizing what it found.
#[tauri::command]
pub async fn clean_scan(app: AppHandle, storage: State<'_, Storage>, root: PathBuf) -> CommandResult<ScanView> {
    // Compare resolved paths so `..` or a symlink cannot reach outside, but
    // scan the path as given (Windows resolves to a `\\?\` form).
    let real = root.canonicalize().map_err(|_| "folder not found".to_string())?;
    let inside = crate::suchi::scan_roots(app.clone(), storage)
        .iter()
        .any(|r| r.path.canonicalize().is_ok_and(|base| real.starts_with(base)));
    if !real.is_dir() || !inside {
        return Err("pick a folder inside your home or one of the listed drives".into());
    }
    let paths = paths(&app)?;
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<Shodhana>();
        let _busy = state.start()?;
        let options = watch::clean_options(&paths.load_settings(), 0);
        let progress = |done, total| {
            let _ = app.emit("clean-progress", Progress { done, total });
        };
        let scan = clean::scan(&root, &options, &state.cancel, progress).map_err(|e| e.to_string())?;
        let view = ScanView::from(&scan);
        let mut watch_state = paths.load_state();
        watch_state.reclaimable = Some(watch::Reclaimable { bytes: view.reclaimable, root: root.clone(), at: view.scanned_at });
        let _ = paths.save_state(&watch_state);
        *state.scan.lock().unwrap() = Some(scan);
        Ok(view)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub fn clean_stop(state: State<'_, Shodhana>) {
    state.cancel.store(true, Ordering::Release);
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoveResult {
    receipts: Vec<Receipt>,
    failed: Vec<(PathBuf, String)>,
    /// Set when a permanent clean freed far less than it removed on btrfs.
    snapshot: Option<SnapshotNotice>,
}

/// Removes the chosen candidates in the chosen mode. Each one is checked
/// again on disk first; release packages are backed up when that setting
/// is on.
#[tauri::command]
pub async fn clean_remove(app: AppHandle, ids: Vec<u32>, mode: Mode) -> CommandResult<RemoveResult> {
    let paths = paths(&app)?;
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<Shodhana>();
        let _busy = state.start()?;
        let settings = paths.load_settings();
        let backup = settings.backup_releases.then(|| paths.backup());
        let mut guard = state.scan.lock().unwrap();
        let scan = guard.as_mut().ok_or("nothing scanned yet")?;
        let before = bhandara_core::volumes().unwrap_or_default();
        let (mut receipts, mut failed) = (Vec::new(), Vec::new());
        for id in ids {
            let Some(c) = scan.candidates.get(id as usize) else { continue };
            match clean::remove(scan, id, mode, backup.as_deref()) {
                Ok(r) => receipts.push(r),
                Err(e) => failed.push((c.path.clone(), e.to_string())),
            }
        }
        clean::append_receipts(&paths.receipts(), &receipts).map_err(|e| e.to_string())?;
        // Removed folders stay in the scan but can no longer pass the
        // on-disk check, so a second remove of the same id is refused.
        let removed: u64 = receipts.iter().map(|r| r.bytes).sum();
        let snapshot = (mode == Mode::Permanent)
            .then(|| {
                let after = bhandara_core::volumes().ok()?;
                let b = watch::volume_of(&before, &scan.root)?;
                let a = watch::volume_of(&after, &scan.root)?;
                watch::snapshot_notice(a, removed, b.avail, a.avail)
            })
            .flatten();
        Ok(RemoveResult { receipts, failed, snapshot })
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn cache_list() -> CommandResult<Vec<Cache>> {
    tauri::async_runtime::spawn_blocking(caches::list).await.map_err(|e| e.to_string())
}

/// Runs one cache's own clean command; returns its output.
#[tauri::command]
pub async fn cache_run(id: String) -> CommandResult<String> {
    tauri::async_runtime::spawn_blocking(move || caches::run(&id).map_err(|e| e.to_string())).await.map_err(|e| e.to_string())?
}

#[tauri::command]
pub fn receipts(app: AppHandle, limit: usize) -> CommandResult<Vec<Receipt>> {
    Ok(clean::read_receipts(&paths(&app)?.receipts(), limit.min(500)))
}

#[tauri::command]
pub fn watch_settings(app: AppHandle) -> CommandResult<Settings> {
    Ok(paths(&app)?.load_settings())
}

#[tauri::command]
pub fn save_watch_settings(app: AppHandle, settings: Settings) -> CommandResult<Settings> {
    let ok = settings.critical_percent > 0.0
        && settings.critical_percent < settings.low_percent
        && settings.low_percent <= 50.0
        && (0.0..=10_000.0).contains(&settings.low_gb)
        && (1..=365).contains(&settings.active_days)
        && (1..=365).contains(&settings.schedule.days);
    if !ok {
        return Err("check the numbers: critical must be above 0 and below low, low at most 50%, days 1 to 365".into());
    }
    paths(&app)?.save_settings(&settings).map_err(|e| e.to_string())?;
    Ok(settings)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WatchStatus {
    levels: std::collections::HashMap<String, watch::SpaceLevel>,
    last_sample: u64,
    samples: usize,
    /// Linux: whether the bhandara-watch user timer is active. None where
    /// the app itself runs the watch.
    timer: Option<bool>,
}

#[tauri::command]
pub async fn watch_status(app: AppHandle) -> CommandResult<WatchStatus> {
    let paths = paths(&app)?;
    tauri::async_runtime::spawn_blocking(move || {
        let state = paths.load_state();
        let timer = cfg!(target_os = "linux").then(|| {
            std::process::Command::new("systemctl")
                .args(["--user", "is-active", "--quiet", "bhandara-watch.timer"])
                .status()
                .is_ok_and(|s| s.success())
        });
        WatchStatus { levels: state.levels, last_sample: state.last_sample, samples: watch::read_history(&paths.history()).len(), timer }
    })
    .await
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn growth(app: AppHandle, days: u64) -> CommandResult<Growth> {
    let paths = paths(&app)?;
    tauri::async_runtime::spawn_blocking(move || watch::growth(&watch::read_history(&paths.history()), days.clamp(1, 90), 20))
        .await
        .map_err(|e| e.to_string())
}

/// Takes a history sample now (a full scan of home, so it takes a while).
#[tauri::command]
pub async fn take_sample(app: AppHandle) -> CommandResult<()> {
    let paths = paths(&app)?;
    let home = app.path().home_dir().map_err(|e| e.to_string())?;
    tauri::async_runtime::spawn_blocking(move || watch::sample(&paths, &home).map(|_| ())).await.map_err(|e| e.to_string())?
}

/// Windows (and other desktops without the Linux user timer): the app runs
/// the watch itself every 15 minutes while it is open in the tray.
#[cfg(all(desktop, not(target_os = "linux")))]
pub fn watch(app: AppHandle) {
    use tauri_plugin_notification::NotificationExt;
    std::thread::spawn(move || {
        loop {
            if let (Ok(paths), Ok(home)) = (paths(&app), app.path().home_dir()) {
                let notify = |title: &str, body: &str| {
                    let _ = app.notification().builder().title(title).body(body).show();
                };
                if let Err(e) = watch::tick(&paths, &home, &notify) {
                    eprintln!("space watch: {e}");
                }
            }
            std::thread::sleep(std::time::Duration::from_secs(15 * 60));
        }
    });
}

/// Windows: opens Storage Sense or Disk Cleanup.
#[tauri::command]
pub fn open_system_cleanup(app: AppHandle, tool: String) -> CommandResult<()> {
    if !cfg!(windows) {
        return Err("only on Windows".into());
    }
    match tool.as_str() {
        "storage-sense" => {
            use tauri_plugin_opener::OpenerExt;
            app.opener().open_url("ms-settings:storagesense", None::<&str>).map_err(|e| e.to_string())
        }
        "disk-cleanup" => std::process::Command::new("cleanmgr.exe").spawn().map(|_| ()).map_err(|e| e.to_string()),
        _ => Err("unknown tool".into()),
    }
}
