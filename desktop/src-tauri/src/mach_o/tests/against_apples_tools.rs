//! We compare each operation with the Apple tool for it, on files we compile.

use crate::mach_o::bundle::{sign_app, AppSeal};
use crate::mach_o::entitlements::{Entitlements, Value};
use crate::mach_o::signature::{code_directory_hashes, sign, Seal};
use crate::mach_o::{dependencies, join, minimum_systems, rename_libraries, slices, Cpu};
use rominabox_scratch::Scratch;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Compile C `source` into `output` for each of `archs`, with `flags`.
fn compile(output: &Path, archs: &[&str], flags: &[&str], source: &str) {
    let file = output.with_extension("c");
    fs::write(&file, source).unwrap();
    let mut cc = Command::new("cc");
    for arch in archs {
        cc.args(["-arch", arch]);
    }
    let status = cc.args(flags).arg("-o").arg(output).arg(&file).status().unwrap();
    assert!(status.success(), "could not compile {}", output.display());
    fs::remove_file(file).unwrap();
}

/// Return what `program` prints, and assert that it succeeded.
fn tool(program: &str, args: &[&str], path: &Path) -> String {
    let output = Command::new(program).args(args).arg(path).output().unwrap();
    assert!(
        output.status.success(),
        "{program} {args:?} {}: {}",
        path.display(),
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).into_owned()
}

/// The description of a signature from `codesign`, and whether it is valid.
fn described(path: &Path, arch: &str) -> String {
    let output = Command::new("/usr/bin/codesign")
        .args(["-d", "-vvvvvv", "-a", arch])
        .arg(path)
        .output()
        .unwrap();
    String::from_utf8_lossy(&output.stderr).into_owned()
}

fn verify(path: &Path) {
    let output = Command::new("/usr/bin/codesign")
        .args(["--verify", "--deep", "--strict", "-vv"])
        .arg(path)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "codesign refused {}:\n{}",
        path.display(),
        String::from_utf8_lossy(&output.stderr)
    );
}

/// The lines of `codesign -d -vvvvvv` that say how a signature is made,
/// not what it hashes: its version, flags and slot counts, page size,
/// executable segment and the hashes of what it seals.
fn shape(description: &str) -> Vec<String> {
    description
        .lines()
        .filter(|line| {
            ["CodeDirectory", "Hash type", "Page size", "Executable Segment", "Internal requirements", "Signature="]
                .iter()
                .any(|start| line.starts_with(start))
                || line.trim_start().starts_with('-')
        })
        .map(str::to_owned)
        .collect()
}

fn rosetta() -> bool {
    Command::new("arch").args(["-x86_64", "/usr/bin/true"]).status().is_ok_and(|status| status.success())
}

const PROGRAM: &str = "#include <stdio.h>\nint main(int argc, char **argv) { printf(\"ran %d\\n\", argc); return 0; }\n";

#[test]
fn slices_are_what_lipo_lists_and_each_is_what_lipo_thins() {
    let root = Scratch::dir("rominabox-mach-o-slices");
    let program = root.path().join("program");
    compile(&program, &["arm64", "x86_64"], &[], PROGRAM);
    let file = fs::read(&program).unwrap();
    let found = slices(&file).unwrap();
    let names: Vec<String> = found.iter().map(|slice| slice.cpu.name()).collect();
    assert_eq!(names.join(" "), tool("lipo", &["-archs"], &program).trim());
    for slice in found {
        let thinned = root.path().join(slice.cpu.name());
        tool("lipo", &["-thin", &slice.cpu.name(), "-output", thinned.to_str().unwrap()], &program);
        assert!(fs::read(&thinned).unwrap() == slice.bytes, "the {} slice differs", slice.cpu.name());
    }
}

#[test]
fn joined_slices_are_what_lipo_creates() {
    let root = Scratch::dir("rominabox-mach-o-join");
    let (apple, intel) = (root.path().join("arm64.dylib"), root.path().join("x86_64.dylib"));
    compile(&apple, &["arm64"], &["-dynamiclib"], "int probe(void) { return 1; }\n");
    compile(&intel, &["x86_64"], &["-dynamiclib"], "int probe(void) { return 2; }\n");
    let joined = join(&[&fs::read(&apple).unwrap(), &fs::read(&intel).unwrap()]).unwrap();
    let created = root.path().join("created.dylib");
    let status = Command::new("lipo")
        .args(["-create", "-output"])
        .arg(&created)
        .arg(&apple)
        .arg(&intel)
        .status()
        .unwrap();
    assert!(status.success());
    assert!(joined == fs::read(&created).unwrap(), "the joined file differs from lipo's");
}

