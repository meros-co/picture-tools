import 'dart:math' as math;
import 'dart:typed_data';
import 'dart:ui' as ui;

import 'package:flutter/widgets.dart';

/// The scope kinds, as the Rust crate's `ScopeKind`.
enum ScopeKind { waveform, parade, vectorscope, histogram }

/// One scope from the Rust crate (`scopes::compute`, or `mpt_scope`): an
/// intensity (0–255) per cell, rows from the top. Waveform and parade are
/// columns × levels (the parade's three panels side by side), the
/// vectorscope a square with Cb across and Cr up, the histogram four rows
/// of bins (Y′, R, G, B).
@immutable
class ScopeFrame {
  const ScopeFrame(this.kind, this.width, this.height, this.data);

  final ScopeKind kind;
  final int width;
  final int height;
  final Uint8List data;
}

/// The colors a scope is drawn in.
@immutable
class ScopeColors {
  const ScopeColors({
    this.trace = const Color(0xFFE8E8E8),
    this.red = const Color(0xFFFF5252),
    this.green = const Color(0xFF69F0AE),
    this.blue = const Color(0xFF448AFF),
    this.graticule = const Color(0x33FFFFFF),
    this.skin = const Color(0xB3FFAB91),
  });

  final Color trace;
  final Color red;
  final Color green;
  final Color blue;
  final Color graticule;
  final Color skin;
}

/// Draws a [ScopeFrame] with its graticule: IRE lines at 0, 25, 50, 75 and
/// 100% on the waveform and parade (with the panels marked), the 75% bar
/// targets, the skin-tone line and the edge on the vectorscope, and the
/// histogram's four channels overlaid.
class ScopeView extends StatefulWidget {
  const ScopeView(this.frame, {super.key, this.colors = const ScopeColors(), this.vectorscopeGain = 1.0});

  final ScopeFrame frame;
  final ScopeColors colors;

  /// The gain the vectorscope was computed with (its `Shape`), for the
  /// targets.
  final double vectorscopeGain;

  @override
  State<ScopeView> createState() => _ScopeViewState();
}

class _ScopeViewState extends State<ScopeView> {
  ui.Image? _trace;
  ScopeFrame? _traceOf;
  bool _making = false;
  bool _again = false;

  @override
  void initState() {
    super.initState();
    _make();
  }

  @override
  void didUpdateWidget(ScopeView old) {
    super.didUpdateWidget(old);
    if (!identical(old.frame, widget.frame) || old.colors != widget.colors) _make();
  }

  /// One image at a time: a frame that comes while one is made waits, and
  /// only the newest is made next.
  void _make() {
    if (_making) {
      _again = true;
      return;
    }
    final f = widget.frame;
    final px = tracePixels(f, widget.colors);
    if (px == null) {
      setState(() => _traceOf = f);
      return;
    }
    _making = true;
    ui.decodeImageFromPixels(px, f.width, f.height, ui.PixelFormat.rgba8888, (image) {
      _making = false;
      if (!mounted) {
        image.dispose();
        return;
      }
      setState(() {
        _trace?.dispose();
        _trace = image;
        _traceOf = f;
      });
      if (_again) {
        _again = false;
        _make();
      }
    });
  }

  @override
  void dispose() {
    _trace?.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) => CustomPaint(
        painter: ScopePainter(
          widget.frame,
          trace: _traceOf?.kind == widget.frame.kind ? _trace : null,
          colors: widget.colors,
          vectorscopeGain: widget.vectorscopeGain,
        ),
        size: Size.infinite,
      );
}

/// The trace as premultiplied RGBA at the frame's size, tinted per panel
/// (the parade's red, green and blue); null for the histogram, which is
/// drawn as curves.
Uint8List? tracePixels(ScopeFrame f, ScopeColors colors) {
  if (f.kind == ScopeKind.histogram || f.width <= 0 || f.height <= 0) return null;
  final px = Uint8List(f.width * f.height * 4);
  final panels = f.kind == ScopeKind.parade ? 3 : 1;
  final panelWidth = f.width ~/ panels;
  final tints = f.kind == ScopeKind.parade ? [colors.red, colors.green, colors.blue] : [colors.trace];
  for (var y = 0; y < f.height; y++) {
    for (var x = 0; x < f.width; x++) {
      final i = y * f.width + x;
      final v = i < f.data.length ? f.data[i] : 0;
      if (v == 0) continue;
      final c = tints[math.min(x ~/ math.max(panelWidth, 1), tints.length - 1)];
      final o = i * 4;
      px[o] = (c.r * 255).round() * v ~/ 255;
      px[o + 1] = (c.g * 255).round() * v ~/ 255;
      px[o + 2] = (c.b * 255).round() * v ~/ 255;
      px[o + 3] = v;
    }
  }
  return px;
}

