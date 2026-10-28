# Bhandara

<a href="https://github.com/sudharamachandranrajeev/brahma"><img src="https://img.shields.io/badge/part%20of-Brahma-8a5cf6" alt="Part of Brahma"></a>

**Bhandara** (भण्डार, "storeroom") is a cross-platform storage monitor for Linux (Omarchy), Windows and Android.

## Modules

- **Bhandara Bar**: Omarchy status bar widget showing a disk usage ring and a storage panel.
- **Bhandara Darshana** (दर्शन, "view"): a fluid, modern storage dashboard.
- **Bhandara Swasthya** (स्वास्थ्य, "health"): disk health and SMART status. *Planned.*
- **Bhandara Suchi** (सूची, "index"): system-wide file index, stats and safe file operations. *Planned.*

## What works today

- Every mounted volume with used/free space, model, filesystem and encryption.
- Live disk read/write throughput (Linux and Windows).
- The largest folders in your home directory.
- Eject for removable drives (Linux and Windows).
- Windows: a tray icon whose ring shows system drive usage. Click it for a quick-view flyout.

## Layout

| Path | What |
|---|---|
| `core/` | Rust crate with all storage logic, shared by every app |
| `app/` | Tauri 2 app (Svelte 5 UI) for Linux, Windows and Android |
| `omarchy/bhandara.bar/` | Omarchy shell bar plugin (QML, self-contained) |

## Omarchy bar plugin

Copy `omarchy/bhandara.bar` to `~/.config/omarchy/plugins/`, then add the **Bhandara Storage** widget to your bar.
Right-click the widget to cycle the label (used %, free space, none). Middle-click opens the volume.

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

CI builds the Windows installers (`.msi`, `.exe`) and an Android debug APK on every push.

## License

[MIT](LICENSE)
