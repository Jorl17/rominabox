//! A Windows kit made on macOS or Linux, compiled with zig.

use super::kit_base;
use std::{fs, path::Path};

/// Compiles `source` for Windows with zig, with `extra` (a resource script, a
/// flag) beside it.
fn for_windows(output: &Path, source: &str, extra: &[&str]) {
    let file = output.with_extension("c");
    fs::write(&file, source).unwrap();
    let status = std::process::Command::new("zig")
        .args(["cc", "-target", "x86_64-windows-gnu", "-O2", "-o"])
        .arg(output)
        .arg(&file)
        .args(extra)
        .current_dir(output.parent().unwrap())
        .status()
        .expect("zig builds Windows programs on this machine; install it (brew install zig)");
    assert!(status.success(), "could not compile {}", output.display());
}

/// A Windows kit like one from a Windows build, made on another machine. It
/// has a player with an icon as group 1, as in RetroArch, a launcher with no
/// resources, one controller profile per driver folder, and the Mega Drive
/// core, each a Windows program compiled with zig.
pub fn windows_kit_here(root: &Path) -> std::path::PathBuf {
    let kit = kit_base(root);
    image::RgbaImage::from_pixel(32, 32, image::Rgba([255, 0, 0, 255]))
        .save(kit.join("bin/retroarch.ico"))
        .unwrap();
    fs::write(kit.join("bin/retroarch.rc"), "1 ICON \"retroarch.ico\"\n").unwrap();
    const MAIN: &str = "int main(void) { return 0; }\n";
    for_windows(&kit.join("bin/retroarch.exe"), MAIN, &["retroarch.rc"]);
    for_windows(&kit.join("bin/launcher.exe"), MAIN, &[]);
    for driver in ["xinput", "dinput"] {
        fs::create_dir_all(kit.join("autoconfig").join(driver)).unwrap();
        fs::write(kit.join("autoconfig").join(driver).join("pad.cfg"), driver).unwrap();
    }
    for_windows(
        &kit.join("cores/genesis_plus_gx_libretro.dll"),
        "__declspec(dllexport) unsigned retro_api_version(void) { return 1; }\n",
        &["-shared"],
    );
    kit
}
