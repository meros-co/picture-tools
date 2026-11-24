//! Video scopes from any frame: waveform, RGB parade, vectorscope and
//! histogram, as grids of intensities an app draws as an image.
//!
//! [`ScopeOptions::CONTROLLER`] gives a production controller's scopes exactly
//! (256-level waveform, 3×128 parade, 256 vectorscope, 256-bin histogram,
//! exposure-curve intensities); [`ScopeOptions::camera`] a camera app's
//! sizes (128 levels across the picture's own columns, 128 vectorscope, 64
//! bins, square-root intensities). Everything else about the two is the
//! same arithmetic.

use serde::{Deserialize, Serialize};

use crate::tools::{LumaWeights, Range};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScopeKind {
    Waveform,
    Parade,
    Vectorscope,
    Histogram,
}

impl ScopeKind {
    pub fn name(self) -> &'static str {
        match self {
            ScopeKind::Waveform => "waveform",
            ScopeKind::Parade => "parade",
            ScopeKind::Vectorscope => "vectorscope",
            ScopeKind::Histogram => "histogram",
        }
    }

    pub fn from_index(i: i32) -> Option<ScopeKind> {
        [ScopeKind::Waveform, ScopeKind::Parade, ScopeKind::Vectorscope, ScopeKind::Histogram].get(i as usize).copied()
    }
}

/// The output shape of each scope.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Shape {
    /// Columns × levels. Zero columns: one per (sampled) picture column.
    pub waveform: (u32, u32),
    /// Columns × levels for each of the three panels, side by side.
    pub parade_panel: (u32, u32),
    /// Square size; neutral at the center, Cb across, Cr up.
    pub vectorscope: u32,
    /// 1.0: the full Cb and Cr range spans the square.
    pub vectorscope_gain: f32,
    /// Bins per row; rows are Y′, R, G, B.
    pub histogram_bins: u32,
}

impl Shape {
    /// A production controller's scopes.
    pub const CONTROLLER: Shape = Shape {
        waveform: (256, 256),
        parade_panel: (128, 256),
        vectorscope: 256,
        vectorscope_gain: 1.0,
        histogram_bins: 256,
    };

    /// A camera app's scopes.
    pub fn camera() -> Shape {
        Shape { waveform: (0, 128), parade_panel: (0, 128), vectorscope: 128, vectorscope_gain: 1.0, histogram_bins: 64 }
    }
}

/// How pixel counts become intensities (0–255).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "curve")]
pub enum Intensity {
    /// 255·(1 − e^(−n/k)) on raw counts: a few pixels show, many don't
    /// saturate.
    Exposure { k: f32 },
    /// 255·√(n / peak), the peak taken per histogram row, per parade panel,
    /// and over the whole waveform or vectorscope.
    PeakSqrt,
    /// 255·n / peak, with the same peaks.
    Linear,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ScopeOptions {
    pub shape: Shape,
    /// For the waveform, parade and vectorscope.
    pub intensity: Intensity,
    pub histogram_intensity: Intensity,
    pub range: Range,
    pub luma: LumaWeights,
}

impl ScopeOptions {
    /// A production controller's scopes, exactly.
    pub const CONTROLLER: ScopeOptions = ScopeOptions {
        shape: Shape::CONTROLLER,
        intensity: Intensity::Exposure { k: 3.0 },
        histogram_intensity: Intensity::PeakSqrt,
        range: Range::Full,
        luma: LumaWeights::Bt709,
    };

    /// A camera app's scopes.
    pub fn camera() -> ScopeOptions {
        ScopeOptions {
            shape: Shape::camera(),
            intensity: Intensity::PeakSqrt,
            histogram_intensity: Intensity::PeakSqrt,
            range: Range::Full,
            luma: LumaWeights::Bt709,
        }
    }
}

impl Default for ScopeOptions {
    fn default() -> Self {
        ScopeOptions::CONTROLLER
    }
}

/// One scope: an intensity per cell, rows from the top.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScopeFrame {
    pub kind: ScopeKind,
    pub width: u32,
    pub height: u32,
    pub data: Vec<u8>,
}

/// A frame ready for scopes: R, G, B and Y′ per pixel, 0–255, full range.
#[derive(Debug, Clone, PartialEq)]
pub struct Picture {
    pixels: Vec<[u8; 4]>,
    width: usize,
    height: usize,
}

