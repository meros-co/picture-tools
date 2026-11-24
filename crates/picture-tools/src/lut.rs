//! 3D LUTs: `.cube` and `.3dl` parsing, trilinear lookup (the CPU
//! reference for the shaders), `.cube` output, and the 2D atlas the shaders
//! sample.

use std::fmt;

/// A 3D LUT: `size`³ RGB entries in `.cube` order (red fastest, then green,
/// then blue). Inputs are mapped from `domain_min..domain_max` to the grid.
#[derive(Debug, Clone, PartialEq)]
pub struct Lut3d {
    pub size: u32,
    pub data: Vec<[f32; 3]>,
    pub domain_min: [f32; 3],
    pub domain_max: [f32; 3],
    pub title: Option<String>,
}

/// Why a LUT file was refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LutError(pub String);

impl fmt::Display for LutError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for LutError {}

fn err(s: impl Into<String>) -> LutError {
    LutError(s.into())
}

/// Largest 3D size accepted.
const MAX_SIZE: usize = 128;
/// A 1D LUT becomes a 3D one of at most this size.
const MAX_FROM_1D: usize = 65;

/// Reads a `.cube` LUT, 3D or 1D (a 1D curve per channel becomes a 3D LUT
/// of up to 65³ the GPU can sample), with optional `TITLE` and
/// `DOMAIN_MIN`/`DOMAIN_MAX`.
pub fn parse_cube(text: &str) -> Result<Lut3d, LutError> {
    let mut size = None;
    let mut one_d = None;
    let mut title = None;
    let mut domain_min = [0.0f32; 3];
    let mut domain_max = [1.0f32; 3];
    let mut data: Vec<f32> = Vec::new();
    for line in text.lines().map(str::trim) {
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some(t) = line.strip_prefix("TITLE") {
            let t = t.trim().trim_matches('"');
            if !t.is_empty() {
                title = Some(t.to_string());
            }
            continue;
        }
        if let Some(n) = line.strip_prefix("LUT_3D_SIZE") {
            size = Some(n.trim().parse::<usize>().map_err(|_| err(format!("bad LUT_3D_SIZE: {line}")))?);
            continue;
        }
        if let Some(n) = line.strip_prefix("LUT_1D_SIZE") {
            one_d = Some(n.trim().parse::<usize>().map_err(|_| err(format!("bad LUT_1D_SIZE: {line}")))?);
            continue;
        }
        if line.starts_with("DOMAIN_MIN") || line.starts_with("DOMAIN_MAX") {
            let v: Vec<f32> = line.split_whitespace().skip(1).map(str::parse).collect::<Result<_, _>>().map_err(|_| err(format!("bad domain: {line}")))?;
            let v: [f32; 3] = v.try_into().map_err(|_| err(format!("bad domain: {line}")))?;
            if line.starts_with("DOMAIN_MIN") { domain_min = v } else { domain_max = v }
            continue;
        }
        // Other keywords (LUT_3D_INPUT_RANGE and vendor ones) are ignored.
        if line.starts_with(|c: char| c.is_ascii_alphabetic()) {
            continue;
        }
        let v: Vec<f32> = line.split_whitespace().map(str::parse).collect::<Result<_, _>>().map_err(|_| err(format!("bad line: {line}")))?;
        if v.len() != 3 {
            return Err(err(format!("bad line: {line}")));
        }
        data.extend(v);
    }
    if (0..3).any(|c| domain_max[c] <= domain_min[c]) {
        return Err(err("DOMAIN_MAX must be above DOMAIN_MIN"));
    }
    if let (Some(n), None) = (one_d, size) {
        if !(2..=65536).contains(&n) || data.len() != n * 3 {
            return Err(err(format!("expected {n} 1D entries, found {}", data.len() / 3)));
        }
        let curve = |c: usize, x: f64| {
            let p = x * (n - 1) as f64;
            let i = (p.floor() as usize).min(n - 2);
            let f = p - i as f64;
            let a = data[i * 3 + c] as f64;
            a + (data[(i + 1) * 3 + c] as f64 - a) * f
        };
        let out = MAX_FROM_1D.min(n.max(2));
        let at = |i: usize| i as f64 / (out - 1) as f64;
        let mut lut = Vec::with_capacity(out * out * out);
        for b in 0..out {
            for g in 0..out {
                for r in 0..out {
                    lut.push([curve(0, at(r)) as f32, curve(1, at(g)) as f32, curve(2, at(b)) as f32]);
                }
            }
        }
        return Ok(Lut3d { size: out as u32, data: lut, domain_min, domain_max, title });
    }
    let size = size.ok_or_else(|| err("no LUT_3D_SIZE"))?;
    if !(2..=MAX_SIZE).contains(&size) || data.len() != size * size * size * 3 {
        return Err(err(format!("expected {size}³ entries, found {}", data.len() / 3)));
    }
    Ok(Lut3d {
        size: size as u32,
        data: data.chunks_exact(3).map(|t| [t[0], t[1], t[2]]).collect(),
        domain_min,
        domain_max,
        title,
    })
}

