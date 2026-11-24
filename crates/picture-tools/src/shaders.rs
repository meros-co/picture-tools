//! The generated shaders (shaders/gen/) and their uniform table, as text.
//! Every dialect binds the same uniforms (shaders/uniforms.json): plain
//! floats and vectors and two samplers, with no uniform blocks, so WebGL 1
//! works.

/// Flutter runtime effect (an `ImageFilter.shader` or `FragmentProgram`).
pub const FLUTTER_FRAGMENT: &str = include_str!("../../../shaders/gen/flutter/picture_tools.frag");
/// OpenGL ES 3.0 fragment shader.
pub const GLES3_FRAGMENT: &str = include_str!("../../../shaders/gen/gles3/picture_tools.frag");
/// Metal: `picture_tools_vertex` and `picture_tools_fragment`.
pub const METAL_SOURCE: &str = include_str!("../../../shaders/gen/metal/picture_tools.metal");
/// WebGL 1 fragment shader.
pub const WEBGL1_FRAGMENT: &str = include_str!("../../../shaders/gen/webgl1/picture_tools.frag");
/// The uniforms' names, types, units and order.
pub const UNIFORMS_JSON: &str = include_str!("../../../shaders/uniforms.json");

#[cfg(test)]
mod tests {
    use crate::codegen::files;

    /// The committed generated files are what the generator makes now:
    /// run `cargo run -p picture-tools-gen` after changing the template or
    /// the uniform table.
    #[test]
    fn generated_files_are_up_to_date() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        for (path, want) in files() {
            let have = std::fs::read_to_string(root.join(path)).unwrap_or_default().replace("\r\n", "\n");
            assert!(have == want, "{path} is stale: run `cargo run -p picture-tools-gen`");
        }
    }

    #[test]
    fn every_dialect_declares_every_uniform() {
        for src in [super::FLUTTER_FRAGMENT, super::GLES3_FRAGMENT, super::METAL_SOURCE, super::WEBGL1_FRAGMENT] {
            for u in crate::tools::UNIFORMS {
                assert!(src.contains(u.name), "{} missing", u.name);
            }
            assert!(!src.contains("MPT_ARGS") && !src.contains("MPT_PASS") && !src.contains("SAMPLE_"));
        }
        assert!(!super::WEBGL1_FRAGMENT.contains("texture("));
        assert!(!super::METAL_SOURCE.contains("vec3") && !super::METAL_SOURCE.contains("U."));
    }
}
