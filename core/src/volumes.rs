use serde::Serialize;

pub const WARNING_FRACTION: f64 = 0.8;
pub const CRITICAL_FRACTION: f64 = 0.9;

/// Display order follows declaration order.
#[derive(Serialize, Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    System,
    Home,
    Internal,
    Removable,
    Boot,
}

#[derive(Serialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Level {
    Normal,
    Warning,
    Critical,
}

impl Level {
    pub fn of(fraction: f64) -> Self {
        if fraction >= CRITICAL_FRACTION {
            Level::Critical
        } else if fraction >= WARNING_FRACTION {
            Level::Warning
        } else {
            Level::Normal
        }
    }
}

#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Volume {
    pub device: String,
    /// Kernel name used to match throughput counters ("dm-0", "sda1").
    pub kname: String,
    pub name: String,
    pub kind: Kind,
    /// Shortest mount point; `mounts` lists every one (btrfs subvolumes).
    pub mount: String,
    pub mounts: Vec<String>,
    pub fstype: String,
    pub label: String,
    pub model: String,
    pub transport: String,
    pub removable: bool,
    pub encrypted: bool,
    pub size: u64,
    pub used: u64,
    pub avail: u64,
    /// `used / (used + avail)`: reserved blocks are excluded because users
    /// cannot fill them.
    pub fraction: f64,
    pub level: Level,
}

impl Volume {
    fn finish(mut self) -> Self {
        let usable = self.used + self.avail;
        self.fraction = if usable > 0 { self.used as f64 / usable as f64 } else { 0.0 };
        self.level = Level::of(self.fraction);
        self
    }
}

fn sort(volumes: &mut [Volume]) {
    volumes.sort_by(|a, b| a.kind.cmp(&b.kind).then_with(|| a.mount.cmp(&b.mount)));
}

#[cfg_attr(target_os = "android", allow(dead_code))]
/// Drive-marketing style size used by file managers for unlabelled volumes:
/// 127.8 GB of partition space reads as a "128 GB Volume".
fn size_label(bytes: u64) -> String {
    let gb = bytes as f64 / 1e9;
    if gb >= 1000.0 {
        format!("{} TB", (gb / 1000.0).round())
    } else if gb >= 10.0 {
        format!("{} GB", gb.round())
    } else if gb >= 1.0 {
        format!("{gb:.1} GB")
    } else {
        format!("{} MB", (gb * 1000.0).round().max(1.0))
    }
}

#[cfg(target_os = "linux")]
pub use linux::volumes;

#[cfg(target_os = "linux")]
mod linux {
    //! Port of the Omarchy bar plugin's Model.js `parseVolumes`: merges
    //! `findmnt` and `lsblk` JSON into one record per block device.

    use super::*;
    use serde_json::Value;
    use std::collections::HashMap;
    use std::io;
    use std::process::Command;

    const HIDDEN_PREFIXES: [&str; 4] = ["/run/credentials", "/var/lib/docker", "/var/lib/containers", "/snap/"];

    pub fn volumes() -> io::Result<Vec<Volume>> {
        let findmnt = run(&["findmnt", "-D", "-J", "-b", "--real", "-o", "SOURCE,TARGET,FSTYPE,SIZE,USED,AVAIL,LABEL"])?;
        let lsblk = run(&["lsblk", "-J", "-b", "-o", "PATH,KNAME,PKNAME,TYPE,RM,HOTPLUG,TRAN,MODEL,VENDOR,LABEL,FSTYPE,SIZE"])?;
        Ok(parse(&findmnt, &lsblk))
    }

    fn run(args: &[&str]) -> io::Result<Value> {
        let output = Command::new(args[0]).args(&args[1..]).output()?;
        if !output.status.success() {
            return Err(io::Error::other(format!("{} exited with {}", args[0], output.status)));
        }
        serde_json::from_slice(&output.stdout).map_err(io::Error::other)
    }

    fn text(node: &Value, key: &str) -> String {
        node.get(key).and_then(Value::as_str).unwrap_or("").trim().to_string()
    }

    fn number(node: &Value, key: &str) -> u64 {
        match node.get(key) {
            Some(Value::Number(n)) => n.as_u64().unwrap_or(0),
            Some(Value::String(s)) => s.parse().unwrap_or(0),
            _ => 0,
        }
    }

    fn flag(node: &Value, key: &str) -> bool {
        matches!(node.get(key), Some(Value::Bool(true))) || node.get(key).and_then(Value::as_str) == Some("1")
    }

