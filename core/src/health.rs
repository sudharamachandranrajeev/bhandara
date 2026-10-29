//! Drive health from smartctl's JSON output (smartmontools 7+).
//!
//! Reading SMART data needs root or admin, so a small helper
//! (`bhandara-health`) runs smartctl on a timer and writes a report file that
//! anyone can read. The app only reads that file and never asks for admin.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

/// A report older than this means the helper stopped running.
pub const STALE_AFTER_SECS: u64 = 2 * 60 * 60;

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "lowercase")]
pub enum Verdict {
    Good,
    Watch,
    Replace,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DriveHealth {
    pub device: String,
    pub model: String,
    pub serial: String,
    /// "NVMe", "ATA" or "SCSI", as smartctl names the protocol.
    pub protocol: String,
    pub rotational: bool,
    pub verdict: Verdict,
    /// Plain-language reasons for a verdict other than Good.
    pub reasons: Vec<String>,
    /// Degrees Celsius.
    pub temperature: Option<i64>,
    pub power_on_hours: Option<u64>,
    /// Percent of rated life used; can pass 100 on NVMe.
    pub wear: Option<u64>,
    /// Bytes written over the drive's life.
    pub written: Option<u64>,
    /// Counters that mean failing media: reallocated, pending,
    /// uncorrectable, mediaErrors.
    pub counters: BTreeMap<String, u64>,
    /// `counters` when the helper first saw this drive; any rise is a
    /// Replace.
    pub baseline: BTreeMap<String, u64>,
    /// Unix seconds.
    pub first_seen: u64,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct HealthReport {
    /// Unix seconds.
    pub generated: u64,
    pub drives: Vec<DriveHealth>,
    /// Drives smartctl could not read, as "device: message".
    pub errors: Vec<String>,
}

impl HealthReport {
    pub fn worst(&self) -> Option<Verdict> {
        self.drives.iter().map(|d| d.verdict).max()
    }

    pub fn is_stale(&self, now: u64) -> bool {
        now.saturating_sub(self.generated) > STALE_AFTER_SECS
    }
}

pub fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

/// Where the helper writes and the app reads. Linux: systemd's
/// RuntimeDirectory, root-owned. Windows: the install folder under Program
/// Files, which only admins can write.
pub fn report_path() -> Option<PathBuf> {
    if cfg!(target_os = "linux") {
        Some(PathBuf::from("/run/bhandara/health.json"))
    } else if cfg!(windows) {
        Some(std::env::current_exe().ok()?.parent()?.join("health.json"))
    } else {
        None
    }
}

pub fn read_report(path: &Path) -> std::io::Result<HealthReport> {
    let text = std::fs::read_to_string(path)?;
    serde_json::from_str(&text).map_err(std::io::Error::other)
}

/// Writes through a temporary file so readers never see half a report.
pub fn write_report(path: &Path, report: &HealthReport) -> std::io::Result<()> {
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_vec_pretty(report).map_err(std::io::Error::other)?)?;
    std::fs::rename(&tmp, path)
}

/// Runs smartctl on every drive it finds. Needs root or admin.
pub fn collect(smartctl: &Path) -> std::io::Result<HealthReport> {
    let scan = run_json(smartctl, &["--scan-open", "--json"])?;
    let mut report = HealthReport { generated: now(), ..Default::default() };
    for device in scan["devices"].as_array().into_iter().flatten() {
        let (Some(name), Some(kind)) = (device["name"].as_str(), device["type"].as_str()) else { continue };
        match run_json(smartctl, &["--json", "--all", "-d", kind, name]) {
            Ok(json) => match parse(&json) {
                Ok(drive) if !report.drives.iter().any(|d| same_drive(d, &drive)) => report.drives.push(drive),
                Ok(_) => {}
                Err(message) => report.errors.push(format!("{name}: {message}")),
            },
            Err(e) => report.errors.push(format!("{name}: {e}")),
        }
    }
    Ok(report)
}

/// smartctl's exit status is a bit mask, so a non-zero exit still carries
/// JSON worth reading; only unreadable output is an error here.
fn run_json(smartctl: &Path, args: &[&str]) -> std::io::Result<Value> {
    let output = Command::new(smartctl).args(args).output()?;
    serde_json::from_slice(&output.stdout).map_err(|e| std::io::Error::other(format!("smartctl output: {e}")))
}

fn same_drive(a: &DriveHealth, b: &DriveHealth) -> bool {
    if a.serial.is_empty() || b.serial.is_empty() { a.device == b.device } else { a.serial == b.serial }
}

/// Carries each drive's baseline forward from the previous report and marks
/// any rise in a failing-media counter as Replace.
pub fn apply_history(previous: Option<&HealthReport>, report: &mut HealthReport) {
    for drive in &mut report.drives {
        match previous.and_then(|p| p.drives.iter().find(|d| same_drive(d, drive))) {
            Some(before) => {
                drive.baseline = before.baseline.clone();
                drive.first_seen = before.first_seen;
            }
            None => {
                drive.baseline = drive.counters.clone();
                drive.first_seen = report.generated;
            }
        }
        for (key, &value) in &drive.counters {
            let base = drive.baseline.get(key).copied().unwrap_or(0);
            if value > base {
                drive.reasons.push(format!("{} rose from {base} to {value}", counter_label(key)));
                drive.verdict = Verdict::Replace;
            }
        }
    }
}

fn counter_label(key: &str) -> &'static str {
    match key {
        "reallocated" => "Reallocated sectors",
        "pending" => "Pending sectors",
        "uncorrectable" => "Uncorrectable errors",
        "mediaErrors" => "Media errors",
        _ => "Error count",
    }
}

