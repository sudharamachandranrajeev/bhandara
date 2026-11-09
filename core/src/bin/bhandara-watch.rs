//! Space watch runner for a user timer (no root needed).
//!
//!   bhandara-watch               alerts; a daily history sample; the
//!                                weekly scheduled clean when it is on
//!   bhandara-watch --sample      take a history sample now
//!   bhandara-watch --print       print settings, levels and what grew in 7 days
//!   bhandara-watch --test-run [FOLDER]  list what a clean would remove
//!   bhandara-watch --test-alert  send a test notification

use bhandara_core::clean;
use bhandara_core::watch::{self, Paths};
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::atomic::AtomicBool;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let result = Paths::standard().ok_or_else(|| "no home folder".to_string()).and_then(|paths| {
        let home = home()?;
        match args.first().map(String::as_str) {
            None => watch::tick(&paths, &home, &notify),
            Some("--sample") => watch::sample(&paths, &home).map(|_| ()),
            Some("--print") => print(&paths),
            Some("--test-run") => test_run(&paths, args.get(1).map(PathBuf::from).unwrap_or(home)),
            Some("--test-alert") => {
                notify("Bhandara test alert", "Space alerts are working.");
                Ok(())
            }
            Some(other) => Err(format!("unknown argument {other}")),
        }
    });
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("bhandara-watch: {e}");
            ExitCode::FAILURE
        }
    }
}

fn home() -> Result<PathBuf, String> {
    std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")).map(PathBuf::from).ok_or_else(|| "no home folder".into())
}

fn print(paths: &Paths) -> Result<(), String> {
    let report = serde_json::json!({
        "settings": paths.load_settings(),
        "state": paths.load_state(),
        "growth7d": watch::growth(&watch::read_history(&paths.history()), 7, 20),
    });
    println!("{}", serde_json::to_string_pretty(&report).map_err(|e| e.to_string())?);
    Ok(())
}

/// Lists what a clean would remove and why the rest is kept. Removes nothing.
fn test_run(paths: &Paths, root: PathBuf) -> Result<(), String> {
    let scan = clean::scan(&root, &watch::clean_options(&paths.load_settings(), 0), &AtomicBool::new(false), |_, _| {}).map_err(|e| e.to_string())?;
    for c in &scan.candidates {
        let what = match &c.skip {
            None => "remove".to_string(),
            Some(skip) => format!("keep: {}", serde_json::to_value(skip).ok().and_then(|v| v["reason"].as_str().map(String::from)).unwrap_or_default()),
        };
        println!("{:>9.2} GB  {:<27} {:<17} {}", c.bytes as f64 / 1e9, c.rule.label(), what, c.path.display());
    }
    println!("\n{:.1} GB could be removed now. Nothing was removed (test run).", scan.reclaimable() as f64 / 1e9);
    let mut state = paths.load_state();
    state.reclaimable = Some(watch::Reclaimable { bytes: scan.reclaimable(), root, at: scan.scanned_at });
    paths.save_state(&state).map_err(|e| e.to_string())
}

/// Desktop notification through notify-send; printed when that fails.
fn notify(title: &str, body: &str) {
    let sent = cfg!(target_os = "linux")
        && std::process::Command::new("notify-send")
            .args(["--app-name=Bhandara", "--icon=drive-harddisk", title, body])
            .status()
            .is_ok_and(|s| s.success());
    if !sent {
        println!("{title}: {body}");
    }
}