    struct Block {
        kname: String,
        pkname: String,
        kind: String,
        removable: bool,
        transport: String,
        model: String,
        vendor: String,
        label: String,
        fstype: String,
        size: u64,
    }

    #[derive(Default)]
    struct Blocks {
        by_path: HashMap<String, usize>,
        by_kname: HashMap<String, usize>,
        all: Vec<Block>,
    }

    impl Blocks {
        fn from(lsblk: &Value) -> Self {
            let mut blocks = Blocks::default();
            if let Some(devices) = lsblk.get("blockdevices").and_then(Value::as_array) {
                for device in devices {
                    blocks.visit(device, "");
                }
            }
            blocks
        }

        // lsblk nests children; flatten with a parent link.
        fn visit(&mut self, node: &Value, parent: &str) {
            let pkname = Some(text(node, "pkname")).filter(|s| !s.is_empty()).unwrap_or_else(|| parent.to_string());
            let block = Block {
                kname: text(node, "kname"),
                pkname,
                kind: text(node, "type"),
                removable: flag(node, "rm") || flag(node, "hotplug"),
                transport: text(node, "tran"),
                model: text(node, "model"),
                vendor: text(node, "vendor"),
                label: text(node, "label"),
                fstype: text(node, "fstype"),
                size: number(node, "size"),
            };
            let index = self.all.len();
            let path = text(node, "path");
            if !path.is_empty() {
                self.by_path.insert(path, index);
            }
            if !block.kname.is_empty() {
                self.by_kname.insert(block.kname.clone(), index);
            }
            let kname = block.kname.clone();
            self.all.push(block);
            for child in node.get("children").and_then(Value::as_array).into_iter().flatten() {
                self.visit(child, &kname);
            }
        }

        fn parent(&self, block: &Block) -> Option<&Block> {
            self.by_kname.get(&block.pkname).map(|&i| &self.all[i])
        }

        fn ancestors<'a>(&'a self, block: &'a Block) -> impl Iterator<Item = &'a Block> {
            std::iter::successors(Some(block), |b| self.parent(b)).take(8)
        }

        fn top_disk<'a>(&'a self, block: &'a Block) -> Option<&'a Block> {
            self.ancestors(block).find(|b| b.kind == "disk")
        }

        fn encrypted(&self, block: &Block) -> bool {
            self.ancestors(block).any(|b| b.kind == "crypt" || b.fstype == "crypto_LUKS")
        }
    }

    fn describe(disk: &Block) -> String {
        if !disk.vendor.is_empty() && !disk.model.to_lowercase().contains(&disk.vendor.to_lowercase()) {
            format!("{} {}", disk.vendor, disk.model)
        } else if disk.model.is_empty() {
            disk.vendor.clone()
        } else {
            disk.model.clone()
        }
    }

    pub(super) fn parse(findmnt: &Value, lsblk: &Value) -> Vec<Volume> {
        let blocks = Blocks::from(lsblk);
        let mut volumes: Vec<Volume> = Vec::new();

        for fs in findmnt.get("filesystems").and_then(Value::as_array).into_iter().flatten() {
            let source = text(fs, "source");
            // Strip the btrfs subvolume suffix: /dev/mapper/root[/@home].
            let device = source.split('[').next().unwrap_or("").to_string();
            let target = text(fs, "target");
            if !device.starts_with("/dev/") || device.starts_with("/dev/loop") {
                continue;
            }
            if HIDDEN_PREFIXES.iter().any(|p| target.starts_with(p)) || number(fs, "size") == 0 {
                continue;
            }
            if let Some(volume) = volumes.iter_mut().find(|v| v.device == device) {
                volume.mounts.push(target);
                continue;
            }
            volumes.push(Volume {
                device,
                kname: String::new(),
                name: String::new(),
                kind: Kind::Internal,
                mount: String::new(),
                mounts: vec![target],
                fstype: text(fs, "fstype"),
                label: text(fs, "label"),
                model: String::new(),
                transport: String::new(),
                removable: false,
                encrypted: false,
                size: number(fs, "size"),
                used: number(fs, "used"),
                avail: number(fs, "avail"),
                fraction: 0.0,
                level: Level::Normal,
            });
        }

        let mut volumes: Vec<Volume> = volumes
            .into_iter()
            .map(|mut v| {
                v.mounts.sort_by(|a, b| a.len().cmp(&b.len()).then_with(|| a.cmp(b)));
                v.mount = v.mounts[0].clone();

                let block = blocks.by_path.get(&v.device).map(|&i| &blocks.all[i]);
                let disk = block.and_then(|b| blocks.top_disk(b));
                v.kname = block.map(|b| b.kname.clone()).unwrap_or_else(|| v.device.trim_start_matches("/dev/").to_string());
                if v.label.is_empty() {
                    v.label = block.map(|b| b.label.clone()).unwrap_or_default();
                }
                v.model = disk.map(describe).unwrap_or_default();
                v.transport = disk.map(|d| d.transport.clone()).unwrap_or_default();
                v.removable = disk.is_some_and(|d| d.removable || d.transport == "usb")
                    || v.mount.starts_with("/run/media/")
                    || v.mount.starts_with("/media/");
                v.encrypted = block.is_some_and(|b| blocks.encrypted(b));

                v.kind = match v.mount.as_str() {
                    "/" => Kind::System,
                    "/boot" | "/efi" | "/boot/efi" => Kind::Boot,
                    _ if v.removable => Kind::Removable,
                    "/home" => Kind::Home,
                    _ => Kind::Internal,
                };
                v.name = match v.kind {
                    Kind::System => "System".into(),
                    Kind::Boot => "Boot".into(),
                    Kind::Home => "Home".into(),
                    _ if !v.label.is_empty() => v.label.clone(),
                    _ => format!("{} Volume", size_label(block.map(|b| b.size).filter(|&s| s > 0).unwrap_or(v.size))),
                };
                v.finish()
            })
            .collect();
        sort(&mut volumes);
        volumes
    }
}

