//! Space watch: low-space alerts with hysteresis, and a daily history of
//! free space and the biggest folders, so "what grew?" has an answer.
//!
//! Files (Linux paths; the Tauri app passes its own folders on every OS):
//!   ~/.config/dev.bhandara.app/watch.json         settings
//!   ~/.local/share/dev.bhandara.app/watch-state.json  alert levels, last runs
//!   ~/.local/share/dev.bhandara.app/history.jsonl     one sample per day
//!   ~/.local/share/dev.bhandara.app/receipts.jsonl    every clean

use crate::clean::Mode;
use crate::{Kind, Volume};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};

const APP_ID: &str = "dev.bhandara.app";
/// Percentage points free space must rise past a threshold to clear it.
const HYSTERESIS: f64 = 2.0;
/// The gigabyte rule applies only to volumes at least this big; on small
/// ones (/boot) it would always fire.
const GB_RULE_MIN_SIZE: u64 = 200_000_000_000;
/// History is kept this long.
pub const HISTORY_DAYS: u64 = 90;
const DAY: u64 = 86_400;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct Settings {
    /// "Low" below this share of free space, in percent.
    pub low_percent: f64,
    /// Also "low" below this many GB free, on volumes of 200 GB or more.
    pub low_gb: f64,
    /// "Critical" below this share, in percent.
    pub critical_percent: f64,
    /// Mount points with alerts turned off.
    pub muted: Vec<String>,
    /// Mount point to Unix time when its snooze ends.
    pub snoozed: HashMap<String, u64>,
    pub schedule: Schedule,
    /// Projects changed within this many days are left alone by every clean.
    pub active_days: u32,
    /// Copy release packages out before their build folder goes.
    pub backup_releases: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            low_percent: 10.0,
            low_gb: 20.0,
            critical_percent: 5.0,
            muted: Vec::new(),
            snoozed: HashMap::new(),
            schedule: Schedule::default(),
            active_days: 14,
            backup_releases: true,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct Schedule {
    /// Off until the user turns it on.
    pub enabled: bool,
    /// Chosen when the schedule is turned on.
    pub mode: Mode,
    /// Only projects untouched this long are cleaned on schedule.
    pub days: u32,
    /// Folders to clean; empty means home.
    pub roots: Vec<PathBuf>,
}

impl Default for Schedule {
    fn default() -> Self {
        Schedule { enabled: false, mode: Mode::Trash, days: 30, roots: Vec::new() }
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Default)]
#[serde(rename_all = "lowercase")]
pub enum SpaceLevel {
    #[default]
    Ok,
    Low,
    Critical,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct State {
    pub levels: HashMap<String, SpaceLevel>,
    pub last_sample: u64,
    pub last_clean: u64,
    /// From the latest clean scan, for the bar.
    pub reclaimable: Option<Reclaimable>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Reclaimable {
    pub bytes: u64,
    pub root: PathBuf,
    pub at: u64,
}

/// Where settings and data live for the standalone `bhandara-watch`; the
/// same folders Tauri gives the app.
pub struct Paths {
    pub settings: PathBuf,
    pub data: PathBuf,
}

impl Paths {
    pub fn new(config_dir: &Path, data_dir: &Path) -> Self {
        Paths { settings: config_dir.join("watch.json"), data: data_dir.to_path_buf() }
    }

    pub fn standard() -> Option<Self> {
        let home = crate::clean::home_dir();
        let (config, data) = if cfg!(windows) {
            let roaming = PathBuf::from(std::env::var_os("APPDATA")?);
            (roaming.clone(), roaming)
        } else {
            let env = |var: &str, fallback: &str| std::env::var_os(var).map(PathBuf::from).or_else(|| home.as_ref().map(|h| h.join(fallback)));
            (env("XDG_CONFIG_HOME", ".config")?, env("XDG_DATA_HOME", ".local/share")?)
        };
        Some(Paths::new(&config.join(APP_ID), &data.join(APP_ID)))
    }

    pub fn state(&self) -> PathBuf {
        self.data.join("watch-state.json")
    }
    pub fn history(&self) -> PathBuf {
        self.data.join("history.jsonl")
    }
    pub fn receipts(&self) -> PathBuf {
        self.data.join("receipts.jsonl")
    }
    pub fn backup(&self) -> PathBuf {
        self.data.join("release-backup")
    }

    pub fn load_settings(&self) -> Settings {
        read_json(&self.settings).unwrap_or_default()
    }
    pub fn save_settings(&self, s: &Settings) -> io::Result<()> {
        write_json(&self.settings, s)
    }
    pub fn load_state(&self) -> State {
        read_json(&self.state()).unwrap_or_default()
    }
    pub fn save_state(&self, s: &State) -> io::Result<()> {
        write_json(&self.state(), s)
    }
}

fn read_json<T: for<'de> Deserialize<'de>>(path: &Path) -> Option<T> {
    serde_json::from_slice(&fs::read(path).ok()?).ok()
}

/// Writes through a temporary file, so a reader never sees half a file.
fn write_json<T: Serialize>(path: &Path, value: &T) -> io::Result<()> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("tmp");
    fs::write(&tmp, serde_json::to_vec_pretty(value).map_err(io::Error::other)?)?;
    fs::rename(tmp, path)
}

/// Volumes worth watching: everything but removable drives.
pub fn watched(volumes: &[Volume]) -> impl Iterator<Item = &Volume> {
    volumes.iter().filter(|v| !v.removable && v.kind != Kind::Removable && v.used + v.avail > 0)
}

/// The level for a volume, given its previous level. Leaving a level needs
/// `HYSTERESIS` points more free space than entering it, so a disk hovering
/// at a threshold does not alert over and over.
pub fn level(previous: SpaceLevel, size: u64, avail: u64, s: &Settings) -> SpaceLevel {
    let usable = size.max(1) as f64;
    let free_pct = avail as f64 / usable * 100.0;
    let margin = |lvl: SpaceLevel| if previous >= lvl { HYSTERESIS } else { 0.0 };
    if free_pct < s.critical_percent + margin(SpaceLevel::Critical) {
        return SpaceLevel::Critical;
    }
    // The GB rule clears 10% above its threshold.
    let gb_margin = if previous >= SpaceLevel::Low { s.low_gb * 0.1 } else { 0.0 };
    let gb_rule = size >= GB_RULE_MIN_SIZE && (avail as f64) < (s.low_gb + gb_margin) * 1e9;
    if free_pct < s.low_percent + margin(SpaceLevel::Low) || gb_rule {
        return SpaceLevel::Low;
    }
    SpaceLevel::Ok
}

/// Updates the levels in `state` and returns the (title, body) of each
/// alert to raise: one per crossing into a worse level, unless muted or
/// snoozed.
pub fn check(volumes: &[Volume], s: &Settings, state: &mut State, now: u64) -> Vec<(String, String)> {
    let mut alerts = Vec::new();
    for v in watched(volumes) {
        let usable = v.used + v.avail;
        let previous = state.levels.get(&v.mount).copied().unwrap_or_default();
        let current = level(previous, usable, v.avail, s);
        state.levels.insert(v.mount.clone(), current);
        let quiet = s.muted.contains(&v.mount) || s.snoozed.get(&v.mount).is_some_and(|&until| now < until);
        if current > previous && !quiet {
            let pct = v.avail as f64 / usable as f64 * 100.0;
            let title = match current {
                SpaceLevel::Critical => format!("{} is almost full", v.name),
                _ => format!("{} is running low on space", v.name),
            };
            alerts.push((title, format!("{} free ({pct:.0}%). Open Bhandara to see what grew and what can be cleaned.", gb(v.avail))));
        }
    }
    alerts
}

fn gb(bytes: u64) -> String {
    format!("{:.1} GB", bytes as f64 / 1e9)
}

// ---------------------------------------------------------------- history

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Sample {
    pub at: u64,
    pub volumes: Vec<VolumeSample>,
    /// Biggest folders under home: (path, bytes on disk).
    pub folders: Vec<(PathBuf, u64)>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct VolumeSample {
    pub mount: String,
    pub name: String,
    pub used: u64,
    pub avail: u64,
}

impl Sample {
    pub fn new(at: u64, volumes: &[Volume], folders: Vec<(PathBuf, u64)>) -> Self {
        let volumes = watched(volumes).map(|v| VolumeSample { mount: v.mount.clone(), name: v.name.clone(), used: v.used, avail: v.avail }).collect();
        Sample { at, volumes, folders }
    }
}

/// Appends a sample and drops those older than `HISTORY_DAYS`.
pub fn record(history: &Path, sample: Sample) -> io::Result<()> {
    let cutoff = sample.at.saturating_sub(HISTORY_DAYS * DAY);
    let mut kept: Vec<Sample> = read_history(history).into_iter().filter(|s| s.at >= cutoff).collect();
    kept.push(sample);
    if let Some(dir) = history.parent() {
        fs::create_dir_all(dir)?;
    }
    let tmp = history.with_extension("tmp");
    let mut file = fs::File::create(&tmp)?;
    for s in &kept {
        writeln!(file, "{}", serde_json::to_string(s).map_err(io::Error::other)?)?;
    }
    drop(file);
    fs::rename(tmp, history)
}

pub fn read_history(history: &Path) -> Vec<Sample> {
    let Ok(file) = fs::File::open(history) else { return Vec::new() };
    io::BufReader::new(file).lines().map_while(Result::ok).filter_map(|l| serde_json::from_str(&l).ok()).collect()
}

#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Growth {
    /// The sample compared against, Unix seconds; 0 when there is none yet.
    pub since: u64,
    pub until: u64,
    pub volumes: Vec<VolumeGrowth>,
    /// Folders that grew the most (new folders included), biggest first.
    pub folders: Vec<FolderGrowth>,
}

#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct VolumeGrowth {
    pub mount: String,
    pub name: String,
    pub used: u64,
    /// Change in used space; positive means it filled up.
    pub delta: i64,
}

#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct FolderGrowth {
    pub path: PathBuf,
    pub bytes: u64,
    pub delta: i64,
}