/// The dependencies of a universal library and the oldest systems for its
/// slices, as listed by `otool -L` and `otool -l`.
#[test]
fn dependencies_and_minimum_systems_are_what_otool_reads() {
    let root = Scratch::dir("rominabox-mach-o-otool");
    let source = "#include <CoreFoundation/CoreFoundation.h>\nint probe(void) { return (int)CFAbsoluteTimeGetCurrent(); }\n";
    let (apple, intel) = (root.path().join("arm64.dylib"), root.path().join("x86_64.dylib"));
    let flags = ["-dynamiclib", "-framework", "CoreFoundation", "-install_name", "@rpath/libprobe.dylib"];
    compile(&apple, &["arm64"], &[&flags[..], &["-mmacosx-version-min=11.0"]].concat(), source);
    compile(&intel, &["x86_64"], &[&flags[..], &["-mmacosx-version-min=10.13"]].concat(), source);
    let library = root.path().join("libprobe.dylib");
    fs::write(&library, join(&[&fs::read(&apple).unwrap(), &fs::read(&intel).unwrap()]).unwrap()).unwrap();
    let file = fs::read(&library).unwrap();

    let own = tool("otool", &["-D"], &library);
    let mut listed: Vec<String> = Vec::new();
    for line in tool("otool", &["-L"], &library).lines() {
        let name = line.trim().split(" (compatibility").next().unwrap().to_string();
        if !line.trim_end().ends_with(':') && !own.contains(&format!("\n{name}")) && !listed.contains(&name) {
            listed.push(name);
        }
    }
    assert_eq!(dependencies(&file).unwrap(), listed);
    assert!(listed.iter().any(|name| name.contains("CoreFoundation")), "{listed:?}");

    // LC_BUILD_VERSION's `minos`, or LC_VERSION_MIN_MACOSX's `version`,
    // which the Intel slice, built for an older system, contains.
    let mut asked = Vec::new();
    let mut version_min = false;
    for line in tool("otool", &["-l"], &library).lines().map(str::trim) {
        if let Some(command) = line.strip_prefix("cmd ") {
            version_min = command == "LC_VERSION_MIN_MACOSX";
        }
        let version = line.strip_prefix("minos ").or(line.strip_prefix("version ").filter(|_| version_min));
        asked.extend(version.map(str::to_owned));
    }
    let read: Vec<String> = minimum_systems(&file)
        .unwrap()
        .iter()
        .map(|parts| parts.iter().map(u32::to_string).collect::<Vec<_>>().join("."))
        .collect();
    assert_eq!(read, asked);
    assert_eq!(read, ["10.13", "11.0"]);
}

/// A library's dependency and its install name changed, as with
/// `install_name_tool -change` and `-id`, including to a longer name.
#[test]
fn renamed_libraries_are_what_install_name_tool_makes() {
    let root = Scratch::dir("rominabox-mach-o-rename");
    let dependency = root.path().join("libdep.dylib");
    compile(&dependency, &["arm64", "x86_64"], &["-dynamiclib", "-install_name", "/opt/elsewhere/libdep.dylib"], "int dep(void) { return 3; }\n");
    let user = root.path().join("libuser.dylib");
    compile(
        &user,
        &["arm64", "x86_64"],
        &["-dynamiclib", "-install_name", "/opt/elsewhere/libuser.dylib", dependency.to_str().unwrap()],
        "int dep(void);\nint user(void) { return dep(); }\n",
    );
    let moved = "@executable_path/../Frameworks/libdep.dylib";
    let renamed = rename_libraries(
        &fs::read(&user).unwrap(),
        |name| (name == "/opt/elsewhere/libdep.dylib").then(|| moved.to_string()),
        Some("@rpath/libuser.dylib"),
    )
    .unwrap();
    let ours = root.path().join("ours.dylib");
    fs::write(&ours, renamed).unwrap();
    let theirs = root.path().join("theirs.dylib");
    fs::copy(&user, &theirs).unwrap();
    let output = Command::new("install_name_tool")
        .args(["-change", "/opt/elsewhere/libdep.dylib", moved, "-id", "@rpath/libuser.dylib"])
        .arg(&theirs)
        .output()
        .unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let without_file_name = |path: &Path| {
        tool("otool", &["-L"], path).replace(path.to_str().unwrap(), "FILE")
    };
    assert_eq!(without_file_name(&ours), without_file_name(&theirs));
    assert!(without_file_name(&ours).contains(moved));
}

