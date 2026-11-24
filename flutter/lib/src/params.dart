import 'package:flutter/foundation.dart';

/// The input's code range.
enum PictureRange { full, limited }

/// The luma weights the tools measure with.
enum LumaWeights {
  bt709([0.2126, 0.7152, 0.0722]),
  bt601([0.299, 0.587, 0.114]),
  bt2020([0.2627, 0.6780, 0.0593]);

  const LumaWeights(this.weights);
  final List<double> weights;
}

/// How zebras are drawn.
@immutable
sealed class ZebraStyle {
  const ZebraStyle();
}

/// Still stripes mixed over the picture in black and white (60% for the
/// first zebra, 50% for the second, on the other diagonal).
class StaticZebras extends ZebraStyle {
  const StaticZebras({this.stripePx = 8});
  final double stripePx;
}

/// Dark gray stripes replacing the picture, moving along the diagonal.
class AnimatedZebras extends ZebraStyle {
  const AnimatedZebras({this.stripePx = 8, this.speedPxPerS = 40});
  final double stripePx;
  final double speedPxPerS;
}

/// A zebra on luma at or above [level], or within [band] of it. Levels are
/// full-range luma, 0–1.09.
@immutable
class Zebra {
  const Zebra(this.level, {this.band});
  final double level;
  final double? band;
}

/// What false color shows outside its bands.
enum FalseColorOutside { picture, grayscale }

/// Luma from [lo] (inclusive) to [hi] (exclusive) shown as [rgb]; the first
/// band that matches wins.
@immutable
class FalseColorBand {
  const FalseColorBand(this.lo, this.hi, this.rgb);
  final double lo;
  final double hi;
  final List<double> rgb;
}

/// A camera app's false color.
const falseColorCamera = [
  FalseColorBand(-1, 0.025, [0.5, 0.0, 0.6]),
  FalseColorBand(0.025, 0.10, [0.0, 0.2, 1.0]),
  FalseColorBand(0.38, 0.42, [0.0, 0.8, 0.2]),
  FalseColorBand(0.52, 0.56, [1.0, 0.45, 0.7]),
  FalseColorBand(0.99, 2, [1.0, 0.0, 0.0]),
  FalseColorBand(0.97, 0.99, [1.0, 0.9, 0.0]),
];

/// A production controller's false color.
const falseColorController = [
  FalseColorBand(-1, 0.025, [0.55, 0.0, 0.7]),
  FalseColorBand(0.025, 0.10, [0.1, 0.25, 1.0]),
  FalseColorBand(0.38, 0.42, [0.1, 0.8, 0.2]),
  FalseColorBand(0.52, 0.56, [1.0, 0.55, 0.75]),
  FalseColorBand(0.97, 0.99, [1.0, 0.95, 0.1]),
  FalseColorBand(0.99, 2, [1.0, 0.1, 0.1]),
];

@immutable
class FalseColor {
  const FalseColor(this.bands, {this.outside = FalseColorOutside.picture});
  final List<FalseColorBand> bands;
  final FalseColorOutside outside;
}

/// Focus peaking: where the luma Laplacian exceeds [threshold], [color].
@immutable
class Peaking {
  const Peaking(this.threshold, {this.color = const [1.0, 0.0, 0.0]});
  final double threshold;
  final List<double> color;
}

/// The most false-color bands and zebras the shader takes.
const maxBands = 8;
const maxZebras = 2;

/// Everything the tools do to a picture: the Rust crate's
/// `PictureToolsParams`.
@immutable
class PictureToolsParams {
  const PictureToolsParams({
    this.range = PictureRange.full,
    this.luma = LumaWeights.bt709,
    this.lut = false,
    this.falseColor,
    this.zebras = const [],
    this.zebraStyle = const StaticZebras(),
    this.peaking,
  });

  final PictureRange range;
  final LumaWeights luma;

  /// Apply the LUT (a [LutAtlas] must then be given to the filter).
  final bool lut;
  final FalseColor? falseColor;
  final List<Zebra> zebras;
  final ZebraStyle zebraStyle;
  final Peaking? peaking;