#[cfg(not(target_os = "linux"))]
pub fn volumes() -> std::io::Result<Vec<Volume>> {
    use sysinfo::Disks;

    #[cfg(windows)]
    let system_drive = std::env::var("SystemDrive").unwrap_or_else(|_| "C:".into()).to_uppercase();

    let mut volumes: Vec<Volume> = Vec::new();
    for disk in Disks::new_with_refreshed_list().list() {
        let mount = disk.mount_point().to_string_lossy().into_owned();
        let device = disk.name().to_string_lossy().into_owned();
        let size = disk.total_space();
        let avail = disk.available_space();
        if size == 0 || volumes.iter().any(|v: &Volume| v.mount == mount) {
            continue;
        }

        #[cfg(target_os = "android")]
        let Some((kind, name)) = android::classify(&mount) else { continue };
        // Android cards cannot be ejected from an app.
        #[cfg(target_os = "android")]
        let removable = false;

        #[cfg(windows)]
        let removable = disk.is_removable();
        #[cfg(windows)]
        let (kind, name) = if mount.trim_end_matches('\\').to_uppercase() == system_drive {
            (Kind::System, format!("System ({})", mount.trim_end_matches('\\')))
        } else {
            let kind = if removable { Kind::Removable } else { Kind::Internal };
            // sysinfo reports the volume label as the name on Windows.
            let name = if device.is_empty() { format!("{} Volume", size_label(size)) } else { device.clone() };
            (kind, format!("{name} ({})", mount.trim_end_matches('\\')))
        };
        volumes.push(
            Volume {
                kname: device.clone(),
                device,
                name,
                kind,
                mounts: vec![mount.clone()],
                mount,
                fstype: disk.file_system().to_string_lossy().into_owned(),
                label: String::new(),
                model: String::new(),
                transport: String::new(),
                removable,
                encrypted: false,
                size,
                used: size.saturating_sub(avail),
                avail,
                fraction: 0.0,
                level: Level::Normal,
            }
            .finish(),
        );
    }
    sort(&mut volumes);
    Ok(volumes)
}

#[cfg(any(target_os = "android", test))]
mod android {
    use super::Kind;

    /// Android mounts shared storage several times over FUSE
    /// (/storage/emulated, /storage/self, ...) on top of /data. Show /data
    /// once as internal storage, plus real cards (/storage/XXXX-XXXX).
    pub fn classify(mount: &str) -> Option<(Kind, String)> {
        if mount == "/data" {
            return Some((Kind::System, "Internal storage".into()));
        }
        let card = mount.strip_prefix("/storage/")?;
        if card.is_empty() || card.contains('/') || card == "emulated" || card == "self" {
            return None;
        }
        Some((Kind::Removable, format!("SD card ({card})")))
    }

