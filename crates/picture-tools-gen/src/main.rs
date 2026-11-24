//! Writes shaders/gen/, shaders/uniforms.json and the packages' shader
//! copies from shaders/picture_tools.glsl.in and the uniform table.
//!
//!     cargo run -p picture-tools-gen

use std::path::Path;

fn main() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    for (path, contents) in meros_picture_tools::codegen::files() {
        let file = root.join(path);
        if let Some(dir) = file.parent() {
            std::fs::create_dir_all(dir).expect("create the directory");
        }
        let changed = std::fs::read_to_string(&file).map_or(true, |old| old != contents);
        if changed {
            std::fs::write(&file, contents).expect("write the file");
            println!("wrote {path}");
        }
    }
}