/// Wider pictures are sampled every `width / 640` pixels in both
/// directions (a camera app's stats frames); narrower ones in full.
fn sample_step(width: usize) -> usize {
    (width / 640).max(1)
}

/// Cb and Cr divisors (2(1 − Kb), 2(1 − Kr)) for the weights.
fn chroma_divisors(luma: LumaWeights) -> (f32, f32) {
    match luma {
        LumaWeights::Bt709 => (1.8556, 1.5748),
        LumaWeights::Bt601 => (1.772, 1.402),
        LumaWeights::Bt2020 => (1.8814, 1.4746),
    }
}

/// Limited-range YUV (16–235, 16–240) to full-range RGB.
fn yuv_to_rgb(y: u8, u: u8, v: u8, luma: LumaWeights) -> [u8; 3] {
    let [kr, _, kb] = luma.weights();
    let (db, dr) = chroma_divisors(luma);
    let y = (y as f32 - 16.0) * (255.0 / 219.0);
    let cb = (u as f32 - 128.0) * (255.0 / 224.0);
    let cr = (v as f32 - 128.0) * (255.0 / 224.0);
    let r = y + dr * cr;
    let b = y + db * cb;
    let g = (y - kr * r - kb * b) / (1.0 - kr - kb);
    [r, g, b].map(|c| c.round().clamp(0.0, 255.0) as u8)
}

impl Picture {
    fn build(width: usize, height: usize, o: &ScopeOptions, mut rgb_at: impl FnMut(usize, usize) -> [u8; 3]) -> Picture {
        let step = sample_step(width);
        let w = width.div_ceil(step);
        let h = height.div_ceil(step);
        let [wr, wg, wb] = o.luma.weights();
        let limited = o.range == Range::Limited;
        let mut pixels = Vec::with_capacity(w * h);
        for y in (0..height).step_by(step) {
            for x in (0..width).step_by(step) {
                let mut px = rgb_at(x, y);
                if limited {
                    px = px.map(|v| ((v as f32 - 16.0) * (255.0 / 219.0)).round().clamp(0.0, 255.0) as u8);
                }
                let (r, g, b) = (px[0] as f32, px[1] as f32, px[2] as f32);
                let luma = wr * r + wg * g + wb * b;
                pixels.push([px[0], px[1], px[2], luma.round().clamp(0.0, 255.0) as u8]);
            }
        }
        Picture { pixels, width: w, height: h }
    }

    /// Four bytes per pixel, R, G, B and padding, rows `stride` bytes apart.
    pub fn from_rgbx(px: &[u8], w: u32, h: u32, stride: u32, o: &ScopeOptions) -> Picture {
        let (w, h, s) = (w as usize, (h as usize).min(px.len() / (stride.max(1) as usize)), stride as usize);
        Picture::build(w, h, o, |x, y| {
            let i = y * s + x * 4;
            [px[i], px[i + 1], px[i + 2]]
        })
    }

    pub fn from_rgba(px: &[u8], w: u32, h: u32, stride: u32, o: &ScopeOptions) -> Picture {
        Picture::from_rgbx(px, w, h, stride, o)
    }

    pub fn from_bgra(px: &[u8], w: u32, h: u32, stride: u32, o: &ScopeOptions) -> Picture {
        let (w, h, s) = (w as usize, (h as usize).min(px.len() / (stride.max(1) as usize)), stride as usize);
        Picture::build(w, h, o, |x, y| {
            let i = y * s + x * 4;
            [px[i + 2], px[i + 1], px[i]]
        })
    }

    /// UYVY 4:2:2, limited range, matrix by `o.luma`.
    pub fn from_uyvy(px: &[u8], w: u32, h: u32, stride: u32, o: &ScopeOptions) -> Picture {
        let (w, h, s) = (w as usize, (h as usize).min(px.len() / (stride.max(1) as usize)), stride as usize);
        let luma = o.luma;
        let full = ScopeOptions { range: Range::Full, ..*o };
        Picture::build(w, h, &full, |x, y| {
            let i = y * s + (x / 2) * 4;
            let yy = if x % 2 == 0 { px[i + 1] } else { px[i + 3] };
            yuv_to_rgb(yy, px[i], px[i + 2], luma)
        })
    }

