import QtQuick

// Circular usage gauge: a faint full-circle track with an arc for the used
// fraction, starting at twelve o'clock.
Canvas {
  id: root

  property real fraction: 0
  property real lineWidth: 2
  property color trackColor: "#33ffffff"
  property color color: "white"
  property real animatedFraction: fraction

  implicitWidth: 16
  implicitHeight: 16
  antialiasing: true

  Behavior on animatedFraction {
    NumberAnimation { duration: 420; easing.type: Easing.OutCubic }
  }

  onAnimatedFractionChanged: requestPaint()
  onColorChanged: requestPaint()
  onTrackColorChanged: requestPaint()
  onLineWidthChanged: requestPaint()
  onWidthChanged: requestPaint()
  onHeightChanged: requestPaint()

  onPaint: {
    var ctx = getContext("2d")
    ctx.reset()

    var size = Math.min(width, height)
    var radius = (size - lineWidth) / 2
    var cx = width / 2
    var cy = height / 2
    var start = -Math.PI / 2
    var value = Math.max(0, Math.min(1, animatedFraction))

    ctx.lineWidth = lineWidth
    ctx.lineCap = "round"

    ctx.strokeStyle = trackColor
    ctx.beginPath()
    ctx.arc(cx, cy, radius, 0, Math.PI * 2, false)
    ctx.stroke()

    if (value <= 0) return
    ctx.strokeStyle = color
    ctx.beginPath()
    ctx.arc(cx, cy, radius, start, start + Math.PI * 2 * value, false)
    ctx.stroke()
  }
}