/// Paints a scope; see [ScopeView].
class ScopePainter extends CustomPainter {
  ScopePainter(this.frame, {this.trace, this.colors = const ScopeColors(), this.vectorscopeGain = 1.0});

  final ScopeFrame frame;
  final ui.Image? trace;
  final ScopeColors colors;
  final double vectorscopeGain;

  /// The 75% bars (BT.709, full range) as (R, G, B), for the targets.
  static const bars = [
    [191, 191, 0],
    [0, 191, 191],
    [0, 191, 0],
    [191, 0, 191],
    [191, 0, 0],
    [0, 0, 191],
  ];

  @override
  void paint(Canvas canvas, Size size) {
    final line = Paint()
      ..color = colors.graticule
      ..strokeWidth = 1;
    switch (frame.kind) {
      case ScopeKind.waveform || ScopeKind.parade:
        for (final f in [0.0, 0.25, 0.5, 0.75, 1.0]) {
          final y = (1 - f) * (size.height - 1) + 0.5;
          canvas.drawLine(Offset(0, y), Offset(size.width, y), line);
        }
        if (frame.kind == ScopeKind.parade) {
          for (final f in [1 / 3, 2 / 3]) {
            canvas.drawLine(Offset(size.width * f, 0), Offset(size.width * f, size.height), line);
          }
        }
        _drawTrace(canvas, Offset.zero & size);
      case ScopeKind.vectorscope:
        final side = math.min(size.width, size.height);
        final origin = Offset((size.width - side) / 2, (size.height - side) / 2);
        final center = origin + Offset(side / 2, side / 2);
        canvas.drawCircle(center, side / 2, line..style = PaintingStyle.stroke);
        canvas.drawLine(Offset(center.dx, origin.dy), Offset(center.dx, origin.dy + side), line);
        canvas.drawLine(Offset(origin.dx, center.dy), Offset(origin.dx + side, center.dy), line);
        // The skin-tone line, 123° from +Cb (Cr up).
        const skin = 123 * math.pi / 180;
        canvas.drawLine(
          center,
          center + Offset(math.cos(skin), -math.sin(skin)) * (side / 2),
          Paint()
            ..color = colors.skin
            ..strokeWidth = 1,
        );
        // Each 75% bar's target: Cb, Cr of the bar at the scope's gain.
        final box = side / 40;
        for (final b in bars) {
          final yy = 0.2126 * b[0] + 0.7152 * b[1] + 0.0722 * b[2];
          final cb = (b[2] - yy) / 1.8556 * vectorscopeGain / 256;
          final cr = (b[0] - yy) / 1.5748 * vectorscopeGain / 256;
          canvas.drawRect(
            Rect.fromCenter(center: center + Offset(cb * side, -cr * side), width: box, height: box),
            line..style = PaintingStyle.stroke,
          );
        }
        _drawTrace(canvas, origin & Size(side, side));
      case ScopeKind.histogram:
        final rows = [colors.trace, colors.red, colors.green, colors.blue];
        final bins = frame.width;
        if (bins < 2) return;
        for (var r = 3; r >= 0; r--) {
          final path = Path()..moveTo(0, size.height);
          for (var i = 0; i < bins; i++) {
            final k = r * bins + i;
            final v = k < frame.data.length ? frame.data[k] : 0;
            path.lineTo(i * size.width / (bins - 1), size.height - v / 255 * size.height);
          }
          path.lineTo(size.width, size.height);
          canvas.drawPath(path, Paint()..color = rows[r].withValues(alpha: r == 0 ? 0.6 : 0.35));
        }
    }
  }

  void _drawTrace(Canvas canvas, Rect into) {
    final t = trace;
    if (t == null) return;
    canvas.drawImageRect(
      t,
      Offset.zero & Size(t.width.toDouble(), t.height.toDouble()),
      into,
      Paint()..filterQuality = FilterQuality.medium,
    );
  }

  @override
  bool shouldRepaint(ScopePainter old) => old.frame != frame || old.trace != trace || old.colors != colors;
}
