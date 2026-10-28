// Unit tests for the QML-free Model.js. Run: node --test omarchy/tests/*.test.mjs
import { test } from "node:test"
import assert from "node:assert/strict"
import { readFileSync } from "node:fs"
import { fileURLToPath } from "node:url"
import vm from "node:vm"

// Model.js is a QML `.pragma library` script, not a module: drop the pragma
// and evaluate it in a fresh context to reach its top-level functions.
const source = readFileSync(fileURLToPath(new URL("../bhandara.bar/Model.js", import.meta.url)), "utf8")
const Model = vm.createContext({})
vm.runInContext(source.replace(/^\.pragma library/m, ""), Model)

// Values from the vm realm fail strict deepEqual on prototype; compare as JSON.
const plain = value => JSON.parse(JSON.stringify(value))

test("formatBytes uses decimal units", () => {
  assert.equal(Model.formatBytes(0), "0 B")
  assert.equal(Model.formatBytes(1500), "1.5 kB")
  assert.equal(Model.formatBytes(371e9), "371 GB")
  assert.equal(Model.formatBytesShort(8.6e9), "8.6G")
})

test("formatRate floors tiny rates to zero", () => {
  assert.equal(Model.formatRate(999), "0 B/s")
  assert.equal(Model.formatRate(2.5e6), "2.5 MB/s")
})

test("usageLevel thresholds", () => {
  assert.equal(Model.usageLevel(0.5), "normal")
  assert.equal(Model.usageLevel(0.8), "warning")
  assert.equal(Model.usageLevel(0.95), "critical")
})

const findmnt = {
  filesystems: [
    { source: "/dev/mapper/root[/@]", target: "/", fstype: "btrfs", size: 1000, used: 600, avail: 300, label: "" },
    { source: "/dev/mapper/root[/@home]", target: "/home", fstype: "btrfs", size: 1000, used: 600, avail: 300, label: "" },
    { source: "/dev/nvme0n1p1", target: "/boot", fstype: "vfat", size: 100, used: 10, avail: 90, label: "" },
    { source: "/dev/sda1", target: "/run/media/user/STICK", fstype: "exfat", size: 64e9, used: 1e9, avail: 63e9, label: "STICK" },
    { source: "/dev/loop0", target: "/snap/x", fstype: "squashfs", size: 1, used: 1, avail: 0 },
    { source: "tmpfs", target: "/tmp", fstype: "tmpfs", size: 10, used: 1, avail: 9 }
  ]
}
const lsblk = {
  blockdevices: [
    { path: "/dev/nvme0n1", kname: "nvme0n1", type: "disk", tran: "nvme", model: "Fast SSD", vendor: "", rm: false, hotplug: false, size: 1e12,
      children: [
        { path: "/dev/nvme0n1p1", kname: "nvme0n1p1", type: "part", fstype: "vfat", size: 100 },
        { path: "/dev/nvme0n1p2", kname: "nvme0n1p2", type: "part", fstype: "crypto_LUKS", size: 1e12,
          children: [{ path: "/dev/mapper/root", kname: "dm-0", type: "crypt", fstype: "btrfs", size: 1e12 }] }
      ] },
    { path: "/dev/sda", kname: "sda", type: "disk", tran: "usb", model: "Flash", vendor: "Acme", rm: true, hotplug: true, size: 64e9,
      children: [{ path: "/dev/sda1", kname: "sda1", type: "part", fstype: "exfat", label: "STICK", size: 64e9 }] }
  ]
}

test("parseVolumes merges subvolumes and orders by kind", () => {
  const volumes = Model.parseVolumes(findmnt, lsblk)
  assert.deepEqual(plain(volumes.map(v => v.kind)), ["system", "removable", "boot"])

  const system = volumes[0]
  assert.equal(system.device, "/dev/mapper/root")
  assert.deepEqual(plain(system.mounts), ["/", "/home"])
  assert.equal(system.kname, "dm-0")
  assert.equal(system.encrypted, true)
  assert.equal(system.model, "Fast SSD")
  assert.equal(system.fraction, 600 / 900)

  const stick = volumes[1]
  assert.equal(stick.name, "STICK")
  assert.equal(stick.model, "Acme Flash")
  assert.equal(stick.glyph, Model.GLYPH_USB)
  assert.equal(Model.volumeMeta(stick), "Acme Flash · exfat · STICK")
})

test("diskstats rates between samples", () => {
  const line = (kname, read, written) => `   8 0 ${kname} 1 0 ${read} 0 1 0 ${written} 0 0 0 0`
  const before = Model.parseDiskstats(line("sda", 0, 0))
  const after = Model.parseDiskstats(line("sda", 2048, 4096))
  const rates = Model.diskRates(before, after, 2)
  assert.deepEqual(plain(rates.sda), { read: 2048 * 512 / 2, write: 4096 * 512 / 2 })
})

test("parseDu returns largest children first", () => {
  const usage = Model.parseDu("10\t/h/a\n30\t/h/b\n50\t/h\n", "/h", 5)
  assert.equal(usage.total, 50)
  assert.deepEqual(plain(usage.folders.map(f => f.name)), ["b", "a"])
})

test("bar label cycles used, free, none", () => {
  const volume = { fraction: 0.42, avail: 8.6e9 }
  assert.equal(Model.nextLabelMode("used"), "free")
  assert.equal(Model.nextLabelMode("none"), "used")
  assert.equal(Model.barLabel(volume, "used"), "42%")
  assert.equal(Model.barLabel(volume, "free"), "8.6G")
  assert.equal(Model.barLabel(volume, "none"), "")
})