/// Reads a `.3dl` LUT: a first line of input points (the mesh), then one
/// integer RGB line per point, blue changing fastest, in the file's own
/// bit depth (10, 12 or 16 bits, from the largest value).
pub fn parse_3dl(text: &str) -> Result<Lut3d, LutError> {
    let mut rows = text.lines().map(str::trim).filter(|l| {
        !l.is_empty() && !l.starts_with('#') && !l.starts_with('<') && !l.starts_with("3DMESH") && !l.starts_with("Mesh")
    });
    let mesh: Vec<u32> = rows
        .next()
        .ok_or_else(|| err("empty .3dl"))?
        .split_whitespace()
        .map(str::parse)
        .collect::<Result<_, _>>()
        .map_err(|_| err("bad .3dl mesh line"))?;
    let size = mesh.len();
    if !(2..=MAX_SIZE).contains(&size) {
        return Err(err(format!("a .3dl mesh of {size} points isn't supported")));
    }
    let values: Vec<[u32; 3]> = rows
        .map(|l| {
            let v: Vec<u32> = l.split_whitespace().map(str::parse).collect::<Result<_, _>>().map_err(|_| err(format!("bad line: {l}")))?;
            <[u32; 3]>::try_from(v).map_err(|_| err(format!("bad line: {l}")))
        })
        .collect::<Result<_, _>>()?;
    if values.len() != size * size * size {
        return Err(err(format!("expected {size}³ entries, found {}", values.len())));
    }
    let top = values.iter().flatten().copied().max().unwrap_or(0);
    let scale = [1023u32, 4095, 65535].into_iter().find(|&m| top <= m).unwrap_or(65535) as f32;
    // .3dl runs blue fastest; .cube (and this type) red fastest.
    let mut data = vec![[0.0f32; 3]; size * size * size];
    for r in 0..size {
        for g in 0..size {
            for b in 0..size {
                let v = values[(r * size + g) * size + b];
                data[(b * size + g) * size + r] = [v[0] as f32 / scale, v[1] as f32 / scale, v[2] as f32 / scale];
            }
        }
    }
    Ok(Lut3d { size: size as u32, data, domain_min: [0.0; 3], domain_max: [1.0; 3], title: None })
}

/// Reads a `.cube` or `.3dl` LUT, by its file name's extension.
pub fn parse_by_name(name: &str, text: &str) -> Result<Lut3d, LutError> {
    if name.to_ascii_lowercase().ends_with(".3dl") { parse_3dl(text) } else { parse_cube(text) }
}

