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
    /// How many discs the game has: the entries of a playlist, or else one.
    pub discs: usize,
    /// The patches we apply to the game file, in order (crate::patches). The
    /// staged bytes of the game file are then the patched game.
    pub patches: Vec<PathBuf>,
    /// The file name of the patched game, when a patch gives one.
    pub patched_name: Option<String>,
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
    let entrypoint = dunce::canonicalize(entrypoint)
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
    // A playlist lists the discs of the game. Any other game is one disc.
    let discs = match sheet_parser(&extension, &systems) {
        Some(SheetParser::Playlist) => {
            let text = fs::read_to_string(&entrypoint)
                .map_err(|error| format!("read playlist {}: {error}", entrypoint.display()))?;
            crate::discs::sheet_references(SheetParser::Playlist, &text)?.len()
        }
        _ => 1,
    };
    let patched = patch_game_file(&entrypoint, &root, &mut files)?;
    Ok(ContentSet {
        entrypoint: entry_relative,
        files,
        discs,
        patches: patched.as_ref().map(|patched| patched.patches.clone()).unwrap_or_default(),
        patched_name: patched.and_then(|patched| patched.made),
    })
}

/// A game that is one file, with the patches beside it that belong to it. We
/// apply them and put the patched game in the export in place of the file.
/// We never apply a patch to a sheet, which only lists the tracks.
fn patch_game_file(
    entrypoint: &Path,
    root: &Path,
    files: &mut [ContentFile],
) -> Result<Option<crate::patches::Patched>, String> {
    let Some(game) = files.first_mut().filter(|file| file.staged_bytes.is_none()) else {
        return Ok(None);
    };
    let beside: Vec<_> = crate::patches::in_folder(root)
        .into_iter()
        .map(|path| (path, crate::patches::Offered::Beside))
        .collect();
    let patched = crate::patches::apply_belonging(entrypoint, &beside)?;
    if let Some(patched) = &patched {
        game.staged_bytes = Some(patched.bytes.clone());
    }
    Ok(patched)
}

/// The path that `path` resolves to, after following any symbolic link.
///
/// When collecting, we refuse a file that resolves outside the game folder.
/// We read a candidate sheet only when it resolves inside, by this check.
enum FolderPath {
    Inside(PathBuf),
    Outside,
    Unreadable(std::io::Error),
}