/// Compares the newest sample with the oldest one inside the last `days`.
/// A folder inside another one that also grew is left out, so the list
/// names each change once.
pub fn growth(samples: &[Sample], days: u64, limit: usize) -> Growth {
    let Some(latest) = samples.iter().max_by_key(|s| s.at) else {
        return Growth { since: 0, until: 0, volumes: Vec::new(), folders: Vec::new() };
    };
    let cutoff = latest.at.saturating_sub(days * DAY);
    let Some(base) = samples.iter().filter(|s| s.at >= cutoff && s.at < latest.at).min_by_key(|s| s.at) else {
        return Growth { since: 0, until: latest.at, volumes: Vec::new(), folders: Vec::new() };
    };
    let volumes = latest
        .volumes
        .iter()
        .map(|v| {
            let before = base.volumes.iter().find(|b| b.mount == v.mount).map_or(v.used, |b| b.used);
            VolumeGrowth { mount: v.mount.clone(), name: v.name.clone(), used: v.used, delta: v.used as i64 - before as i64 }
        })
        .collect();
    let before: HashMap<&PathBuf, u64> = base.folders.iter().map(|(p, b)| (p, *b)).collect();
    // A folder missing from the old sample was below its top list; treat it
    // as new only when the old sample covered folders that small.
    let floor = base.folders.iter().map(|(_, b)| *b).min().unwrap_or(0);
    let mut grew: Vec<FolderGrowth> = latest
        .folders
        .iter()
        .filter_map(|(path, bytes)| {
            let old = before.get(path).copied().unwrap_or(if *bytes > floor { 0 } else { *bytes });
            let delta = *bytes as i64 - old as i64;
            (delta > 0).then(|| FolderGrowth { path: path.clone(), bytes: *bytes, delta })
        })
        .collect();
    grew.sort_by_key(|g| std::cmp::Reverse(g.delta));
    // Keep the deepest folder that explains most of a parent's growth.
    let mut kept: Vec<FolderGrowth> = Vec::new();
    for g in grew {
        if let Some(parent) = kept.iter_mut().find(|k| g.path.starts_with(&k.path) && g.delta * 10 >= k.delta * 8) {
            *parent = g;
        } else if !kept.iter().any(|k| k.path.starts_with(&g.path) || g.path.starts_with(&k.path)) {
            kept.push(g);
        }
        if kept.len() >= limit {
            break;
        }
    }
    Growth { since: base.at, until: latest.at, volumes, folders: kept }
}

