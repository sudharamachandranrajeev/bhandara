# Bhandara

<a href="https://github.com/sudharamachandranrajeev/brahma"><img src="https://img.shields.io/badge/part%20of-Brahma-8a5cf6" alt="Part of Brahma"></a>

**Bhandara** (भण्डार, "storeroom") is a cross-platform storage monitor for Linux (Omarchy), Windows and Android.

## Modules

- **Bhandara Bar**: Omarchy status bar widget showing a disk usage ring and a storage panel.
- **Bhandara Darshana** (दर्शन, "view"): a fluid, modern storage dashboard.
- **Bhandara Swasthya** (स्वास्थ्य, "health"): drive health from SMART data.
- **Bhandara Suchi** (सूची, "index"): system-wide file index, stats and safe file operations. *Planned.*

## What works today

- Every mounted volume with used/free space, model, filesystem and encryption.
- Live disk read/write throughput (Linux and Windows).
- The largest folders in your home directory.
- Eject for removable drives (Linux and Windows).
- Drive health (Linux and Windows) for NVMe and SATA drives: a plain verdict (Good, Watch, Replace) with the reasons, life used, temperature, hours powered on and data written. A notification when a drive's verdict gets worse.
- Windows: a tray icon whose ring shows system drive usage, turning amber or red when a drive needs attention. Click it for a quick-view flyout.
- Omarchy: the bar ring also reflects drive health, and the panel lists each drive.

## Layout

| Path | What |
|---|---|
| `core/` | Rust crate with all storage logic, shared by every app |
| `app/` | Tauri 2 app (Svelte 5 UI) for Linux, Windows and Android |
| `omarchy/bhandara.bar/` | Omarchy shell bar plugin (QML, self-contained) |
| `packaging/linux/` | systemd units for the drive health helper |

## Omarchy bar plugin

Copy `omarchy/bhandara.bar` to `~/.config/omarchy/plugins/`, then add the **Bhandara Storage** widget to your bar.
Right-click the widget to cycle the label (used %, free space, none). Middle-click opens the volume.

## Drive health

Reading SMART data needs root or admin, so the apps never read drives themselves. A small helper, `bhandara-health`, runs [smartctl](https://www.smartmontools.org/) every 30 minutes and writes a report that anyone can read. Bhandara only reads that report.

A drive is **Replace** when it reports a SMART failure, a critical NVMe warning, 95% or more of its rated life used, a failing attribute, or a rise in reallocated, pending, uncorrectable or media-error counts since Bhandara first saw it. It is **Watch** when any of those counts is above zero, life used is 80% or more, or it runs hot.

**Windows**: the installer sets everything up. It installs Bhandara for all users and adds a scheduled task, *Bhandara Health*, that runs the helper as SYSTEM. smartctl ships with the installer. If the task is missing, the dashboard offers *Turn on health monitoring*, which asks for admin once.

**Linux**: install smartmontools, then the helper and its timer:

```sh
sudo pacman -S smartmontools      # or your distribution's package
cargo build --release -p bhandara-core --bin bhandara-health
sudo install -m755 target/release/bhandara-health /usr/local/bin/
sudo install -m644 packaging/linux/bhandara-health.{service,timer} /etc/systemd/system/
sudo systemctl daemon-reload
sudo systemctl enable --now bhandara-health.timer
```

The report is written to `/run/bhandara/health.json`. Run `sudo bhandara-health --print` to see it straight away.

**Android**: SMART data needs root, so drive health is not shown.

## Development

You need Rust (stable), Node 22+, and the [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/) for your platform.

```sh
cd app
npm install
npm run tauri dev           # run the app
npm run tauri build         # build installers

cargo test --workspace      # Rust tests
node --test omarchy/tests/*.test.mjs   # bar plugin tests (from the repo root)
```

On Linux the app opens as a normal window. To try the tray icon instead, run it with `BHANDARA_TRAY=1`.

CI builds the Windows installer (`.exe`) and an Android debug APK on every push.

## License

[MIT](LICENSE)

The Windows installer also ships smartctl from [smartmontools](https://www.smartmontools.org/), a separate program under the GNU GPL v2; its license is installed next to it as `smartmontools-COPYING.txt`. Source: <https://github.com/smartmontools/smartmontools>.
