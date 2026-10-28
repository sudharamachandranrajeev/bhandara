.pragma library

// Pure helpers for the storage widget. Kept free of QML so they can be unit
// tested outside the shell.

var GLYPH_SYSTEM = "\u{F02CA}"     // md-harddisk
var GLYPH_BOOT = "\u{F048B}"       // md-server
var GLYPH_USB = "\u{F129E}"        // md-usb_flash_drive
var GLYPH_SD = "\u{F07DC}"         // md-micro_sd
var GLYPH_FOLDER = "\u{F0256}"     // md-folder_outline
var GLYPH_OPEN = "\u{F0770}"       // md-folder_open
var GLYPH_EJECT = "\u{F01EA}"      // md-eject
var GLYPH_READ = "\u{F0045}"       // md-arrow_down
var GLYPH_WRITE = "\u{F005D}"      // md-arrow_up
var GLYPH_LOCK = "\u{F033E}"       // md-lock

var WARNING_FRACTION = 0.8
var CRITICAL_FRACTION = 0.9

var HIDDEN_PREFIXES = ["/run/credentials", "/var/lib/docker", "/var/lib/containers", "/snap/"]

function formatBytes(bytes) {
  var value = Number(bytes) || 0
  var units = ["B", "kB", "MB", "GB", "TB", "PB"]
  var unit = 0
  while (Math.abs(value) >= 1000 && unit < units.length - 1) {
    value /= 1000
    unit++
  }
  if (unit === 0) return Math.round(value) + " B"
  return (value < 10 ? value.toFixed(1) : Math.round(value).toString()) + " " + units[unit]
}

// Compact form for the bar: "371G", "8.6G".
function formatBytesShort(bytes) {
  var text = formatBytes(bytes)
  var parts = text.split(" ")
  return parts[0] + (parts[1] === "B" ? "B" : parts[1].charAt(0).toUpperCase())
}

function formatRate(bytesPerSecond) {
  var value = Math.max(0, Number(bytesPerSecond) || 0)
  if (value < 1000) return "0 B/s"
  return formatBytes(value) + "/s"
}

// Drive-marketing style size used by file managers for unlabelled volumes:
// 127.8 GB of partition space reads as a "128 GB Volume".
function volumeSizeLabel(bytes) {
  var gb = (Number(bytes) || 0) / 1e9
  if (gb >= 1000) return Math.round(gb / 1000) + " TB"
  if (gb >= 10) return Math.round(gb) + " GB"
  if (gb >= 1) return gb.toFixed(1) + " GB"
  return Math.max(1, Math.round(gb * 1000)) + " MB"
}

function usageLevel(fraction) {
  if (fraction >= CRITICAL_FRACTION) return "critical"
  if (fraction >= WARNING_FRACTION) return "warning"
  return "normal"
}

function percentText(fraction) {
  return Math.round((Number(fraction) || 0) * 100) + "%"
}

// lsblk -J nests children; flatten into { path: node } with a parent link.
function flattenBlockDevices(lsblkJson) {
  var byPath = {}
  var byKname = {}

  function visit(node, parentKname) {
    if (!node) return
    var entry = {
      path: node.path || "",
      kname: node.kname || "",
      pkname: node.pkname || parentKname || "",
      type: node.type || "",
      rm: node.rm === true || node.rm === "1",
      hotplug: node.hotplug === true || node.hotplug === "1",
      tran: node.tran || "",
      model: (node.model || "").trim(),
      vendor: (node.vendor || "").trim(),
      label: node.label || "",
      fstype: node.fstype || "",
      size: Number(node.size) || 0
    }
    if (entry.path) byPath[entry.path] = entry
    if (entry.kname) byKname[entry.kname] = entry
    var children = node.children || []
    for (var i = 0; i < children.length; i++) visit(children[i], entry.kname)
  }

  var devices = lsblkJson && lsblkJson.blockdevices ? lsblkJson.blockdevices : []
  for (var i = 0; i < devices.length; i++) visit(devices[i], "")
  return { byPath: byPath, byKname: byKname }
}

