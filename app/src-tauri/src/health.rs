//! Swasthya: reads the report the `bhandara-health` helper writes, and on
//! desktop raises a notification when a drive's verdict gets worse.

use bhandara_core::health::{self, HealthReport, Verdict};
use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Serialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum State {
    /// A fresh report.
    Ok,
    /// The helper wrote a report but has stopped refreshing it.
    Stale,
    /// No report: the helper is not installed or has not run yet.
    Missing,
    /// No SMART access on this platform (Android without root).
    Unsupported,
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct HealthStatus {
    pub state: State,
    pub report: Option<HealthReport>,
    /// Windows: the app can install the helper's scheduled task.
    pub can_install: bool,
}

pub fn status() -> HealthStatus {
    let Some(path) = health::report_path() else {
        return HealthStatus { state: State::Unsupported, report: None, can_install: false };
    };
    let report = health::read_report(&path).ok();
    let state = match &report {
        Some(r) if r.is_stale(health::now()) => State::Stale,
        Some(_) => State::Ok,
        None => State::Missing,
    };
    HealthStatus { state, report, can_install: cfg!(windows) && state != State::Ok && windows::helper().is_some_and(|h| h.exists()) }
}

/// The worst verdict in a fresh report; nothing when health is unknown.
pub fn worst() -> Option<Verdict> {
    let status = status();
    if status.state != State::Ok {
        return None;
    }
    status.report?.worst()
}

/// Drives whose verdict got worse since they were last seen, as
/// notification (title, body) pairs. `seen` is updated in place.
pub fn alerts(seen: &mut BTreeMap<String, Verdict>, report: &HealthReport) -> Vec<(String, String)> {
    let mut alerts = Vec::new();
    for drive in &report.drives {
        let key = if drive.serial.is_empty() { drive.device.clone() } else { drive.serial.clone() };
        let before = seen.insert(key, drive.verdict).unwrap_or(Verdict::Good);
        if drive.verdict > before {
            let name = if drive.model.is_empty() { &drive.device } else { &drive.model };
            let advice = match drive.verdict {
                Verdict::Replace => "back it up and plan to replace it",
                _ => "keep an eye on it",
            };
            alerts.push((format!("{name}: {advice}"), drive.reasons.join(". ")));
        }
    }
    alerts
}

#[cfg(desktop)]
pub fn watch(app: tauri::AppHandle) {
    use std::time::Duration;
    use tauri::Manager;
    use tauri_plugin_notification::NotificationExt;

    const EVERY: Duration = Duration::from_secs(5 * 60);

    std::thread::spawn(move || {
        let Ok(dir) = app.path().app_data_dir() else { return };
        let path = dir.join("health-seen.json");
        loop {
            let status = status();
            if let (State::Ok, Some(report)) = (status.state, status.report) {
                let mut seen: BTreeMap<String, Verdict> =
                    std::fs::read(&path).ok().and_then(|bytes| serde_json::from_slice(&bytes).ok()).unwrap_or_default();
                for (title, body) in alerts(&mut seen, &report) {
                    let _ = app.notification().builder().title(title).body(body).show();
                }
                let _ = std::fs::create_dir_all(&dir);
                let _ = std::fs::write(&path, serde_json::to_vec(&seen).unwrap_or_default());
            }
            std::thread::sleep(EVERY);
        }
    });
}

/// Windows: asks for admin once (UAC) to add the SYSTEM scheduled task. The
/// NSIS installer does this at install time; this covers a skipped or
/// failed install step.
pub fn install() -> Result<(), String> {
    #[cfg(windows)]
    return windows::install();
    #[cfg(not(windows))]
    Err("the health helper is installed with systemd on Linux; see the README".into())
}

#[cfg(windows)]
mod windows {
    use std::os::windows::process::CommandExt;
    use std::path::PathBuf;
    use std::process::Command;

    const CREATE_NO_WINDOW: u32 = 0x0800_0000;

    /// The installer puts the helper next to the app.
    pub fn helper() -> Option<PathBuf> {
        Some(std::env::current_exe().ok()?.parent()?.join("bhandara-health.exe"))
    }

    pub fn install() -> Result<(), String> {
        let helper = helper().filter(|h| h.exists()).ok_or("the health helper is missing; reinstall Bhandara")?;
        let quoted = helper.display().to_string().replace('\'', "''");
        let script = format!(
            "$p = Start-Process -FilePath '{quoted}' -ArgumentList '--install' -Verb RunAs -WindowStyle Hidden -Wait -PassThru; exit $p.ExitCode"
        );
        let status = Command::new("powershell")
            .args(["-NoProfile", "-NonInteractive", "-Command", &script])
            .creation_flags(CREATE_NO_WINDOW)
            .status()
            .map_err(|e| e.to_string())?;
        if status.success() { Ok(()) } else { Err("health monitoring was not turned on".into()) }
    }
}

#[cfg(not(windows))]
mod windows {
    pub fn helper() -> Option<std::path::PathBuf> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bhandara_core::health::DriveHealth;

    fn drive(serial: &str, verdict: Verdict) -> DriveHealth {
        DriveHealth {
            device: "/dev/sda".into(),
            model: "Example SSD".into(),
            serial: serial.into(),
            protocol: "ATA".into(),
            rotational: false,
            verdict,
            reasons: vec!["Pending sectors: 8".into()],
            temperature: None,
            power_on_hours: None,
            wear: None,
            written: None,
            counters: BTreeMap::new(),
            baseline: BTreeMap::new(),
            first_seen: 0,
        }
    }

    #[test]
    fn alerts_only_when_a_verdict_gets_worse() {
        let mut seen = BTreeMap::new();
        let report = |verdict| HealthReport { generated: 0, drives: vec![drive("A", verdict)], errors: vec![] };

        assert!(alerts(&mut seen, &report(Verdict::Good)).is_empty());
        let raised = alerts(&mut seen, &report(Verdict::Watch));
        assert_eq!(raised, vec![("Example SSD: keep an eye on it".to_string(), "Pending sectors: 8".to_string())]);
        assert!(alerts(&mut seen, &report(Verdict::Watch)).is_empty(), "no repeat for the same verdict");
        assert_eq!(alerts(&mut seen, &report(Verdict::Replace)).len(), 1);
        assert!(alerts(&mut seen, &report(Verdict::Watch)).is_empty(), "getting better is not an alert");
    }

    #[test]
    fn first_sighting_of_a_bad_drive_alerts() {
        let mut seen = BTreeMap::new();
        let report = HealthReport { generated: 0, drives: vec![drive("B", Verdict::Replace)], errors: vec![] };
        assert_eq!(alerts(&mut seen, &report)[0].0, "Example SSD: back it up and plan to replace it");
    }
}