    /// NV12 4:2:0 (a Y plane, then interleaved Cb Cr), limited range,
    /// matrix by `o.luma`; both planes `stride` bytes per row.
    pub fn from_nv12(px: &[u8], w: u32, h: u32, stride: u32, o: &ScopeOptions) -> Picture {
        let (w, h, s) = (w as usize, h as usize, stride as usize);
        let uv = s * h;
        let h = if px.len() >= uv + s * h.div_ceil(2) { h } else { 0 };
        let luma = o.luma;
        let full = ScopeOptions { range: Range::Full, ..*o };
        Picture::build(w, h, &full, |x, y| {
            let c = uv + (y / 2) * s + (x / 2) * 2;
            yuv_to_rgb(px[y * s + x], px[c], px[c + 1], luma)
        })
    }

    /// Sampled size.
    pub fn size(&self) -> (u32, u32) {
        (self.width as u32, self.height as u32)
    }

    /// (R, G, B, Y′) of each sampled pixel, rows from the top.
    pub fn pixels(&self) -> &[[u8; 4]] {
        &self.pixels
    }
}

/// Counts to intensities, normalized over each `group` cells.
fn intensities(counts: &[u32], curve: Intensity, group: usize) -> Vec<u8> {
    match curve {
        Intensity::Exposure { k } => counts
            .iter()
            .map(|&n| if n == 0 { 0 } else { (255.0 * (1.0 - (-(n as f32) / k).exp())).round() as u8 })
            .collect(),
        Intensity::PeakSqrt | Intensity::Linear => {
            let mut out = Vec::with_capacity(counts.len());
            for chunk in counts.chunks(group.max(1)) {
                let peak = (*chunk.iter().max().unwrap_or(&1)).max(1) as f32;
                out.extend(chunk.iter().map(|&n| {
                    let f = n as f32 / peak;
                    let v = if matches!(curve, Intensity::PeakSqrt) { f.sqrt() } else { f };
                    (v * 255.0).round() as u8
                }));
            }
            out
        }
    }
}

/// Like [`intensities`] for a parade: panels side by side in each row, each
/// normalized to its own peak.
fn panel_intensities(counts: &[u32], curve: Intensity, panel: usize, panels: usize, rows: usize) -> Vec<u8> {
    if let Intensity::Exposure { .. } = curve {
        return intensities(counts, curve, counts.len());
    }
    let width = panel * panels;
    let mut out = vec![0u8; counts.len()];
    for p in 0..panels {
        let cell = |row: usize, col: usize| row * width + p * panel + col;
        let peak = (0..rows).flat_map(|r| (0..panel).map(move |c| (r, c))).map(|(r, c)| counts[cell(r, c)]).max().unwrap_or(1).max(1) as f32;
        for r in 0..rows {
            for c in 0..panel {
                let f = counts[cell(r, c)] as f32 / peak;
                let v = if matches!(curve, Intensity::PeakSqrt) { f.sqrt() } else { f };
                out[cell(r, c)] = (v * 255.0).round() as u8;
            }
        }
    }
    out
}

