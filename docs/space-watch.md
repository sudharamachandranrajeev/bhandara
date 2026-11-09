# Cleaning and space watch (Bhandara Shodhana)

Bhandara can find build output that a tool will make again, remove it when you say so, warn you when a drive runs low, and show what grew.

## Clean

Open the dashboard, then **Clean**, pick a folder, and press **Find build output**.

A folder is listed only when a marker file proves which tool made it:

| What | Proof |
|---|---|
| Rust `target/` | a `CACHEDIR.TAG` file from Cargo inside it |
| Flutter `build/`, `.dart_tool/` | a `pubspec.yaml` next to it |
| `node_modules/` | a `package.json` next to it |
| Gradle `build/`, `.gradle/` | a `build.gradle`, `build.gradle.kts` or `settings.gradle*` next to it |
| Python `.venv/` | a `pyvenv.cfg` inside it and a `pyproject.toml`, `requirements.txt`, `setup.py` or `Pipfile` next to it |
| `__pycache__/` | nothing but `.pyc` files inside it |
| any other folder | a `CACHEDIR.TAG` file inside it |

A folder named `build` with no marker is never listed.

Some folders are listed but cannot be picked, because a guard protects them:

- **Project changed recently.** Its own files changed in the last 14 days. You can change the number under Watch.
- **Git tracks files inside it.** So it is not pure output.
- **A program is running in the project.** For example a build or a dev server. Linux only.
- **Synced by Syncthing.** It is inside a Syncthing folder whose `.stignore` does not ignore it. Removing it here would remove it on your other devices too.
- **It is or holds a mount point.**

Nothing is picked at first. Press **Test run** to see exactly which folders would go. Then choose:

- **Move to Trash** can be undone, but frees no space until you empty the Trash.
- **Delete permanently** frees the space now. The next build makes the folders again.

Before a folder goes, release packages inside it (APK, AAB, AppImage, deb, rpm, msi, exe) are copied to `~/.local/share/dev.bhandara.app/release-backup/`. You can turn this off under Watch. Every removal is written to the receipts list.

**btrfs snapshots.** On btrfs, snapshots keep removed files on the disk. If a permanent clean frees less than half of what it removed, Bhandara says so and shows the `snapper` commands. Bhandara never deletes snapshots itself.

**Package caches.** The same view lists caches for mise, npm, uv, pip, Yarn and Dart/Flutter, with their size. **Clean** runs that tool's own command. Commands that need an administrator are only shown, with a copy button:

- the pacman cache: `paccache -rk1`
- the system journal: `journalctl --vacuum-size=500M`

Docker and yay are only shown as well.

## Watch

**Low-space alerts.** A drive is *Low* below 10% free. A drive of 200 GB or more is also *Low* below 20 GB free. It is *Critical* below 5% free. You get one notification each time a drive crosses into a worse level. Free space must rise 2 points before the drive can alert again. You can snooze or turn off alerts for each drive. Removable drives are not watched.

**What grew.** Once a day Bhandara records free space and the 200 biggest folders in your home folder. The Watch view lists what grew over 7 or 30 days. History is kept for 90 days.

**Scheduled clean.** This is off unless you turn it on. Once a week it removes build output from projects untouched for 30 days. The same guards apply. It uses the mode you choose when you turn it on, and sends a notification saying how much it freed.

## Install the background watch

**Linux** uses a user timer, so it needs no root. From the repository:

```sh
cargo build --release -p bhandara-core --bin bhandara-watch
install -Dm755 target/release/bhandara-watch ~/.local/bin/bhandara-watch
install -Dm644 packaging/linux/bhandara-watch.{service,timer} -t ~/.config/systemd/user/
systemctl --user daemon-reload
systemctl --user enable --now bhandara-watch.timer
```

To check it, run each of these:

- `bhandara-watch --test-alert` sends a test notification.
- `bhandara-watch --test-run ~/Work` lists what a clean would remove, and removes nothing.
- `bhandara-watch --print` shows the levels and what grew.

**Windows**: the tray app runs the watch every 15 minutes while it is open.

**Android**: levels and history update while Bhandara is open. There are no background alerts. Build output and package caches do not apply on Android.

## Files

All of these are in `~/.local/share/dev.bhandara.app/` (Windows: `%APPDATA%\dev.bhandara.app\`). Settings are in `~/.config/dev.bhandara.app/watch.json`.

| File | What it holds |
|---|---|
| `watch-state.json` | alert levels, last runs, the amount that can be cleaned (the bar reads this) |
| `history.jsonl` | one sample per day |
| `receipts.jsonl` | every folder removed |
| `release-backup/` | release packages copied out before a clean |