/// One drive's `smartctl --json --all` output.
pub fn parse(json: &Value) -> Result<DriveHealth, String> {
    let exit = json["smartctl"]["exit_status"].as_u64().unwrap_or(0);
    // Bits 0 and 1: bad arguments, or the device could not be opened.
    if exit & 0b11 != 0 {
        let message = json["smartctl"]["messages"][0]["string"].as_str().unwrap_or("smartctl could not read the drive");
        return Err(message.to_string());
    }
    let device = json["device"]["name"].as_str().ok_or("no device name")?.to_string();
    let mut drive = DriveHealth {
        device,
        model: text(&json["model_name"]).or_else(|| text(&json["model_family"])).unwrap_or_default(),
        serial: text(&json["serial_number"]).unwrap_or_default(),
        protocol: text(&json["device"]["protocol"]).unwrap_or_default(),
        rotational: json["rotation_rate"].as_u64().is_some_and(|r| r > 0),
        verdict: Verdict::Good,
        reasons: Vec::new(),
        temperature: json["temperature"]["current"].as_i64(),
        power_on_hours: json["power_on_time"]["hours"].as_u64(),
        wear: None,
        written: None,
        counters: BTreeMap::new(),
        baseline: BTreeMap::new(),
        first_seen: 0,
    };
    let mut flag = |verdict: Verdict, reason: String, drive: &mut DriveHealth| {
        drive.verdict = drive.verdict.max(verdict);
        drive.reasons.push(reason);
    };

    if json["smart_status"]["passed"].as_bool() == Some(false) {
        flag(Verdict::Replace, "The drive reports a SMART failure".into(), &mut drive);
    }

    let nvme = &json["nvme_smart_health_information_log"];
    if nvme.is_object() {
        read_nvme(nvme, &mut drive, &mut flag);
    } else {
        read_ata(json, &mut drive, &mut flag);
    }

    if let Some(wear) = drive.wear {
        if wear >= 95 {
            flag(Verdict::Replace, format!("{wear}% of rated life used"), &mut drive);
        } else if wear >= 80 {
            flag(Verdict::Watch, format!("{wear}% of rated life used"), &mut drive);
        }
    }
    let hot = if drive.rotational { 55 } else { 70 };
    if let Some(t) = drive.temperature.filter(|&t| t >= hot) {
        flag(Verdict::Watch, format!("Running hot at {t} °C"), &mut drive);
    }
    for (key, &value) in &drive.counters.clone() {
        if value > 0 {
            flag(Verdict::Watch, format!("{}: {value}", counter_label(key)), &mut drive);
        }
    }
    Ok(drive)
}