  /// From the Rust crate's JSON form (serde): `{"range": "limited",
  /// "luma": "bt601", "lut": true, "false_color": {"bands": [{"lo", "hi",
  /// "rgb"}], "outside": "grayscale"}, "zebras": [{"level", "band"}],
  /// "zebra_style": {"style": "animated", "stripe_px", "speed_px_per_s"},
  /// "peaking": {"threshold", "color"}}`; missing fields take defaults.
  factory PictureToolsParams.fromJson(Map<String, dynamic> j) {
    List<double> nums(Object? v) => [for (final x in (v as List? ?? const [])) (x as num).toDouble()];
    final fc = j['false_color'] as Map<String, dynamic>?;
    final style = j['zebra_style'] as Map<String, dynamic>?;
    final peak = j['peaking'] as Map<String, dynamic>?;
    return PictureToolsParams(
      range: j['range'] == 'limited' ? PictureRange.limited : PictureRange.full,
      luma: switch (j['luma']) {
        'bt601' => LumaWeights.bt601,
        'bt2020' => LumaWeights.bt2020,
        _ => LumaWeights.bt709,
      },
      lut: j['lut'] == true,
      falseColor: fc == null
          ? null
          : FalseColor(
              [
                for (final b in (fc['bands'] as List? ?? const []))
                  FalseColorBand((b['lo'] as num).toDouble(), (b['hi'] as num).toDouble(), nums(b['rgb'])),
              ],
              outside: fc['outside'] == 'grayscale' ? FalseColorOutside.grayscale : FalseColorOutside.picture,
            ),
      zebras: [
        for (final z in (j['zebras'] as List? ?? const []))
          Zebra((z['level'] as num).toDouble(), band: (z['band'] as num?)?.toDouble()),
      ],
      zebraStyle: style?['style'] == 'animated'
          ? AnimatedZebras(
              stripePx: (style?['stripe_px'] as num? ?? 8).toDouble(),
              speedPxPerS: (style?['speed_px_per_s'] as num? ?? 40).toDouble(),
            )
          : StaticZebras(stripePx: (style?['stripe_px'] as num? ?? 8).toDouble()),
      peaking: peak == null ? null : Peaking((peak['threshold'] as num).toDouble(), color: nums(peak['color'])),
    );
  }

  bool get isIdentity => !lut && falseColor == null && zebras.isEmpty && peaking == null;

  /// True when the output changes with time (animated zebras).
  bool get animates => zebras.isNotEmpty && zebraStyle is AnimatedZebras;

  /// The shader's floats in `shaders/uniforms.json` order (64 of them),
  /// for an output of [size] at [timeS] with an N³ LUT.
  List<double> uniforms({
    required double width,
    required double height,
    int lutSize = 33,
    double timeS = 0,
  }) {
    final out = <double>[];
    void pad(Iterable<double> v, int n) {
      final l = v.take(n).toList();
      out.addAll(l);
      for (var i = l.length; i < n; i++) {
        out.add(0);
      }
    }

    final fc = falseColor;
    final bands = fc?.bands.take(maxBands).toList() ?? const <FalseColorBand>[];
    final z = zebras.take(maxZebras).toList();
    final (stripe, speed, style) = switch (zebraStyle) {
      StaticZebras(:final stripePx) => (stripePx, 0.0, 0.0),
      AnimatedZebras(:final stripePx, :final speedPxPerS) => (stripePx, speedPxPerS, 1.0),
    };
    out
      ..addAll([width, height])
      ..add(timeS)
      ..add(range == PictureRange.limited ? 1 : 0)
      ..addAll(luma.weights)
      ..add(lut ? 1 : 0)
      ..add(lutSize < 2 ? 2 : lutSize.toDouble())
      ..add(fc != null ? 1 : 0)
      ..add(fc?.outside == FalseColorOutside.grayscale ? 1 : 0)
      ..add(bands.length.toDouble());
    pad(bands.map((b) => b.lo), maxBands);
    pad(bands.map((b) => b.hi), maxBands);
    pad(bands.expand((b) => b.rgb), maxBands * 3);
    out.add(z.length.toDouble());
    pad(z.map((e) => e.level), maxZebras);
    pad(z.map((e) => e.band ?? 0), maxZebras);
    out
      ..add(stripe < 1 ? 1 : stripe)
      ..add(speed)
      ..add(style)
      ..add(peaking == null || peaking!.threshold < 0 ? 0 : peaking!.threshold)
      ..addAll(peaking?.color ?? const [1.0, 0.0, 0.0]);
    return out;
  }
}
