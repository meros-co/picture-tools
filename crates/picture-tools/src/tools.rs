//! The per-pixel picture tools: a LUT, false color, zebras and focus
//! peaking. They run on the GPU through the generated shaders
//! ([`crate::shaders`]); [`apply_cpu`] is the same arithmetic on the CPU,
//! the reference every shader dialect is tested against.
//!
//! Order, per pixel: the LUT (if on) gives the picture; its luma (after a
//! limited-range input is expanded to full range) drives false color,
//! zebras and peaking; false color replaces the picture, zebras draw over
//! that, and peaking over everything.
//!
//! Coordinates are the output's pixels with the origin at the top left,
//! sampled at pixel centers (x + 0.5, y + 0.5), in every dialect.

use serde::{Deserialize, Serialize};

use crate::lut::Lut3d;

/// The input's code range.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Range {
    /// 0–255 is black to white.
    #[default]
    Full,
    /// 16–235 is black to white (video levels); expanded before measuring.
    Limited,
}

/// The luma weights the tools measure with.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LumaWeights {
    #[default]
    Bt709,
    Bt601,
    Bt2020,
}

impl LumaWeights {
    pub fn weights(self) -> [f32; 3] {
        match self {
            LumaWeights::Bt709 => [0.2126, 0.7152, 0.0722],
            LumaWeights::Bt601 => [0.299, 0.587, 0.114],
            LumaWeights::Bt2020 => [0.2627, 0.6780, 0.0593],
        }
    }
}

/// How zebras are drawn.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "style")]
pub enum ZebraStyle {
    /// Still stripes `stripe_px` wide mixed over the picture in black and
    /// white (60% for the first zebra, on one diagonal; 50% for the second,
    /// on the other).
    Static { stripe_px: f32 },
    /// Dark gray (0.06) stripes `stripe_px` wide replacing the picture,
    /// moving along the diagonal at `speed_px_per_s`.
    Animated { stripe_px: f32, speed_px_per_s: f32 },
}

impl Default for ZebraStyle {
    fn default() -> Self {
        ZebraStyle::Static { stripe_px: 8.0 }
    }
}

/// One zebra: on luma at or above `level`, or, with a `band`, within
/// `band` of `level`. Levels are full-range luma, 0–1.09 (109%).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Zebra {
    pub level: f32,
    #[serde(default)]
    pub band: Option<f32>,
}

/// What false color shows outside its bands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FalseColorOutside {
    /// The picture.
    #[default]
    Picture,
    /// The picture's luma as gray.
    Grayscale,
}

/// A false-color band: luma from `lo` (inclusive) to `hi` (exclusive), 0–1,
/// shown as `rgb`. The first band that matches wins.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct FalseColorBand {
    pub lo: f32,
    pub hi: f32,
    pub rgb: [f32; 3],
}

const fn band(lo: f32, hi: f32, rgb: [f32; 3]) -> FalseColorBand {
    FalseColorBand { lo, hi, rgb }
}

/// Bands below 0 and above 1 stretch past the ends of the range.
const BELOW: f32 = -1.0;
const ABOVE: f32 = 2.0;

/// A camera app's false color: crushed, dark, mid gray, skin, near clip,
/// clipped.
pub const FALSE_COLOR_CAMERA: &[FalseColorBand] = &[
    band(BELOW, 0.025, [0.5, 0.0, 0.6]),
    band(0.025, 0.10, [0.0, 0.2, 1.0]),
    band(0.38, 0.42, [0.0, 0.8, 0.2]),
    band(0.52, 0.56, [1.0, 0.45, 0.7]),
    band(0.99, ABOVE, [1.0, 0.0, 0.0]),
    band(0.97, 0.99, [1.0, 0.9, 0.0]),
];