/// After a permanent clean: if the volume freed less than half of what was
/// removed and it is btrfs, snapshots are probably still holding the data.
#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SnapshotNotice {
    pub mount: String,
    pub removed: u64,
    pub freed: u64,
    /// Snapshots in /.snapshots, when it can be read.
    pub snapshots: Option<usize>,
    pub commands: Vec<String>,
}

pub fn snapshot_notice(volume: &Volume, removed: u64, avail_before: u64, avail_after: u64) -> Option<SnapshotNotice> {
    let freed = avail_after.saturating_sub(avail_before);
    if volume.fstype != "btrfs" || removed == 0 || freed * 2 >= removed {
        return None;
    }
    let snapshots = fs::read_dir("/.snapshots").ok().map(|d| d.flatten().filter(|e| e.file_type().is_ok_and(|t| t.is_dir())).count());
    Some(SnapshotNotice {
        mount: volume.mount.clone(),
        removed,
        freed,
        snapshots,
        commands: vec!["sudo snapper list".into(), "sudo snapper delete <first>-<last>".into(), "sudo btrfs filesystem usage /".into()],
    })
}

/// The watched volume holding `path`: the longest mount point it sits under.
pub fn volume_of<'a>(volumes: &'a [Volume], path: &Path) -> Option<&'a Volume> {
    volumes
        .iter()
        .flat_map(|v| v.mounts.iter().chain(std::iter::once(&v.mount)).map(move |m| (v, m)))
        .filter(|(_, m)| path.starts_with(m.as_str()))
        .max_by_key(|(_, m)| m.len())
        .map(|(v, _)| v)
}

