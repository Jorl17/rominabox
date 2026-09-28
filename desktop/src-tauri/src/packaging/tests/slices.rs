//! The code of a Mac app for more than one processor: the slices we read from
//! the files, join, and attach the launch library to.

use super::*;
use crate::mach_o::signature::{sign, Seal};
use crate::packaging::slices::{self, Arch};
use std::process::Command;

/// The processors `path` has code for, as `lipo` lists them, in order.
fn listed(path: &Path) -> Vec<String> {
    let output = Command::new("lipo").arg("-archs").arg(path).output().unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let mut archs: Vec<String> = String::from_utf8_lossy(&output.stdout)
        .split_whitespace()
        .map(str::to_owned)
        .collect();
    archs.sort();
    archs
}

/// Compiles C `sources` into `output` for `archs`, with `flags`.
fn compile(output: &Path, archs: &[Arch], flags: &[&str], sources: &[PathBuf]) {
    let mut cc = Command::new("cc");
    for arch in archs {
        cc.args(["-arch", arch.name()]);
    }
    let status = cc.args(flags).arg("-o").arg(output).args(sources).status().unwrap();
    assert!(status.success(), "could not compile {}", output.display());
}

fn source(root: &Path, name: &str, text: &str) -> PathBuf {
    let path = root.join(name);
    fs::write(&path, text).unwrap();
    path
}

/// Run an Intel Mac program through Rosetta. Returns `None`, with a message,
/// when we cannot run one on this host.
fn rosetta() -> Option<()> {
    let runs = Command::new("arch")
        .args(["-x86_64", "/usr/bin/true"])
        .status()
        .is_ok_and(|status| status.success());
    if !runs {
        eprintln!("SKIPPED the Intel half: this Mac cannot run x86_64 programs (softwareupdate --install-rosetta)");
    }
    runs.then_some(())
}

#[test]
fn two_core_libraries_join_into_one_that_lists_both() {
    let root = rominabox_scratch::Scratch::dir("rominabox-join-cores");
    let core = source(&root, "core.c", "unsigned retro_api_version(void) { return 1; }\n");
    let (apple, intel) = (root.join("arm64.dylib"), root.join("x86_64.dylib"));
    compile(&apple, &[Arch::Arm64], &["-dynamiclib"], std::slice::from_ref(&core));
    compile(&intel, &[Arch::X86_64], &["-dynamiclib"], std::slice::from_ref(&core));
    assert_eq!(listed(&apple), ["arm64"]);
    assert_eq!(listed(&intel), ["x86_64"]);

    let joined = root.join("game-core.dylib");
    slices::join(
        &[(Arch::Arm64, apple), (Arch::X86_64, intel)],
        &joined,
        "The core",
        ErrorStage::Stage,
    )
    .unwrap();
    assert_eq!(listed(&joined), ["arm64", "x86_64"]);
    assert_eq!(
        fs::read_dir(&root).unwrap().count(),
        4,
        "joining left files behind"
    );
}

#[test]
fn a_part_without_its_slice_is_refused_in_the_authors_words() {
    let root = rominabox_scratch::Scratch::dir("rominabox-join-missing");
    let core = source(&root, "core.c", "unsigned retro_api_version(void) { return 1; }\n");
    let apple = root.join("arm64.dylib");
    compile(&apple, &[Arch::Arm64], &["-dynamiclib"], std::slice::from_ref(&core));

    let error = slices::join(
        &[(Arch::Arm64, apple.clone()), (Arch::X86_64, apple)],
        &root.join("game-core.dylib"),
        "The Mega Drive core",
        ErrorStage::Stage,
    )
    .unwrap_err();
    assert_eq!(error.stage, ErrorStage::Refused);
    assert_eq!(
        error.sentence(),
        "The Mega Drive core has no Intel version, so the game cannot run on Intel Macs."
    );
    assert!(!root.join("game-core.dylib").exists());
}