/// An IEEE half-float from an f32 (round to nearest even), for the 16-bit
/// atlas.
fn f32_to_f16(v: f32) -> u16 {
    let bits = v.to_bits();
    let sign = ((bits >> 16) & 0x8000) as u16;
    let exp = ((bits >> 23) & 0xff) as i32;
    let mant = bits & 0x7f_ffff;
    if exp == 0xff {
        return sign | 0x7c00 | if mant != 0 { 0x200 } else { 0 };
    }
    let e = exp - 127 + 15;
    if e >= 0x1f {
        return sign | 0x7c00;
    }
    if e <= 0 {
        if e < -10 {
            return sign;
        }
        let m = mant | 0x80_0000;
        let shift = (14 - e) as u32;
        let half = m >> shift;
        let rem = m & ((1 << shift) - 1);
        let mid = 1 << (shift - 1);
        let round = (rem > mid || (rem == mid && half & 1 == 1)) as u32;
        return sign | (half + round) as u16;
    }
    let half = ((e as u32) << 10) | (mant >> 13);
    let rem = mant & 0x1fff;
    let round = (rem > 0x1000 || (rem == 0x1000 && half & 1 == 1)) as u32;
    sign | (half + round) as u16
}

impl Lut3d {
    /// The LUT that changes nothing.
    pub fn identity(size: u32) -> Lut3d {
        Lut3d::per_channel(size, |x| x)
    }

    /// A LUT applying `f` to each channel alike (inputs 0–1).
    pub fn per_channel(size: u32, f: impl Fn(f64) -> f64) -> Lut3d {
        let n = size.max(2) as usize;
        let curve: Vec<f32> = (0..n).map(|i| f(i as f64 / (n - 1) as f64) as f32).collect();
        let mut data = Vec::with_capacity(n * n * n);
        for b in 0..n {
            for g in 0..n {
                for r in 0..n {
                    data.push([curve[r], curve[g], curve[b]]);
                }
            }
        }
        Lut3d { size: n as u32, data, domain_min: [0.0; 3], domain_max: [1.0; 3], title: None }
    }

    fn at(&self, r: usize, g: usize, b: usize) -> [f32; 3] {
        let n = self.size as usize;
        self.data[(b * n + g) * n + r]
    }

    /// The LUT at `rgb`, trilinearly interpolated as a GPU samples it:
    /// the CPU reference.
    pub fn apply(&self, rgb: [f32; 3]) -> [f32; 3] {
        let n = self.size as usize;
        let split = |c: usize| {
            let lo = self.domain_min[c] as f64;
            let hi = self.domain_max[c] as f64;
            let v = ((rgb[c] as f64 - lo) / (hi - lo)).clamp(0.0, 1.0);
            let x = v * (n - 1) as f64;
            let i = (x.floor() as usize).min(n - 2);
            (i, x - i as f64)
        };
        let ((r0, fr), (g0, fg), (b0, fb)) = (split(0), split(1), split(2));
        let lerp = |a: f64, b: f64, t: f64| a + (b - a) * t;
        let mut out = [0.0f32; 3];
        for (c, o) in out.iter_mut().enumerate() {
            let v = |r, g, b| self.at(r, g, b)[c] as f64;
            let x00 = lerp(v(r0, g0, b0), v(r0 + 1, g0, b0), fr);
            let x10 = lerp(v(r0, g0 + 1, b0), v(r0 + 1, g0 + 1, b0), fr);
            let x01 = lerp(v(r0, g0, b0 + 1), v(r0 + 1, g0, b0 + 1), fr);
            let x11 = lerp(v(r0, g0 + 1, b0 + 1), v(r0 + 1, g0 + 1, b0 + 1), fr);
            *o = lerp(lerp(x00, x10, fg), lerp(x01, x11, fg), fb) as f32;
        }
        out
    }

    /// Writes the LUT as a `.cube` file (with its title and domain).
    pub fn to_cube(&self) -> String {
        let mut s = String::new();
        if let Some(t) = &self.title {
            s.push_str(&format!("TITLE \"{t}\"\n"));
        }
        s.push_str(&format!("LUT_3D_SIZE {}\n", self.size));
        if self.domain_min != [0.0; 3] || self.domain_max != [1.0; 3] {
            let d = |v: [f32; 3]| format!("{:.6} {:.6} {:.6}", v[0], v[1], v[2]);
            s.push_str(&format!("DOMAIN_MIN {}\nDOMAIN_MAX {}\n", d(self.domain_min), d(self.domain_max)));
        }
        for t in &self.data {
            s.push_str(&format!("{:.6} {:.6} {:.6}\n", t[0], t[1], t[2]));
        }
        s
    }