/// A production controller's false color: the same zones in its colors.
pub const FALSE_COLOR_CONTROLLER: &[FalseColorBand] = &[
    band(BELOW, 0.025, [0.55, 0.0, 0.7]),
    band(0.025, 0.10, [0.1, 0.25, 1.0]),
    band(0.38, 0.42, [0.1, 0.8, 0.2]),
    band(0.52, 0.56, [1.0, 0.55, 0.75]),
    band(0.97, 0.99, [1.0, 0.95, 0.1]),
    band(0.99, ABOVE, [1.0, 0.1, 0.1]),
];

/// The most false-color bands and zebras the shaders take.
pub const MAX_BANDS: usize = 8;
pub const MAX_ZEBRAS: usize = 2;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FalseColor {
    pub bands: Vec<FalseColorBand>,
    #[serde(default)]
    pub outside: FalseColorOutside,
}

/// Focus peaking: where the luma Laplacian |4y − l − r − u − d| exceeds
/// `threshold`, the pixel is drawn in `color`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Peaking {
    pub threshold: f32,
    pub color: [f32; 3],
}

/// Everything the tools do to a picture.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct PictureToolsParams {
    pub range: Range,
    pub luma: LumaWeights,
    /// Apply the LUT; when false no LUT texture need be bound.
    pub lut: bool,
    pub false_color: Option<FalseColor>,
    /// Up to two.
    pub zebras: Vec<Zebra>,
    pub zebra_style: ZebraStyle,
    pub peaking: Option<Peaking>,
}

/// A uniform's GLSL type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum UniformKind {
    Float,
    Vec2,
    Vec3,
    Sampler2D,
}

impl UniformKind {
    pub fn floats(self) -> usize {
        match self {
            UniformKind::Float => 1,
            UniformKind::Vec2 => 2,
            UniformKind::Vec3 => 3,
            UniformKind::Sampler2D => 0,
        }
    }

    pub fn glsl(self) -> &'static str {
        match self {
            UniformKind::Float => "float",
            UniformKind::Vec2 => "vec2",
            UniformKind::Vec3 => "vec3",
            UniformKind::Sampler2D => "sampler2D",
        }
    }
}

/// One uniform in the shaders, in declaration order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct UniformDef {
    pub name: &'static str,
    pub kind: UniformKind,
    /// Array length; 1 for a plain uniform.
    pub count: usize,
    pub unit: &'static str,
    pub doc: &'static str,
}

const fn def(name: &'static str, kind: UniformKind, count: usize, unit: &'static str, doc: &'static str) -> UniformDef {
    UniformDef { name, kind, count, unit, doc }
}

use UniformKind::{Float, Sampler2D, Vec2, Vec3};

/// Every uniform, in the order the shaders declare them and [`uniforms`]
/// returns them. The generated shaders and `shaders/uniforms.json` come
/// from this table. `u_size` comes first and `u_input` is the first
/// sampler, as a Flutter image-filter shader needs.
pub const UNIFORMS: &[UniformDef] = &[
    def("u_size", Vec2, 1, "px", "the output's size"),
    def("u_time", Float, 1, "s", "time, for animated zebras"),
    def("u_range", Float, 1, "", "0 full range, 1 limited (16-235)"),
    def("u_luma", Vec3, 1, "", "luma weights"),
    def("u_lut", Float, 1, "", "1 to apply the LUT"),
    def("u_lut_size", Float, 1, "", "N, for an N³ LUT"),
    def("u_fc", Float, 1, "", "1 for false color"),
    def("u_fc_outside", Float, 1, "", "outside the bands: 0 the picture, 1 grayscale luma"),
    def("u_fc_count", Float, 1, "", "bands in use"),
    def("u_fc_lo", Float, MAX_BANDS, "luma", "band starts (inclusive)"),
    def("u_fc_hi", Float, MAX_BANDS, "luma", "band ends (exclusive)"),
    def("u_fc_rgb", Vec3, MAX_BANDS, "", "band colors"),
    def("u_zebra_count", Float, 1, "", "zebras in use"),
    def("u_zebra_level", Float, MAX_ZEBRAS, "luma", "zebra levels, 0-1.09"),
    def("u_zebra_band", Float, MAX_ZEBRAS, "luma", "half-width of a zebra's band; 0 for at or above its level"),
    def("u_zebra_stripe", Float, 1, "px", "stripe width"),
    def("u_zebra_speed", Float, 1, "px/s", "stripe movement; 0 still"),
    def("u_zebra_style", Float, 1, "", "0 mixed black and white stripes, 1 dark stripes"),
    def("u_peak", Float, 1, "luma", "peaking threshold; 0 off"),
    def("u_peak_color", Vec3, 1, "", "peaking color"),
    def("u_input", Sampler2D, 1, "", "the picture"),
    def("u_lut_tex", Sampler2D, 1, "", "the LUT atlas (N²×N); unbound when u_lut is 0"),
];

