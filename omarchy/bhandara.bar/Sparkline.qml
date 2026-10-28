import QtQuick

// Throughput history as a smooth line over a soft gradient. Samples scroll in
// from the right; the scale adapts to the largest visible sample.
Canvas {
  id: root

  property var samples: []
  property int capacity: 60
  property color color: "white"
  property real minimumScale: 1024 * 1024

  antialiasing: true

  onSamplesChanged: requestPaint()
  onColorChanged: requestPaint()
  onWidthChanged: requestPaint()
  onHeightChanged: requestPaint()

  onPaint: {
    var ctx = getContext("2d")
    ctx.reset()

    var count = samples ? samples.length : 0
    var top = 2
    var bottom = height - 1.5
    var step = width / Math.max(1, capacity - 1)

    ctx.strokeStyle = Qt.rgba(color.r, color.g, color.b, 0.18)
    ctx.lineWidth = 1
    ctx.beginPath()
    ctx.moveTo(0, bottom)
    ctx.lineTo(width, bottom)
    ctx.stroke()

    if (count < 2) return

    var max = minimumScale
    for (var i = 0; i < count; i++) max = Math.max(max, samples[i])
    max *= 1.15

    function px(index) { return width - (count - 1 - index) * step }
    function py(index) { return bottom - (bottom - top) * samples[index] / max }

    ctx.beginPath()
    ctx.moveTo(px(0), py(0))
    for (var j = 1; j < count; j++) {
      var midX = (px(j - 1) + px(j)) / 2
      ctx.bezierCurveTo(midX, py(j - 1), midX, py(j), px(j), py(j))
    }

    ctx.lineWidth = 1.6
    ctx.lineJoin = "round"
    ctx.strokeStyle = color
    ctx.stroke()

    ctx.lineTo(px(count - 1), height)
    ctx.lineTo(px(0), height)
    ctx.closePath()
    var gradient = ctx.createLinearGradient(0, top, 0, height)
    gradient.addColorStop(0, Qt.rgba(color.r, color.g, color.b, 0.32))
    gradient.addColorStop(1, Qt.rgba(color.r, color.g, color.b, 0.0))
    ctx.fillStyle = gradient
    ctx.fill()
  }
}
