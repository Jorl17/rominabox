use std::fs;
use std::path::Path;

fn main() {
    shader_previews();
    patch_formats();
    game_data();
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        command_line_resource();
    }
}

/// The RetroArch patch formats (IPS, UPS, BPS and xdelta), which we compile
/// from the fork we build the player from, so we apply a patch in the builder
/// exactly as in the player (src/patching.rs).
fn patch_formats() {
    let retroarch = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../vendor/retroarch");
    let sources = [
        "tasks/patch_stream.c",
        "libretro-common/encodings/encoding_crc32.c",
        // The CPU check for the faster CRC-32 path on an x86 processor, and
        // the timer for the sleeps in that file.
        "libretro-common/features/features_cpu.c",
        "libretro-common/time/rtime.c",
        "libretro-common/encodings/encoding_vcdiff.c",
        // The LZMA2 decoder with which we read xdelta3's LZMA in VCDIFF.
        "libretro-common/formats/7z/r7z_lzma.c",
        "libretro-common/formats/7z/r7z_lzma_stream.c",
        "libretro-common/formats/7z/r7z_lzma2.c",
    ];
    for source in sources.iter().chain(&["tasks/patch_stream.h"]) {
        println!("cargo:rerun-if-changed={}", retroarch.join(source).display());
    }
    cc::Build::new()
        .files(sources.iter().map(|source| retroarch.join(source)))
        .include(retroarch.join("libretro-common/include"))
        .define("HAVE_XDELTA", None)
        .warnings(false)
        .compile("retroarch_patch_formats");
}

/// The zips of a game's data (src/game_data.rs): the same C code as in the
/// player and the launcher (desktop/src-tauri/gamedata), on miniz and the
/// launcher's file layer, so the builder writes and reads exactly the zips a
/// game does.
fn game_data() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let launcher = root.join("desktop/src-tauri/launcher");
    let gamedata = root.join("desktop/src-tauri/gamedata");
    let platform = if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") { "windows" } else { "posix" };
    let ours = [
        gamedata.join("game_data.c"),
        launcher.join("portable_fs.c"),
        launcher.join(platform).join("portable_fs.c"),
    ];
    let includes = [
        launcher.clone(),
        launcher.join(platform),
        gamedata.clone(),
        root.join("vendor/miniz"),
        root.join("vendor/retroarch"),
    ];
    for watched in [&gamedata, &launcher, &root.join("vendor/miniz"), &root.join("vendor/retroarch/menu/drivers/rmlui/declarations.inc")] {
        println!("cargo:rerun-if-changed={}", watched.display());
    }
    cc::Build::new()
        .files(&ours)
        .includes(&includes)
        .flag_if_supported("-std=gnu99")
        .compile("rominabox_game_data");
    // miniz is the library's own code, which we compile without our warnings.
    cc::Build::new()
        .file(gamedata.join("zip_library.c"))
        .includes(&includes)
        .warnings(false)
        .compile("rominabox_zip_library");
}

/// The shader previews we include in the exporter, one for each picture in
/// integrations/shaders/previews, named for the shader it shows. To add a
/// preset to the catalogue with its picture, you need no line of code here.
fn shader_previews() {
    let folder = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../integrations/shaders/previews");
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

/// The icon, name and version of the command line on Windows. They are the
/// builder's, from the builder's configuration, and we write them in the
/// form that `try_build` in tauri-build uses for the builder. We give them
/// only to the command line, and not to the tests and examples here.
fn command_line_resource() {
    let builder = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../src-tauri");
    let configuration = builder.join("tauri.conf.json");
    println!("cargo:rerun-if-changed={}", configuration.display());
    let config: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&configuration).expect("the builder's configuration"))
            .expect("the builder's configuration is JSON");
    let text = |key: &str| config[key].as_str().unwrap_or_else(|| panic!("tauri.conf.json declares no {key}"));
    let icon = config["bundle"]["icon"]
        .as_array()
        .and_then(|icons| icons.iter().filter_map(|icon| icon.as_str()).find(|icon| icon.ends_with(".ico")))
        .map(|icon| builder.join(icon))
        .expect("tauri.conf.json names a Windows icon");
    println!("cargo:rerun-if-changed={}", icon.display());
    let (name, version) = (text("productName"), text("version"));
    // A Windows version is four 16-bit numbers: major, minor, patch and a
    // numeric build, as in 1.2.3+4.
    let (release, build) = version.split_once('+').unwrap_or((version, "0"));
    let numbers = release
        .split('-')
        .next()
        .unwrap()
        .split('.')
        .chain([build])
        .map(|number| number.parse::<u64>().unwrap_or(0))
        .fold(0, |packed, number| packed << 16 | number);
    // With no publisher declared, the company is the second part of the
    // identifier, for example rominabox in com.rominabox.desktop.
    let company = config["bundle"]["publisher"]
        .as_str()
        .or_else(|| text("identifier").split('.').nth(1))
        .unwrap_or(text("identifier"));
    let mut resource = tauri_winres::WindowsResource::new();
    resource
        .set_icon_with_id(&icon.display().to_string(), "32512")
        .set_version_info(tauri_winres::VersionInfo::FILEVERSION, numbers)
        .set_version_info(tauri_winres::VersionInfo::PRODUCTVERSION, numbers)
        .set("FileVersion", version)
        .set("ProductVersion", version)
        .set("ProductName", name)
        .set("FileDescription", name)
        .set("CompanyName", company);
    resource.compile_for(&["rominabox-cli"]).expect("the command line's Windows resource");
}
