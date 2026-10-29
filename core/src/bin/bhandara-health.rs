//! Root/SYSTEM helper: runs smartctl on every drive and writes the health
//! report the Bhandara app reads.
//!
//!   bhandara-health            write the report (needs root or admin)
//!   bhandara-health --print    print the report instead
//!   bhandara-health --install  Windows: add the SYSTEM scheduled task
//!   bhandara-health --uninstall  Windows: remove it

use bhandara_core::health;
use std::path::PathBuf;
use std::process::ExitCode;

fn main() -> ExitCode {
    let result = match std::env::args().nth(1).as_deref() {
        None => write(),
        Some("--print") => print(),
        #[cfg(windows)]
        Some("--install") => windows::install(),
        #[cfg(windows)]
        Some("--uninstall") => windows::uninstall(),
        Some(other) => Err(format!("unknown argument {other}")),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("bhandara-health: {e}");
            ExitCode::FAILURE
        }
    }
}

/// Windows ships smartctl.exe next to this helper; elsewhere it comes from
/// the system's smartmontools package.
fn smartctl() -> PathBuf {
    if cfg!(windows)
        && let Some(bundled) = std::env::current_exe().ok().and_then(|exe| Some(exe.parent()?.join("smartctl.exe")))
        && bundled.exists()
    {
        return bundled;
    }
    PathBuf::from("smartctl")
}

fn report() -> Result<(health::HealthReport, PathBuf), String> {
    let path = health::report_path().ok_or("drive health is not supported on this system")?;
    let mut report = health::collect(&smartctl()).map_err(|e| match e.kind() {
        std::io::ErrorKind::NotFound => "smartctl not found; install smartmontools".to_string(),
        _ => format!("running smartctl: {e}"),
    })?;
    let previous = health::read_report(&path).ok();
    health::apply_history(previous.as_ref(), &mut report);
    Ok((report, path))
}

fn write() -> Result<(), String> {
    let (report, path) = report()?;
    health::write_report(&path, &report).map_err(|e| format!("writing {}: {e}", path.display()))
}

fn print() -> Result<(), String> {
    let (report, _) = report()?;
    println!("{}", serde_json::to_string_pretty(&report).map_err(|e| e.to_string())?);
    Ok(())
}

#[cfg(windows)]
mod windows {
    use std::process::Command;

    const TASK: &str = "Bhandara Health";

    /// The task runs this exe as SYSTEM, so the exe must sit where only
    /// admins can write: under Program Files.
    pub fn install() -> Result<(), String> {
        let exe = std::env::current_exe().map_err(|e| e.to_string())?;
        let protected = ["ProgramW6432", "ProgramFiles"].iter().filter_map(std::env::var_os).any(|dir| exe.starts_with(dir));
        if !protected {
            return Err(format!("refusing to schedule {} as SYSTEM outside Program Files", exe.display()));
        }
        let action = format!("\"{}\"", exe.display());
        schtasks(&["/Create", "/TN", TASK, "/TR", &action, "/SC", "MINUTE", "/MO", "30", "/RU", "SYSTEM", "/RL", "HIGHEST", "/F"])?;
        schtasks(&["/Run", "/TN", TASK])
    }

    pub fn uninstall() -> Result<(), String> {
        schtasks(&["/Delete", "/TN", TASK, "/F"])
    }

    fn schtasks(args: &[&str]) -> Result<(), String> {
        let output = Command::new("schtasks").args(args).output().map_err(|e| e.to_string())?;
        if output.status.success() { Ok(()) } else { Err(String::from_utf8_lossy(&output.stderr).trim().to_string()) }
    }
}
