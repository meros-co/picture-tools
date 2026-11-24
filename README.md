# picture-tools

Picture tools for video apps, in one place for every platform:

- **3D LUTs:** `.cube` (3D and 1D, any domain) and `.3dl` parsing,
  trilinear lookup, `.cube` output, and the 2D atlas the shaders sample.
- **False color, zebras and focus peaking:** one shader source, generated
  for Flutter, OpenGL ES 3.0, Metal and WebGL 1, with a CPU reference
  (`tools::apply_cpu`) it's tested against.
- **Scopes:** waveform, RGB parade, vectorscope and histogram from any frame
  (RGBA, BGRA, UYVY, NV12), as intensity grids an app draws.

Presets keep the looks of the apps that use it: a camera app's
(`ScopeOptions::camera()`, `FALSE_COLOR_CAMERA`, still zebras) and a
production controller's (`ScopeOptions::CONTROLLER`, `FALSE_COLOR_CONTROLLER` with
grayscale outside the bands, animated zebras). A monitor app picks its own.

## Layout

| Path | What |
|---|---|
| `crates/picture-tools/` | The Rust crate `meros-picture-tools`: `lut`, `tools`, `scopes`, `shaders` (the generated sources as strings), and the C ABI (feature `capi`) |
| `crates/picture-tools-gen/` | The shader generator (`cargo run -p picture-tools-gen`) |
| `shaders/picture_tools.glsl.in` | The one shader source |
| `shaders/gen/` | Generated, committed: `flutter/`, `gles3/`, `metal/`, `webgl1/` |
| `shaders/uniforms.json` | Each uniform's name, type, unit, meaning and order |
| `flutter/` | Flutter package `meros_picture_tools`: `PictureToolsFilter`, `ScopeView` |
| `swift/` | Swift package `MerosPictureTools` (iOS 15, tvOS 15, macOS 12): `PictureToolsRenderer` (Metal), `ScopeView` (SwiftUI) |
| `include/meros_picture_tools.h` | The C ABI's header (cbindgen; `scripts/gen-header.sh`) |

## Rust

```toml
[dependencies]
meros-picture-tools = { git = "https://github.com/meros-co/picture-tools" }
```

```rust
use meros_picture_tools::{lut, scopes, tools};

let look = lut::parse_by_name("look.cube", &text)?;          // .cube or .3dl
let (w, h, atlas) = look.atlas_rgba8();                        // for the shader's u_lut_tex

let params = tools::PictureToolsParams {
    false_color: Some(tools::FalseColor { bands: tools::FALSE_COLOR_CAMERA.to_vec(), outside: tools::FalseColorOutside::Grayscale }),
    zebras: vec![tools::Zebra { level: 0.95, band: None }],
    ..Default::default()
};
let floats: Vec<f32> = tools::uniforms_for(&params, [1920.0, 1080.0], look.size, time_s)
    .into_iter().flat_map(|u| u.value).collect();               // bind in uniforms.json order

let o = scopes::ScopeOptions::CONTROLLER;
let pic = scopes::Picture::from_rgba(&rgba, 480, 270, 480 * 4, &o);
let waveform = scopes::compute(scopes::ScopeKind::Waveform, &pic, &o); // 256 × 256 intensities
```

`std` and `serde` only; builds for desktop, mobile, tvOS and
`wasm32-unknown-unknown` with Rust 1.89 or later.

## The shader

Every dialect binds the same uniforms (see `shaders/uniforms.json`): plain
`float`, `vec2` and `vec3`, fixed-size arrays, and two samplers, `u_input`
(the picture) and `u_lut_tex` (the LUT atlas, N²×N, blue slices side by
side). With `u_lut` at 0 the LUT is never sampled, so nothing needs binding
there (Metal still wants a texture in the slot; any will do). Coordinates
are the output's pixels from the top left in every dialect.

- **Flutter:** an image-filter shader (`ImageFilter.shader`; the engine sets
  `u_size` and `u_input`) or a `FragmentProgram`.
- **GLES 3.0:** the vertex stage passes the picture's coordinate as `v_uv`;
  for an external (camera) texture, put `#define MPT_EXTERNAL_INPUT` after
  the `#version` line.
- **Metal:** `picture_tools_vertex` (a full-screen triangle) and
  `picture_tools_fragment`; uniforms at buffer 0 as the packed floats
  `mpt_uniforms` returns, the picture at texture 0, the atlas at texture 1.
- **WebGL 1:** the picture uploaded top row first; a quad over a viewport of
  `u_size`.

Change `shaders/picture_tools.glsl.in` or the uniform table in
`crates/picture-tools/src/tools.rs`, then run `cargo run -p
picture-tools-gen`; a test fails while the generated files are stale.

## Flutter

```yaml
dependencies:
  meros_picture_tools:
    git:
      url: https://github.com/meros-co/picture-tools
      path: flutter
```

```dart
PictureToolsFilter(
  params: const PictureToolsParams(
    falseColor: FalseColor(falseColorCamera, outside: FalseColorOutside.grayscale),
    zebras: [Zebra(0.95)],
  ),
  lut: atlas, // LutAtlas.fromRgba8(...) from the Rust atlas, when params.lut
  child: Texture(textureId: id),
)
ScopeView(ScopeFrame(ScopeKind.waveform, 256, 256, bytes))
```

`PictureToolsFilter` is an image-filter shader, which needs Flutter's
Impeller renderer: the default on Android, iOS and macOS; on Windows and
Linux the app turns Impeller on. `PictureToolsParams.fromJson` reads the
Rust crate's JSON form.

## Swift

```swift
let renderer = try PictureToolsRenderer(device: device, pixelFormat: view.colorPixelFormat)
renderer.encode(into: encoder, input: texture, params: params, lut: lut, width: w, height: h, time: t)
ScopeView(ScopeFrame.compute(.vectorscope, format: .bgra, pixels: buf, width: w, height: h, stride: s)!)
```

The app links a Rust library with this crate's `capi` feature (its own,
or the crate's static library); the package carries the header.

## C

`mpt_lut_parse`, `mpt_lut_atlas_rgba8`, `mpt_lut_free`, `mpt_uniforms`,
`mpt_scope`, `mpt_shader_source` and `mpt_string_free`; see
`include/meros_picture_tools.h`.

## Tests

- `cargo test`: LUTs (against a camera app's LUT code, on fixtures),
  tools (`apply_cpu`: false-color edges, zebras, peaking, ranges and
  weights), scopes (byte for byte against a production controller's scope
  code and its goldens; pixel formats; vectorscope targets), the C ABI, and
  that the generated files are current.
- `tests/browser_shaders.rs`: the WebGL 1 and GLES 3 shaders (GLES 3 as
  WebGL 2) in headless Chromium against `apply_cpu` within ±2 code values,
  with and without a LUT texture, and the WebGL 1 shader with the
  controller's presets against the controller's own shader. Skipped without
  Chrome, Edge or Chromium (or `PICTURE_TOOLS_BROWSER`).
- `cd flutter && flutter test`: the Flutter shader against `apply_cpu`
  (cases the crate generates), the uniform packing against the crate's, and
  `ScopeView` goldens.
- `cd swift && swift test` (on a Mac, with the crate built for it): the
  Metal shader in XCTest, and the C ABI from Swift.
- `cargo deny check`; CI also builds with Rust 1.89 and for
  `wasm32-unknown-unknown`.

## License

MIT; see `LICENSE`.