fn read_nvme(log: &Value, drive: &mut DriveHealth, flag: &mut impl FnMut(Verdict, String, &mut DriveHealth)) {
    let warning = log["critical_warning"].as_u64().unwrap_or(0);
    for (bit, verdict, reason) in [
        (0x01, Verdict::Replace, "Spare capacity is below the drive's threshold"),
        (0x02, Verdict::Watch, "Temperature is outside the drive's limits"),
        (0x04, Verdict::Replace, "The drive reports degraded reliability"),
        (0x08, Verdict::Replace, "The drive has switched to read-only"),
        (0x10, Verdict::Watch, "The drive's backup power has failed"),
    ] {
        if warning & bit != 0 {
            flag(verdict, reason.into(), drive);
        }
    }
    drive.wear = log["percentage_used"].as_u64();
    // NVMe data units are 1000 blocks of 512 bytes.
    drive.written = log["data_units_written"].as_u64().map(|units| units.saturating_mul(512_000));
    drive.power_on_hours = drive.power_on_hours.or(log["power_on_hours"].as_u64());
    drive.temperature = drive.temperature.or(log["temperature"].as_i64());
    if let Some(errors) = log["media_errors"].as_u64() {
        drive.counters.insert("mediaErrors".into(), errors);
    }
}

fn read_ata(json: &Value, drive: &mut DriveHealth, flag: &mut impl FnMut(Verdict, String, &mut DriveHealth)) {
    let attributes = json["ata_smart_attributes"]["table"].as_array().map(Vec::as_slice).unwrap_or_default();
    let block = json["logical_block_size"].as_u64().unwrap_or(512);
    // Some vendors pack extra data into the high bytes of the raw value.
    let raw = |id: u64| attributes.iter().find(|a| a["id"].as_u64() == Some(id)).and_then(|a| a["raw"]["value"].as_u64()).map(|v| v & 0xFFFF_FFFF);

    for attribute in attributes {
        let name = attribute["name"].as_str().unwrap_or("attribute").replace('_', " ");
        match attribute["when_failed"].as_str() {
            Some("now") => flag(Verdict::Replace, format!("{name} is below the failure threshold"), drive),
            Some("past") => flag(Verdict::Watch, format!("{name} was below the failure threshold in the past"), drive),
            _ => {}
        }
    }

    if let Some(n) = raw(5) {
        drive.counters.insert("reallocated".into(), n);
    }
    if let Some(n) = raw(197) {
        drive.counters.insert("pending".into(), n);
    }
    let uncorrectable = [raw(187), raw(198)];
    if uncorrectable.iter().any(Option::is_some) {
        drive.counters.insert("uncorrectable".into(), uncorrectable.iter().flatten().sum());
    }

    let statistic = |wanted: &str| {
        json["ata_device_statistics"]["pages"].as_array()?.iter()
            .flat_map(|page| page["table"].as_array().into_iter().flatten())
            .find(|entry| entry["name"].as_str() == Some(wanted))?["value"].as_u64()
    };
    // The standard device statistic first; vendor attributes only by the
    // names smartctl's drive database gives them, since their units vary.
    drive.wear = statistic("Percentage Used Endurance Indicator").or_else(|| {
        attributes.iter()
            .find(|a| matches!(a["name"].as_str(), Some("Wear_Leveling_Count" | "SSD_Life_Left" | "Media_Wearout_Indicator" | "Percent_Lifetime_Remain")))
            .and_then(|a| a["value"].as_u64())
            .filter(|&remaining| remaining <= 100)
            .map(|remaining| 100 - remaining)
    });
    drive.written = statistic("Logical Sectors Written").map(|sectors| sectors.saturating_mul(block)).or_else(|| {
        attributes.iter().find_map(|a| {
            let raw = a["raw"]["value"].as_u64()?;
            let unit = match a["name"].as_str()? {
                "Total_LBAs_Written" => block,
                "Host_Writes_GiB" | "Lifetime_Writes_GiB" => 1 << 30,
                "Host_Writes_32MiB" => 32 << 20,
                _ => return None,
            };
            Some(raw.saturating_mul(unit))
        })
    });
}

