# Drive health (Bhandara Swasthya)


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

