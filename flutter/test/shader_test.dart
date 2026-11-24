import 'dart:convert';
import 'dart:io';
import 'dart:typed_data';
import 'dart:ui' as ui;

import 'package:flutter/painting.dart';
import 'package:flutter_test/flutter_test.dart';

Future<ui.Image> _image(int w, int h, Uint8List rgba) async {
  final buffer = await ui.ImmutableBuffer.fromUint8List(rgba);
  final d = ui.ImageDescriptor.raw(buffer, width: w, height: h, pixelFormat: ui.PixelFormat.rgba8888);
  final codec = await d.instantiateCodec();
  return (await codec.getNextFrame()).image;
}

/// The Flutter shader against the Rust CPU reference (`apply_cpu`), on
/// cases the Rust crate generates: within ±2 code values.
void main() {
  TestWidgetsFlutterBinding.ensureInitialized();
  final data = jsonDecode(File('test/fixtures/shader_cases.json').readAsStringSync()) as Map<String, dynamic>;
  final w = data['w'] as int, h = data['h'] as int;

  for (final c in (data['cases'] as List).cast<Map<String, dynamic>>()) {
    testWidgets('the shader matches the CPU reference: ${c['name']}', (tester) async {
      await tester.runAsync(() async {
        final program = await ui.FragmentProgram.fromAsset('shaders/picture_tools.frag');
        final shader = program.fragmentShader();
        final input = await _image(w, h, base64Decode(data['image'] as String));
        final lut = c['lut'] as Map<String, dynamic>?;
        final blank = await _image(1, 1, Uint8List.fromList([0, 0, 0, 255]));
        final floats = (c['floats'] as List).cast<num>();
        for (var i = 0; i < floats.length; i++) {
          shader.setFloat(i, floats[i].toDouble());
        }
        shader.setImageSampler(0, input, filterQuality: ui.FilterQuality.none);
        shader.setImageSampler(
          1,
          lut == null ? blank : await _image(lut['w'] as int, lut['h'] as int, base64Decode(lut['px'] as String)),
          filterQuality: ui.FilterQuality.low,
        );
        final recorder = ui.PictureRecorder();
        ui.Canvas(recorder).drawRect(Rect.fromLTWH(0, 0, w.toDouble(), h.toDouble()), ui.Paint()..shader = shader);
        final out = await recorder.endRecording().toImage(w, h);
        final bytes = (await out.toByteData(format: ui.ImageByteFormat.rawStraightRgba))!.buffer.asUint8List();
        final want = base64Decode(c['expected'] as String);
        var worst = 0, over = 0;
        for (var i = 0; i < want.length; i++) {
          if (i % 4 == 3) continue;
          final d = (bytes[i] - want[i]).abs();
          if (d > worst) worst = d;
          if (d > 2) over++;
        }
        expect(over, 0, reason: 'worst difference $worst');
      });
    });
  }
}
