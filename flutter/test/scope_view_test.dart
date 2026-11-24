import 'dart:math' as math;
import 'dart:typed_data';

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:meros_picture_tools/meros_picture_tools.dart';

/// A waveform of a luma ramp: a diagonal line.
ScopeFrame _waveform() {
  const w = 256, h = 256;
  final d = Uint8List(w * h);
  for (var x = 0; x < w; x++) {
    d[(255 - x) * w + x] = 255;
    if (x > 0) d[(256 - x) * w + x] = 120;
  }
  return ScopeFrame(ScopeKind.waveform, w, h, d);
}

/// A parade of three flat levels.
ScopeFrame _parade() {
  const w = 384, h = 256;
  final d = Uint8List(w * h);
  for (var x = 0; x < w; x++) {
    final level = [200, 120, 40][x ~/ 128];
    d[(255 - level) * w + x] = 230;
  }
  return ScopeFrame(ScopeKind.parade, w, h, d);
}

/// A vectorscope with a dot at each 75% bar.
ScopeFrame _vectorscope() {
  const s = 256;
  final d = Uint8List(s * s);
  for (final b in ScopePainter.bars) {
    final y = 0.2126 * b[0] + 0.7152 * b[1] + 0.0722 * b[2];
    final x = (128 + (b[2] - y) / 1.8556).round(), r = (128 - (b[0] - y) / 1.5748).round();
    for (var dy = -2; dy <= 2; dy++) {
      for (var dx = -2; dx <= 2; dx++) {
        d[(r + dy) * s + x + dx] = 255;
      }
    }
  }
  return ScopeFrame(ScopeKind.vectorscope, s, s, d);
}

/// A histogram: four bell curves.
ScopeFrame _histogram() {
  const bins = 256;
  final d = Uint8List(4 * bins);
  for (var r = 0; r < 4; r++) {
    for (var i = 0; i < bins; i++) {
      final c = 60.0 + r * 45;
      d[r * bins + i] = (255 * math.exp(-math.pow((i - c) / 25, 2))).round();
    }
  }
  return ScopeFrame(ScopeKind.histogram, bins, 4, d);
}

Widget _view(ScopeFrame f) => Directionality(
      textDirection: TextDirection.ltr,
      child: Center(
        child: RepaintBoundary(
          child: Container(
            color: const Color(0xFF101010),
            width: 256,
            height: 160,
            child: ScopeView(f),
          ),
        ),
      ),
    );

void main() {
  for (final (name, frame) in [
    ('waveform', _waveform()),
    ('parade', _parade()),
    ('vectorscope', _vectorscope()),
    ('histogram', _histogram()),
  ]) {
    testWidgets('a $name draws with its graticule', (tester) async {
      await tester.runAsync(() async {
        await tester.pumpWidget(_view(frame));
        // The trace image is made asynchronously.
        await Future<void>.delayed(const Duration(milliseconds: 200));
      });
      await tester.pump();
      await expectLater(find.byType(RepaintBoundary).first, matchesGoldenFile('goldens/scope_$name.png'));
    });
  }

  test('the parade is tinted per panel', () {
    final px = tracePixels(_parade(), const ScopeColors())!;
    int at(int x) => ((255 - [200, 120, 40][x ~/ 128]) * 384 + x) * 4;
    expect(px[at(10)] > px[at(10) + 1], isTrue, reason: 'red panel');
    expect(px[at(140) + 1] > px[at(140)], isTrue, reason: 'green panel');
    expect(px[at(300) + 2] > px[at(300)], isTrue, reason: 'blue panel');
    expect(tracePixels(_histogram(), const ScopeColors()), isNull);
  });
}
