import 'dart:typed_data';
import 'dart:ui' as ui;

import 'package:flutter/scheduler.dart';
import 'package:flutter/widgets.dart';

import 'params.dart';

/// A LUT as the shader samples it: the N²×N RGBA atlas (blue slices side
/// by side) the Rust crate's `Lut3d::atlas_rgba8` makes.
class LutAtlas {
  LutAtlas._(this.image, this.size);

  final ui.Image image;

  /// N, for an N³ LUT.
  final int size;

  /// From the atlas' RGBA pixels (`width` = N², `height` = N).
  static Future<LutAtlas> fromRgba8(int width, int height, Uint8List rgba) async {
    final buffer = await ui.ImmutableBuffer.fromUint8List(rgba);
    final descriptor = ui.ImageDescriptor.raw(
      buffer,
      width: width,
      height: height,
      pixelFormat: ui.PixelFormat.rgba8888,
    );
    final codec = await descriptor.instantiateCodec();
    final frame = await codec.getNextFrame();
    codec.dispose();
    descriptor.dispose();
    buffer.dispose();
    return LutAtlas._(frame.image, height);
  }

  void dispose() => image.dispose();
}

/// The picture-tools shader, loaded once.
class PictureToolsProgram {
  PictureToolsProgram._();

  static Future<ui.FragmentProgram>? _program;

  /// The shader from this package's assets.
  static Future<ui.FragmentProgram> load() => _program ??=
      ui.FragmentProgram.fromAsset('packages/meros_picture_tools/shaders/picture_tools.frag');

  /// A 1×1 image for the LUT sampler while the LUT is off (every sampler
  /// must be bound; the shader doesn't read this one then).
  static Future<ui.Image> blank() => _blank ??= () async {
        final recorder = ui.PictureRecorder();
        ui.Canvas(recorder).drawRect(const Rect.fromLTWH(0, 0, 1, 1), ui.Paint()..color = const Color(0xFF000000));
        return recorder.endRecording().toImage(1, 1);
      }();
  static Future<ui.Image>? _blank;

  /// Sets every uniform after the size (which an image filter's engine
  /// sets) and the LUT sampler.
  static void configure(
    ui.FragmentShader shader,
    PictureToolsParams params, {
    required double width,
    required double height,
    LutAtlas? lut,
    required ui.Image blank,
    double timeS = 0,
    bool setSize = false,
  }) {
    final floats = params.uniforms(
      width: width,
      height: height,
      lutSize: lut?.size ?? 33,
      timeS: timeS,
    );
    for (var i = setSize ? 0 : 2; i < floats.length; i++) {
      shader.setFloat(i, floats[i]);
    }
    shader.setImageSampler(1, params.lut && lut != null ? lut.image : blank);
  }
}

/// Runs the picture tools over [child] (a video texture, a camera preview,
/// any widget) as an image filter.
///
/// Image-filter shaders need Flutter's Impeller renderer: the default on
/// Android, iOS and macOS; on Windows and Linux, the app enables Impeller.
/// Where it's not available ([ui.ImageFilter.isShaderFilterSupported]
/// false), the child is shown as it is.
class PictureToolsFilter extends StatefulWidget {
  const PictureToolsFilter({
    super.key,
    required this.params,
    this.lut,
    required this.child,
  });

  final PictureToolsParams params;
  final LutAtlas? lut;
  final Widget child;

  @override
  State<PictureToolsFilter> createState() => _PictureToolsFilterState();
}

class _PictureToolsFilterState extends State<PictureToolsFilter> with SingleTickerProviderStateMixin {
  ui.FragmentShader? _shader;
  ui.Image? _blank;
  Ticker? _ticker;
  double _time = 0;

  @override
  void initState() {
    super.initState();
    if (ui.ImageFilter.isShaderFilterSupported) {
      Future.wait([PictureToolsProgram.load(), PictureToolsProgram.blank()]).then((loaded) {
        if (!mounted) return;
        setState(() {
          _shader = (loaded[0] as ui.FragmentProgram).fragmentShader();
          _blank = loaded[1] as ui.Image;
        });
      });
    }
    _syncTicker();
  }

  @override
  void didUpdateWidget(PictureToolsFilter old) {
    super.didUpdateWidget(old);
    _syncTicker();
  }

  /// Animated zebras need a frame per tick; nothing else does.
  void _syncTicker() {
    if (widget.params.animates) {
      _ticker ??= createTicker((elapsed) => setState(() => _time = elapsed.inMicroseconds / 1e6))..start();
    } else {
      _ticker?.dispose();
      _ticker = null;
    }
  }

  @override
  void dispose() {
    _ticker?.dispose();
    _shader?.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final shader = _shader;
    final blank = _blank;
    if (shader == null || blank == null || widget.params.isIdentity) {
      return widget.child;
    }
    return LayoutBuilder(builder: (context, constraints) {
      PictureToolsProgram.configure(
        shader,
        widget.params,
        width: constraints.maxWidth,
        height: constraints.maxHeight,
        lut: widget.lut,
        blank: blank,
        timeS: _time,
      );
      return ImageFiltered(imageFilter: ui.ImageFilter.shader(shader), child: widget.child);
    });
  }
}
