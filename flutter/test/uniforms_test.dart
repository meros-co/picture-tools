import 'dart:convert';
import 'dart:io';

import 'package:flutter_test/flutter_test.dart';
import 'package:meros_picture_tools/meros_picture_tools.dart';

/// The Dart packing against the Rust crate's (test vectors it generates).
void main() {
  final vectors = (jsonDecode(File('test/fixtures/uniform_vectors.json').readAsStringSync())['vectors'] as List)
      .cast<Map<String, dynamic>>();

  for (final v in vectors) {
    test('uniforms match the Rust crate: ${v['name']}', () {
      final params = PictureToolsParams.fromJson(v['params'] as Map<String, dynamic>);
      final size = (v['size'] as List).cast<num>();
      final floats = params.uniforms(
        width: size[0].toDouble(),
        height: size[1].toDouble(),
        lutSize: v['lut_size'] as int,
        timeS: (v['time'] as num).toDouble(),
      );
      final want = (v['floats'] as List).cast<num>();
      expect(floats.length, want.length);
      for (var i = 0; i < want.length; i++) {
        expect(floats[i], closeTo(want[i].toDouble(), 1e-6), reason: 'float $i');
      }
    });
  }

  test('the uniform count is the table\'s', () {
    final table = jsonDecode(File('../shaders/uniforms.json').readAsStringSync());
    expect(const PictureToolsParams().uniforms(width: 1, height: 1).length, table['floats']);
  });
}
