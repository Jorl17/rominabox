//! Fill the shader library from libretro's packs: every preset in the shader
//! catalog, with every file it lists, at its path in its pack.
//!
//!     cargo run --example import_shaders -- GLSL_PACK SLANG_PACK
//!
//! Each pack is a checkout of the commit recorded for it in the catalog's
//! `libraries`, and we refuse any other commit. We copy the files into
//! integrations/shaders/library/<glsl|slang>/, replacing any already there,
//! unchanged except that we write text with LF line endings. To find the
//! files we use the same code as an export, so the library contains what
//! games need. We list anything else in the library, for removal by hand.
//!
//! We arranged the presets in the `rominabox` folders of the library from the
//! files of the packs. We keep them as they are, and copy the files from the
//! packs that we name in them.

use rominabox_engine::shaders::{arranged, library_files, library_presets};
use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;
use std::process::Command;

fn main() {
    if let Err(message) = run() {
        eprintln!("{message}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let arguments: Vec<PathBuf> = std::env::args_os().skip(1).map(PathBuf::from).collect();
    let [glsl, slang] = arguments.as_slice() else {
        return Err("usage: import_shaders GLSL_PACK SLANG_PACK".into());
    };
    let repository = rominabox_engine::repo::root();
    let catalog: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(repository.join("integrations/shaders/catalog.json"))
            .map_err(|error| format!("could not read the shader catalog: {error}"))?,
    )
    .map_err(|error| format!("could not read the shader catalog: {error}"))?;
    let library = repository.join("integrations/shaders/library");
    let packs = [("glsl", glsl), ("slang", slang)];
    for (folder, pack) in packs {
        let wanted = catalog["libraries"][folder]["commit"]
            .as_str()
            .ok_or_else(|| format!("the shader catalog records no {folder} commit"))?;
        let head = Command::new("git")
            .arg("-C")
            .arg(pack)
            .args(["rev-parse", "HEAD"])
            .output()
            .map_err(|error| format!("could not run git: {error}"))?;
        let head = String::from_utf8_lossy(&head.stdout).trim().to_string();
        if head != wanted {
            return Err(format!("{} is at {head:?}, not the catalog's {wanted}", pack.display()));
        }
    }

    let mut named: BTreeSet<PathBuf> = BTreeSet::new();
    for preset in library_presets()? {
        let (folder, path) = preset.split_once('/').expect("a library path starts with its folder");
        let (_, pack) = packs.iter().find(|(name, _)| *name == folder).expect("a known folder");
        // We find the files of an arranged preset in the library, where it is.
        let root = if arranged(path) { library.join(folder) } else { pack.to_path_buf() };
        for (_, name) in library_files(&root, path)? {
            let destination = library.join(folder).join(&name);
            if arranged(&name) {
                named.insert(destination);
                continue;
            }
            let source = pack.join(&name);
            fs::create_dir_all(destination.parent().expect("a file has a folder"))
                .map_err(|error| format!("could not create a folder for {name}: {error}"))?;
            let mut bytes = fs::read(&source)
                .map_err(|error| format!("could not read {}: {error}", source.display()))?;
            // We write text with LF, as in the repository on every host, even if
            // the pack checkout has CRLF. We copy pictures as they are.
            if !bytes.contains(&0) {
                let mut lines = Vec::with_capacity(bytes.len());
                for (at, byte) in bytes.iter().enumerate() {
                    if !(*byte == b'\r' && bytes.get(at + 1) == Some(&b'\n')) {
                        lines.push(*byte);
                    }
                }
                bytes = lines;
            }
            fs::write(&destination, bytes)
                .map_err(|error| format!("could not write {}: {error}", destination.display()))?;
            named.insert(destination);
        }
        println!("{preset}");
    }

    let mut extra = Vec::new();
    let mut folders = vec![library.clone()];
    while let Some(folder) = folders.pop() {
        for entry in fs::read_dir(&folder).map_err(|error| error.to_string())? {
            let path = entry.map_err(|error| error.to_string())?.path();
            if path.is_dir() {
                folders.push(path);
            } else if !named.contains(&path) {
                extra.push(path);
            }
        }
    }
    for path in extra {
        println!("no preset names {}", path.display());
    }
    Ok(())
}