    /// The entries on a 0–1 input grid: the data itself for a 0–1 domain,
    /// else the LUT resampled there, since the shaders take inputs as 0–1.
    fn unit_grid(&self) -> Vec<[f32; 3]> {
        if self.domain_min == [0.0; 3] && self.domain_max == [1.0; 3] {
            return self.data.clone();
        }
        let n = self.size as usize;
        let at = |i: usize| i as f32 / (n - 1) as f32;
        let mut out = Vec::with_capacity(n * n * n);
        for b in 0..n {
            for g in 0..n {
                for r in 0..n {
                    out.push(self.apply([at(r), at(g), at(b)]));
                }
            }
        }
        out
    }

    fn atlas<T: Copy>(&self, convert: impl Fn(f32) -> T, one: T) -> (u32, u32, Vec<T>) {
        let n = self.size as usize;
        let grid = self.unit_grid();
        let (w, h) = (n * n, n);
        let mut out = Vec::with_capacity(w * h * 4);
        for g in 0..n {
            for b in 0..n {
                for r in 0..n {
                    let v = grid[(b * n + g) * n + r];
                    out.extend([convert(v[0]), convert(v[1]), convert(v[2]), one]);
                }
            }
        }
        (w as u32, h as u32, out)
    }

    /// The LUT as the shaders sample it: an N²×N RGBA image with blue
    /// slices side by side (x = blue × N + red, y = green), 8 bits.
    pub fn atlas_rgba8(&self) -> (u32, u32, Vec<u8>) {
        self.atlas(|v| (v.clamp(0.0, 1.0) * 255.0).round() as u8, 255)
    }