/// One scope of `pic`.
pub fn compute(kind: ScopeKind, pic: &Picture, o: &ScopeOptions) -> ScopeFrame {
    let width = pic.width.max(1);
    let shape = &o.shape;
    match kind {
        ScopeKind::Waveform => {
            let cols = if shape.waveform.0 == 0 { pic.width.max(1) } else { shape.waveform.0 as usize };
            let levels = shape.waveform.1.max(1) as usize;
            let mut counts = vec![0u32; cols * levels];
            for (i, p) in pic.pixels.iter().enumerate() {
                let column = (i % width) * cols / width;
                let row = levels - 1 - (p[3] as usize * levels / 256);
                counts[row * cols + column] += 1;
            }
            ScopeFrame { kind, width: cols as u32, height: levels as u32, data: intensities(&counts, o.intensity, counts.len()) }
        }
        ScopeKind::Parade => {
            let panel = if shape.parade_panel.0 == 0 { pic.width.max(1) } else { shape.parade_panel.0 as usize };
            let levels = shape.parade_panel.1.max(1) as usize;
            let total = panel * 3;
            let mut counts = vec![0u32; total * levels];
            for (i, p) in pic.pixels.iter().enumerate() {
                let column = (i % width) * panel / width;
                for c in 0..3 {
                    let row = levels - 1 - (p[c] as usize * levels / 256);
                    counts[row * total + c * panel + column] += 1;
                }
            }
            ScopeFrame { kind, width: total as u32, height: levels as u32, data: panel_intensities(&counts, o.intensity, panel, 3, levels) }
        }
        ScopeKind::Vectorscope => {
            let size = shape.vectorscope.max(1) as usize;
            let (db, dr) = chroma_divisors(o.luma);
            let center = size as f32 / 2.0;
            let scale = shape.vectorscope_gain * size as f32 / 256.0;
            let top = (size - 1) as f32;
            let mut counts = vec![0u32; size * size];
            for p in &pic.pixels {
                let (r, b, y) = (p[0] as f32, p[2] as f32, p[3] as f32);
                let cb = (b - y) / db;
                let cr = (r - y) / dr;
                let x = (center + cb * scale).round().clamp(0.0, top) as usize;
                let row = (center - cr * scale).round().clamp(0.0, top) as usize;
                counts[row * size + x] += 1;
            }
            ScopeFrame { kind, width: size as u32, height: size as u32, data: intensities(&counts, o.intensity, counts.len()) }
        }
        ScopeKind::Histogram => {
            let bins = shape.histogram_bins.max(1) as usize;
            let mut counts = vec![0u32; 4 * bins];
            for p in &pic.pixels {
                counts[p[3] as usize * bins / 256] += 1;
                for c in 0..3 {
                    counts[(c + 1) * bins + p[c] as usize * bins / 256] += 1;
                }
            }
            ScopeFrame { kind, width: bins as u32, height: 4, data: intensities(&counts, o.histogram_intensity, bins) }
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// The production controller's scopes as they were, verbatim but for
    /// the GStreamer feed: `compute` with `ScopeOptions::CONTROLLER` must match
    /// them byte for byte.
    pub(crate) mod controller {
        pub const WIDTH: usize = 480;

        pub struct ScopeFrame {
            pub width: u32,
            pub height: u32,
            pub data: Vec<u8>,
        }

        pub struct Picture {
            pixels: Vec<[u8; 4]>,
            width: usize,
        }

        impl Picture {
            pub fn new(rgbx: &[u8], stride: usize) -> Picture {
                let height = rgbx.len() / stride.max(1);
                let width = stride / 4;
                let mut pixels = Vec::new();
                for row in rgbx.chunks(stride).take(height) {
                    for px in row.chunks_exact(4).take(width.min(WIDTH)) {
                        let (r, g, b) = (px[0] as f32, px[1] as f32, px[2] as f32);
                        let y = 0.2126 * r + 0.7152 * g + 0.0722 * b;
                        pixels.push([px[0], px[1], px[2], y.round().clamp(0.0, 255.0) as u8]);
                    }
                }
                Picture { pixels, width: width.min(WIDTH) }
            }

            pub fn waveform(&self) -> ScopeFrame {
                let mut counts = vec![0u32; 256 * 256];
                for (i, p) in self.pixels.iter().enumerate() {
                    let column = (i % self.width) * 256 / self.width;
                    counts[(255 - p[3] as usize) * 256 + column] += 1;
                }
                frame(256, 256, &counts)
            }

            pub fn parade(&self) -> ScopeFrame {
                let mut counts = vec![0u32; 384 * 256];
                for (i, p) in self.pixels.iter().enumerate() {
                    let column = (i % self.width) * 128 / self.width;
                    for c in 0..3 {
                        counts[(255 - p[c] as usize) * 384 + c * 128 + column] += 1;
                    }
                }
                frame(384, 256, &counts)
            }

            pub fn vectorscope(&self) -> ScopeFrame {
                let mut counts = vec![0u32; 256 * 256];
                for p in &self.pixels {
                    let (r, b, y) = (p[0] as f32, p[2] as f32, p[3] as f32);
                    let cb = (b - y) / 1.8556;
                    let cr = (r - y) / 1.5748;
                    let x = (128.0 + cb).round().clamp(0.0, 255.0) as usize;
                    let row = (128.0 - cr).round().clamp(0.0, 255.0) as usize;
                    counts[row * 256 + x] += 1;
                }
                frame(256, 256, &counts)
            }

            pub fn histogram(&self) -> ScopeFrame {
                let mut bins = [[0u32; 256]; 4];
                for p in &self.pixels {
                    bins[0][p[3] as usize] += 1;
                    for c in 0..3 {
                        bins[c + 1][p[c] as usize] += 1;
                    }
                }
                let mut data = Vec::with_capacity(4 * 256);
                for channel in &bins {
                    let peak = (*channel.iter().max().unwrap_or(&1)).max(1) as f32;
                    data.extend(channel.iter().map(|&n| ((n as f32 / peak).sqrt() * 255.0).round() as u8));
                }
                ScopeFrame { width: 256, height: 4, data }
            }
        }

        fn frame(width: u32, height: u32, counts: &[u32]) -> ScopeFrame {
            ScopeFrame {
                width,
                height,
                data: counts
                    .iter()
                    .map(|&n| if n == 0 { 0 } else { (255.0 * (1.0 - (-(n as f32) / 3.0).exp())).round() as u8 })
                    .collect(),
            }
        }
    }

    /// The controller's own test picture: left half black, right half
    /// white, a red square.
    pub(crate) fn controller_test_picture() -> (Vec<u8>, usize, usize) {
        let (w, h) = (64usize, 36usize);
        let mut rgbx = vec![0u8; w * h * 4];
        for y in 0..h {
            for x in 0..w {
                let i = (y * w + x) * 4;
                let px = if x < 8 && y < 8 {
                    [255, 0, 0]
                } else if x >= w / 2 {
                    [255, 255, 255]
                } else {
                    [0, 0, 0]
                };
                rgbx[i..i + 3].copy_from_slice(&px);
            }
        }
        (rgbx, w, h)
    }

    /// A busy 480×270 frame: gradients, bars and noise-like texture, so
    /// every level and many vectorscope cells are hit.
    pub(crate) fn busy_picture() -> (Vec<u8>, usize, usize) {
        let (w, h) = (480usize, 270usize);
        let mut rgbx = vec![0u8; w * h * 4];
        let mut seed = 0x1234_5678u32;
        for y in 0..h {
            for x in 0..w {
                seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                let noise = (seed >> 24) as u8;
                let px = match y * 4 / h {
                    0 => [(x * 255 / (w - 1)) as u8; 3],
                    1 => BARS[x * 8 / w],
                    2 => [(x * 255 / w) as u8, (y * 255 / h) as u8, noise],
                    _ => [noise, noise.wrapping_mul(3), noise.wrapping_add(80)],
                };
                rgbx[(y * w + x) * 4..][..3].copy_from_slice(&px);
            }
        }
        (rgbx, w, h)
    }

    /// 75% bars, BT.709 full range.
    pub(crate) const BARS: [[u8; 3]; 8] =
        [[191, 191, 191], [191, 191, 0], [0, 191, 191], [0, 191, 0], [191, 0, 191], [191, 0, 0], [0, 0, 191], [0, 0, 0]];

    fn all_kinds() -> [ScopeKind; 4] {
        [ScopeKind::Waveform, ScopeKind::Parade, ScopeKind::Vectorscope, ScopeKind::Histogram]
    }

    fn controller_frame(kind: ScopeKind, p: &controller::Picture) -> controller::ScopeFrame {
        match kind {
            ScopeKind::Waveform => p.waveform(),
            ScopeKind::Parade => p.parade(),
            ScopeKind::Vectorscope => p.vectorscope(),
            ScopeKind::Histogram => p.histogram(),
        }
    }

    #[test]
    fn controller_scopes_match_byte_for_byte() {
        for (rgbx, w, h) in [controller_test_picture(), busy_picture()] {
            let theirs = controller::Picture::new(&rgbx, w * 4);
            let ours = Picture::from_rgbx(&rgbx, w as u32, h as u32, (w * 4) as u32, &ScopeOptions::CONTROLLER);
            for kind in all_kinds() {
                let a = compute(kind, &ours, &ScopeOptions::CONTROLLER);
                let b = controller_frame(kind, &theirs);
                assert_eq!((a.width, a.height), (b.width, b.height), "{kind:?}");
                assert!(a.data == b.data, "{kind:?} on {w}×{h} differs");
            }
        }
    }

    /// The goldens in tests/golden were made by the controller's code on
    /// the busy picture; `compute` must still give them.
    #[test]
    fn controller_goldens_still_match() {
        let (rgbx, w, h) = busy_picture();
        let pic = Picture::from_rgbx(&rgbx, w as u32, h as u32, (w * 4) as u32, &ScopeOptions::CONTROLLER);
        let goldens: [(ScopeKind, &[u8]); 4] = [
            (ScopeKind::Waveform, include_bytes!("../tests/golden/controller_waveform.bin")),
            (ScopeKind::Parade, include_bytes!("../tests/golden/controller_parade.bin")),
            (ScopeKind::Vectorscope, include_bytes!("../tests/golden/controller_vectorscope.bin")),
            (ScopeKind::Histogram, include_bytes!("../tests/golden/controller_histogram.bin")),
        ];
        for (kind, golden) in goldens {
            assert!(compute(kind, &pic, &ScopeOptions::CONTROLLER).data == golden, "{kind:?}");
        }
    }

    /// Writes the goldens from the controller's own code (run once, before
    /// the move): `PICTURE_TOOLS_WRITE_GOLDENS=1 cargo test write_goldens`.
    #[test]
    fn write_goldens() {
        if std::env::var("PICTURE_TOOLS_WRITE_GOLDENS").is_err() {
            return;
        }
        let (rgbx, w, _) = busy_picture();
        let p = controller::Picture::new(&rgbx, w * 4);
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden");
        std::fs::create_dir_all(&dir).unwrap();
        for kind in all_kinds() {
            std::fs::write(dir.join(format!("controller_{}.bin", kind.name())), controller_frame(kind, &p).data).unwrap();
        }
    }

    // The controller's three tests, on `compute`.

    fn controller_picture() -> Picture {
        let (rgbx, w, h) = controller_test_picture();
        Picture::from_rgbx(&rgbx, w as u32, h as u32, (w * 4) as u32, &ScopeOptions::CONTROLLER)
    }

    #[test]
    fn the_waveform_puts_black_low_on_the_left_and_white_high_on_the_right() {
        let w = compute(ScopeKind::Waveform, &controller_picture(), &ScopeOptions::CONTROLLER);
        let at = |level: usize, column: usize| w.data[(255 - level) * 256 + column];
        assert!(at(0, 40) > 0 && at(255, 40) == 0, "left: black");
        assert!(at(255, 200) > 0 && at(0, 200) == 0, "right: white");
    }

    #[test]
    fn red_lands_toward_red_on_the_vectorscope() {
        let v = compute(ScopeKind::Vectorscope, &controller_picture(), &ScopeOptions::CONTROLLER);
        let lit: Vec<(usize, usize)> = (0..256 * 256).filter(|&i| v.data[i] > 0).map(|i| (i % 256, i / 256)).collect();
        assert!(lit.contains(&(128, 128)), "neutral black and white at the centre");
        assert!(lit.iter().any(|&(x, y)| x < 128 && y < 64), "red up and left: {lit:?}");
    }

    #[test]
    fn the_histogram_counts_levels() {
        let h = compute(ScopeKind::Histogram, &controller_picture(), &ScopeOptions::CONTROLLER);
        assert_eq!(h.data.len(), 4 * 256);
        assert!(h.data[0] > 200, "half the luma is black");
        assert!(h.data[255] > 200, "half is white");
        assert_eq!(h.data[128], 0, "none is mid-grey");
    }

    /// Bars in every pixel format reach the same RGB (within YUV rounding).
    #[test]
    fn every_pixel_format_agrees_on_color_bars() {
        let (w, h) = (64usize, 8usize);
        let rgb = |x: usize| BARS[x * 8 / w];
        let rgba: Vec<u8> = (0..w * h).flat_map(|i| { let c = rgb(i % w); [c[0], c[1], c[2], 255] }).collect();
        let bgra: Vec<u8> = (0..w * h).flat_map(|i| { let c = rgb(i % w); [c[2], c[1], c[0], 255] }).collect();
        // Limited-range BT.709 YUV of each bar.
        let to_yuv = |c: [u8; 3]| {
            let (r, g, b) = (c[0] as f32 / 255.0, c[1] as f32 / 255.0, c[2] as f32 / 255.0);
            let y = 0.2126 * r + 0.7152 * g + 0.0722 * b;
            let cb = (b - y) / 1.8556;
            let cr = (r - y) / 1.5748;
            [(16.0 + 219.0 * y).round() as u8, (128.0 + 224.0 * cb).round() as u8, (128.0 + 224.0 * cr).round() as u8]
        };
        let uyvy: Vec<u8> = (0..h).flat_map(|_| (0..w / 2).flat_map(|p| {
            let [y0, u, v] = to_yuv(rgb(p * 2));
            let [y1, _, _] = to_yuv(rgb(p * 2 + 1));
            [u, y0, v, y1]
        }).collect::<Vec<u8>>()).collect();
        let mut nv12: Vec<u8> = (0..w * h).map(|i| to_yuv(rgb(i % w))[0]).collect();
        for _ in 0..h / 2 {
            for p in 0..w / 2 {
                let [_, u, v] = to_yuv(rgb(p * 2));
                nv12.extend([u, v]);
            }
        }
        let o = ScopeOptions::CONTROLLER;
        let pics = [
            Picture::from_rgba(&rgba, w as u32, h as u32, (w * 4) as u32, &o),
            Picture::from_bgra(&bgra, w as u32, h as u32, (w * 4) as u32, &o),
            Picture::from_uyvy(&uyvy, w as u32, h as u32, (w * 2) as u32, &o),
            Picture::from_nv12(&nv12, w as u32, h as u32, w as u32, &o),
        ];
        for pic in &pics[1..] {
            assert_eq!(pic.size(), pics[0].size());
            for (a, b) in pic.pixels().iter().zip(pics[0].pixels()) {
                for c in 0..4 {
                    assert!((a[c] as i32 - b[c] as i32).abs() <= 2, "{a:?} vs {b:?}");
                }
            }
        }
    }

    /// The 75% bars land where a BT.709 vectorscope graticule puts them:
    /// gain 1, Cb across, Cr up, neutral at 128.
    #[test]
    fn bars_land_on_the_vectorscope_targets() {
        let o = ScopeOptions::CONTROLLER;
        for c in &BARS[1..7] {
            let px: Vec<u8> = [c[0], c[1], c[2], 255].repeat(16);
            let pic = Picture::from_rgba(&px, 4, 4, 16, &o);
            let v = compute(ScopeKind::Vectorscope, &pic, &o);
            let lit = v.data.iter().position(|&n| n > 0).unwrap();
            let (x, row) = ((lit % 256) as f32, (lit / 256) as f32);
            let (r, g, b) = (c[0] as f32, c[1] as f32, c[2] as f32);
            let y = (0.2126 * r + 0.7152 * g + 0.0722 * b).round();
            let want_x = 128.0 + (b - y) / 1.8556;
            let want_row = 128.0 - (r - y) / 1.5748;
            assert!((x - want_x).abs() <= 0.5 && (row - want_row).abs() <= 0.5, "{c:?}: ({x}, {row}) vs ({want_x}, {want_row})");
        }
        // Red is up and left, cyan down and right.
        let lit = |c: [u8; 3]| {
            let pic = Picture::from_rgba(&[c[0], c[1], c[2], 255], 1, 1, 4, &o);
            let v = compute(ScopeKind::Vectorscope, &pic, &o);
            let i = v.data.iter().position(|&n| n > 0).unwrap();
            (i % 256, i / 256)
        };
        let (rx, ry) = lit(BARS[5]);
        let (cx, cy) = lit(BARS[2]);
        assert!(rx < 128 && ry < 128 && cx > 128 && cy > 128);
    }

    #[test]
    fn camera_shapes_follow_the_picture() {
        let o = ScopeOptions::camera();
        // A 1920-wide stats frame is sampled every third pixel: 640 columns.
        let px = vec![128u8; 1920 * 1080 * 4];
        let pic = Picture::from_rgba(&px, 1920, 1080, 1920 * 4, &o);
        assert_eq!(pic.size(), (640, 360));
        let sizes: Vec<(u32, u32)> = all_kinds().iter().map(|&k| {
            let f = compute(k, &pic, &o);
            (f.width, f.height)
        }).collect();
        assert_eq!(sizes, vec![(640, 128), (1920, 128), (128, 128), (64, 4)]);
        // A 320-wide one in full.
        let small = Picture::from_rgba(&vec![128u8; 320 * 180 * 4], 320, 180, 1280, &o);
        assert_eq!(compute(ScopeKind::Waveform, &small, &o).width, 320);
    }

    #[test]
    fn square_root_intensities_normalize_per_panel() {
        let o = ScopeOptions::camera();
        // Bright red: every parade panel has one lit level at its own peak.
        let px = [255u8, 128, 0, 255].repeat(64);
        let pic = Picture::from_rgba(&px, 8, 8, 32, &o);
        let p = compute(ScopeKind::Parade, &pic, &o);
        let max_in = |panel: usize| (0..p.height as usize).flat_map(|r| (0..8).map(move |c| (r, c))).map(|(r, c)| p.data[r * 24 + panel * 8 + c]).max().unwrap();
        assert_eq!((max_in(0), max_in(1), max_in(2)), (255, 255, 255));
    }
}