/// We attach the launch library to every slice of the player in a kit
/// (`scripts/build_launcher.py --kit`, as in `scripts/build_kit.py`), and on
/// each processor main receives the arguments prepared in the library, through
/// the trampoline we write at the entry, after we sign both as in an export.
/// The library here is the argument hand-over code of the launcher, with a
/// constructor that uses it.
#[test]
fn the_launch_trampoline_hands_main_its_arguments_on_both_processors() {
    let root = rominabox_scratch::Scratch::dir("rominabox-launch-slices");
    let both = [Arch::Arm64, Arch::X86_64];
    let kit = root.join("kit");
    fs::create_dir_all(kit.join("bin")).unwrap();
    let player = kit.join("bin/retroarch");
    compile(
        &player,
        &both,
        &["-Wl,-headerpad_max_install_names"],
        &[source(
            &root,
            "player.c",
            // With `kept`, the program has the data segment for the launch slots.
            "#include <stdio.h>\nint kept = 1;\nint main(int argc, char **argv) {\n  printf(\"%d\", argc);\n  for (int i = 0; i < argc; i++) printf(\" %s\", argv[i]);\n  printf(\"\\n\");\n  return kept - 1;\n}\n",
        )],
    );
    let attached = Command::new(crate::repo::python())
        .arg(crate::repo::at("scripts/build_launcher.py"))
        .arg("--kit")
        .arg(&kit)
        .output()
        .unwrap();
    assert!(attached.status.success(), "{}", String::from_utf8_lossy(&attached.stderr));
    assert_eq!(listed(&player), ["arm64", "x86_64"]);
    let library = kit.join("bin/librominabox-launch.dylib");
    let launcher = crate::repo::at("desktop/src-tauri/launcher/macos");
    compile(
        &library,
        &both,
        &[
            "-dynamiclib",
            "-Wl,-install_name,@executable_path/librominabox-launch.dylib",
            &format!("-I{}", launcher.display()),
        ],
        &[
            launcher.join("arguments.c"),
            source(
                &root,
                "hand_over.c",
                "#include \"arguments.h\"\nstatic char *forwarded[] = {\"forwarded\", \"by-the-launcher\", 0};\n__attribute__((constructor)) static void hand_over(void) { rominabox_publish_arguments(2, forwarded); }\n",
            ),
        ],
    );
    for (code, identifier) in [(&library, "librominabox-launch"), (&player, "retroarch")] {
        let signed = sign(&fs::read(code).unwrap(), &Seal { identifier, ..Seal::default() }).unwrap();
        fs::write(code, signed).unwrap();
    }

    let run = |prefix: &[&str]| {
        let mut command = Command::new(prefix.first().copied().unwrap_or(player.to_str().unwrap()));
        command.args(prefix.iter().skip(1));
        if !prefix.is_empty() {
            command.arg(&player);
        }
        let output = command.args(["started", "with", "these"]).output().unwrap();
        (
            output.status.code(),
            String::from_utf8_lossy(&output.stdout).into_owned(),
        )
    };
    assert_eq!(
        run(&[]),
        (Some(0), "2 forwarded by-the-launcher\n".to_string()),
        "on this Mac"
    );
    if rosetta().is_some() {
        assert_eq!(
            run(&["arch", "-x86_64"]),
            (Some(0), "2 forwarded by-the-launcher\n".to_string()),
            "on an Intel Mac"
        );
    }
}

/// What a universal library loads, each once. The output of `otool -L` has a
/// header for each slice and the name of the library first in each, and
/// neither is a library it loads, which we would try to move next to it.
#[test]
fn a_universal_librarys_dependencies_are_what_it_loads() {
    let root = rominabox_scratch::Scratch::dir("rominabox-dependencies");
    let source = root.join("probe.c");
    fs::write(&source, "int probe(void) { return 1; }\n").unwrap();
    let library = root.join("libprobe.dylib");
    let built = Command::new("cc")
        .args(["-arch", "arm64", "-arch", "x86_64", "-dynamiclib"])
        .args(["-install_name", "@rpath/libprobe.dylib", "-o"])
        .arg(&library)
        .arg(&source)
        .status()
        .unwrap();
    assert!(built.success());
    assert_eq!(
        crate::packaging::macos::macho_dependencies(&library).unwrap(),
        ["/usr/lib/libSystem.B.dylib"]
    );
    let _ = fs::remove_dir_all(&root);
}
