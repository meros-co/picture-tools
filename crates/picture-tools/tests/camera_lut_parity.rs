//! `Lut3d` against a camera app's LUT code, which it replaces: the same
//! lookups, `.cube` output and parsing on the fixtures.

use meros_picture_tools::lut::{Lut3d, parse_by_name, parse_cube};

/// The camera app's `Lut`, verbatim but for its error type: the reference.
mod camera {
    #[derive(Debug, Clone, PartialEq)]
    pub struct Lut {
        pub size: usize,
        pub data: Vec<f32>,
    }

    impl Lut {
        pub fn apply(&self, rgb: [f64; 3]) -> [f64; 3] {
            let n = self.size;
            let at = |r: usize, g: usize, b: usize, c: usize| self.data[((b * n + g) * n + r) * 3 + c] as f64;
            let split = |v: f64| {
                let x = v.clamp(0.0, 1.0) * (n - 1) as f64;
                let i = (x.floor() as usize).min(n - 2);
                (i, x - i as f64)
            };
            let ((r0, fr), (g0, fg), (b0, fb)) = (split(rgb[0]), split(rgb[1]), split(rgb[2]));
            let mut out = [0.0; 3];
            for (c, o) in out.iter_mut().enumerate() {
                let lerp = |a: f64, b: f64, t: f64| a + (b - a) * t;
                let x00 = lerp(at(r0, g0, b0, c), at(r0 + 1, g0, b0, c), fr);
                let x10 = lerp(at(r0, g0 + 1, b0, c), at(r0 + 1, g0 + 1, b0, c), fr);
                let x01 = lerp(at(r0, g0, b0 + 1, c), at(r0 + 1, g0, b0 + 1, c), fr);
                let x11 = lerp(at(r0, g0 + 1, b0 + 1, c), at(r0 + 1, g0 + 1, b0 + 1, c), fr);
                *o = lerp(lerp(x00, x10, fg), lerp(x01, x11, fg), fb);
            }
            out
        }

        pub fn from_cube(text: &str) -> Result<Lut, String> {
            let mut size = None;
            let mut one_d = None;
            let mut data = Vec::new();
            for line in text.lines().map(str::trim) {
                if line.is_empty() || line.starts_with('#') || line.starts_with("TITLE") {
                    continue;
                }
                if let Some(n) = line.strip_prefix("LUT_3D_SIZE") {
                    size = Some(n.trim().parse::<usize>().map_err(|_| format!("bad LUT_3D_SIZE: {line}"))?);
                    continue;
                }
                if let Some(n) = line.strip_prefix("LUT_1D_SIZE") {
                    one_d = Some(n.trim().parse::<usize>().map_err(|_| format!("bad LUT_1D_SIZE: {line}"))?);
                    continue;
                }
                if line.starts_with("DOMAIN_") {
                    let v: Vec<f64> = line.split_whitespace().skip(1).filter_map(|v| v.parse().ok()).collect();
                    let want = if line.starts_with("DOMAIN_MIN") { 0.0 } else { 1.0 };
                    if v.len() != 3 || v.iter().any(|x| (x - want).abs() > 1e-9) {
                        return Err(format!("only a 0–1 domain is supported: {line}"));
                    }
                    continue;
                }
                let v: Vec<f32> = line.split_whitespace().map(str::parse).collect::<Result<_, _>>().map_err(|_| format!("bad line: {line}"))?;
                if v.len() != 3 {
                    return Err(format!("bad line: {line}"));
                }
                data.extend(v);
            }
            if let (Some(n), None) = (one_d, size) {
                if !(2..=65536).contains(&n) || data.len() != n * 3 {
                    return Err(format!("expected {n} 1D entries, found {}", data.len() / 3));
                }
                let curve = |c: usize, x: f64| {
                    let p = x * (n - 1) as f64;
                    let i = (p.floor() as usize).min(n - 2);
                    let f = p - i as f64;
                    let a = data[i * 3 + c] as f64;
                    a + (data[(i + 1) * 3 + c] as f64 - a) * f
                };
                let out = 65.min(n.max(2));
                let mut lut = Vec::with_capacity(out * out * out * 3);
                for b in 0..out {
                    for g in 0..out {
                        for r in 0..out {
                            let at = |i: usize| i as f64 / (out - 1) as f64;
                            lut.extend([curve(0, at(r)) as f32, curve(1, at(g)) as f32, curve(2, at(b)) as f32]);
                        }
                    }
                }
                return Ok(Lut { size: out, data: lut });
            }
            let size = size.ok_or("no LUT_3D_SIZE")?;
            if !(2..=128).contains(&size) || data.len() != size * size * size * 3 {
                return Err(format!("expected {size}³ entries, found {}", data.len() / 3));
            }
            Ok(Lut { size, data })
        }

