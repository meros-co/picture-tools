//! The WebGL 1 and GLES 3 shaders (GLES 3 as WebGL 2, the same GLSL ES
//! 3.00) run in headless Chromium against `apply_cpu`, and the WebGL 1
//! shader with a production controller's presets against that controller's
//! own shader. Needs a Chromium-based browser: `PICTURE_TOOLS_BROWSER`, or
//! Chrome, Edge or Chromium where they normally install; skipped without one.

use std::path::PathBuf;
use std::process::Command;

use meros_picture_tools::lut::Lut3d;
use meros_picture_tools::shaders::{GLES3_FRAGMENT, WEBGL1_FRAGMENT};
use meros_picture_tools::tools::*;

const W: u32 = 96;
const H: u32 = 64;

fn browser() -> Option<PathBuf> {
    if let Ok(p) = std::env::var("PICTURE_TOOLS_BROWSER") {
        return Some(p.into());
    }
    [
        r"C:\Program Files\Google\Chrome\Application\chrome.exe",
        r"C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe",
        "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
        "/usr/bin/chromium",
        "/usr/bin/chromium-browser",
        "/usr/bin/google-chrome",
    ]
    .iter()
    .map(PathBuf::from)
    .find(|p| p.exists())
}

/// A test picture: a luma ramp, 75% bars, a ramp into clipping, a hard
/// edge, and flat mid grays.
fn picture() -> Vec<u8> {
    let mut px = Vec::with_capacity((W * H * 4) as usize);
    let bars: [[u8; 3]; 8] = [[191, 191, 191], [191, 191, 0], [0, 191, 191], [0, 191, 0], [191, 0, 191], [191, 0, 0], [0, 0, 191], [0, 0, 0]];
    for y in 0..H {
        for x in 0..W {
            let c = match y * 6 / H {
                0 => [(x * 255 / (W - 1)) as u8; 3],
                1 => bars[(x * 8 / W) as usize],
                2 => [(230 + x * 25 / (W - 1)) as u8; 3],
                3 => if x < W / 2 { [10, 10, 10] } else { [240, 240, 240] },
                4 => [(x * 2) as u8, 120, (255 - x * 2) as u8],
                _ => if x < W / 2 { [102, 102, 102] } else { [138, 138, 138] },
            };
            px.extend([c[0], c[1], c[2], 255]);
        }
    }
    px
}