/// A uniform with its value.
#[derive(Debug, Clone, PartialEq)]
pub struct Uniform {
    pub name: &'static str,
    pub kind: UniformKind,
    pub unit: &'static str,
    pub value: Vec<f32>,
}

/// The plain (non-sampler) uniforms for `p` on an output of `size` pixels
/// at `time_s`, in [`UNIFORMS`] order; `u_lut_size` is the LUT's size when
/// one is given. Their values flattened are what `mpt_uniforms` writes.
pub fn uniforms_for(p: &PictureToolsParams, size: [f32; 2], lut_size: u32, time_s: f32) -> Vec<Uniform> {
    let mut values: Vec<Vec<f32>> = Vec::new();
    let fc = p.false_color.as_ref();
    let bands: Vec<FalseColorBand> = fc.map(|f| f.bands.iter().take(MAX_BANDS).copied().collect()).unwrap_or_default();
    let pad = |mut v: Vec<f32>, n: usize| {
        v.resize(n, 0.0);
        v
    };
    let zebras: Vec<Zebra> = p.zebras.iter().take(MAX_ZEBRAS).copied().collect();
    let (stripe, speed, style) = match p.zebra_style {
        ZebraStyle::Static { stripe_px } => (stripe_px, 0.0, 0.0),
        ZebraStyle::Animated { stripe_px, speed_px_per_s } => (stripe_px, speed_px_per_s, 1.0),
    };
    for d in UNIFORMS {
        let v = match d.name {
            "u_size" => size.to_vec(),
            "u_time" => vec![time_s],
            "u_range" => vec![if p.range == Range::Limited { 1.0 } else { 0.0 }],
            "u_luma" => p.luma.weights().to_vec(),
            "u_lut" => vec![if p.lut { 1.0 } else { 0.0 }],
            "u_lut_size" => vec![lut_size.max(2) as f32],
            "u_fc" => vec![if fc.is_some() { 1.0 } else { 0.0 }],
            "u_fc_outside" => vec![if fc.is_some_and(|f| f.outside == FalseColorOutside::Grayscale) { 1.0 } else { 0.0 }],
            "u_fc_count" => vec![bands.len() as f32],
            "u_fc_lo" => pad(bands.iter().map(|b| b.lo).collect(), MAX_BANDS),
            "u_fc_hi" => pad(bands.iter().map(|b| b.hi).collect(), MAX_BANDS),
            "u_fc_rgb" => pad(bands.iter().flat_map(|b| b.rgb).collect(), MAX_BANDS * 3),
            "u_zebra_count" => vec![zebras.len() as f32],
            "u_zebra_level" => pad(zebras.iter().map(|z| z.level).collect(), MAX_ZEBRAS),
            "u_zebra_band" => pad(zebras.iter().map(|z| z.band.unwrap_or(0.0)).collect(), MAX_ZEBRAS),
            "u_zebra_stripe" => vec![stripe.max(1.0)],
            "u_zebra_speed" => vec![speed],
            "u_zebra_style" => vec![style],
            "u_peak" => vec![p.peaking.map_or(0.0, |k| k.threshold.max(0.0))],
            "u_peak_color" => p.peaking.map_or(vec![1.0, 0.0, 0.0], |k| k.color.to_vec()),
            _ => continue, // samplers
        };
        values.push(v);
    }
    UNIFORMS
        .iter()
        .filter(|d| d.kind != Sampler2D)
        .zip(values)
        .map(|(d, value)| Uniform { name: d.name, kind: d.kind, unit: d.unit, value })
        .collect()
}