fn text(value: &Value) -> Option<String> {
    value.as_str().map(str::trim).filter(|s| !s.is_empty()).map(String::from)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(name: &str) -> Value {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/smartctl").join(name);
        serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
    }

    #[test]
    fn healthy_nvme() {
        let drive = parse(&fixture("nvme-good.json")).unwrap();
        assert_eq!(drive.verdict, Verdict::Good, "{:?}", drive.reasons);
        assert_eq!(drive.protocol, "NVMe");
        assert_eq!(drive.wear, Some(3));
        assert_eq!(drive.written, Some(25_000_000 * 512_000));
        assert_eq!(drive.temperature, Some(41));
        assert_eq!(drive.counters["mediaErrors"], 0);
    }

    #[test]
    fn worn_nvme_with_spare_warning() {
        let drive = parse(&fixture("nvme-worn.json")).unwrap();
        assert_eq!(drive.verdict, Verdict::Replace);
        assert!(drive.reasons.iter().any(|r| r.contains("Spare capacity")));
        assert!(drive.reasons.iter().any(|r| r == "97% of rated life used"));
        assert!(drive.reasons.iter().any(|r| r == "Media errors: 12"));
    }

    #[test]
    fn sata_ssd_reads_device_statistics() {
        let drive = parse(&fixture("sata-ssd.json")).unwrap();
        assert_eq!(drive.verdict, Verdict::Good, "{:?}", drive.reasons);
        assert_eq!(drive.wear, Some(7));
        assert_eq!(drive.written, Some(40_000_000_000 * 512));
        assert!(!drive.rotational);
    }

    #[test]
    fn hdd_with_pending_sectors_is_watch() {
        let drive = parse(&fixture("hdd-pending.json")).unwrap();
        assert_eq!(drive.verdict, Verdict::Watch);
        assert!(drive.rotational);
        assert_eq!(drive.counters["pending"], 8);
        assert_eq!(drive.counters["uncorrectable"], 2);
        assert_eq!(drive.written, None, "no standard source for bytes written");
    }

    #[test]
    fn failed_smart_is_replace() {
        let mut json = fixture("sata-ssd.json");
        json["smart_status"]["passed"] = false.into();
        assert_eq!(parse(&json).unwrap().verdict, Verdict::Replace);
    }

    #[test]
    fn unreadable_drive_is_an_error() {
        let json = serde_json::json!({
            "smartctl": { "exit_status": 2, "messages": [{ "string": "Smartctl open device: /dev/sdz failed: Permission denied" }] },
            "device": { "name": "/dev/sdz" }
        });
        assert!(parse(&json).unwrap_err().contains("Permission denied"));
    }

    #[test]
    fn rising_counter_since_first_seen_is_replace() {
        let mut first = HealthReport { generated: 100, drives: vec![parse(&fixture("hdd-pending.json")).unwrap()], errors: vec![] };
        apply_history(None, &mut first);
        assert_eq!(first.drives[0].baseline["pending"], 8);
        assert_eq!(first.drives[0].verdict, Verdict::Watch);

        let mut json = fixture("hdd-pending.json");
        json["ata_smart_attributes"]["table"][2]["raw"]["value"] = 12.into();
        let mut second = HealthReport { generated: 200, drives: vec![parse(&json).unwrap()], errors: vec![] };
        apply_history(Some(&first), &mut second);
        let drive = &second.drives[0];
        assert_eq!(drive.verdict, Verdict::Replace);
        assert_eq!(drive.first_seen, 100);
        assert!(drive.reasons.iter().any(|r| r == "Pending sectors rose from 8 to 12"));
        assert_eq!(second.worst(), Some(Verdict::Replace));
    }

    #[test]
    fn report_round_trips_and_goes_stale() {
        let report = HealthReport { generated: 1_000, drives: vec![parse(&fixture("nvme-good.json")).unwrap()], errors: vec![] };
        let json = serde_json::to_string(&report).unwrap();
        assert_eq!(serde_json::from_str::<HealthReport>(&json).unwrap(), report);
        assert!(!report.is_stale(1_000 + STALE_AFTER_SECS));
        assert!(report.is_stale(1_001 + STALE_AFTER_SECS));
    }
}