fn b64(data: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for chunk in data.chunks(3) {
        let b = [chunk[0], *chunk.get(1).unwrap_or(&0), *chunk.get(2).unwrap_or(&0)];
        let n = (b[0] as u32) << 16 | (b[1] as u32) << 8 | b[2] as u32;
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(T[(n >> (18 - 6 * i) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

struct Case {
    name: &'static str,
    params: PictureToolsParams,
    lut: Option<Lut3d>,
    time: f32,
}

fn cases() -> Vec<Case> {
    let base = PictureToolsParams::default();
    let fc = |bands: &[FalseColorBand], outside| PictureToolsParams {
        false_color: Some(FalseColor { bands: bands.to_vec(), outside }),
        ..Default::default()
    };
    let cross = Lut3d {
        data: Lut3d::identity(9)
            .data
            .iter()
            .map(|c| [0.8 * c[0] + 0.2 * c[1], 0.1 * c[0] + 0.8 * c[1] + 0.1 * c[2], (0.3 * c[2] + 0.7 * c[0] * c[2]).powf(0.8)])
            // On 8-bit steps, as the atlas holds them: otherwise half a code
            // of rounding moves a pixel across a false-color band edge.
            .map(|c: [f32; 3]| c.map(|v| (v * 255.0).round() / 255.0))
            .collect(),
        ..Lut3d::identity(9)
    };
    vec![
        Case { name: "plain", params: base.clone(), lut: None, time: 0.0 },
        Case { name: "false color, controller", params: fc(FALSE_COLOR_CONTROLLER, FalseColorOutside::Grayscale), lut: None, time: 0.0 },
        Case { name: "false color, camera", params: fc(FALSE_COLOR_CAMERA, FalseColorOutside::Picture), lut: None, time: 0.0 },
        Case {
            name: "two still zebras",
            params: PictureToolsParams {
                zebras: vec![Zebra { level: 0.9, band: None }, Zebra { level: 0.5, band: Some(0.025) }],
                ..Default::default()
            },
            lut: None,
            time: 0.0,
        },
        Case {
            name: "animated zebra",
            params: PictureToolsParams {
                zebras: vec![Zebra { level: 0.95, band: None }],
                zebra_style: ZebraStyle::Animated { stripe_px: 8.0, speed_px_per_s: 40.0 },
                ..Default::default()
            },
            lut: None,
            time: 1.25,
        },
        Case {
            name: "peaking",
            params: PictureToolsParams { peaking: Some(Peaking { threshold: 0.15, color: [1.0, 0.0, 0.0] }), ..Default::default() },
            lut: None,
            time: 0.0,
        },
        Case { name: "LUT", params: PictureToolsParams { lut: true, ..Default::default() }, lut: Some(cross.clone()), time: 0.0 },
        Case {
            name: "LUT, false color and peaking",
            params: PictureToolsParams {
                lut: true,
                peaking: Some(Peaking { threshold: 0.2, color: [0.1, 1.0, 0.1] }),
                ..fc(FALSE_COLOR_CAMERA, FalseColorOutside::Grayscale)
            },
            lut: Some(cross),
            time: 0.0,
        },
        Case {
            name: "limited range, BT.601",
            params: PictureToolsParams {
                range: Range::Limited,
                luma: LumaWeights::Bt601,
                zebras: vec![Zebra { level: 0.98, band: None }],
                ..Default::default()
            },
            lut: None,
            time: 0.0,
        },
    ]
}

/// The production controller's WebGL shader as it was, for comparison.
const CONTROLLER_FRAGMENT: &str = "
precision mediump float;
uniform sampler2D picture;
uniform int mode;
uniform float zebra;
uniform float time;
varying vec2 at;
void main() {
  vec3 c = texture2D(picture, at).rgb;
  float y = dot(c, vec3(0.2126, 0.7152, 0.0722)) * 100.0;
  if (mode == 1) {
    float stripe = mod(gl_FragCoord.x + gl_FragCoord.y + time * 40.0, 16.0);
    if (y >= zebra && stripe < 8.0) c = vec3(0.06);
    gl_FragColor = vec4(c, 1.0);
  } else if (mode == 2) {
    vec3 grey = vec3(y / 100.0);
    vec3 o = grey;
    if (y < 2.5) o = vec3(0.55, 0.0, 0.7);
    else if (y < 10.0) o = vec3(0.1, 0.25, 1.0);
    else if (y >= 38.0 && y < 42.0) o = vec3(0.1, 0.8, 0.2);
    else if (y >= 52.0 && y < 56.0) o = vec3(1.0, 0.55, 0.75);
    else if (y >= 97.0 && y < 99.0) o = vec3(1.0, 0.95, 0.1);
    else if (y >= 99.0) o = vec3(1.0, 0.1, 0.1);
    gl_FragColor = vec4(o, 1.0);
  } else {
    gl_FragColor = vec4(c, 1.0);
  }
}";

const CONTROLLER_VERTEX: &str = "
attribute vec2 corner;
varying vec2 at;
void main() {
  at = vec2((corner.x + 1.0) / 2.0, (1.0 - corner.y) / 2.0);
  gl_Position = vec4(corner, 0.0, 1.0);
}";

const HARNESS: &str = r#"<!doctype html><meta charset="utf-8"><body><script>
const D = DATA;
function bytes(s) { const b = atob(s); const a = new Uint8Array(b.length); for (let i = 0; i < b.length; i++) a[i] = b.charCodeAt(i); return a; }
function render(version, vs, fs, setup) {
  const c = document.createElement('canvas'); c.width = D.w; c.height = D.h;
  const gl = c.getContext(version, { premultipliedAlpha: false, preserveDrawingBuffer: true, antialias: false });
  if (!gl) throw new Error('no ' + version);
  const sh = (t, s) => { const x = gl.createShader(t); gl.shaderSource(x, s); gl.compileShader(x); if (!gl.getShaderParameter(x, gl.COMPILE_STATUS)) throw new Error(gl.getShaderInfoLog(x)); return x; };
  const p = gl.createProgram(); gl.attachShader(p, sh(gl.VERTEX_SHADER, vs)); gl.attachShader(p, sh(gl.FRAGMENT_SHADER, fs)); gl.linkProgram(p);
  if (!gl.getProgramParameter(p, gl.LINK_STATUS)) throw new Error(gl.getProgramInfoLog(p));
  gl.useProgram(p);
  const buf = gl.createBuffer(); gl.bindBuffer(gl.ARRAY_BUFFER, buf);
  gl.bufferData(gl.ARRAY_BUFFER, new Float32Array([-1, -1, 1, -1, -1, 1, 1, 1]), gl.STATIC_DRAW);
  const corner = gl.getAttribLocation(p, 'corner'); gl.enableVertexAttribArray(corner); gl.vertexAttribPointer(corner, 2, gl.FLOAT, false, 0, 0);
  const tex = (unit, w, h, px, linear) => { const t = gl.createTexture(); gl.activeTexture(gl.TEXTURE0 + unit); gl.bindTexture(gl.TEXTURE_2D, t);
    const f = linear ? gl.LINEAR : gl.NEAREST;
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MIN_FILTER, f); gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MAG_FILTER, f);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_S, gl.CLAMP_TO_EDGE); gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_T, gl.CLAMP_TO_EDGE);
    gl.pixelStorei(gl.UNPACK_ALIGNMENT, 1);
    gl.texImage2D(gl.TEXTURE_2D, 0, gl.RGBA, w, h, 0, gl.RGBA, gl.UNSIGNED_BYTE, px); };
  setup(gl, p, tex);
  gl.viewport(0, 0, D.w, D.h); gl.drawArrays(gl.TRIANGLE_STRIP, 0, 4);
  const out = new Uint8Array(D.w * D.h * 4); gl.readPixels(0, 0, D.w, D.h, gl.RGBA, gl.UNSIGNED_BYTE, out);
  // Bottom row first; turn it the right way up.
  const flipped = new Uint8Array(out.length);
  for (let y = 0; y < D.h; y++) flipped.set(out.subarray((D.h - 1 - y) * D.w * 4, (D.h - y) * D.w * 4), y * D.w * 4);
  return flipped;
}
function ours(dialect, k) {
  const vs1 = 'attribute vec2 corner; void main() { gl_Position = vec4(corner, 0.0, 1.0); }';
  const vs3 = '#version 300 es\nin vec2 corner; out vec2 v_uv; void main() { v_uv = vec2((corner.x + 1.0) / 2.0, (1.0 - corner.y) / 2.0); gl_Position = vec4(corner, 0.0, 1.0); }';
  return render(dialect == 'webgl1' ? 'webgl' : 'webgl2', dialect == 'webgl1' ? vs1 : vs3, dialect == 'webgl1' ? D.webgl1 : D.gles3, (gl, p, tex) => {
    tex(0, D.w, D.h, bytes(D.image), false);
    gl.uniform1i(gl.getUniformLocation(p, 'u_input'), 0);
    if (k.lut) { tex(1, k.lut.w, k.lut.h, bytes(k.lut.px), true); gl.uniform1i(gl.getUniformLocation(p, 'u_lut_tex'), 1); }
    for (const u of D.uniforms) {
      if (u.type == 'sampler2D') continue;
      const v = new Float32Array(k.uniforms.slice(u.offset, u.offset + u.size));
      const loc = gl.getUniformLocation(p, u.name);
      if (loc === null) continue;
      if (u.type == 'float') gl.uniform1fv(loc, v); else if (u.type == 'vec2') gl.uniform2fv(loc, v); else gl.uniform3fv(loc, v);
    }
  });
}
function controller(mode, zebra, time) {
  return render('webgl', D.controller_vs, D.controller_fs, (gl, p, tex) => {
    tex(0, D.w, D.h, bytes(D.image), false);
    gl.uniform1i(gl.getUniformLocation(p, 'picture'), 0);
    gl.uniform1i(gl.getUniformLocation(p, 'mode'), mode);
    gl.uniform1f(gl.getUniformLocation(p, 'zebra'), zebra);
    gl.uniform1f(gl.getUniformLocation(p, 'time'), time);
  });
}
function compare(a, b) {
  let max = 0, over = 0, at = -1;
  for (let i = 0; i < a.length; i++) { if (i % 4 == 3) continue; const d = Math.abs(a[i] - b[i]); if (d > 2) { over++; if (at < 0) at = i >> 2; } if (d > max) max = d; }
  return { max, over, at };
}
const results = [];
for (const k of D.cases) {
  for (const dialect of ['webgl1', 'gles3']) {
    try { results.push({ name: k.name + ' (' + dialect + ')', ...compare(ours(dialect, k), bytes(k.expected)) }); }
    catch (e) { results.push({ name: k.name + ' (' + dialect + ')', error: String(e) }); }
  }
}
for (const c of D.controller) {
  try { results.push({ name: 'controller: ' + c.name, ...compare(controller(c.mode, c.zebra, c.time), ours('webgl1', D.cases[c.case])) }); }
  catch (e) { results.push({ name: 'controller: ' + c.name, error: String(e) }); }
}
document.body.textContent = 'MPT-RESULT-BEGIN' + JSON.stringify(results) + 'MPT-RESULT-END';
</script>"#;

#[test]
fn shaders_match_the_cpu_reference_and_the_controller() {
    let Some(browser) = browser() else {
        eprintln!("no Chromium-based browser; skipped (set PICTURE_TOOLS_BROWSER)");
        return;
    };
    let image = picture();
    let uniform_table: serde_json::Value = serde_json::from_str(meros_picture_tools::shaders::UNIFORMS_JSON).unwrap();
    let uniforms: Vec<serde_json::Value> = uniform_table["uniforms"]
        .as_array()
        .unwrap()
        .iter()
        .map(|u| {
            let floats = match u["type"].as_str().unwrap() {
                "vec2" => 2,
                "vec3" => 3,
                "float" => 1,
                _ => 0,
            } * u["count"].as_u64().unwrap();
            serde_json::json!({ "name": u["name"], "type": u["type"], "offset": u["offset"], "size": floats })
        })
        .collect();
    let all = cases();
    let case_json: Vec<serde_json::Value> = all
        .iter()
        .map(|k| {
            let mut expected = image.clone();
            apply_cpu(&k.params, k.lut.as_ref(), &mut expected, W, H, k.time);
            let floats: Vec<f32> = uniforms_for(&k.params, [W as f32, H as f32], k.lut.as_ref().map_or(33, |l| l.size), k.time)
                .into_iter()
                .flat_map(|u| u.value)
                .collect();
            let lut = k.lut.as_ref().filter(|_| k.params.lut).map(|l| {
                let (w, h, px) = l.atlas_rgba8();
                serde_json::json!({ "w": w, "h": h, "px": b64(&px) })
            });
            serde_json::json!({ "name": k.name, "uniforms": floats, "lut": lut, "expected": b64(&expected) })
        })
        .collect();
    let data = serde_json::json!({
        "w": W,
        "h": H,
        "image": b64(&image),
        "webgl1": WEBGL1_FRAGMENT,
        "gles3": GLES3_FRAGMENT,
        "uniforms": uniforms,
        "cases": case_json,
        "controller_vs": CONTROLLER_VERTEX,
        "controller_fs": CONTROLLER_FRAGMENT,
        // Its zebra (mode 1, at 95%) and false color (mode 2) against ours
        // with its presets.
        "controller": [
            { "name": "zebra", "mode": 1, "zebra": 95.0, "time": 1.25, "case": 4 },
            { "name": "false color", "mode": 2, "zebra": 95.0, "time": 0.0, "case": 1 },
        ],
    });
    let dir = std::env::temp_dir().join(format!("picture-tools-browser-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let page = dir.join("harness.html");
    std::fs::write(&page, HARNESS.replace("DATA", &data.to_string())).unwrap();
    let url = format!("file:///{}", page.to_string_lossy().replace('\\', "/").trim_start_matches('/'));
    let out = Command::new(&browser)
        .args([
            "--headless=new",
            "--disable-gpu",
            "--use-angle=swiftshader",
            "--enable-unsafe-swiftshader",
            "--no-first-run",
            "--allow-file-access-from-files",
            "--virtual-time-budget=20000",
            &format!("--user-data-dir={}", dir.join("profile").to_string_lossy()),
            "--dump-dom",
            &url,
        ])
        .output()
        .expect("run the browser");
    let _ = std::fs::remove_dir_all(&dir);
    let dom = String::from_utf8_lossy(&out.stdout);
    let json = dom
        .split("MPT-RESULT-BEGIN")
        .nth(1)
        .and_then(|s| s.split("MPT-RESULT-END").next())
        .unwrap_or_else(|| panic!("no results from the browser: {dom} {}", String::from_utf8_lossy(&out.stderr)));
    let results: Vec<serde_json::Value> = serde_json::from_str(json).unwrap();
    let mut failed = Vec::new();
    for r in &results {
        eprintln!("{r}");
        if r.get("error").is_some() || r["over"].as_u64().unwrap_or(1) > 0 {
            failed.push(r["name"].to_string());
        }
    }
    assert!(failed.is_empty(), "beyond ±2 code values or failing: {failed:?}");
    assert_eq!(results.len(), all.len() * 2 + 2);
}
