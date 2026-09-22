//! Collect every file needed to load a game, and no other file.
//!
//! A cartridge is one file. A sheet lists other files. In the console
//! package we declare which sheets each console uses, so we do not handle
//! each console here. We use this code for exports and for project
//! archives, so neither of them can leave a disc track out.

use std::collections::HashSet;
use std::fs;
use std::path::{Component, Path, PathBuf};

use crate::systems::{self, Companion, SheetParser, System};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ContentFile {
    pub source: PathBuf,
    pub relative: PathBuf,
    /// Normalized manifest bytes when the staged file must use portable path
    /// separators. Otherwise we copy the file directly from `source`.
    pub staged_bytes: Option<Vec<u8>>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ContentSet {
    pub entrypoint: PathBuf,
    pub files: Vec<ContentFile>,
}

/// Find every file referenced from `entrypoint`, without leaving its
/// folder. Refuse a symbolic link that resolves to a path outside that
/// folder.
pub fn collect(entrypoint: &Path) -> Result<ContentSet, String> {
    collect_for(entrypoint, None)
}

/// Same as [`collect`], with the declarations of one console, for when we
/// already know which console the game is for.
///
/// We must not refuse a disc for one console because of a companion file
/// for another console beside discs of the same kind. For example, we need
/// the `.sub` beside a CloneCD sheet to run it in Beetle PCE Fast. With no
/// console chosen, we require a companion only when every console does.
pub fn collect_for(entrypoint: &Path, system_id: Option<&str>) -> Result<ContentSet, String> {
    if !entrypoint.is_file() {
        return Err(format!(
            "game content does not exist or is not a regular file: {}",
            entrypoint.display()
        ));
    }
    let systems = match system_id {
        None => systems::registry().iter().collect::<Vec<_>>(),
        Some(id) => vec![systems::find(id).ok_or_else(|| format!("unknown console: {id}"))?],
    };
    let entrypoint = fs::canonicalize(entrypoint)
        .map_err(|error| format!("resolve game content {}: {error}", entrypoint.display()))?;
    let root = entrypoint
        .parent()
        .ok_or_else(|| "game content has no containing directory".to_string())?
        .to_path_buf();
    let name = entrypoint
        .file_name()
        .ok_or_else(|| "game content has no filename".to_string())?;
    let entry_relative = PathBuf::from(name);
    let extension = extension_of(&entrypoint);
    // We have no parser for a format still declared recognise-only, so we
    // refuse it here, before we open any track.
    if sheet_parser(&extension, &systems).is_none()
        && systems
            .iter()
            .any(|system| system.is_recognize_only(&extension))
    {
        return Err(format!(
            "{} manifests can be recognised but not exported yet: they point at other files, and collecting those safely is not implemented.",
            extension.to_ascii_uppercase()
        ));
    }
    let mut files = Vec::new();
    let mut seen = HashSet::new();
    gather(
        &entrypoint,
        &entry_relative,
        &root,
        &systems,
        &mut files,
        &mut seen,
    )?;
    Ok(ContentSet {
        entrypoint: entry_relative,
        files,
    })
}

fn gather(
    absolute: &Path,
    relative: &Path,
    root: &Path,
    systems: &[&System],
    files: &mut Vec<ContentFile>,
    seen: &mut HashSet<PathBuf>,
) -> Result<(), String> {
    if !seen.insert(relative.to_path_buf()) {
        return Ok(());
    }
    let extension = extension_of(absolute);
    let parser = sheet_parser(&extension, systems);
    let (staged_bytes, references) = match parser {
        Some(parser) => {
            let text = fs::read_to_string(absolute).map_err(|error| {
                format!("read {} sheet {}: {error}", extension, absolute.display())
            })?;
            let references = crate::discs::sheet_references(parser, &text)?;
            if references.is_empty() {
                return Err(empty_sheet(parser, absolute));
            }
            (staged_sheet(parser, &text), references)
        }
        None => (None, Vec::new()),
    };
    files.push(ContentFile {
        source: absolute.to_path_buf(),
        relative: relative.to_path_buf(),
        staged_bytes,
    });
    for companion in companions_for(&extension, systems) {
        let Some(stem) = absolute.file_stem() else {
            continue;
        };
        let directory = absolute.parent().unwrap_or(root);
        let sibling = directory.join(stem).with_extension(&companion.extension);
        if sibling == absolute {
            continue;
        }
        if !sibling.is_file() {
            if companion.required {
                let name = sibling
                    .file_name()
                    .map(|name| name.to_string_lossy().into_owned())
                    .unwrap_or_else(|| companion.extension.clone());
                return Err(format!(
                    "missing {} file {name} (from {})",
                    companion.extension,
                    absolute.display()
                ));
            }
            continue;
        }
        let sibling_name = sibling
            .file_name()
            .ok_or_else(|| format!("support file has no name: {}", sibling.display()))?;
        let sibling_relative = relative.with_file_name(sibling_name);
        let resolved = fs::canonicalize(&sibling)
            .map_err(|error| format!("resolve {} {}: {error}", companion.extension, sibling.display()))?;
        if !resolved.starts_with(root) {
            return Err(format!(
                "{} file escapes the game content folder: {}",
                companion.extension,
                sibling_relative.display()
            ));
        }
        gather(&resolved, &sibling_relative, root, systems, files, seen)?;
    }
    let mut followed = false;
    for reference in references {
        validate_relative_content_path(&reference)?;
        let candidate = absolute
            .parent()
            .unwrap_or(root)
            .join(&reference);
        if !candidate.is_file() {
            return Err(missing_reference(parser, &reference, absolute));
        }
        let resolved = fs::canonicalize(&candidate).map_err(|error| {
            format!(
                "resolve {} reference {}: {error}",
                extension,
                candidate.display()
            )
        })?;
        if !resolved.starts_with(root) {
            return Err(format!(
                "referenced file escapes the game content folder: {}",
                reference.display()
            ));
        }
        let child_relative = match relative.parent() {
            Some(parent) if !parent.as_os_str().is_empty() => parent.join(&reference),
            _ => reference.clone(),
        };
        let before = files.len();
        gather(
            &resolved,
            &child_relative,
            root,
            systems,
            files,
            seen,
        )?;
        if files.len() > before {
            followed = true;
        }
    }
    if parser.is_some() && !followed {
        return Err(format!(
            "{} sheet names no file besides itself: {}",
            extension.to_ascii_uppercase(),
            absolute.display()
        ));
    }
    Ok(())
}

fn empty_sheet(parser: SheetParser, path: &Path) -> String {
    match parser {
        SheetParser::Cue => format!(
            "CUE sheet has no FILE entries and cannot be exported safely: {}",
            path.display()
        ),
        SheetParser::Gdi => format!("GDI sheet names no track files: {}", path.display()),
        SheetParser::Playlist => format!("playlist names no discs: {}", path.display()),
        SheetParser::Toc => format!("TOC sheet names no track files: {}", path.display()),
    }
}

fn missing_reference(parser: Option<SheetParser>, reference: &Path, sheet: &Path) -> String {
    if parser == Some(SheetParser::Cue) {
        return format!(
            "CUE sheet references a missing track file: {} (from {})",
            reference.display(),
            sheet.display()
        );
    }
    format!(
        "{} sheet references a missing file: {} (from {})",
        extension_of(sheet).to_ascii_uppercase(),
        reference.display(),
        sheet.display()
    )
}

fn staged_sheet(parser: SheetParser, text: &str) -> Option<Vec<u8>> {
    // On macOS a sheet for a core must use POSIX separators. We leave the
    // source untouched and normalize only our staged or project copy.
    match parser {
        SheetParser::Cue => Some(normalize_cue_separators(text).into_bytes()),
        _ if text.contains('\\') => Some(text.replace('\\', "/").into_bytes()),
        _ => None,
    }
}

fn sheet_parser(extension: &str, systems: &[&System]) -> Option<SheetParser> {
    let mut found = None;
    for system in systems {
        for sheet in &system.sheets {
            if sheet.extension.eq_ignore_ascii_case(extension) {
                found = Some(sheet.parser);
            }
        }
    }
    found
}

fn companions_for(host: &str, systems: &[&System]) -> Vec<Companion> {
    let mut found: Vec<Companion> = Vec::new();
    for system in systems {
        for companion in &system.companions {
            if !companion_applies(companion, host) {
                continue;
            }
            if found
                .iter()
                .any(|existing| existing.extension.eq_ignore_ascii_case(&companion.extension))
            {
                continue;
            }
            found.push(companion.clone());
        }
    }
    if systems.len() == 1 {
        return found;
    }
    // With no console chosen, we require a sibling only when every console
    // that takes this file requires it. Otherwise we would refuse a
    // PlayStation CloneCD dump without the `.sub` required for PC Engine CD.
    let hosts: Vec<_> = systems
        .iter()
        .filter(|system| {
            system
                .extensions
                .iter()
                .any(|declared| declared.eq_ignore_ascii_case(host))
        })
        .collect();
    for companion in &mut found {
        companion.required = !hosts.is_empty()
            && hosts.iter().all(|system| {
                system.companions.iter().any(|candidate| {
                    candidate.required
                        && candidate.extension.eq_ignore_ascii_case(&companion.extension)
                        && companion_applies(candidate, host)
                })
            });
    }
    found
}

fn companion_applies(companion: &Companion, host: &str) -> bool {
    match &companion.when {
        Some(when) => when.eq_ignore_ascii_case(host),
        None => true,
    }
}

/// The game file for a dropped path.
///
/// A folder is not a game file, so when someone drops a folder, for example
/// one with a Dreamcast game, we use the GD-ROM inside it. A sibling `.sbi` is
/// subchannel data, not a game, so for that file we use the disc next to it.
pub fn resolve_dropped(path: &Path) -> Result<PathBuf, String> {
    if path.is_dir() {
        return sole_game_in(path);
    }
    if !path.is_file() {
        return Err(format!(
            "game content does not exist or is not a regular file: {}",
            path.display()
        ));
    }
    let extension = extension_of(path);
    if is_support_extension(&extension) {
        if let Some(game) = sibling_game(path) {
            return Ok(game);
        }
    }
    Ok(path.to_path_buf())
}

fn sole_game_in(directory: &Path) -> Result<PathBuf, String> {
    let mut games = Vec::new();
    let mut sheets = Vec::new();
    let entries = fs::read_dir(directory)
        .map_err(|error| format!("read game folder {}: {error}", directory.display()))?;
    for entry in entries {
        let entry =
            entry.map_err(|error| format!("read game folder {}: {error}", directory.display()))?;
        if !entry
            .file_type()
            .map(|kind| kind.is_file())
            .unwrap_or(false)
        {
            continue;
        }
        let path = entry.path();
        let extension = extension_of(&path);
        if extension.is_empty() || is_support_extension(&extension) {
            continue;
        }
        if crate::systems::candidates_for_extension(&extension).is_empty() {
            continue;
        }
        if is_sheet(&extension) {
            sheets.push(path);
        } else {
            games.push(path);
        }
    }
    if sheets.len() == 1 {
        return Ok(sheets.remove(0));
    }
    // A multi-disc folder contains the playlist and the discs it lists.
    // The playlist is the game, and the discs are the files it lists.
    let playlists: Vec<_> = sheets
        .iter()
        .filter(|path| is_playlist(&extension_of(path)))
        .cloned()
        .collect();
    if playlists.len() == 1 {
        return Ok(playlists.into_iter().next().expect("one playlist"));
    }
    if sheets.is_empty() && games.len() == 1 {
        return Ok(games.remove(0));
    }
    if sheets.is_empty() && games.is_empty() {
        return Err(format!(
            "This folder has no game file: {}",
            directory.display()
        ));
    }
    Err(format!(
        "This folder contains more than one game. Drop the game file itself: {}",
        directory.display()
    ))
}

fn sibling_game(support: &Path) -> Option<PathBuf> {
    let stem = support.file_stem()?;
    let directory = support.parent()?;
    let mut found = Vec::new();
    for system in crate::systems::registry() {
        for extension in &system.extensions {
            let candidate = directory.join(stem).with_extension(extension);
            if candidate.is_file() && !found.contains(&candidate) {
                found.push(candidate);
            }
        }
    }
    if found.len() == 1 {
        return found.pop();
    }
    found
        .into_iter()
        .find(|candidate| is_sheet(&extension_of(candidate)))
}

fn extension_of(path: &Path) -> String {
    path.extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase()
}

fn is_support_extension(extension: &str) -> bool {
    systems::registry().iter().any(|system| {
        system
            .companions
            .iter()
            .any(|companion| companion.extension.eq_ignore_ascii_case(extension))
    })
}

fn is_sheet(extension: &str) -> bool {
    systems::registry().iter().any(|system| {
        system
            .sheets
            .iter()
            .any(|sheet| sheet.extension.eq_ignore_ascii_case(extension))
            || system.is_recognize_only(extension)
    })
}

fn is_playlist(extension: &str) -> bool {
    systems::registry().iter().any(|system| {
        system.sheets.iter().any(|sheet| {
            sheet.extension.eq_ignore_ascii_case(extension)
                && sheet.parser == SheetParser::Playlist
        })
    })
}

fn normalize_cue_separators(cue: &str) -> String {
    cue.lines()
        .map(|line| {
            let trimmed = line.trim_start();
            let indentation = &line[..line.len() - trimmed.len()];
            let Some(rest) = crate::discs::strip_ascii_keyword(trimmed, "FILE") else {
                return line.to_string();
            };
            format!("{indentation}FILE{}", rest.replace('\\', "/"))
        })
        .collect::<Vec<_>>()
        .join("\n")
        + if cue.ends_with('\n') { "\n" } else { "" }
}

fn validate_relative_content_path(path: &Path) -> Result<(), String> {
    let text = path.to_string_lossy();
    if text.is_empty()
        || text.contains(':')
        || path.is_absolute()
        || path
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(format!(
            "CUE track path must stay within the game content folder: {}",
            path.display()
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn fixture(name: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "rominabox-content-{name}-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir_all(&path).unwrap();
        path
    }

    #[test]
    fn cue_collects_each_referenced_track_with_relative_layout() {
        let root = fixture("multi-track");
        fs::create_dir(root.join("audio")).unwrap();
        fs::write(root.join("disc.bin"), b"data").unwrap();
        fs::write(root.join("audio/track 02.bin"), b"audio").unwrap();
        let cue = root.join("game.cue");
        fs::write(
            &cue,
            "FILE \"disc.bin\" BINARY\n  TRACK 01 MODE1/2352\nFILE \"audio\\track 02.bin\" BINARY\n  TRACK 02 AUDIO\n",
        )
        .unwrap();

        let content = collect(&cue).unwrap();
        let relative = content
            .files
            .iter()
            .map(|file| file.relative.clone())
            .collect::<Vec<_>>();
        assert_eq!(
            relative,
            ["game.cue", "disc.bin", "audio/track 02.bin"]
                .map(PathBuf::from)
                .to_vec()
        );
    }

    #[test]
    fn cue_rejects_parent_traversal() {
        let root = fixture("traversal");
        let cue = root.join("game.cue");
        fs::write(&cue, "FILE \"../secret.bin\" BINARY\n").unwrap();

        let error = collect(&cue).unwrap_err();
        assert!(error.contains("must stay within"), "{error}");
    }

    #[test]
    fn cue_reports_a_missing_track_instead_of_exporting_only_the_sheet() {
        let root = fixture("missing");
        let cue = root.join("game.cue");
        fs::write(&cue, "FILE \"missing.bin\" BINARY\n").unwrap();

        let error = collect(&cue).unwrap_err();
        assert!(error.contains("missing track file"), "{error}");
        assert!(error.contains("missing.bin"), "{error}");
    }

    /// We refuse a playlist with a missing disc and name that disc in the
    /// message, as we do for a cue.
    #[test]
    fn a_playlist_names_the_disc_it_is_missing() {
        let root = fixture("playlist-missing");
        let playlist = root.join("game.m3u");
        fs::write(&playlist, b"disc1.cue\n").unwrap();

        let refusal = collect(&playlist).expect_err("the disc is not there");
        assert!(refusal.contains("disc1.cue"), "{refusal}");
    }

    /// A Dreamcast multi-disc game is a playlist of GD-ROM layouts, and each
    /// layout lists tracks. We follow both kinds of sheet in one pass, or we
    /// would leave out the tracks of the second disc.
    #[test]
    fn a_playlist_of_gd_rom_layouts_collects_every_track() {
        let root = fixture("playlist-gdi");
        fs::write(root.join("track.bin"), b"data").unwrap();
        fs::write(root.join("game.gdi"), "1\n1 0 4 2352 track.bin 0\n").unwrap();
        let playlist = root.join("game.m3u");
        fs::write(&playlist, "game.gdi\n").unwrap();

        let names = relative_names(&collect(&playlist).unwrap());
        assert!(names.iter().any(|name| name == "game.m3u"), "{names:?}");
        assert!(names.iter().any(|name| name == "game.gdi"), "{names:?}");
        assert!(names.iter().any(|name| name == "track.bin"), "{names:?}");
    }

    fn relative_names(content: &ContentSet) -> Vec<String> {
        content
            .files
            .iter()
            .map(|file| file.relative.to_string_lossy().replace('\\', "/"))
            .collect()
    }

    /// The files that go with a dropped game, according to each console's
    /// package. We list them here apart from the code that collects files,
    /// so that we notice when a package stops declaring a sheet.
    #[test]
    fn every_console_declares_what_travels_with_its_games() {
        let sheets = |id: &str| -> Vec<String> {
            systems::find(id)
                .unwrap()
                .sheets
                .iter()
                .map(|sheet| format!("{}:{}", sheet.extension, sheet.parser.as_str()))
                .collect()
        };
        let required = |id: &str| -> Vec<String> {
            systems::find(id)
                .unwrap()
                .companions
                .iter()
                .filter(|companion| companion.required)
                .map(|companion| {
                    format!(
                        "{}->{}",
                        companion.when.as_deref().unwrap_or("*"),
                        companion.extension
                    )
                })
                .collect()
        };
        let optional = |id: &str| -> Vec<String> {
            systems::find(id)
                .unwrap()
                .companions
                .iter()
                .filter(|companion| !companion.required)
                .map(|companion| {
                    format!(
                        "{}->{}",
                        companion.when.as_deref().unwrap_or("*"),
                        companion.extension
                    )
                })
                .collect()
        };
        assert_eq!(sheets("dreamcast"), ["gdi:gdi", "m3u:playlist", "cue:cue"]);
        assert!(required("dreamcast").is_empty());
        assert_eq!(sheets("ps1"), ["cue:cue", "m3u:playlist", "toc:toc"]);
        assert_eq!(required("ps1"), ["ccd->img"]);
        assert_eq!(optional("ps1"), ["*->sbi", "ccd->sub"]);
        assert_eq!(sheets("pcecd"), ["cue:cue", "m3u:playlist", "toc:toc"]);
        assert_eq!(required("pcecd"), ["ccd->img", "ccd->sub"]);
        assert_eq!(optional("pcecd"), ["*->sbi"]);
        assert_eq!(sheets("segacd"), ["cue:cue", "m3u:playlist"]);
        assert!(required("segacd").is_empty() && optional("segacd").is_empty());
        assert_eq!(sheets("ps2"), ["cue:cue", "m3u:playlist"]);
        assert_eq!(sheets("gamecube"), ["m3u:playlist"]);
        for id in [
            "atari2600",
            "atari5200",
            "atari7800",
            "gamegear",
            "gb",
            "gba",
            "gbc",
            "lynx",
            "mastersystem",
            "megadrive",
            "n64",
            "neogeopocket",
            "neogeopocketcolor",
            "nes",
            "pce",
            "sg1000",
            "snes",
            "wonderswan",
            "wonderswancolor",
        ] {
            assert!(sheets(id).is_empty(), "{id} is a cartridge: {}", sheets(id).join(","));
            assert!(required(id).is_empty() && optional(id).is_empty(), "{id}");
        }
        let declared: Vec<&str> = systems::registry()
            .iter()
            .flat_map(|system| system.recognize_only.iter().map(String::as_str))
            .collect();
        assert!(
            declared.is_empty(),
            "a sheet that is followed is not recognise-only: {declared:?}"
        );
        let mut ids: Vec<&str> = systems::registry().iter().map(|system| system.id.as_str()).collect();
        ids.sort_unstable();
        assert_eq!(
            ids,
            [
                "atari2600",
                "atari5200",
                "atari7800",
                "dreamcast",
                "gamecube",
                "gamegear",
                "gb",
                "gba",
                "gbc",
                "lynx",
                "mastersystem",
                "megadrive",
                "n64",
                "neogeopocket",
                "neogeopocketcolor",
                "nes",
                "pce",
                "pcecd",
                "ps1",
                "ps2",
                "segacd",
                "sg1000",
                "snes",
                "wonderswan",
                "wonderswancolor",
            ]
        );
    }

    /// When someone drops a folder, we use its layout. A multi-disc folder
    /// also contains the discs the playlist lists, and we use the playlist.
    #[test]
    fn a_dropped_folder_resolves_to_the_playlist_not_one_disc() {
        let root = fixture("folder-playlist");
        fs::write(root.join("track.bin"), b"data").unwrap();
        fs::write(root.join("game.gdi"), "1\n1 0 4 2352 track.bin 0\n").unwrap();
        fs::write(root.join("game.m3u"), "game.gdi\n").unwrap();

        let resolved = resolve_dropped(&root).unwrap();
        assert_eq!(
            resolved.file_name().and_then(|name| name.to_str()),
            Some("game.m3u")
        );
        let names = relative_names(&collect(&resolved).unwrap());
        assert!(names.iter().any(|name| name == "track.bin"), "{names:?}");
    }

    #[test]
    fn every_declared_sheet_is_followed_and_a_missing_file_is_named() {
        for system in systems::registry() {
            for sheet in &system.sheets {
                let root = fixture(&format!("{}-{}", system.id, sheet.extension));
                let entry = root.join(format!("game.{}", sheet.extension));
                fs::write(root.join("track.bin"), b"data").unwrap();
                fs::write(&entry, sheet_text(sheet.parser, "track.bin")).unwrap();
                let collected = collect_for(&entry, Some(&system.id))
                    .unwrap_or_else(|error| panic!("{} {}: {error}", system.id, sheet.extension));
                let names = relative_names(&collected);
                assert!(
                    names.iter().any(|name| name == "track.bin"),
                    "{} {} did not bring track.bin: {names:?}",
                    system.id,
                    sheet.extension
                );

                let missing_root = fixture(&format!("{}-{}-missing", system.id, sheet.extension));
                let missing = missing_root.join(format!("game.{}", sheet.extension));
                fs::write(&missing, sheet_text(sheet.parser, "absent.bin")).unwrap();
                let error = collect_for(&missing, Some(&system.id)).unwrap_err();
                assert!(
                    error.contains("absent.bin"),
                    "{} {} did not name the missing file: {error}",
                    system.id,
                    sheet.extension
                );
            }
            for extension in &system.extensions {
                if system
                    .sheets
                    .iter()
                    .any(|sheet| sheet.extension == *extension)
                    || extension == "ccd"
                {
                    continue;
                }
                let root = fixture(&format!("{}-bare-{extension}", system.id));
                let file = root.join(format!("game.{extension}"));
                fs::write(&file, b"data").unwrap();
                let collected = collect_for(&file, Some(&system.id)).unwrap_or_else(|error| {
                    panic!("{} .{extension} is one file: {error}", system.id)
                });
                assert_eq!(
                    collected.files.len(),
                    1,
                    "{} .{extension} collected {:?}",
                    system.id,
                    relative_names(&collected)
                );
            }
        }
    }

    #[test]
    fn a_clonecd_sheet_refuses_the_sibling_its_core_requires() {
        let root = fixture("pce-ccd");
        fs::write(root.join("game.ccd"), b"[CloneCD]\n").unwrap();
        fs::write(root.join("game.img"), b"data").unwrap();
        let missing = collect_for(&root.join("game.ccd"), Some("pcecd")).unwrap_err();
        assert!(missing.contains("game.sub"), "{missing}");

        fs::write(root.join("game.sub"), b"sub").unwrap();
        let names = relative_names(&collect_for(&root.join("game.ccd"), Some("pcecd")).unwrap());
        for name in ["game.ccd", "game.img", "game.sub"] {
            assert!(names.iter().any(|found| found == name), "{names:?}");
        }

        // A CloneCD image runs in PCSX without the subchannel file but not
        // in Beetle. We declare that difference in the console packages.
        let ps1 = fixture("ps1-ccd");
        fs::write(ps1.join("game.ccd"), b"[CloneCD]\n").unwrap();
        fs::write(ps1.join("game.img"), b"data").unwrap();
        let names = relative_names(&collect_for(&ps1.join("game.ccd"), Some("ps1")).unwrap());
        assert!(names.iter().any(|name| name == "game.img"), "{names:?}");
        assert!(!names.iter().any(|name| name.ends_with(".sub")), "{names:?}");
    }

    fn sheet_text(parser: SheetParser, name: &str) -> String {
        match parser {
            SheetParser::Cue => format!("FILE \"{name}\" BINARY\n  TRACK 01 MODE1/2352\n"),
            SheetParser::Gdi => format!("1\n1 0 4 2352 {name} 0\n"),
            SheetParser::Playlist => format!("{name}\n"),
            SheetParser::Toc => format!("CD_ROM\nTRACK MODE1_RAW\nDATAFILE \"{name}\" 0\n"),
        }
    }
}