/// We sign programs here, unsigned ones and ones signed by the linker, for
/// each processor and for both. Each must pass `codesign`, get the same
/// description from it as a copy signed by `codesign`, and run.
#[test]
fn a_signed_program_is_what_codesign_accepts_and_runs() {
    let root = Scratch::dir("rominabox-mach-o-sign");
    for (name, archs, flags) in [
        ("apple", &["arm64"][..], &[][..]),
        ("intel-unsigned", &["x86_64"][..], &["-Wl,-no_adhoc_codesign"][..]),
        ("both", &["arm64", "x86_64"][..], &[][..]),
    ] {
        let program = root.path().join(name);
        compile(&program, archs, flags, PROGRAM);
        let theirs = root.path().join(format!("{name}-codesign"));
        fs::copy(&program, &theirs).unwrap();
        tool("/usr/bin/codesign", &["--force", "--sign", "-", "--identifier", "probe"], &theirs);

        let signed = sign(&fs::read(&program).unwrap(), &Seal { identifier: "probe", ..Seal::default() }).unwrap();
        fs::write(&program, signed).unwrap();
        verify(&program);
        for arch in archs {
            assert_eq!(shape(&described(&program, arch)), shape(&described(&theirs, arch)), "{name}, {arch}");
            let runs = *arch == "arm64" || rosetta();
            if runs {
                let output = Command::new("arch").args([&format!("-{arch}")]).arg(&program).arg("x").output().unwrap();
                assert_eq!(String::from_utf8_lossy(&output.stdout), "ran 2\n", "{name} on {arch}");
            } else {
                eprintln!("SKIPPED running {name} for {arch}: this Mac cannot run it");
            }
        }
        let cdhashes: Vec<Cpu> = code_directory_hashes(&fs::read(&program).unwrap())
            .unwrap()
            .into_iter()
            .map(|(cpu, _)| cpu)
            .collect();
        assert_eq!(cdhashes.len(), archs.len());
    }
}

/// Our seal of an app equals the seal from `codesign`: the same
/// CodeResources byte for byte, the same hashes of what we seal the main
/// program with, and it passes deep and strict verification by `codesign`.
#[test]
fn an_app_is_sealed_as_codesign_seals_it() {
    let root = Scratch::dir("rominabox-mach-o-app");
    let app = root.path().join("Sealed Game.app");
    let contents = app.join("Contents");
    for folder in ["MacOS", "Resources/content", "Resources/menu assets", "Frameworks"] {
        fs::create_dir_all(contents.join(folder)).unwrap();
    }
    fs::write(
        contents.join("Info.plist"),
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n<plist version=\"1.0\"><dict>\n<key>CFBundleExecutable</key><string>retroarch</string>\n<key>CFBundleIdentifier</key><string>app.rominabox.test.sealed</string>\n<key>CFBundlePackageType</key><string>APPL</string>\n</dict></plist>\n",
    )
    .unwrap();
    let executable = contents.join("MacOS/retroarch");
    compile(&executable, &["arm64", "x86_64"], &[], PROGRAM);
    let library = contents.join("MacOS/librominabox-launch.dylib");
    compile(&library, &["arm64", "x86_64"], &["-dynamiclib"], "int launch(void) { return 1; }\n");
    let core = contents.join("Resources/game-core.dylib");
    compile(&core, &["arm64"], &["-dynamiclib"], "int core(void) { return 2; }\n");
    fs::write(contents.join("Resources/content/Pokémon Gold & Silver.gbc"), b"cartridge").unwrap();
    fs::write(contents.join("Resources/menu assets/menu.rml"), b"<rml/>").unwrap();
    fs::write(contents.join("Resources/game.json"), b"{}").unwrap();
    let entitlements = Entitlements::default()
        .with("com.apple.security.app-sandbox", Value::Bool(true))
        .with("com.apple.security.device.usb", Value::Bool(true))
        .with(
            "com.apple.security.temporary-exception.files.home-relative-path.read-only",
            Value::Strings(vec!["/Library/Application Support/ROM-in-a-Box/Games/test/".into()]),
        );

    sign_app(
        &AppSeal {
            app: &app,
            executable: &executable,
            identifier: "app.rominabox.test.sealed",
            entitlements: &entitlements,
            nested: &[library.clone(), core.clone()],
        },
        &|| false,
    )
    .unwrap();
    verify(&app);
    let shown = tool("/usr/bin/codesign", &["-d", "--entitlements", "-"], &app);
    assert!(shown.contains("com.apple.security.app-sandbox"), "{shown}");
    assert!(shown.contains("/Library/Application Support/ROM-in-a-Box/Games/test/"), "{shown}");

    // We seal a copy again with `codesign`. Because we sign only the app, the
    // nested code keeps our signatures, so CodeResources can match.
    let copy = root.path().join("Copy.app");
    let status = Command::new("ditto").arg(&app).arg(&copy).status().unwrap();
    assert!(status.success());
    let plist = root.path().join("entitlements.plist");
    fs::write(&plist, entitlements.xml()).unwrap();
    let output = Command::new("/usr/bin/codesign")
        .args(["--force", "--sign", "-", "--entitlements"])
        .arg(&plist)
        .arg(&copy)
        .output()
        .unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let resources = |app: &Path| fs::read_to_string(app.join("Contents/_CodeSignature/CodeResources")).unwrap();
    assert_eq!(resources(&app), resources(&copy));
    for arch in ["arm64", "x86_64"] {
        let sealed_with = |app: &PathBuf| -> Vec<String> {
            described(app, arch)
                .lines()
                .filter(|line| line.trim_start().starts_with('-'))
                .map(str::to_owned)
                .collect()
        };
        assert_eq!(sealed_with(&app), sealed_with(&copy), "{arch}");
        assert_eq!(shape(&described(&app, arch)), shape(&described(&copy, arch)), "{arch}");
    }
}
