use std::fs;
use std::path::Path;

fn main() {
    shader_previews();
    tauri_build::build()
}

/// The shader previews we include in the exporter, one for each picture in
/// integrations/shaders/previews, named for the shader it shows. To add a
/// preset to the catalogue with its picture, you need no line of code here.
fn shader_previews() {
    let folder = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../integrations/shaders/previews");
    println!("cargo:rerun-if-changed={}", folder.display());
    let mut pictures: Vec<_> = fs::read_dir(&folder)
        .expect("the shader previews folder")
        .map(|entry| entry.expect("a shader preview").path())
        .filter(|path| path.extension().is_some_and(|extension| extension == "png"))
        .collect();
    pictures.sort();
    let entries: String = pictures
        .iter()
        .map(|path| {
            let id = path.file_stem().and_then(|stem| stem.to_str()).expect("a preview named for its shader");
            let path = fs::canonicalize(path).expect("a shader preview's path");
            format!("    ({id:?}, include_bytes!({path:?})),\n")
        })
        .collect();
    let out = std::env::var("OUT_DIR").expect("Cargo's OUT_DIR");
    fs::write(Path::new(&out).join("shader_previews.rs"), format!("&[\n{entries}]\n"))
        .expect("the shader preview table");
}
