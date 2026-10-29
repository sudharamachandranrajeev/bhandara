//! Tray icon with a live usage ring and a quick-view flyout window. On by
//! default on Windows; elsewhere set BHANDARA_TRAY=1 (Omarchy has its bar).
//! The ring's colour is the worse of system-drive usage and drive health.

use crate::health;
use bhandara_core::health::Verdict;
use bhandara_core::{Kind, Level, Volume};
use std::time::Duration;
use tauri::image::Image;
use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{App, AppHandle, Manager, PhysicalPosition, Rect, WebviewUrl, WebviewWindowBuilder, WindowEvent};

const TRAY_ID: &str = "bhandara";
const FLYOUT: &str = "flyout";
const FLYOUT_SIZE: (f64, f64) = (360.0, 600.0);
const ICON_SIZE: u32 = 32;
const REFRESH: Duration = Duration::from_secs(60);

pub fn enabled() -> bool {
    cfg!(windows) || std::env::var_os("BHANDARA_TRAY").is_some()
}

pub fn setup(app: &App) -> tauri::Result<()> {
    let main = app.get_webview_window("main").expect("main window is declared in tauri.conf.json");
    if !enabled() {
        return main.show();
    }

    // Closing the dashboard keeps Bhandara running in the tray.
    let dashboard = main.clone();
    main.on_window_event(move |event| {
        if let WindowEvent::CloseRequested { api, .. } = event {
            api.prevent_close();
            let _ = dashboard.hide();
        }
    });

    let flyout = WebviewWindowBuilder::new(app, FLYOUT, WebviewUrl::App("index.html".into()))
        .title("Bhandara")
        .inner_size(FLYOUT_SIZE.0, FLYOUT_SIZE.1)
        .decorations(false)
        .resizable(false)
        .skip_taskbar(true)
        .always_on_top(true)
        .visible(false)
        .build()?;
    let handle = flyout.clone();
    flyout.on_window_event(move |event| {
        if let WindowEvent::Focused(false) = event {
            let _ = handle.hide();
        }
    });

    let open = MenuItem::with_id(app, "open", "Open Bhandara", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
    let (summary, drives) = (system_summary(), health::worst());
    TrayIconBuilder::with_id(TRAY_ID)
        .icon(ring_icon(summary.as_ref(), drives))
        .tooltip(tooltip(summary.as_ref(), drives))
        .menu(&Menu::with_items(app, &[&open, &quit])?)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id().as_ref() {
            "open" => show_dashboard(app),
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, rect, .. } = event {
                toggle_flyout(tray.app_handle(), rect);
            }
        })
        .build(app)?;

    let app = app.handle().clone();
    std::thread::spawn(move || loop {
        std::thread::sleep(REFRESH);
        let (summary, drives) = (system_summary(), health::worst());
        if let Some(tray) = app.tray_by_id(TRAY_ID) {
            let _ = tray.set_icon(Some(ring_icon(summary.as_ref(), drives)));
            let _ = tray.set_tooltip(Some(tooltip(summary.as_ref(), drives)));
        }
    });
    Ok(())
}

fn show_dashboard(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    }
}

fn toggle_flyout(app: &AppHandle, anchor: Rect) {
    let Some(flyout) = app.get_webview_window(FLYOUT) else { return };
    if flyout.is_visible().unwrap_or(false) {
        let _ = flyout.hide();
        return;
    }
    if let Some(position) = flyout_position(&flyout, anchor) {
        let _ = flyout.set_position(position);
    }
    let _ = flyout.show();
    let _ = flyout.set_focus();
}

/// Centre the flyout on the tray icon, on the side away from the taskbar,
/// kept inside the monitor's work area.
fn flyout_position(flyout: &tauri::WebviewWindow, anchor: Rect) -> Option<PhysicalPosition<i32>> {
    let monitor = flyout.current_monitor().ok().flatten().or_else(|| flyout.primary_monitor().ok().flatten())?;
    let scale = monitor.scale_factor();
    let icon = anchor.position.to_physical::<f64>(scale);
    let icon_size = anchor.size.to_physical::<f64>(scale);
    let (width, height) = (FLYOUT_SIZE.0 * scale, FLYOUT_SIZE.1 * scale);
    let margin = 12.0 * scale;

    let area = monitor.work_area();
    let (left, top) = (area.position.x as f64, area.position.y as f64);
    let (right, bottom) = (left + area.size.width as f64, top + area.size.height as f64);

    let x = (icon.x + icon_size.width / 2.0 - width / 2.0).clamp(left + margin, right - width - margin);
    let below = icon.y < (top + bottom) / 2.0;
    let y = if below { icon.y + icon_size.height + margin } else { icon.y - height - margin };
    let y = y.clamp(top + margin, bottom - height - margin);
    Some(PhysicalPosition::new(x.round() as i32, y.round() as i32))
}

fn system_summary() -> Option<Volume> {
    let volumes = bhandara_core::volumes().ok()?;
    volumes.iter().find(|v| v.kind == Kind::System).or(volumes.first()).cloned()
}

