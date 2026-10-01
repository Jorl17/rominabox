//! The Windows kit we compile on Windows, for Windows.

use super::{
    kit_base, PLAYER_ICON_COLOUR, PLAYER_ICON_SCRIPT, PLAYER_ICON_SIZE, PROGRAM_THAT_ENDS, STAND_IN_CORE,
    WINDOWS_JOYPAD_DRIVERS,
};
use std::{fs, path::Path, process::Command};

/// Compiles `source` into the library `output`, linked with `with`.
pub fn library(output: &Path, source: &str, with: &[&Path]) {
    let file = output.with_extension("c");
    fs::write(&file, source).unwrap();
    let status = Command::new("cc")
        .args(["-shared", "-O2", "-o"])
        .arg(output)
        .arg(&file)
        .args(with)
        .status()
        .expect("the toolchain's cc runs");
    assert!(status.success(), "could not compile {}", output.display());
}

/// An `.ico` of one `size`-pixel square of `colour`.
pub fn icon_of(colour: [u8; 4], size: u32) -> Vec<u8> {
    let pixels = image::RgbaImage::from_pixel(size, size, image::Rgba(colour));
    let frame = image::codecs::ico::IcoFrame::as_png(
        pixels.as_raw(),
        size,
        size,
        image::ExtendedColorType::Rgba8,
    )
    .unwrap();
    let mut bytes = Vec::new();
    image::codecs::ico::IcoEncoder::new(&mut bytes)
        .encode_images(&[frame])
        .unwrap();
    bytes
}

/// Compiles a program that only ends, with the resources of `script` when
/// there is one.
pub fn program(output: &Path, script: Option<&str>) {
    program_from(output, PROGRAM_THAT_ENDS, script);
}

/// Compiles the program `code`, with the resources of `script` when there is
/// one.
pub fn program_from(output: &Path, code: &str, script: Option<&str>) {
    let source = output.with_extension("c");
    fs::write(&source, code).unwrap();
    let mut compile = Command::new("cc");
    compile.args(["-O2", "-o"]).arg(output).arg(&source);
    if let Some(script) = script {
        let rc = output.with_extension("rc");
        let object = output.with_extension("res.o");
        fs::write(&rc, script).unwrap();
        let status = Command::new("windres")
            .arg(&rc)
            .arg("-o")
            .arg(&object)
            .current_dir(output.parent().unwrap())
            .status()
            .expect("the toolchain's windres runs");
        assert!(status.success(), "could not compile {}", rc.display());
        compile.arg(&object);
    }
    assert!(compile.status().unwrap().success(), "could not compile {}", output.display());
}

/// A Windows kit, with a player that has an icon as group 1, as in
/// RetroArch, a launcher with no resources, one controller profile per
/// driver folder, and the Mega Drive core.
pub fn windows_kit(root: &Path) -> std::path::PathBuf {
    let kit = kit_base(root);
    fs::write(kit.join("bin/retroarch.ico"), icon_of(PLAYER_ICON_COLOUR, PLAYER_ICON_SIZE)).unwrap();
    program(&kit.join("bin/retroarch.exe"), Some(PLAYER_ICON_SCRIPT));
    program(&kit.join("bin/launcher.exe"), None);
    for driver in WINDOWS_JOYPAD_DRIVERS {
        fs::create_dir_all(kit.join("autoconfig").join(driver)).unwrap();
        fs::write(kit.join("autoconfig").join(driver).join("pad.cfg"), driver).unwrap();
    }
    library(
        &kit.join("cores/genesis_plus_gx_libretro.dll"),
        STAND_IN_CORE,
        &[],
    );
    kit
}