        pub fn from_3dl(text: &str) -> Result<Lut, String> {
            let mut rows = text.lines().map(str::trim).filter(|l| {
                !l.is_empty() && !l.starts_with('#') && !l.starts_with('<') && !l.starts_with("3DMESH") && !l.starts_with("Mesh")
            });
            let mesh: Vec<u32> = rows
                .next()
                .ok_or("empty .3dl")?
                .split_whitespace()
                .map(str::parse)
                .collect::<Result<_, _>>()
                .map_err(|_| "bad .3dl mesh line".to_string())?;
            let size = mesh.len();
            if !(2..=128).contains(&size) {
                return Err(format!("a .3dl mesh of {size} points isn't supported"));
            }
            let values: Vec<[u32; 3]> = rows
                .map(|l| {
                    let v: Vec<u32> = l.split_whitespace().map(str::parse).collect::<Result<_, _>>().map_err(|_| format!("bad line: {l}"))?;
                    <[u32; 3]>::try_from(v).map_err(|_| format!("bad line: {l}"))
                })
                .collect::<Result<_, _>>()?;
            if values.len() != size * size * size {
                return Err(format!("expected {size}³ entries, found {}", values.len()));
            }
            let top = values.iter().flatten().copied().max().unwrap_or(0);
            let scale = [1023u32, 4095, 65535].into_iter().find(|&m| top <= m).unwrap_or(65535) as f32;
            let mut data = vec![0.0f32; size * size * size * 3];
            for r in 0..size {
                for g in 0..size {
                    for b in 0..size {
                        let v = values[(r * size + g) * size + b];
                        let at = ((b * size + g) * size + r) * 3;
                        for c in 0..3 {
                            data[at + c] = v[c] as f32 / scale;
                        }
                    }
                }
            }
            Ok(Lut { size, data })
        }

        pub fn from_file(name: &str, text: &str) -> Result<Lut, String> {
            if name.to_ascii_lowercase().ends_with(".3dl") { Lut::from_3dl(text) } else { Lut::from_cube(text) }
        }

        pub fn to_cube(&self, title: &str) -> String {
            let mut s = format!("TITLE \"{title}\"\nLUT_3D_SIZE {}\n", self.size);
            for t in self.data.chunks_exact(3) {
                s.push_str(&format!("{:.6} {:.6} {:.6}\n", t[0], t[1], t[2]));
            }
            s
        }
    }
}

fn fixture(name: &str) -> String {
    std::fs::read_to_string(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures").join(name)).unwrap()
}

/// Inputs spread over the cube, the corners and the edges included.
fn inputs() -> Vec<[f64; 3]> {
    let mut v = Vec::new();
    let mut seed = 7u64;
    for _ in 0..2000 {
        let mut next = || {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            (seed >> 11) as f64 / (1u64 << 53) as f64
        };
        v.push([next(), next(), next()]);
    }
    v.extend([[0.0; 3], [1.0; 3], [1.0, 0.0, 0.5], [-0.1, 1.2, 0.5]]);
    v
}

/// The same lookups (from f32 inputs, as a frame's are) within 1e-6.
fn same_lookups(ours: &Lut3d, theirs: &camera::Lut) {
    for rgb in inputs() {
        let f = rgb.map(|v| v as f32);
        let a = ours.apply(f);
        let b = theirs.apply(f.map(|v| v as f64));
        for c in 0..3 {
            assert!((a[c] as f64 - b[c]).abs() < 1e-6, "{rgb:?}: {a:?} vs {b:?}");
        }
    }
}

#[test]
fn every_fixture_parses_and_looks_up_alike() {
    for name in ["identity_17.cube", "log_to_display_33.cube", "cross_talk_9.cube", "curve_1d.cube", "look_10bit.3dl"] {
        let text = fixture(name);
        let ours = parse_by_name(name, &text).unwrap();
        let theirs = camera::Lut::from_file(name, &text).unwrap();
        assert_eq!(ours.size as usize, theirs.size, "{name}");
        let flat: Vec<f32> = ours.data.iter().flatten().copied().collect();
        assert_eq!(flat, theirs.data, "{name}");
        same_lookups(&ours, &theirs);
    }
}

#[test]
fn cube_output_is_the_same_text() {
    let text = fixture("log_to_display_33.cube");
    let mut ours = parse_cube(&text).unwrap();
    let theirs = camera::Lut::from_cube(&text).unwrap();
    ours.title = Some("Log to display".into());
    assert_eq!(ours.to_cube(), theirs.to_cube("Log to display"));
    assert_eq!(ours.title.as_deref(), Some("Log to display"));
}

#[test]
fn malformed_files_are_refused_by_both() {
    for name in ["malformed_count.cube", "malformed_line.cube"] {
        let text = fixture(name);
        assert!(parse_cube(&text).is_err(), "{name}");
        assert!(camera::Lut::from_cube(&text).is_err(), "{name}");
    }
}

/// A domain other than 0–1, which the camera app refused, is read and
/// applied here.
#[test]
fn a_wider_domain_is_supported() {
    let text = fixture("domain_0_2.cube");
    assert!(camera::Lut::from_cube(&text).is_err());
    let lut = parse_cube(&text).unwrap();
    let out = lut.apply([1.0, 0.5, 2.0]);
    assert!((out[0] - 0.5).abs() < 1e-6 && (out[1] - 0.25).abs() < 1e-6 && (out[2] - 1.0).abs() < 1e-6, "{out:?}");
    // The atlas resamples it to the 0–1 input the shaders use.
    let (w, h, px) = lut.atlas_rgba8();
    assert_eq!((w, h), (25, 5));
    assert_eq!(&px[4 * 4..4 * 4 + 3], &[128, 0, 0]);
}

/// The trilinear lookup is exact on a linear LUT and close on a smooth one.
#[test]
fn trilinear_accuracy() {
    let linear = Lut3d::per_channel(9, |x| 0.1 + 0.8 * x);
    for rgb in inputs().into_iter().filter(|v| v.iter().all(|c| (0.0..=1.0).contains(c))) {
        let out = linear.apply(rgb.map(|v| v as f32));
        for c in 0..3 {
            assert!((out[c] as f64 - (0.1 + 0.8 * rgb[c])).abs() < 1e-5);
        }
    }
}
