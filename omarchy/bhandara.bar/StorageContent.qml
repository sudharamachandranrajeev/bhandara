import QtQuick
import qs.Commons
import qs.Ui
import "Model.js" as Model

// Body of the storage panel: hero gauge, live throughput, volumes, and the
// largest folders in $HOME. All data comes from the owning Panel.
Column {
  id: root

  // The storage Panel that owns the data (volumes, rates, home usage).
  required property var panel

  readonly property var bar: root.panel.bar
  readonly property color fg: root.panel.fg
  readonly property color dim: root.panel.dim
  spacing: Style.space(14)

  // ---------- Hero ----------
  PanelHero {
    foreground: root.fg
    fontFamily: root.bar ? root.bar.fontFamily : Style.font.family
    title: "Storage"
    meta: root.panel.barVolume
      ? Model.formatBytes(root.panel.barVolume.avail) + " free of " + Model.formatBytes(root.panel.barVolume.used + root.panel.barVolume.avail)
      : "Reading drives…"
    detail: root.panel.volumes.length === 1 ? "1 volume" : root.panel.volumes.length + " volumes"
    iconComponent: Item {
      implicitWidth: Style.space(58)
      implicitHeight: Style.space(58)

      UsageRing {
        anchors.fill: parent
        lineWidth: Style.space(5)
        fraction: root.panel.barVolume ? root.panel.barVolume.fraction : 0
        color: root.panel.levelColor(root.panel.barVolume ? root.panel.barVolume.level : "normal")
        trackColor: Util.alpha(root.fg, 0.1)
      }

      Text {
        anchors.centerIn: parent
        textFormat: Text.PlainText
        text: root.panel.barVolume ? Model.percentText(root.panel.barVolume.fraction) : "—"
        color: root.fg
        font.family: root.bar ? root.bar.fontFamily : Style.font.family
        font.pixelSize: Style.font.subtitle
        font.bold: true
      }
    }
  }

  // ---------- Live throughput ----------
  Row {
    width: parent.width
    spacing: Style.space(8)

    ThroughputCard {
      width: (parent.width - parent.spacing) / 2
      title: "READ"
      glyph: Model.GLYPH_READ
      rate: root.panel.totalRead
      samples: root.panel.readHistory
      tint: Color.accent
    }

    ThroughputCard {
      width: (parent.width - parent.spacing) / 2
      title: "WRITE"
      glyph: Model.GLYPH_WRITE
      rate: root.panel.totalWrite
      samples: root.panel.writeHistory
      tint: Qt.tint(Color.accent, Util.alpha(root.fg, 0.35))
    }
  }

  PanelSeparator { foreground: root.fg }

  // ---------- Volumes ----------
  Column {
    width: parent.width
    spacing: Style.space(4)

    PanelSectionHeader {
      text: "VOLUMES"
      foreground: root.fg
      fontFamily: root.bar ? root.bar.fontFamily : Style.font.family
    }

    Repeater {
      model: root.panel.volumes

      VolumeRow {
        required property var modelData
        required property int index
        width: parent.width
        volume: modelData
        hasCursor: root.panel.cursorIndex === index
        onHoveredChanged: if (hovered) root.panel.cursorIndex = index
      }
    }
  }

  PanelSeparator {
    foreground: root.fg
    visible: root.panel.homeUsage.folders.length > 0
  }

  // ---------- Largest folders in $HOME ----------
  Column {
    width: parent.width
    spacing: Style.space(2)
    visible: root.panel.homeUsage.folders.length > 0

    PanelSectionHeader {
      text: "HOME · " + Model.formatBytes(root.panel.homeUsage.total)
      foreground: root.fg
      fontFamily: root.bar ? root.bar.fontFamily : Style.font.family
    }

    Repeater {
      model: root.panel.homeUsage.folders

      FolderRow {
        required property var modelData
        required property int index
        width: parent.width
        folder: modelData
        share: root.panel.homeUsage.folders.length > 0 && root.panel.homeUsage.folders[0].bytes > 0
          ? modelData.bytes / root.panel.homeUsage.folders[0].bytes : 0
        hasCursor: root.panel.cursorIndex === root.panel.volumes.length + index
        onHoveredChanged: if (hovered) root.panel.cursorIndex = root.panel.volumes.length + index
      }
    }
  }

  // ---------- Components ----------

  component Caption: Text {
    textFormat: Text.PlainText
    color: root.dim
    font.family: root.bar ? root.bar.fontFamily : Style.font.family
    font.pixelSize: Style.font.caption
    font.bold: true
    font.letterSpacing: 1.1
  }

  component Body: Text {
    textFormat: Text.PlainText
    color: root.fg
    font.family: root.bar ? root.bar.fontFamily : Style.font.family
    font.pixelSize: Style.font.bodySmall
    elide: Text.ElideRight
  }

  component ThroughputCard: Rectangle {
    id: card
    property string title: ""
    property string glyph: ""
    property real rate: 0
    property var samples: []
    property color tint: Color.accent

    implicitHeight: cardColumn.implicitHeight + Style.space(10) * 2
    radius: Style.cornerRadius
    color: Util.alpha(root.fg, 0.05)

    Column {
      id: cardColumn
      anchors.fill: parent
      anchors.margins: Style.space(10)
      spacing: Style.space(6)

      Item {
        width: parent.width
        implicitHeight: Math.max(cardTitle.implicitHeight, cardRate.implicitHeight)

        Caption {
          id: cardTitle
          anchors.left: parent.left
          anchors.verticalCenter: parent.verticalCenter
          text: card.glyph + " " + card.title
          color: card.rate > 1000 ? card.tint : root.dim
        }

        Text {
          id: cardRate
          anchors.right: parent.right
          anchors.verticalCenter: parent.verticalCenter
          textFormat: Text.PlainText
          text: Model.formatRate(card.rate)
          color: root.fg
          font.family: root.bar ? root.bar.fontFamily : Style.font.family
          font.pixelSize: Style.font.body
          font.bold: true
        }
      }

      Sparkline {
        width: parent.width
        height: Style.space(30)
        samples: card.samples
        capacity: root.panel.historyLength
        color: card.tint
      }
    }
  }

  component VolumeRow: Rectangle {
    id: row
    property var volume: ({})
    property bool hasCursor: false
    readonly property bool hovered: rowMouse.containsMouse || ejectButton.hovering
    readonly property var rate: root.panel.rates[volume.kname] || ({ read: 0, write: 0 })
    readonly property bool active: rate.read + rate.write > 1000
    readonly property color levelTint: root.panel.levelColor(volume.level)

    implicitHeight: rowContent.implicitHeight + Style.space(8) * 2
    radius: Style.cornerRadius
    color: hasCursor ? Style.hoverFillFor(root.fg, Color.accent) : "transparent"

    Behavior on color { ColorAnimation { duration: 80 } }

    MouseArea {
      id: rowMouse
      anchors.fill: parent
      hoverEnabled: true
      cursorShape: Qt.PointingHandCursor
      onClicked: root.panel.openPath(row.volume.mount)
    }

    Text {
      id: volumeGlyph
      anchors.left: parent.left
      anchors.leftMargin: Style.space(8)
      anchors.top: parent.top
      anchors.topMargin: Style.space(8)
      textFormat: Text.PlainText
      width: Style.space(26)
      horizontalAlignment: Text.AlignHCenter
      text: row.volume.glyph || ""
      color: row.volume.level === "critical" ? Color.urgent : root.fg
      font.family: root.bar ? root.bar.fontFamily : Style.font.family
      font.pixelSize: Style.font.display
    }

    Column {
      id: rowContent
      anchors.left: volumeGlyph.right
      anchors.leftMargin: Style.space(12)
      anchors.right: parent.right
      anchors.rightMargin: Style.space(8)
      anchors.top: parent.top
      anchors.topMargin: Style.space(8)
      spacing: Style.space(4)

      Item {
        width: parent.width
        implicitHeight: Math.max(nameText.implicitHeight, percent.implicitHeight, ejectButton.visible ? ejectButton.height : 0)

        Text {
          id: nameText
          anchors.left: parent.left
          anchors.right: percent.left
          anchors.rightMargin: Style.space(8)
          anchors.verticalCenter: parent.verticalCenter
          textFormat: Text.PlainText
          text: row.volume.name || ""
          color: root.fg
          font.family: root.bar ? root.bar.fontFamily : Style.font.family
          font.pixelSize: Style.font.body
          font.bold: true
          elide: Text.ElideRight
        }

        Text {
          id: percent
          anchors.right: ejectButton.visible ? ejectButton.left : parent.right
          anchors.rightMargin: ejectButton.visible ? Style.space(6) : 0
          anchors.verticalCenter: parent.verticalCenter
          textFormat: Text.PlainText
          text: Model.percentText(row.volume.fraction)
          color: row.volume.level === "normal" ? root.fg : row.levelTint
          font.family: root.bar ? root.bar.fontFamily : Style.font.family
          font.pixelSize: Style.font.body
          font.bold: true
        }

        PanelActionButton {
          id: ejectButton
          property bool hovering: false
          visible: row.volume.kind === "removable"
          anchors.right: parent.right
          anchors.verticalCenter: parent.verticalCenter
          size: Style.space(22)
          iconText: Model.GLYPH_EJECT
          fontSize: Style.font.bodySmall
          foreground: root.dim
          hoverColor: root.fg
          tooltipText: "Eject " + (row.volume.name || "")
          onClicked: root.panel.eject(row.volume)
          onHovered: function(h) { hovering = h }
        }
      }

      Body {
        width: parent.width
        text: Model.volumeMeta(row.volume)
        color: root.dim
        font.pixelSize: Style.font.caption
      }

      Item {
        width: parent.width
        height: Style.space(6)

        Rectangle {
          anchors.fill: parent
          radius: height / 2
          color: Util.alpha(root.fg, 0.1)
        }

        Rectangle {
          height: parent.height
          radius: height / 2
          width: Math.max(height, parent.width * Math.min(1, row.volume.fraction || 0))
          gradient: Gradient {
            orientation: Gradient.Horizontal
            GradientStop { position: 0; color: Qt.tint(row.levelTint, Util.alpha(root.fg, 0.25)) }
            GradientStop { position: 1; color: row.levelTint }
          }

          Behavior on width { NumberAnimation { duration: 320; easing.type: Easing.OutCubic } }
        }
      }

      Item {
        width: parent.width
        implicitHeight: usedText.implicitHeight

        Body {
          id: usedText
          anchors.left: parent.left
          text: Model.formatBytes(row.volume.used) + " used"
          color: root.dim
        }

        Body {
          anchors.horizontalCenter: parent.horizontalCenter
          visible: row.active
          text: Model.GLYPH_READ + " " + Model.formatRate(row.rate.read) + "  " + Model.GLYPH_WRITE + " " + Model.formatRate(row.rate.write)
          color: Color.accent
        }

        Body {
          anchors.right: parent.right
          text: Model.formatBytes(row.volume.avail) + " free"
          color: root.fg
        }
      }
    }
  }

  component FolderRow: Rectangle {
    id: folderRow
    property var folder: ({})
    property real share: 0
    property bool hasCursor: false
    readonly property bool hovered: folderMouse.containsMouse

    implicitHeight: Style.space(26)
    radius: Style.cornerRadius
    color: hasCursor ? Style.hoverFillFor(root.fg, Color.accent) : "transparent"

    Behavior on color { ColorAnimation { duration: 80 } }

    MouseArea {
      id: folderMouse
      anchors.fill: parent
      hoverEnabled: true
      cursorShape: Qt.PointingHandCursor
      onClicked: root.panel.openPath(folderRow.folder.path)
    }

    Text {
      id: folderGlyph
      anchors.left: parent.left
      anchors.leftMargin: Style.space(8)
      anchors.verticalCenter: parent.verticalCenter
      textFormat: Text.PlainText
      text: Model.GLYPH_FOLDER
      color: root.dim
      font.family: root.bar ? root.bar.fontFamily : Style.font.family
      font.pixelSize: Style.font.body
    }

    Body {
      id: folderName
      anchors.left: folderGlyph.right
      anchors.leftMargin: Style.space(10)
      anchors.verticalCenter: parent.verticalCenter
      width: Style.space(96)
      text: folderRow.folder.name || ""
    }

    Item {
      anchors.left: folderName.right
      anchors.leftMargin: Style.space(8)
      anchors.right: folderSize.left
      anchors.rightMargin: Style.space(10)
      anchors.verticalCenter: parent.verticalCenter
      height: Style.space(4)

      Rectangle {
        anchors.fill: parent
        radius: height / 2
        color: Util.alpha(root.fg, 0.08)
      }

      Rectangle {
        height: parent.height
        radius: height / 2
        width: Math.max(height, parent.width * folderRow.share)
        color: Util.alpha(Color.accent, 0.4 + 0.6 * folderRow.share)

        Behavior on width { NumberAnimation { duration: 320; easing.type: Easing.OutCubic } }
      }
    }

    Body {
      id: folderSize
      anchors.right: parent.right
      anchors.rightMargin: Style.space(8)
      anchors.verticalCenter: parent.verticalCenter
      width: Style.space(52)
      horizontalAlignment: Text.AlignRight
      text: Model.formatBytes(folderRow.folder.bytes)
      color: root.dim
    }
  }
}