// ---------------------------------------------------------------- runner

/// A new history sample once this much time has passed since the last.
const SAMPLE_EVERY: u64 = 20 * 3600;
const CLEAN_EVERY: u64 = 7 * DAY;

/// One pass of the watch: alerts, then a daily sample, then the weekly
/// scheduled clean when it is on. Run every 15 minutes by the Linux user
/// timer, or by the desktop app's own loop elsewhere.
pub fn tick(paths: &Paths, home: &Path, notify: &dyn Fn(&str, &str)) -> Result<(), String> {
    let settings = paths.load_settings();
    let mut state = paths.load_state();
    let now = crate::health::now();
    let volumes = crate::volumes().map_err(|e| format!("reading volumes: {e}"))?;
    for (title, body) in check(&volumes, &settings, &mut state, now) {
        notify(&title, &body);
    }
    paths.save_state(&state).map_err(|e| e.to_string())?;
    if now.saturating_sub(state.last_sample) >= SAMPLE_EVERY {
        sample(paths, home)?;
    }
    if settings.schedule.enabled && now.saturating_sub(state.last_clean) >= CLEAN_EVERY {
        scheduled_clean(paths, &settings, home, notify)?;
    }
    Ok(())
}

/// Records free space per volume and the biggest folders under `home`.
pub fn sample(paths: &Paths, home: &Path) -> Result<Sample, String> {
    let now = crate::health::now();
    let volumes = crate::volumes().map_err(|e| e.to_string())?;
    let index = crate::index::scan(home, &[], &std::sync::atomic::AtomicBool::new(false), |_, _| {}).map_err(|e| e.to_string())?;
    let sample = Sample::new(now, &volumes, index.largest_folders(4, 200));
    record(&paths.history(), sample.clone()).map_err(|e| e.to_string())?;
    let mut state = paths.load_state();
    state.last_sample = now;
    paths.save_state(&state).map_err(|e| e.to_string())?;
    Ok(sample)
}

/// Clean options from the settings, never looser than `min_days`.
pub fn clean_options(settings: &Settings, min_days: u32) -> crate::clean::Options {
    crate::clean::Options { active_days: settings.active_days.max(min_days), ..Default::default() }
}