    #[cfg(test)]
    #[test]
    fn keeps_data_and_cards_only() {
        assert_eq!(classify("/data"), Some((Kind::System, "Internal storage".into())));
        assert_eq!(classify("/storage/1A2B-3C4D"), Some((Kind::Removable, "SD card (1A2B-3C4D)".into())));
        for hidden in ["/storage/emulated", "/storage/emulated/0", "/storage/self", "/storage/self/primary", "/system", "/data/media", "/mnt/pass_through/0/emulated"] {
            assert_eq!(classify(hidden), None, "{hidden}");
        }
    }
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;
    use serde_json::json;

    // Same fixture as omarchy/tests/model.test.mjs so both ports agree.
    fn fixture() -> Vec<Volume> {
        let findmnt = json!({ "filesystems": [
            { "source": "/dev/mapper/root[/@]", "target": "/", "fstype": "btrfs", "size": 1000, "used": 600, "avail": 300 },
            { "source": "/dev/mapper/root[/@home]", "target": "/home", "fstype": "btrfs", "size": 1000, "used": 600, "avail": 300 },
            { "source": "/dev/nvme0n1p1", "target": "/boot", "fstype": "vfat", "size": 100, "used": 10, "avail": 90 },
            { "source": "/dev/sda1", "target": "/run/media/user/STICK", "fstype": "exfat", "size": 64e9 as u64, "used": 1e9 as u64, "avail": 63e9 as u64, "label": "STICK" },
            { "source": "/dev/loop0", "target": "/snap/x", "fstype": "squashfs", "size": 1, "used": 1, "avail": 0 },
            { "source": "tmpfs", "target": "/tmp", "fstype": "tmpfs", "size": 10, "used": 1, "avail": 9 }
        ]});
        let lsblk = json!({ "blockdevices": [
            { "path": "/dev/nvme0n1", "kname": "nvme0n1", "type": "disk", "tran": "nvme", "model": "Fast SSD", "rm": false, "hotplug": false, "size": 1e12 as u64,
              "children": [
                { "path": "/dev/nvme0n1p1", "kname": "nvme0n1p1", "type": "part", "fstype": "vfat", "size": 100 },
                { "path": "/dev/nvme0n1p2", "kname": "nvme0n1p2", "type": "part", "fstype": "crypto_LUKS", "size": 1e12 as u64,
                  "children": [{ "path": "/dev/mapper/root", "kname": "dm-0", "type": "crypt", "fstype": "btrfs", "size": 1e12 as u64 }] }
              ]},
            { "path": "/dev/sda", "kname": "sda", "type": "disk", "tran": "usb", "model": "Flash", "vendor": "Acme", "rm": true, "hotplug": true, "size": 64e9 as u64,
              "children": [{ "path": "/dev/sda1", "kname": "sda1", "type": "part", "fstype": "exfat", "label": "STICK", "size": 64e9 as u64 }] }
        ]});
        linux::parse(&findmnt, &lsblk)
    }

    #[test]
    fn merges_subvolumes_and_orders_by_kind() {
        let volumes = fixture();
        let kinds: Vec<Kind> = volumes.iter().map(|v| v.kind).collect();
        assert_eq!(kinds, [Kind::System, Kind::Removable, Kind::Boot]);

        let system = &volumes[0];
        assert_eq!(system.device, "/dev/mapper/root");
        assert_eq!(system.mounts, ["/", "/home"]);
        assert_eq!(system.kname, "dm-0");
        assert!(system.encrypted);
        assert_eq!(system.model, "Fast SSD");
        assert_eq!(system.fraction, 600.0 / 900.0);

        let stick = &volumes[1];
        assert_eq!(stick.name, "STICK");
        assert_eq!(stick.model, "Acme Flash");
        assert!(stick.removable && !stick.encrypted);
    }

    #[test]
    fn level_thresholds() {
        assert_eq!(Level::of(0.5), Level::Normal);
        assert_eq!(Level::of(0.8), Level::Warning);
        assert_eq!(Level::of(0.95), Level::Critical);
    }

    #[test]
    fn size_labels() {
        assert_eq!(size_label(127_800_000_000), "128 GB");
        assert_eq!(size_label(2_000_000_000_000), "2 TB");
        assert_eq!(size_label(4_200_000_000), "4.2 GB");
    }

    #[test]
    fn serializes_camel_case() {
        let value = serde_json::to_value(&fixture()[0]).unwrap();
        assert_eq!(value["kind"], "system");
        assert_eq!(value["level"], "normal");
        assert!(value.get("fraction").is_some());
    }
}
