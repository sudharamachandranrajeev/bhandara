import QtQuick
import Quickshell
import Quickshell.Io
import qs.Commons
import qs.Ui
import "Model.js" as Model

// Storage bar widget: a usage ring (plus optional label) in the bar, and a
// panel with every mounted volume, live disk throughput, and the largest
// folders in $HOME.
Panel {
  id: root
  moduleName: "bhandara.bar"
  ipcTarget: "bhandara.bar"

  readonly property string home: Quickshell.env("HOME")
  readonly property string labelMode: setting("label", "used")
  readonly property string barMount: setting("mount", "/")
  readonly property color fg: bar ? bar.foreground : Color.foreground
  readonly property color dim: Qt.darker(fg, 1.4)
  readonly property int historyLength: 60

  property var volumes: []
  property var rates: ({})
  property var previousStats: null
  property real previousStatsAt: 0
  property string knownDevices: ""
  property var readHistory: []
  property var writeHistory: []
  property real totalRead: 0
  property real totalWrite: 0
  property var homeUsage: ({ total: 0, folders: [] })
  property real homeScannedAt: 0
  property int cursorIndex: -1

  readonly property var barVolume: Model.findVolume(volumes, barMount)
  readonly property bool busy: totalRead + totalWrite > 256 * 1024
  readonly property int rowCount: volumes.length + homeUsage.folders.length

  function levelColor(level) {
    if (level === "critical") return Color.urgent
    if (level === "warning") return Qt.tint(Color.accent, Util.alpha(Color.urgent, 0.6))
    return Color.accent
  }

  function barRingColor(level) {
    if (level === "critical") return Color.urgent
    if (level === "warning") return Qt.tint(fg, Util.alpha(Color.urgent, 0.6))
    return fg
  }

  function refreshVolumes() {
    if (!volumesProc.running) volumesProc.running = true
  }

  function refreshHome(force) {
    var stale = Date.now() - homeScannedAt > 5 * 60 * 1000
    if ((force || stale) && !duProc.running) duProc.running = true
  }

  function refreshAll() {
    refreshVolumes()
    refreshHome(true)
  }

  function applyVolumes(raw) {
    var parts = String(raw || "").split("\u001e")
    try {
      volumes = Model.parseVolumes(JSON.parse(parts[0] || "{}"), JSON.parse(parts[1] || "{}"))
    } catch (e) {
      console.warn("bhandara: could not parse volume data: " + e)
    }
    if (cursorIndex >= rowCount) cursorIndex = rowCount - 1
  }

  function applyDiskstats(raw) {
    var stats = Model.parseDiskstats(raw)
    var now = Date.now()

    // A device appearing or vanishing means something was (un)mounted.
    var devices = Object.keys(stats).sort().join(",")
    if (knownDevices !== "" && devices !== knownDevices) refreshVolumes()
    knownDevices = devices

    if (previousStats) {
      rates = Model.diskRates(previousStats, stats, (now - previousStatsAt) / 1000)
      var total = Model.totalRates(rates, volumes)
      totalRead = total.read
      totalWrite = total.write
      readHistory = Model.pushSample(readHistory, total.read, historyLength)
      writeHistory = Model.pushSample(writeHistory, total.write, historyLength)
    }
    previousStats = stats
    previousStatsAt = now
  }

  function openPath(path) {
    Quickshell.execDetached(["xdg-open", path])
    close()
  }

  function eject(volume) {
    if (!volume || volume.kind !== "removable" || ejectProc.running) return
    ejectProc.volumeName = volume.name
    ejectProc.command = ["gio", "mount", "--eject", volume.mount]
    ejectProc.running = true
  }

  function rowAt(index) {
    if (index < 0) return null
    if (index < volumes.length) return { volume: volumes[index] }
    var folder = homeUsage.folders[index - volumes.length]
    return folder ? { folder: folder } : null
  }

  function activateCursor() {
    var row = rowAt(cursorIndex)
    if (!row) return
    openPath(row.volume ? row.volume.mount : row.folder.path)
  }

  function moveCursor(delta) {
    if (rowCount === 0) return
    if (cursorIndex < 0) cursorIndex = delta > 0 ? 0 : rowCount - 1
    else cursorIndex = Math.max(0, Math.min(rowCount - 1, cursorIndex + delta))
  }

  function cycleLabel() {
    root.settings = Object.assign({}, root.settings, { label: Model.nextLabelMode(root.labelMode) })
    if (root.bar && root.bar.shell) root.bar.shell.updateEntryInline(root.moduleName, root.settings)
  }

  onOpenedChanged: {
    if (opened) {
      cursorIndex = -1
      refreshVolumes()
      refreshHome(false)
    }
  }

  implicitWidth: button.implicitWidth
  implicitHeight: button.implicitHeight

  Component.onCompleted: {
    refreshVolumes()
    statsProc.running = true
  }

  Process {
    id: volumesProc
    command: ["sh", "-c",
      "findmnt -D -J -b --real -o SOURCE,TARGET,FSTYPE,SIZE,USED,AVAIL,LABEL; printf '\\036';"
      + " lsblk -J -b -o PATH,KNAME,PKNAME,TYPE,RM,HOTPLUG,TRAN,MODEL,VENDOR,LABEL,FSTYPE,SIZE"]
    stdout: StdioCollector { waitForEnd: true; onStreamFinished: root.applyVolumes(text) }
  }

  Process {
    id: statsProc
    command: ["cat", "/proc/diskstats"]
    stdout: StdioCollector { waitForEnd: true; onStreamFinished: root.applyDiskstats(text) }
  }

  Process {
    id: duProc
    command: ["sh", "-c", "exec nice -n 19 ionice -c3 du -x -d1 -b \"$HOME\" 2>/dev/null"]
    stdout: StdioCollector {
      waitForEnd: true
      onStreamFinished: {
        root.homeUsage = Model.parseDu(text, root.home, 5)
        root.homeScannedAt = Date.now()
      }
    }
  }

  Process {
    id: ejectProc
    property string volumeName: ""
    onExited: function(exitCode) {
      if (exitCode === 0) {
        Quickshell.execDetached(["omarchy-notification-send", "-g", Model.GLYPH_EJECT,
          ejectProc.volumeName + " ejected", "It is now safe to unplug the device"])
      } else {
        Quickshell.execDetached(["omarchy-notification-send", "-u", "normal", "-g", Model.GLYPH_EJECT,
          "Couldn't eject " + ejectProc.volumeName, "A program is still using it"])
      }
      root.refreshVolumes()
    }
  }

  // Throughput: every second while the panel is open, slower otherwise so the
  // bar's activity dot still notices large copies.
  Timer {
    interval: root.opened ? 1000 : 3000
    running: true
    repeat: true
    onTriggered: if (!statsProc.running) statsProc.running = true
  }

  Timer {
    interval: root.opened ? 5000 : 60000
    running: true
    repeat: true
    onTriggered: root.refreshVolumes()
  }

  // ---------- Bar button ----------

  Row {
    id: button
    spacing: 0

    readonly property string label: Model.barLabel(root.barVolume, root.labelMode)
    readonly property string tooltip: root.barVolume
      ? root.barVolume.name + " · " + Model.formatBytes(root.barVolume.avail) + " free of "
        + Model.formatBytes(root.barVolume.used + root.barVolume.avail)
      : "Storage"

    function press(b) {
      if (b === Qt.RightButton) root.cycleLabel()
      else if (b === Qt.MiddleButton) root.openPath(root.barVolume ? root.barVolume.mount : root.home)
      else root.toggle()
    }

    BarIconButton {
      id: ringButton
      bar: root.bar
      tooltipText: button.tooltip
      slotSize: button.label !== "" ? Style.bar.iconCanvas + Style.space(10) : Style.bar.iconSlot
      onPressed: function(b) { button.press(b) }

      iconComponent: Item {
        UsageRing {
          anchors.fill: parent
          anchors.margins: 1
          lineWidth: Math.max(2, Style.space(2.2))
          fraction: root.barVolume ? root.barVolume.fraction : 0
          color: root.barRingColor(root.barVolume ? root.barVolume.level : "normal")
          trackColor: Util.alpha(root.bar ? root.bar.barForeground : Color.foreground, 0.22)
        }

        // Disk activity: lights up while data is moving.
        Rectangle {
          width: Math.max(4, Style.space(4))
          height: width
          radius: width / 2
          anchors.horizontalCenter: parent.horizontalCenter
          anchors.verticalCenter: parent.verticalCenter
          color: Color.accent
          opacity: root.busy ? 1 : 0

          Behavior on opacity { NumberAnimation { duration: 260 } }

          SequentialAnimation on scale {
            running: root.busy
            loops: Animation.Infinite
            NumberAnimation { from: 1; to: 0.55; duration: 600; easing.type: Easing.InOutSine }
            NumberAnimation { from: 0.55; to: 1; duration: 600; easing.type: Easing.InOutSine }
          }
        }
      }
    }

    WidgetButton {
      bar: root.bar
      visible: text !== "" && !ringButton.vertical
      text: button.label
      tooltipText: button.tooltip
      horizontalMargin: 2
      fontSize: Style.font.bodySmall
      onPressed: function(b) { button.press(b) }
    }

    Item {
      visible: button.label !== "" && !ringButton.vertical
      width: Style.space(6)
      height: 1
    }
  }

  // ---------- Panel ----------

  KeyboardPanel {
    id: panel
    anchorItem: button
    owner: root
    bar: root.bar
    open: root.opened
    focusTarget: keyCatcher
    contentWidth: panel.fittedContentWidth(Style.space(420))
    contentHeight: panel.fittedContentHeight(column.implicitHeight)

    PanelKeyCatcher {
      id: keyCatcher
      anchors.fill: parent
      onMoveRequested: function(dx, dy) { if (dy !== 0) root.moveCursor(dy) }
      onActivateRequested: root.activateCursor()
      onDeleteRequested: {
        var row = root.rowAt(root.cursorIndex)
        if (row && row.volume) root.eject(row.volume)
      }
      onCloseRequested: root.close()
      onTabRequested: function(direction) { root.switchPanel(direction) }
      onTextKey: function(t) { if (t === "r") root.refreshAll() }

      StorageContent {
        id: column
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.top: parent.top
        panel: root
      }
    }
  }
}