fn format_bytes(bytes: u64) -> String {
    let units = ["B", "kB", "MB", "GB", "TB", "PB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1000.0 && unit < units.len() - 1 {
        value /= 1000.0;
        unit += 1;
    }
    if unit == 0 || value >= 10.0 { format!("{} {}", value.round(), units[unit]) } else { format!("{value:.1} {}", units[unit]) }
}

fn tooltip(volume: Option<&Volume>, drives: Option<Verdict>) -> String {
    let usage = match volume {
        Some(v) => format!("{} · {} free of {}", v.name, format_bytes(v.avail), format_bytes(v.used + v.avail)),
        None => "Bhandara".into(),
    };
    match drives {
        Some(Verdict::Watch) => format!("{usage}\nA drive needs watching"),
        Some(Verdict::Replace) => format!("{usage}\nA drive should be replaced"),
        _ => usage,
    }
}

/// Usage colours the ring; a drive health problem can only make it worse.
fn icon_level(volume: Option<&Volume>, drives: Option<Verdict>) -> Level {
    let usage = volume.map_or(Level::Normal, |v| v.level);
    match drives {
        Some(Verdict::Replace) => Level::Critical,
        Some(Verdict::Watch) if usage == Level::Normal => Level::Warning,
        _ => usage,
    }
}

/// Anti-aliased usage ring drawn clockwise from twelve o'clock.
fn ring_icon(volume: Option<&Volume>, drives: Option<Verdict>) -> Image<'static> {
    let fraction = volume.map_or(0.0, |v| v.fraction.clamp(0.0, 1.0));
    let fill: [u8; 3] = match icon_level(volume, drives) {
        Level::Normal => [122, 162, 247],
        Level::Warning => [224, 175, 104],
        Level::Critical => [247, 118, 142],
    };
    let size = ICON_SIZE as f64;
    let (center, outer, inner) = (size / 2.0, size / 2.0 - 1.0, size / 2.0 - 6.0);

    let mut rgba = Vec::with_capacity((ICON_SIZE * ICON_SIZE * 4) as usize);
    for y in 0..ICON_SIZE {
        for x in 0..ICON_SIZE {
            let (dx, dy) = (x as f64 + 0.5 - center, y as f64 + 0.5 - center);
            let distance = (dx * dx + dy * dy).sqrt();
            let coverage = (outer - distance + 0.5).clamp(0.0, 1.0) * (distance - inner + 0.5).clamp(0.0, 1.0);
            // Angle from twelve o'clock, clockwise, as a 0..1 turn.
            let turn = (dx.atan2(-dy) / std::f64::consts::TAU).rem_euclid(1.0);
            let (rgb, alpha) = if turn <= fraction { (fill, 1.0) } else { ([200, 200, 210], 0.35) };
            rgba.extend_from_slice(&[rgb[0], rgb[1], rgb[2], (coverage * alpha * 255.0).round() as u8]);
        }
    }
    Image::new_owned(rgba, ICON_SIZE, ICON_SIZE)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_like_the_bar() {
        assert_eq!(format_bytes(207_490_203_648), "207 GB");
        assert_eq!(format_bytes(8_600_000_000), "8.6 GB");
        assert_eq!(format_bytes(512), "512 B");
    }

    #[test]
    fn ring_fills_clockwise_from_top() {
        let volume = Volume { fraction: 0.25, level: Level::Normal, ..sample() };
        let image = ring_icon(Some(&volume), None);
        let pixel = |x: u32, y: u32| {
            let i = ((y * ICON_SIZE + x) * 4) as usize;
            &image.rgba()[i..i + 4]
        };
        // Top-right quadrant of the ring is filled; bottom-left is track.
        assert_eq!(&pixel(22, 4)[..3], &[122, 162, 247]);
        assert_eq!(&pixel(9, 27)[..3], &[200, 200, 210]);
        // The hole in the middle is transparent.
        assert_eq!(pixel(16, 16)[3], 0);
    }

    #[test]
    fn health_can_only_raise_the_colour() {
        let normal = Volume { level: Level::Normal, ..sample() };
        let critical = Volume { level: Level::Critical, ..sample() };
        assert_eq!(icon_level(Some(&normal), None), Level::Normal);
        assert_eq!(icon_level(Some(&normal), Some(Verdict::Good)), Level::Normal);
        assert_eq!(icon_level(Some(&normal), Some(Verdict::Watch)), Level::Warning);
        assert_eq!(icon_level(Some(&normal), Some(Verdict::Replace)), Level::Critical);
        assert_eq!(icon_level(Some(&critical), Some(Verdict::Watch)), Level::Critical);
        assert!(tooltip(Some(&normal), Some(Verdict::Replace)).ends_with("\nA drive should be replaced"));
    }

    fn sample() -> Volume {
        Volume {
            device: String::new(),
            kname: String::new(),
            name: "System".into(),
            kind: Kind::System,
            mount: "/".into(),
            mounts: vec!["/".into()],
            fstype: String::new(),
            label: String::new(),
            model: String::new(),
            transport: String::new(),
            removable: false,
            encrypted: false,
            size: 0,
            used: 0,
            avail: 0,
            fraction: 0.0,
            level: Level::Normal,
        }
    }
}