function topDisk(entry, blocks) {
  var current = entry
  var guard = 0
  while (current && current.type !== "disk" && current.pkname && guard++ < 8) {
    current = blocks.byKname[current.pkname]
  }
  return current && current.type === "disk" ? current : null
}

function isEncrypted(entry, blocks) {
  var current = entry
  var guard = 0
  while (current && guard++ < 8) {
    if (current.type === "crypt" || current.fstype === "crypto_LUKS") return true
    current = current.pkname ? blocks.byKname[current.pkname] : null
  }
  return false
}

function stripSubvolume(source) {
  var at = String(source || "").indexOf("[")
  return at === -1 ? String(source || "") : String(source).slice(0, at)
}

function isHiddenTarget(target) {
  for (var i = 0; i < HIDDEN_PREFIXES.length; i++) {
    if (target.indexOf(HIDDEN_PREFIXES[i]) === 0) return true
  }
  return false
}

function describeDisk(disk) {
  if (!disk) return ""
  var vendor = disk.vendor
  var model = disk.model
  if (vendor && model.toLowerCase().indexOf(vendor.toLowerCase()) === -1) return vendor + " " + model
  return model || vendor
}

var KIND_ORDER = { system: 0, home: 1, internal: 2, removable: 3, boot: 4 }

// Merge `findmnt -D -J -b --real` and `lsblk -J -b` output into volume
// records, one per block device, ordered for display.
function parseVolumes(findmntJson, lsblkJson) {
  var blocks = flattenBlockDevices(lsblkJson)
  var filesystems = findmntJson && findmntJson.filesystems ? findmntJson.filesystems : []
  var byDevice = {}
  var order = []

  for (var i = 0; i < filesystems.length; i++) {
    var fs = filesystems[i]
    var device = stripSubvolume(fs.source)
    var target = String(fs.target || "")
    if (device.indexOf("/dev/") !== 0 || device.indexOf("/dev/loop") === 0) continue
    if (isHiddenTarget(target)) continue
    if (!(Number(fs.size) > 0)) continue

    var volume = byDevice[device]
    if (!volume) {
      volume = {
        device: device,
        fstype: fs.fstype || "",
        label: fs.label || "",
        size: Number(fs.size) || 0,
        used: Number(fs.used) || 0,
        avail: Number(fs.avail) || 0,
        mounts: []
      }
      byDevice[device] = volume
      order.push(device)
    }
    volume.mounts.push(target)
  }

  var volumes = []
  for (var j = 0; j < order.length; j++) {
    var v = byDevice[order[j]]
    v.mounts.sort(function(a, b) { return a.length - b.length || (a < b ? -1 : 1) })
    v.mount = v.mounts[0]

    var block = blocks.byPath[v.device] || null
    var disk = block ? topDisk(block, blocks) : null
    v.kname = block ? block.kname : v.device.replace("/dev/", "")
    v.label = v.label || (block ? block.label : "")
    v.model = describeDisk(disk)
    v.transport = disk ? disk.tran : ""
    v.removable = !!(disk && (disk.rm || disk.hotplug || disk.tran === "usb")) || v.mount.indexOf("/run/media/") === 0 || v.mount.indexOf("/media/") === 0
    v.encrypted = block ? isEncrypted(block, blocks) : false

    if (v.mount === "/") v.kind = "system"
    else if (v.mount === "/boot" || v.mount === "/efi" || v.mount === "/boot/efi") v.kind = "boot"
    else if (v.removable) v.kind = "removable"
    else if (v.mount === "/home") v.kind = "home"
    else v.kind = "internal"

    if (v.kind === "system") v.name = "System"
    else if (v.kind === "boot") v.name = "Boot"
    else if (v.kind === "home") v.name = "Home"
    else if (v.label) v.name = v.label
    else v.name = volumeSizeLabel(block && block.size ? block.size : v.size) + " Volume"

    // Space accounting: `used + avail` excludes reserved blocks, which is
    // what users can actually fill.
    var usable = v.used + v.avail
    v.fraction = usable > 0 ? v.used / usable : 0
    v.level = usageLevel(v.fraction)
    v.glyph = glyphFor(v)
    volumes.push(v)
  }

  volumes.sort(function(a, b) {
    return (KIND_ORDER[a.kind] - KIND_ORDER[b.kind]) || (a.mount < b.mount ? -1 : 1)
  })
  return volumes
}