/// Removes every unguarded candidate whose project is older than the
/// schedule's age rule, in the mode chosen when the schedule was turned on.
pub fn scheduled_clean(paths: &Paths, settings: &Settings, home: &Path, notify: &dyn Fn(&str, &str)) -> Result<(), String> {
    use crate::clean;
    let roots = if settings.schedule.roots.is_empty() { vec![home.to_path_buf()] } else { settings.schedule.roots.clone() };
    let options = clean_options(settings, settings.schedule.days);
    let backup = settings.backup_releases.then(|| paths.backup());
    let before = crate::volumes().map_err(|e| e.to_string())?;
    let (mut receipts, mut failed) = (Vec::new(), 0);
    for root in &roots {
        let scan = clean::scan(root, &options, &std::sync::atomic::AtomicBool::new(false), |_, _| {}).map_err(|e| format!("{}: {e}", root.display()))?;
        for c in scan.candidates.iter().filter(|c| c.skip.is_none()) {
            match clean::remove(&scan, c.id, settings.schedule.mode, backup.as_deref()) {
                Ok(mut r) => {
                    r.scheduled = true;
                    receipts.push(r);
                }
                Err(_) => failed += 1,
            }
        }
    }
    clean::append_receipts(&paths.receipts(), &receipts).map_err(|e| e.to_string())?;
    let mut state = paths.load_state();
    state.last_clean = crate::health::now();
    paths.save_state(&state).map_err(|e| e.to_string())?;
    if receipts.is_empty() && failed == 0 {
        return Ok(());
    }
    let removed: u64 = receipts.iter().map(|r| r.bytes).sum();
    let verb = if settings.schedule.mode == clean::Mode::Permanent { "Removed" } else { "Moved to Trash" };
    let mut body = format!("{verb} {} of build output from {} folders.", gb(removed), receipts.len());
    if failed > 0 {
        body.push_str(&format!(" {failed} could not be removed."));
    }
    if settings.schedule.mode == clean::Mode::Permanent {
        let after = crate::volumes().map_err(|e| e.to_string())?;
        if let (Some(b), Some(a)) = (volume_of(&before, &roots[0]), volume_of(&after, &roots[0]))
            && snapshot_notice(a, removed, b.avail, a.avail).is_some()
        {
            body.push_str(" Snapshots are still holding most of it; open Bhandara for details.");
        }
    }
    notify("Bhandara cleaned up", &body);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Level;

    const GB: u64 = 1_000_000_000;

    fn volume(mount: &str, size: u64, avail: u64) -> Volume {
        Volume {
            device: "/dev/x".into(),
            kname: "x".into(),
            name: mount.into(),
            kind: Kind::System,
            mount: mount.into(),
            mounts: vec![mount.into()],
            fstype: "btrfs".into(),
            label: String::new(),
            model: String::new(),
            transport: String::new(),
            removable: false,
            encrypted: false,
            size,
            used: size - avail,
            avail,
            fraction: 0.0,
            level: Level::Normal,
        }
    }

    #[test]
    fn levels_use_percent_and_gb_rules() {
        let s = Settings::default();
        assert_eq!(level(SpaceLevel::Ok, 100 * GB, 50 * GB, &s), SpaceLevel::Ok);
        assert_eq!(level(SpaceLevel::Ok, 100 * GB, 9 * GB, &s), SpaceLevel::Low);
        assert_eq!(level(SpaceLevel::Ok, 100 * GB, 4 * GB, &s), SpaceLevel::Critical);
        // 475 GB disk, 19 GB free = 4%: critical; 30 GB free = 6.3%: low by percent.
        assert_eq!(level(SpaceLevel::Ok, 475 * GB, 19 * GB, &s), SpaceLevel::Critical);
        // 1 TB disk, 15 GB free = 1.5%: critical. 1 TB, 60 GB free = 6%: low.
        assert_eq!(level(SpaceLevel::Ok, 1000 * GB, 60 * GB, &s), SpaceLevel::Low);
        // 4 TB with 300 GB free is 7.5%: low; 500 GB free: ok.
        assert_eq!(level(SpaceLevel::Ok, 4000 * GB, 300 * GB, &s), SpaceLevel::Low);
        assert_eq!(level(SpaceLevel::Ok, 4000 * GB, 500 * GB, &s), SpaceLevel::Ok);
        // With a lower percent rule, the GB rule fires on its own (and clears 10% above it).
        let gb_only = Settings { low_percent: 5.0, critical_percent: 1.0, ..Settings::default() };
        assert_eq!(level(SpaceLevel::Ok, 250 * GB, 19 * GB, &gb_only), SpaceLevel::Low);
        assert_eq!(level(SpaceLevel::Low, 250 * GB, 21 * GB, &gb_only), SpaceLevel::Low);
        assert_eq!(level(SpaceLevel::Low, 250 * GB, 23 * GB, &gb_only), SpaceLevel::Ok);
        // A 1 GB /boot with 500 MB free: the GB rule does not apply.
        assert_eq!(level(SpaceLevel::Ok, GB, GB / 2, &s), SpaceLevel::Ok);
    }

    #[test]
    fn hysteresis_holds_a_level() {
        let s = Settings::default();
        // Low at 9%; rising to 11% stays low; 12.5% clears.
        assert_eq!(level(SpaceLevel::Low, 100 * GB, 11 * GB, &s), SpaceLevel::Low);
        assert_eq!(level(SpaceLevel::Low, 100 * GB, 12_500_000_000, &s), SpaceLevel::Ok);
        assert_eq!(level(SpaceLevel::Critical, 100 * GB, 6 * GB, &s), SpaceLevel::Critical);
        assert_eq!(level(SpaceLevel::Critical, 100 * GB, 8 * GB, &s), SpaceLevel::Low);
    }

    #[test]
    fn alerts_once_per_crossing_and_respect_mute_and_snooze() {
        let mut s = Settings::default();
        let mut state = State::default();
        let low = [volume("/", 100 * GB, 9 * GB)];
        assert_eq!(check(&low, &s, &mut state, 0).len(), 1);
        assert!(check(&low, &s, &mut state, 1).is_empty());
        let critical = [volume("/", 100 * GB, 3 * GB)];
        assert_eq!(check(&critical, &s, &mut state, 2).len(), 1);
        // Recover, then fall again while snoozed: no alert, but the level is tracked.
        check(&[volume("/", 100 * GB, 50 * GB)], &s, &mut state, 3);
        s.snoozed.insert("/".into(), 100);
        assert!(check(&low, &s, &mut state, 4).is_empty());
        assert_eq!(state.levels["/"], SpaceLevel::Low);
        state.levels.clear();
        s.snoozed.clear();
        s.muted.push("/".into());
        assert!(check(&low, &s, &mut state, 5).is_empty());
    }

    #[test]
    fn removable_drives_are_not_watched() {
        let mut usb = volume("/run/media/u/STICK", 100 * GB, GB);
        usb.removable = true;
        assert!(check(&[usb], &Settings::default(), &mut State::default(), 0).is_empty());
    }

    fn sample(at: u64, used: u64, folders: &[(&str, u64)]) -> Sample {
        Sample {
            at,
            volumes: vec![VolumeSample { mount: "/".into(), name: "System".into(), used, avail: 0 }],
            folders: folders.iter().map(|(p, b)| (PathBuf::from(p), *b)).collect(),
        }
    }

    #[test]
    fn growth_names_the_folder_that_grew() {
        let samples = vec![
            sample(0, 100, &[("/h/Work", 50), ("/h/Work/a", 40), ("/h/Work/a/target", 30), ("/h/Docs", 10), ("/h/Music", 5)]),
            sample(3 * DAY, 120, &[("/h/Work", 60), ("/h/Docs", 10)]),
            sample(8 * DAY, 200, &[("/h/Work", 140), ("/h/Work/a", 130), ("/h/Work/a/target", 120), ("/h/Docs", 12), ("/h/new", 20)]),
        ];
        let g = growth(&samples, 7, 10);
        assert_eq!(g.since, 3 * DAY);
        assert_eq!(g.volumes[0].delta, 80);
        let names: Vec<_> = g.folders.iter().map(|f| (f.path.to_string_lossy().into_owned(), f.delta)).collect();
        assert_eq!(names, vec![("/h/Work/a/target".into(), 120), ("/h/new".into(), 20), ("/h/Docs".into(), 2)]);
        assert_eq!(growth(&samples[..1], 7, 10).since, 0);
    }

    #[test]
    fn history_keeps_ninety_days() {
        let dir = std::env::temp_dir().join(format!("bhandara-watch-{}", std::process::id()));
        let file = dir.join("history.jsonl");
        let _ = fs::remove_dir_all(&dir);
        record(&file, sample(0, 1, &[])).unwrap();
        record(&file, sample(50 * DAY, 2, &[])).unwrap();
        record(&file, sample(95 * DAY, 3, &[])).unwrap();
        let kept: Vec<u64> = read_history(&file).iter().map(|s| s.at).collect();
        assert_eq!(kept, vec![50 * DAY, 95 * DAY]);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn snapshot_notice_only_when_space_is_held() {
        let v = volume("/", 475 * GB, 100 * GB);
        assert!(snapshot_notice(&v, 200 * GB, 100 * GB, 290 * GB).is_none());
        let n = snapshot_notice(&v, 200 * GB, 100 * GB, 150 * GB).unwrap();
        assert_eq!(n.freed, 50 * GB);
        let mut ext4 = v.clone();
        ext4.fstype = "ext4".into();
        assert!(snapshot_notice(&ext4, 200 * GB, 100 * GB, 150 * GB).is_none());
    }

    #[test]
    fn settings_tolerate_missing_fields() {
        let s: Settings = serde_json::from_str(r#"{"lowPercent": 15}"#).unwrap();
        assert_eq!(s.low_percent, 15.0);
        assert_eq!(s.schedule, Schedule::default());
        assert!(s.backup_releases);
    }
}