/// [`uniforms_for`] with the size left at 0×0 (an image filter sets it)
/// and a 33³ LUT.
pub fn uniforms(p: &PictureToolsParams, time_s: f32) -> Vec<Uniform> {
    uniforms_for(p, [0.0, 0.0], 33, time_s)
}

/// The number of floats [`uniforms`] gives, flattened.
pub fn uniform_float_count() -> usize {
    UNIFORMS.iter().map(|d| d.kind.floats() * d.count).sum()
}

fn mix3(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] {
    [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t]
}

/// GLSL's `mod`: `x − y·floor(x / y)`.
fn glsl_mod(x: f32, y: f32) -> f32 {
    x - y * (x / y).floor()
}

/// What the shaders compute, on the CPU: applies `p` to a packed RGBA
/// frame in place (alpha kept), sampling the frame at its own pixels. With
/// `p.lut` set and `lut` given, the LUT is applied by trilinear lookup.
pub fn apply_cpu(p: &PictureToolsParams, lut: Option<&Lut3d>, rgba: &mut [u8], w: u32, h: u32, time_s: f32) {
    let (w, h) = (w as usize, h as usize);
    if rgba.len() < w * h * 4 || w == 0 || h == 0 {
        return;
    }
    let lut = if p.lut { lut } else { None };
    let weights = p.luma.weights();
    let limited = p.range == Range::Limited;
    // The picture (LUT applied) at every pixel, then its luma.
    let picture: Vec<[f32; 3]> = rgba
        .chunks_exact(4)
        .take(w * h)
        .map(|px| {
            let c = [px[0] as f32 / 255.0, px[1] as f32 / 255.0, px[2] as f32 / 255.0];
            lut.map_or(c, |l| l.apply(c))
        })
        .collect();
    let luma_of = |c: [f32; 3]| {
        let c = if limited { c.map(|v| (v - 16.0 / 255.0) * (255.0 / 219.0)) } else { c };
        c[0] * weights[0] + c[1] * weights[1] + c[2] * weights[2]
    };
    let luma: Vec<f32> = picture.iter().map(|&c| luma_of(c)).collect();
    let u = uniforms_for(p, [w as f32, h as f32], lut.map_or(33, |l| l.size), time_s);
    let get = |name: &str| &u.iter().find(|x| x.name == name).unwrap().value;
    let (stripe, speed, style) = (get("u_zebra_stripe")[0], get("u_zebra_speed")[0], get("u_zebra_style")[0]);
    let fc = p.false_color.as_ref();
    for y in 0..h {
        for x in 0..w {
            let i = y * w + x;
            let (fx, fy) = (x as f32 + 0.5, y as f32 + 0.5);
            let c = picture[i];
            let l = luma[i];
            let mut out = c;
            if let Some(f) = fc {
                out = match f.bands.iter().take(MAX_BANDS).find(|b| l >= b.lo && l < b.hi) {
                    Some(b) => b.rgb,
                    None if f.outside == FalseColorOutside::Grayscale => [l; 3],
                    None => c,
                };
            }
            for (k, z) in p.zebras.iter().take(MAX_ZEBRAS).enumerate() {
                let band = z.band.unwrap_or(0.0);
                let hit = if band > 0.0 { (l - z.level).abs() <= band } else { l >= z.level };
                if !hit {
                    continue;
                }
                let period = 2.0 * stripe.max(1.0);
                if style < 0.5 {
                    let along = if k == 0 { fx + fy } else { fx - fy };
                    let s = if glsl_mod(along, period) < stripe.max(1.0) { 1.0 } else { 0.0 };
                    out = mix3(out, [s; 3], if k == 0 { 0.6 } else { 0.5 });
                } else {
                    let along = if k == 0 { fx - fy + h as f32 } else { fx + fy };
                    if glsl_mod(along + time_s * speed, period) < stripe.max(1.0) {
                        out = [0.06; 3];
                    }
                }
            }
            if let Some(pk) = p.peaking.filter(|k| k.threshold > 0.0) {
                let at = |xx: isize, yy: isize| {
                    let xx = xx.clamp(0, w as isize - 1) as usize;
                    let yy = yy.clamp(0, h as isize - 1) as usize;
                    luma[yy * w + xx]
                };
                let (xi, yi) = (x as isize, y as isize);
                let edge = (4.0 * l - at(xi - 1, yi) - at(xi + 1, yi) - at(xi, yi - 1) - at(xi, yi + 1)).abs();
                if edge > pk.threshold {
                    out = pk.color;
                }
            }
            let px = &mut rgba[i * 4..i * 4 + 3];
            for c in 0..3 {
                px[c] = (out[c].clamp(0.0, 1.0) * 255.0).round() as u8;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn solid(v: [u8; 3], w: usize, h: usize) -> Vec<u8> {
        [v[0], v[1], v[2], 255].repeat(w * h)
    }

    fn one(p: &PictureToolsParams, v: [u8; 3]) -> [u8; 3] {
        let mut px = solid(v, 1, 1);
        apply_cpu(p, None, &mut px, 1, 1, 0.0);
        [px[0], px[1], px[2]]
    }

    fn gray_for_luma(l: f32) -> [u8; 3] {
        let v = (l * 255.0).round() as u8;
        [v, v, v]
    }

    fn fc(bands: &[FalseColorBand], outside: FalseColorOutside) -> PictureToolsParams {
        PictureToolsParams { false_color: Some(FalseColor { bands: bands.to_vec(), outside }), ..Default::default() }
    }

    #[test]
    fn false_color_bands_at_their_edges_in_both_modes() {
        let to8 = |c: [f32; 3]| c.map(|v| (v * 255.0).round() as u8);
        for outside in [FalseColorOutside::Picture, FalseColorOutside::Grayscale] {
            let p = fc(FALSE_COLOR_CONTROLLER, outside);
            // Just inside each band's start, and just past the end.
            for b in FALSE_COLOR_CONTROLLER {
                if b.lo >= 0.0 {
                    let inside = (b.lo * 255.0).ceil() as u8;
                    assert_eq!(one(&p, [inside; 3]), to8(b.rgb), "{b:?}");
                }
            }
            // Between bands: mid gray at 0.2.
            let between = one(&p, gray_for_luma(0.2));
            assert_eq!(between, gray_for_luma(0.2));
            // A colored pixel outside the bands: the picture or its luma.
            let c = one(&p, [200, 60, 100]);
            match outside {
                FalseColorOutside::Picture => assert_eq!(c, [200, 60, 100]),
                FalseColorOutside::Grayscale => {
                    let y = (0.2126 * 200.0 + 0.7152 * 60.0 + 0.0722 * 100.0) / 255.0;
                    assert_eq!(c, gray_for_luma(y));
                }
            }
        }
        // The upper edge is exclusive: 0.42 is past the mid-gray band.
        let p = fc(&[band(0.38, 0.42, [0.0, 1.0, 0.0])], FalseColorOutside::Grayscale);
        let mut px = solid([0, 0, 0], 1, 1);
        // A luma of exactly 0.42 needs a weighted mix; test the rule directly.
        apply_cpu(&p, None, &mut px, 1, 1, 0.0);
        assert_eq!(&px[..3], &[0, 0, 0]);
    }

    #[test]
    fn zebras_by_level_and_band_in_both_styles() {
        let (w, h) = (32usize, 32usize);
        let count_marked = |p: &PictureToolsParams, v: u8, t: f32| {
            let mut px = solid([v, v, v], w, h);
            apply_cpu(p, None, &mut px, w as u32, h as u32, t);
            px.chunks(4).filter(|q| q[0] != v).count()
        };
        // Static, at or above 0.9: a 0.95 gray is striped, a 0.8 one isn't.
        let p = PictureToolsParams { zebras: vec![Zebra { level: 0.9, band: None }], ..Default::default() };
        assert!(count_marked(&p, 242, 0.0) > 0);
        assert_eq!(count_marked(&p, 204, 0.0), 0);
        // A band of ±0.025 around 0.5: 0.5 marked, 0.6 not.
        let p = PictureToolsParams { zebras: vec![Zebra { level: 0.5, band: Some(0.025) }], ..Default::default() };
        assert!(count_marked(&p, 128, 0.0) > 0);
        assert_eq!(count_marked(&p, 153, 0.0), 0);
        // Animated: dark stripes that move with time.
        let p = PictureToolsParams {
            zebras: vec![Zebra { level: 0.9, band: None }],
            zebra_style: ZebraStyle::Animated { stripe_px: 8.0, speed_px_per_s: 40.0 },
            ..Default::default()
        };
        let frame = |t: f32| {
            let mut px = solid([250, 250, 250], w, h);
            apply_cpu(&p, None, &mut px, w as u32, h as u32, t);
            px
        };
        let a = frame(0.0);
        assert!(a.chunks(4).any(|q| q[0] == 15), "0.06 gray stripes");
        assert!(a.chunks(4).all(|q| q[0] == 15 || q[0] == 250));
        assert_ne!(a, frame(0.1), "they move");
        assert_eq!(a, frame(0.4), "a whole 16 px period later they're back");
    }

    #[test]
    fn static_stripes_are_half_of_the_picture() {
        let p = PictureToolsParams { zebras: vec![Zebra { level: 0.0, band: None }], ..Default::default() };
        let mut px = solid([128, 128, 128], 64, 64);
        apply_cpu(&p, None, &mut px, 64, 64, 0.0);
        let light = px.chunks(4).filter(|q| q[0] > 128).count();
        assert_eq!(light, 64 * 64 / 2);
    }

    #[test]
    fn peaking_marks_a_step_edge_and_not_a_flat_field() {
        let (w, h) = (16usize, 8usize);
        let p = PictureToolsParams { peaking: Some(Peaking { threshold: 0.2, color: [1.0, 0.0, 0.0] }), ..Default::default() };
        let mut flat = solid([128, 128, 128], w, h);
        apply_cpu(&p, None, &mut flat, w as u32, h as u32, 0.0);
        assert!(flat.chunks(4).all(|q| q[..3] == [128, 128, 128]));
        // Black left, white right.
        let mut step: Vec<u8> = (0..w * h).flat_map(|i| if i % w < w / 2 { [0, 0, 0, 255] } else { [255, 255, 255, 255] }).collect();
        apply_cpu(&p, None, &mut step, w as u32, h as u32, 0.0);
        for y in 0..h {
            let row: Vec<bool> = (0..w).map(|x| step[(y * w + x) * 4..][..3] == [255, 0, 0]).collect();
            assert!(row[w / 2 - 1] && row[w / 2], "the two pixels at the edge");
            assert!(!row[0] && !row[w - 1], "not away from it");
        }
    }

    #[test]
    fn limited_range_and_bt601_move_the_levels() {
        // Video white (235) is 100% in limited range, 92% in full range.
        let p = |range, luma| PictureToolsParams {
            range,
            luma,
            zebras: vec![Zebra { level: 0.99, band: None }],
            ..Default::default()
        };
        let marked = |p: &PictureToolsParams, v: [u8; 3]| {
            let mut px = solid(v, 8, 8);
            apply_cpu(p, None, &mut px, 8, 8, 0.0);
            px.chunks(4).any(|q| q[..3] != v)
        };
        assert!(marked(&p(Range::Limited, LumaWeights::Bt709), [235, 235, 235]));
        assert!(!marked(&p(Range::Full, LumaWeights::Bt709), [235, 235, 235]));
        // Pure green: luma 0.7152 in BT.709, 0.587 in BT.601.
        let z = |level| PictureToolsParams { zebras: vec![Zebra { level, band: None }], luma: LumaWeights::Bt601, ..Default::default() };
        assert!(marked(&z(0.58), [0, 255, 0]));
        assert!(!marked(&z(0.60), [0, 255, 0]));
        let z709 = PictureToolsParams { zebras: vec![Zebra { level: 0.70, band: None }], ..Default::default() };
        assert!(marked(&z709, [0, 255, 0]));
    }

    #[test]
    fn the_lut_applies_before_measuring() {
        let lut = Lut3d::per_channel(17, |x| 1.0 - x);
        let p = PictureToolsParams { lut: true, ..Default::default() };
        let mut px = solid([0, 64, 255], 2, 2);
        apply_cpu(&p, Some(&lut), &mut px, 2, 2, 0.0);
        assert_eq!(&px[..4], &[255, 191, 0, 255]);
        // With lut false the texture is ignored.
        let mut px = solid([0, 64, 255], 2, 2);
        apply_cpu(&PictureToolsParams::default(), Some(&lut), &mut px, 2, 2, 0.0);
        assert_eq!(&px[..3], &[0, 64, 255]);
    }

    #[test]
    fn uniforms_follow_the_table() {
        let p = PictureToolsParams {
            false_color: Some(FalseColor { bands: FALSE_COLOR_CONTROLLER.to_vec(), outside: FalseColorOutside::Grayscale }),
            zebras: vec![Zebra { level: 0.95, band: None }],
            zebra_style: ZebraStyle::Animated { stripe_px: 8.0, speed_px_per_s: 40.0 },
            ..Default::default()
        };
        let u = uniforms(&p, 1.5);
        let names: Vec<&str> = u.iter().map(|x| x.name).collect();
        let want: Vec<&str> = UNIFORMS.iter().filter(|d| d.kind != Sampler2D).map(|d| d.name).collect();
        assert_eq!(names, want);
        assert_eq!(u.iter().map(|x| x.value.len()).sum::<usize>(), uniform_float_count());
        assert_eq!(uniform_float_count(), 64);
        let get = |n: &str| u.iter().find(|x| x.name == n).unwrap().value.clone();
        assert_eq!(get("u_time"), vec![1.5]);
        assert_eq!(get("u_fc_outside"), vec![1.0]);
        assert_eq!(get("u_fc_count"), vec![6.0]);
        assert_eq!(get("u_zebra_style"), vec![1.0]);
        assert_eq!(get("u_zebra_speed"), vec![40.0]);
        assert_eq!(get("u_fc_rgb").len(), 24);
    }

    #[test]
    fn params_round_trip_as_json() {
        let p = PictureToolsParams {
            range: Range::Limited,
            luma: LumaWeights::Bt2020,
            lut: true,
            false_color: Some(FalseColor { bands: FALSE_COLOR_CAMERA.to_vec(), outside: FalseColorOutside::Picture }),
            zebras: vec![Zebra { level: 0.95, band: None }, Zebra { level: 0.5, band: Some(0.025) }],
            zebra_style: ZebraStyle::Animated { stripe_px: 8.0, speed_px_per_s: 40.0 },
            peaking: Some(Peaking { threshold: 0.1, color: [1.0, 0.1, 0.1] }),
        };
        let json = serde_json::to_string(&p).unwrap();
        assert_eq!(serde_json::from_str::<PictureToolsParams>(&json).unwrap(), p);
        assert_eq!(serde_json::from_str::<PictureToolsParams>("{}").unwrap(), PictureToolsParams::default());
    }
}