    /// [`atlas_rgba8`](Self::atlas_rgba8) as half floats (values outside
    /// 0–1 kept), for an RGBA16F texture.
    pub fn atlas_rgba16f(&self) -> (u32, u32, Vec<u16>) {
        self.atlas(f32_to_f16, f32_to_f16(1.0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: [f32; 3], b: [f32; 3], tol: f32) -> bool {
        (0..3).all(|c| (a[c] - b[c]).abs() <= tol)
    }

    #[test]
    fn identity_is_exact_and_trilinear_is_accurate() {
        let lut = Lut3d::identity(17);
        for v in [[0.0, 0.5, 1.0], [0.123, 0.456, 0.789], [0.999, 0.001, 0.333]] {
            assert!(close(lut.apply(v), v, 1e-5), "{v:?}");
        }
        // A smooth non-linear curve at a 33-point grid: trilinear error is
        // the curve's, and between grid points the interpolation is exact
        // on a linear one.
        let gamma = Lut3d::per_channel(33, |x| x.powf(1.0 / 2.4));
        let on_grid = gamma.apply([16.0 / 32.0; 3]);
        assert!((on_grid[0] - (0.5f32).powf(1.0 / 2.4)).abs() < 1e-5);
        let ramp = Lut3d::per_channel(5, |x| 0.25 + 0.5 * x);
        assert!(close(ramp.apply([0.3, 0.6, 0.9]), [0.4, 0.55, 0.7], 1e-5));
    }

    #[test]
    fn cube_files_round_trip_with_title_and_domain() {
        let mut lut = Lut3d::per_channel(9, |x| x * x);
        lut.title = Some("A curve".into());
        let back = parse_cube(&lut.to_cube()).unwrap();
        assert_eq!((back.size, back.title.as_deref()), (9, Some("A curve")));
        assert!(back.data.iter().zip(&lut.data).all(|(a, b)| close(*a, *b, 1e-5)));
        // A wider domain maps inputs onto the grid.
        let text = "# comment\nDOMAIN_MIN 0 0 0\nDOMAIN_MAX 2 2 2\nLUT_3D_SIZE 2\n0 0 0\n1 0 0\n0 1 0\n1 1 0\n0 0 1\n1 0 1\n0 1 1\n1 1 1\n";
        let wide = parse_cube(text).unwrap();
        assert!(close(wide.apply([1.0, 1.0, 1.0]), [0.5, 0.5, 0.5], 1e-6));
        assert!(close(wide.apply([2.0, 0.0, 4.0]), [1.0, 0.0, 1.0], 1e-6));
        let again = parse_cube(&wide.to_cube()).unwrap();
        assert_eq!((again.domain_min, again.domain_max), ([0.0; 3], [2.0; 3]));
    }

    #[test]
    fn malformed_files_are_refused() {
        assert!(parse_cube("LUT_3D_SIZE 2\n0 0 0\n").is_err());
        assert!(parse_cube("LUT_1D_SIZE 16\n").is_err());
        assert!(parse_cube("LUT_3D_SIZE 2\n0 0\n").is_err());
        assert!(parse_cube("LUT_3D_SIZE x\n").is_err());
        assert!(parse_cube("DOMAIN_MIN 1 1 1\nDOMAIN_MAX 0 0 0\nLUT_3D_SIZE 2\n").is_err());
        assert!(parse_cube("").is_err());
        assert!(parse_3dl("").is_err());
        assert!(parse_3dl("0 1023\n0 0 0\n").is_err());
    }

    #[test]
    fn one_d_cubes_become_3d() {
        let lut = parse_cube("LUT_1D_SIZE 3\n0 0 0\n0.25 0.5 0.75\n1 1 1\n").unwrap();
        assert_eq!(lut.size, 3);
        assert!(close(lut.apply([0.5; 3]), [0.25, 0.5, 0.75], 1e-3));
    }

    #[test]
    fn three_dl_reads_blue_fastest_in_its_own_depth() {
        let mut text = String::from("0 1023\n");
        for r in [0, 1023] {
            for g in [0, 1023] {
                for b in [0, 1023] {
                    text.push_str(&format!("{b} {g} {r}\n"));
                }
            }
        }
        let lut = parse_by_name("Look.3DL", &text).unwrap();
        let out = lut.apply([1.0, 0.0, 0.0]);
        assert!((out[2] - 1.0).abs() < 1e-6 && out[0].abs() < 1e-6, "{out:?}");
    }

    #[test]
    fn the_atlas_puts_blue_slices_side_by_side() {
        // A LUT whose entry is its own grid position: atlas pixel (x, y)
        // holds red = x mod N, green = y, blue = x div N.
        let n = 4u32;
        let mut lut = Lut3d::identity(n);
        lut.data = (0..n * n * n)
            .map(|i| [(i % n) as f32 / 3.0, (i / n % n) as f32 / 3.0, (i / (n * n)) as f32 / 3.0])
            .collect();
        let (w, h, px) = lut.atlas_rgba8();
        assert_eq!((w, h, px.len()), (16, 4, 16 * 4 * 4));
        for y in 0..h {
            for x in 0..w {
                let p = &px[((y * w + x) * 4) as usize..][..4];
                let want = [(x % n) as f32 / 3.0, y as f32 / 3.0, (x / n) as f32 / 3.0];
                for c in 0..3 {
                    assert_eq!(p[c], (want[c] * 255.0).round() as u8, "({x}, {y})");
                }
                assert_eq!(p[3], 255);
            }
        }
        let (_, _, half) = lut.atlas_rgba16f();
        assert_eq!(half[3], 0x3c00); // 1.0
        assert_eq!(f32_to_f16(0.5), 0x3800);
        assert_eq!(f32_to_f16(-2.0), 0xc000);
        assert_eq!(f32_to_f16(65504.0), 0x7bff);
    }
}
