//! A Windows kit made on macOS or Linux, compiled with zig.

use super::{
    kit_base, PLAYER_ICON_COLOUR, PLAYER_ICON_SCRIPT, PLAYER_ICON_SIZE, PROGRAM_THAT_ENDS, STAND_IN_CORE,
    WINDOWS_JOYPAD_DRIVERS,
};
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
    image::RgbaImage::from_pixel(PLAYER_ICON_SIZE, PLAYER_ICON_SIZE, image::Rgba(PLAYER_ICON_COLOUR))
        .save(kit.join("bin/retroarch.ico"))
        .unwrap();
    fs::write(kit.join("bin/retroarch.rc"), PLAYER_ICON_SCRIPT).unwrap();
    for_windows(&kit.join("bin/retroarch.exe"), PROGRAM_THAT_ENDS, &["retroarch.rc"]);
    for_windows(&kit.join("bin/launcher.exe"), PROGRAM_THAT_ENDS, &[]);
    for driver in WINDOWS_JOYPAD_DRIVERS {
        fs::create_dir_all(kit.join("autoconfig").join(driver)).unwrap();
        fs::write(kit.join("autoconfig").join(driver).join("pad.cfg"), driver).unwrap();
    }
    for_windows(
        &kit.join("cores/genesis_plus_gx_libretro.dll"),
        STAND_IN_CORE,
        &["-shared"],
    );
    kit
}