function glyphFor(volume) {
  if (volume.kind === "boot") return GLYPH_BOOT
  if (volume.kind !== "removable") return GLYPH_SYSTEM
  if (volume.fstype === "vfat" || volume.fstype === "exfat") {
    // Cameras and phones expose their cards as USB mass storage.
    return volume.model.toLowerCase().indexOf("dsc") !== -1 || volume.transport === "mmc" ? GLYPH_SD : GLYPH_USB
  }
  return GLYPH_USB
}

function volumeMeta(volume) {
  var parts = []
  if (volume.model) parts.push(volume.model)
  if (volume.fstype) parts.push(volume.fstype)
  if (volume.encrypted) parts.push("encrypted")
  if (volume.kind === "removable" || volume.kind === "internal") parts.push(shortMount(volume.mount))
  return parts.join(" · ")
}

function shortMount(path) {
  var match = /^\/run\/media\/[^/]+\/(.+)$/.exec(path)
  return match ? match[1] : path
}

// /proc/diskstats → { kname: { read: bytes, written: bytes } }
function parseDiskstats(text) {
  var stats = {}
  var lines = String(text || "").split("\n")
  for (var i = 0; i < lines.length; i++) {
    var fields = lines[i].trim().split(/\s+/)
    if (fields.length < 10) continue
    stats[fields[2]] = {
      read: (Number(fields[5]) || 0) * 512,
      written: (Number(fields[9]) || 0) * 512
    }
  }
  return stats
}

// Byte rates per kname between two diskstats samples taken `seconds` apart.
function diskRates(previous, current, seconds) {
  var rates = {}
  if (!previous || !current || !(seconds > 0)) return rates
  for (var kname in current) {
    var before = previous[kname]
    if (!before) continue
    rates[kname] = {
      read: Math.max(0, (current[kname].read - before.read) / seconds),
      write: Math.max(0, (current[kname].written - before.written) / seconds)
    }
  }
  return rates
}

function totalRates(rates, volumes) {
  var total = { read: 0, write: 0 }
  for (var i = 0; i < volumes.length; i++) {
    var rate = rates[volumes[i].kname]
    if (!rate) continue
    total.read += rate.read
    total.write += rate.write
  }
  return total
}

function pushSample(history, value, limit) {
  var next = history.slice(Math.max(0, history.length - limit + 1))
  next.push(value)
  return next
}

// `du -x -d1 -b <home>` → largest direct children, biggest first.
function parseDu(text, home, limit) {
  var folders = []
  var total = 0
  var lines = String(text || "").split("\n")
  for (var i = 0; i < lines.length; i++) {
    var match = /^(\d+)\s+(.+)$/.exec(lines[i])
    if (!match) continue
    var bytes = Number(match[1])
    var path = match[2]
    if (path === home) {
      total = bytes
      continue
    }
    folders.push({ path: path, name: path.slice(path.lastIndexOf("/") + 1), bytes: bytes })
  }
  folders.sort(function(a, b) { return b.bytes - a.bytes })
  return { total: total, folders: folders.slice(0, limit || 5) }
}

// Bar label modes, cycled by right-click.
var LABEL_MODES = ["used", "free", "none"]

function nextLabelMode(mode) {
  var index = LABEL_MODES.indexOf(mode)
  return LABEL_MODES[(index + 1) % LABEL_MODES.length]
}

function barLabel(volume, mode) {
  if (!volume || mode === "none") return ""
  if (mode === "free") return formatBytesShort(volume.avail)
  return percentText(volume.fraction)
}

function findVolume(volumes, mount) {
  for (var i = 0; i < volumes.length; i++) {
    if (volumes[i].mounts.indexOf(mount) !== -1) return volumes[i]
  }
  return volumes.length > 0 ? volumes[0] : null
}