fn folder_path(root: &Path, path: &Path) -> FolderPath {
    let root = match dunce::canonicalize(root) {
        Ok(root) => root,
        Err(error) => return FolderPath::Unreadable(error),
    };
    let resolved = match dunce::canonicalize(path) {
        Ok(resolved) => resolved,
        Err(error) => return FolderPath::Unreadable(error),
    };
    if resolved.starts_with(&root) {
        FolderPath::Inside(resolved)
    } else {
        FolderPath::Outside
    }
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
        let resolved = match folder_path(root, &sibling) {
            FolderPath::Inside(resolved) => resolved,
            FolderPath::Unreadable(error) => {
                return Err(format!(
                    "resolve {} {}: {error}",
                    companion.extension,
                    sibling.display()
                ));
            }
            FolderPath::Outside => {
                return Err(format!(
                    "{} file escapes the game content folder: {}",
                    companion.extension,
                    sibling_relative.display()
                ));
            }
        };
        gather(&resolved, &sibling_relative, root, systems, files, seen)?;
    }
    let mut followed = false;
    for reference in references {
        validate_relative_content_path(&reference)?;
        let candidate = absolute.parent().unwrap_or(root).join(&reference);
        if !candidate.is_file() {
            return Err(missing_reference(parser, &reference, absolute));
        }
        let resolved = match folder_path(root, &candidate) {
            FolderPath::Inside(resolved) => resolved,
            FolderPath::Unreadable(error) => {
                return Err(format!(
                    "resolve {} reference {}: {error}",
                    extension,
                    candidate.display()
                ));
            }
            FolderPath::Outside => {
                return Err(format!(
                    "referenced file escapes the game content folder: {}",
                    reference.display()
                ));
            }
        };
        let child_relative = match relative.parent() {
            Some(parent) if !parent.as_os_str().is_empty() => parent.join(&reference),
            _ => reference.clone(),
        };
        let before = files.len();
        gather(&resolved, &child_relative, root, systems, files, seen)?;
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
            if found.iter().any(|existing| {
                existing
                    .extension
                    .eq_ignore_ascii_case(&companion.extension)
            }) {
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
                        && candidate
                            .extension
                            .eq_ignore_ascii_case(&companion.extension)
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

/// How many sheets in the dropped file's folder we open to see whether one
/// lists the file.
///
/// We identify a dropped track, such as a GD-ROM `.bin`, by the `.gdi` beside
/// it. When someone drops a cartridge in a folder full of unrelated sheets,
/// we must not open them all, so we read up to 100, closest names first, and
/// skip the rest.
const SHEET_MATCH_LIMIT: usize = 100;

/// The size of the largest file we treat as a sheet.
///
/// A sheet is a few lines of text that name other files. A disc image with
/// the extension `.cue` or `.gdi` is not a sheet, and if we read it, we would
/// take the image into the drop. No layout that the packages describe comes
/// near a megabyte, so a larger candidate cannot be a sheet for the drop.
const SHEET_BYTE_LIMIT: u64 = 1024 * 1024;

/// The game file that a drop refers to.
///
/// For a dropped folder, or a dropped track of a GD-ROM, we use the `.gdi`
/// that lists the track. We do the same for a companion that the console
/// package declares, such as the `.sbi` beside a CHD. When a playlist in
/// that folder lists the sheet, we use the playlist, so that a player who
/// starts from one disc of a set can change to the next.
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
    let Some(directory) = path
        .parent()
        .filter(|directory| !directory.as_os_str().is_empty())
    else {
        return Ok(path.to_path_buf());
    };
    let resolved = claimed_file(directory, path)?;
    playlist_over_sheet(directory, &resolved)
}

/// The one sheet or companion that names `path`, or `path` when nothing does.
fn claimed_file(directory: &Path, path: &Path) -> Result<PathBuf, String> {
    let mut claimers = sheets_naming(directory, path)?;
    for host in companion_hosts(path) {
        if !claimers.iter().any(|existing| same_file(existing, &host)) {
            claimers.push(host);
        }
    }
    match claimers.len() {
        0 => Ok(path.to_path_buf()),
        1 => Ok(claimers.remove(0)),
        _ => Err(ambiguous_drop(path, &claimers)),
    }
}

/// When `resolved` is a disc sheet, return the playlist in the same folder
/// that lists it. A playlist is itself a sheet, but we do not apply this a
/// second time, so we leave a playlist of playlists as it was dropped.
fn playlist_over_sheet(directory: &Path, resolved: &Path) -> Result<PathBuf, String> {
    if !disc_sheet(resolved) {
        return Ok(resolved.to_path_buf());
    }
    let mut playlists = playlists_naming(directory, resolved)?;
    match playlists.len() {
        0 => Ok(resolved.to_path_buf()),
        1 => Ok(playlists.remove(0)),
        _ => Err(ambiguous_drop(resolved, &playlists)),
    }
}

fn disc_sheet(path: &Path) -> bool {
    matches!(
        declared_sheet_parser(&extension_of(path)),
        Some(parser) if parser != SheetParser::Playlist
    )
}

/// Sheets in this folder whose text lists `dropped`, closest names first.
///
/// The name of a track contains `(Track 3)` and the name of the layout does
/// not. If we read in directory order, we could reach the cap on unrelated
/// sheets and never open the one that lists the file.
fn sheets_naming(directory: &Path, dropped: &Path) -> Result<Vec<PathBuf>, String> {
    candidates_naming(directory, dropped, |_| true)
}

/// Playlists in this folder whose text lists `sheet`. We read them as in
/// [`sheets_naming`], with the same folder, parsers, size limit, symlink
/// check, order of likeness and cap. We count only playlists against the
/// cap, so unrelated cues in the folder cannot keep us from the playlist.
fn playlists_naming(directory: &Path, sheet: &Path) -> Result<Vec<PathBuf>, String> {
    candidates_naming(directory, sheet, |candidate| {
        is_playlist(&extension_of(candidate))
    })
}

fn candidates_naming(
    directory: &Path,
    dropped: &Path,
    accept: impl Fn(&Path) -> bool,
) -> Result<Vec<PathBuf>, String> {
    let mut sheets = sheet_files(directory)?;
    sheets.retain(|sheet| accept(sheet) && !same_file(sheet, dropped));
    sheets.sort_by(|left, right| {
        sheet_likeness(dropped, right)
            .cmp(&sheet_likeness(dropped, left))
            .then_with(|| left.file_name().cmp(&right.file_name()))
    });
    let mut named = Vec::new();
    for sheet in sheets.into_iter().take(SHEET_MATCH_LIMIT) {
        let Some(parser) = declared_sheet_parser(&extension_of(&sheet)) else {
            continue;
        };
        // We must not refuse the drop for a file with a sheet extension that
        // is not text. Such a file lists nothing. We do not read a file past
        // SHEET_BYTE_LIMIT, because it cannot be a sheet that lists this file.
        let Ok(metadata) = fs::metadata(&sheet) else {
            continue;
        };
        if metadata.len() > SHEET_BYTE_LIMIT {
            continue;
        }
        let Ok(text) = fs::read_to_string(&sheet) else {
            continue;
        };
        let Ok(references) = crate::discs::sheet_references(parser, &text) else {
            continue;
        };
        if references
            .iter()
            .any(|reference| reference_names(reference, dropped))
        {
            named.push(sheet);
        }
    }
    Ok(named)
}

fn sheet_files(directory: &Path) -> Result<Vec<PathBuf>, String> {
    let entries = fs::read_dir(directory)
        .map_err(|error| format!("read game folder {}: {error}", directory.display()))?;
    let mut sheets = Vec::new();
    for entry in entries {
        let entry =
            entry.map_err(|error| format!("read game folder {}: {error}", directory.display()))?;
        let path = entry.path();
        if declared_sheet_parser(&extension_of(&path)).is_none() {
            continue;
        }
        // With `is_file` a link to a file is a candidate and a directory named
        // `.cue` is not. We check with `folder_path`, as when collecting.
        if !path.is_file() {
            continue;
        }
        if !matches!(folder_path(directory, &path), FolderPath::Inside(_)) {
            continue;
        }
        sheets.push(path);
    }
    Ok(sheets)
}

fn declared_sheet_parser(extension: &str) -> Option<SheetParser> {
    let systems = systems::registry().iter().collect::<Vec<_>>();
    sheet_parser(extension, &systems)
}

/// Disc files of the same name, for which a package lists `dropped` as a
/// companion. A CHD does not name its `.sbi`, so we keep that subchannel
/// file with the disc because the package lists a companion with the same
/// stem and that extension.
fn companion_hosts(dropped: &Path) -> Vec<PathBuf> {
    let extension = extension_of(dropped);
    let Some(stem) = dropped.file_stem() else {
        return Vec::new();
    };
    let Some(directory) = dropped.parent() else {
        return Vec::new();
    };
    let mut hosts: Vec<PathBuf> = Vec::new();
    for system in systems::registry() {
        for host_extension in &system.extensions {
            let applies = system.companions.iter().any(|companion| {
                companion.extension.eq_ignore_ascii_case(&extension)
                    && companion_applies(companion, host_extension)
            });
            if !applies {
                continue;
            }
            let candidate = directory.join(stem).with_extension(host_extension);
            if candidate.is_file()
                && !same_file(&candidate, dropped)
                && !hosts.iter().any(|existing| same_file(existing, &candidate))
            {
                hosts.push(candidate);
            }
        }
    }
    hosts
}

fn reference_names(reference: &Path, dropped: &Path) -> bool {
    // A sheet may name a file in a subfolder. We look for the dropped file
    // only in its own folder, so a file named deeper down is another file.
    let components: Vec<_> = reference.components().collect();
    let [Component::Normal(name)] = components.as_slice() else {
        return false;
    };
    let Some(dropped_name) = dropped.file_name() else {
        return false;
    };
    if *name == dropped_name {
        return true;
    }
    // The name in the sheet may differ in case. It is the same file on
    // Windows and macOS, but not on a case-sensitive volume.
    if !name.eq_ignore_ascii_case(dropped_name) {
        return false;
    }
    let Some(directory) = dropped.parent() else {
        return false;
    };
    same_file(&directory.join(name), dropped)
}

fn same_file(left: &Path, right: &Path) -> bool {
    if left == right {
        return true;
    }
    match (dunce::canonicalize(left), dunce::canonicalize(right)) {
        (Ok(left), Ok(right)) => left == right,
        _ => false,
    }
}

fn sheet_likeness(dropped: &Path, sheet: &Path) -> usize {
    let dropped_name = normalize_name(&without_track_markers(&stem_text(dropped)));
    let sheet_name = normalize_name(&stem_text(sheet));
    if !dropped_name.is_empty() && dropped_name == sheet_name {
        return usize::MAX;
    }
    dropped_name
        .chars()
        .zip(sheet_name.chars())
        .take_while(|(left, right)| left == right)
        .count()
}

fn stem_text(path: &Path) -> String {
    path.file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("")
        .to_owned()
}

fn normalize_name(value: &str) -> String {
    value
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_ascii_lowercase()
}

/// Remove a label such as `(Track 3)`, which dumping tools add and which is
/// not part of the disc's name. The layout's name does not contain it, so
/// with the label left on, many unrelated names could rank above the layout.
fn without_track_markers(stem: &str) -> String {
    let mut kept = String::with_capacity(stem.len());
    let mut rest = stem;
    loop {
        let Some(open) = rest.find('(') else {
            kept.push_str(rest);
            break;
        };
        let after_open = &rest[open + 1..];
        let Some(close) = after_open.find(')') else {
            kept.push_str(rest);
            break;
        };
        kept.push_str(&rest[..open]);
        let inner = &after_open[..close];
        if !is_track_marker(inner) {
            kept.push('(');
            kept.push_str(inner);
            kept.push(')');
        }
        rest = &after_open[close + 1..];
    }
    kept.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn is_track_marker(inner: &str) -> bool {
    let mut words = inner.split_whitespace();
    let (Some(label), Some(number)) = (words.next(), words.next()) else {
        return false;
    };
    words.next().is_none()
        && label.eq_ignore_ascii_case("track")
        && !number.is_empty()
        && number.bytes().all(|byte| byte.is_ascii_digit())
}

fn ambiguous_drop(dropped: &Path, claimers: &[PathBuf]) -> String {
    let mut names: Vec<String> = claimers
        .iter()
        .map(|path| {
            path.file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_else(|| path.display().to_string())
        })
        .collect();
    names.sort();
    let dropped_name = dropped
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| dropped.display().to_string());
    format!(
        "More than one file in this folder names {dropped_name}. Drop one of them: {}",
        names.join(", ")
    )
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
            sheet.extension.eq_ignore_ascii_case(extension) && sheet.parser == SheetParser::Playlist
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
mod tests;
