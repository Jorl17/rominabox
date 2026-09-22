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
    let root = match fs::canonicalize(root) {
        Ok(root) => root,
        Err(error) => return FolderPath::Unreadable(error),
    };
    let resolved = match fs::canonicalize(path) {
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
    match (fs::canonicalize(left), fs::canonicalize(right)) {
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
            assert!(
                sheets(id).is_empty(),
                "{id} is a cartridge: {}",
                sheets(id).join(",")
            );
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
        let mut ids: Vec<&str> = systems::registry()
            .iter()
            .map(|system| system.id.as_str())
            .collect();
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

    /// A track file need not have the sheet's name. We use a cue with another
    /// name for a dropped `track03.bin` when the text of the cue lists it.
    #[test]
    fn a_dropped_track_is_the_sheet_that_names_it() {
        let root = fixture("track03");
        let track = root.join("track03.bin");
        fs::write(&track, b"track").unwrap();
        let sheet = root.join("anything.cue");
        fs::write(&sheet, "FILE \"track03.bin\" BINARY\n  TRACK 03 AUDIO\n").unwrap();

        let resolved = resolve_dropped(&track).unwrap();
        assert_eq!(resolved, sheet, "the track was not the disc");
        assert_eq!(
            relative_names(&collect(&resolved).unwrap()),
            relative_names(&collect(&sheet).unwrap()),
            "dropping the track collected a different set than dropping the sheet"
        );
    }

    /// The common case of a cue and the bin it lists.
    #[test]
    fn a_dropped_bin_beside_its_cue_is_that_cue() {
        let root = fixture("cue-bin");
        let track = root.join("disc.bin");
        fs::write(&track, b"data").unwrap();
        let cue = root.join("game.cue");
        fs::write(&cue, "FILE \"disc.bin\" BINARY\n  TRACK 01 MODE1/2352\n").unwrap();

        assert_eq!(resolve_dropped(&track).unwrap(), cue);
    }

    /// A CHD does not contain the subchannel file of a game such as Ape
    /// Escape. In the PlayStation package we declare that a `.sbi` goes with
    /// the disc of the same name, as if a sheet named it as a track.
    #[test]
    fn a_subchannel_file_beside_its_disc_is_that_disc() {
        let root = fixture("sbi");
        let disc = root.join("Ape Escape.chd");
        let subchannel = root.join("Ape Escape.sbi");
        fs::write(&disc, b"not a real disc").unwrap();
        fs::write(&subchannel, b"subchannel").unwrap();

        assert_eq!(resolve_dropped(&subchannel).unwrap(), disc);
    }

    /// We use the `.ccd` as the CloneCD image. The `.img` is a companion listed
    /// in the package, and we do not treat it as a cartridge in the folder.
    #[test]
    fn a_clonecd_image_beside_its_sheet_is_that_sheet() {
        let root = fixture("ccd-img");
        let sheet = root.join("game.ccd");
        let image = root.join("game.img");
        fs::write(&sheet, b"[CloneCD]\n").unwrap();
        fs::write(&image, b"data").unwrap();

        assert_eq!(resolve_dropped(&image).unwrap(), sheet);
    }

    /// Mega Drive dumps and disc tracks both often end in `.bin`. When no sheet in the
    /// folder lists a dropped `.bin`, we use that file itself.
    #[test]
    fn a_bin_beside_an_unrelated_sheet_stays_that_file() {
        let root = fixture("loose-bin");
        let cartridge = root.join("Sonic.bin");
        fs::write(&cartridge, b"SEGA").unwrap();
        fs::write(root.join("notes.gdi"), "1\n1 0 4 2352 \"other.bin\" 0\n").unwrap();

        assert_eq!(resolve_dropped(&cartridge).unwrap(), cartridge);
    }

    /// When two sheets name one track, we report both and the author chooses
    /// between them.
    #[test]
    fn two_sheets_that_name_one_file_ask_for_one() {
        let root = fixture("two-sheets");
        let track = root.join("track.bin");
        fs::write(&track, b"data").unwrap();
        fs::write(
            root.join("Alpha.cue"),
            "FILE \"track.bin\" BINARY\n  TRACK 01 MODE1/2352\n",
        )
        .unwrap();
        fs::write(root.join("Beta.gdi"), "1\n1 0 4 2352 \"track.bin\" 0\n").unwrap();

        let resolved = resolve_dropped(&track);
        let Err(error) = resolved else {
            panic!(
                "two sheets name the track but it resolved to {}",
                resolved.unwrap().display()
            );
        };
        assert!(error.contains("Alpha.cue"), "{error}");
        assert!(error.contains("Beta.gdi"), "{error}");
        assert!(
            error.to_ascii_lowercase().contains("drop"),
            "the error does not ask for one of them: {error}"
        );
    }

    /// The playlist lists the layout, and the layout lists the track. We use
    /// the playlist for a dropped track, so the player can change discs.
    #[test]
    fn a_track_is_the_playlist_that_names_the_sheet_that_names_it() {
        let root = fixture("track-playlist");
        let track = root.join("track.bin");
        fs::write(&track, b"data").unwrap();
        let gdi = root.join("game.gdi");
        fs::write(&gdi, "1\n1 0 4 2352 \"track.bin\" 0\n").unwrap();
        let playlist = root.join("game.m3u");
        fs::write(&playlist, "game.gdi\n").unwrap();

        assert_eq!(
            resolve_dropped(&track).unwrap(),
            playlist,
            "dropping the track is not dropping the playlist"
        );
    }

    /// A multi-disc game of three cue/bin pairs and a playlist that lists the
    /// three cues. Dropping the playlist, a cue or a track of another disc
    /// gives the same entry and the same files.
    #[test]
    fn dropping_any_file_of_a_multi_disc_game_is_the_playlist() {
        let root = fixture("final-fantasy-vii");
        let playlist = root.join("Final Fantasy VII.m3u");
        let mut lines = Vec::new();
        for disc in 1..=3 {
            let cue_name = format!("Final Fantasy VII (Disc {disc}).cue");
            let bin_name = format!("Final Fantasy VII (Disc {disc}).bin");
            fs::write(root.join(&bin_name), b"data").unwrap();
            fs::write(
                root.join(&cue_name),
                format!("FILE \"{bin_name}\" BINARY\n  TRACK 01 MODE1/2352\n"),
            )
            .unwrap();
            lines.push(cue_name);
        }
        fs::write(&playlist, lines.join("\n") + "\n").unwrap();

        let drops = [
            playlist.clone(),
            root.join("Final Fantasy VII (Disc 2).cue"),
            root.join("Final Fantasy VII (Disc 3).bin"),
        ];
        let listed: Vec<_> = drops
            .iter()
            .map(|dropped| {
                let resolved = resolve_dropped(dropped).unwrap();
                let set = collect(&resolved).unwrap();
                let files = relative_names(&set);
                (resolved, set.entrypoint, files)
            })
            .collect();
        assert_eq!(listed[0].0, playlist, "dropping the playlist");
        assert_eq!(listed[1].0, listed[0].0, "dropping disc 2's cue");
        assert_eq!(listed[2].0, listed[0].0, "dropping disc 3's track");
        assert_eq!(
            listed[1].1, listed[0].1,
            "disc 2's cue is a different entry"
        );
        assert_eq!(
            listed[2].1, listed[0].1,
            "disc 3's track is a different entry"
        );
        assert_eq!(
            listed[1].2, listed[0].2,
            "disc 2's cue collects different files"
        );
        assert_eq!(
            listed[2].2, listed[0].2,
            "disc 3's track collects different files"
        );
        for name in [
            "Final Fantasy VII.m3u",
            "Final Fantasy VII (Disc 1).cue",
            "Final Fantasy VII (Disc 1).bin",
            "Final Fantasy VII (Disc 2).cue",
            "Final Fantasy VII (Disc 2).bin",
            "Final Fantasy VII (Disc 3).cue",
            "Final Fantasy VII (Disc 3).bin",
        ] {
            assert!(
                listed[0].2.iter().any(|file| file == name),
                "{name} is not in the game: {:?}",
                listed[0].2
            );
        }
    }

    /// Two playlists name the sheet. Whether someone drops the track or the
    /// sheet, we ask them to choose one playlist, in the same words.
    #[test]
    fn two_playlists_that_name_one_sheet_ask_for_one() {
        let root = fixture("two-playlists");
        let track = root.join("track.bin");
        fs::write(&track, b"data").unwrap();
        let cue = root.join("game.cue");
        fs::write(&cue, "FILE \"track.bin\" BINARY\n  TRACK 01 MODE1/2352\n").unwrap();
        fs::write(root.join("Alpha.m3u"), "game.cue\n").unwrap();
        fs::write(root.join("Beta.m3u"), "game.cue\n").unwrap();

        let from_track = resolve_dropped(&track);
        let Err(from_track) = from_track else {
            panic!(
                "two playlists name the sheet but the track resolved to {}",
                from_track.unwrap().display()
            );
        };
        let from_sheet = resolve_dropped(&cue).expect_err("two playlists name the sheet");
        assert_eq!(from_track, from_sheet);
        assert!(from_track.contains("Alpha.m3u"), "{from_track}");
        assert!(from_track.contains("Beta.m3u"), "{from_track}");
        assert!(from_track.contains("game.cue"), "{from_track}");
        assert!(
            from_track.to_ascii_lowercase().contains("drop"),
            "the error does not ask for one of them: {from_track}"
        );
    }

    /// We do not read a playlist past the sheet size limit, so we use the
    /// sheet that lists the track.
    #[test]
    fn a_playlist_larger_than_the_limit_does_not_take_the_sheet() {
        let root = fixture("huge-playlist");
        let track = root.join("track03.bin");
        fs::write(&track, b"track").unwrap();
        let cue = root.join("game.cue");
        fs::write(&cue, "FILE \"track03.bin\" BINARY\n  TRACK 03 AUDIO\n").unwrap();
        fs::write(
            root.join("game.m3u"),
            text_of_length("game.cue\n", sheet_bytes(SHEET_BYTE_LIMIT) + 1),
        )
        .unwrap();

        assert_eq!(
            resolve_dropped(&track).unwrap(),
            cue,
            "a playlist past the size limit was read"
        );
    }

    /// We still read a playlist whose size is exactly the limit, and use it.
    #[test]
    fn a_playlist_at_the_size_limit_takes_the_sheet() {
        let root = fixture("playlist-at-limit");
        let track = root.join("track03.bin");
        fs::write(&track, b"track").unwrap();
        fs::write(
            root.join("game.cue"),
            "FILE \"track03.bin\" BINARY\n  TRACK 03 AUDIO\n",
        )
        .unwrap();
        let playlist = root.join("game.m3u");
        fs::write(
            &playlist,
            text_of_length("game.cue\n", sheet_bytes(SHEET_BYTE_LIMIT)),
        )
        .unwrap();

        assert_eq!(resolve_dropped(&track).unwrap(), playlist);
    }

    /// The playlist is a link to a file in the same folder. We treat a link
    /// that resolves inside the folder as a sheet, as when collecting.
    #[cfg(unix)]
    #[test]
    fn a_symlinked_playlist_inside_the_folder_takes_the_sheet() {
        let root = fixture("playlist-symlink-inside");
        let track = root.join("track.bin");
        fs::write(&track, b"data").unwrap();
        fs::write(
            root.join("game.cue"),
            "FILE \"track.bin\" BINARY\n  TRACK 01 MODE1/2352\n",
        )
        .unwrap();
        let body = root.join("body");
        fs::write(&body, "game.cue\n").unwrap();
        let playlist = root.join("game.m3u");
        std::os::unix::fs::symlink(&body, &playlist).unwrap();

        assert_eq!(
            resolve_dropped(&track).unwrap(),
            playlist,
            "a playlist linked inside the folder was not read"
        );
    }

    /// A link to a file in another folder is not a playlist in this folder.
    #[cfg(unix)]
    #[test]
    fn a_symlinked_playlist_outside_the_folder_is_not_read() {
        let root = fixture("playlist-symlink-outside");
        let elsewhere = fixture("playlist-symlink-elsewhere");
        let track = root.join("track.bin");
        fs::write(&track, b"data").unwrap();
        let cue = root.join("game.cue");
        fs::write(&cue, "FILE \"track.bin\" BINARY\n  TRACK 01 MODE1/2352\n").unwrap();
        let body = elsewhere.join("body");
        fs::write(&body, "game.cue\n").unwrap();
        std::os::unix::fs::symlink(&body, root.join("game.m3u")).unwrap();

        assert_eq!(
            resolve_dropped(&track).unwrap(),
            cue,
            "a playlist linked outside the folder was read"
        );
    }

    /// A playlist in the parent folder can have the sheet's filename and
    /// still be another game. We read only the folder of the dropped file.
    #[test]
    fn a_playlist_outside_the_dropped_files_folder_does_not_take_the_sheet() {
        let parent = fixture("playlist-parent");
        let root = parent.join("disc");
        fs::create_dir(&root).unwrap();
        let track = root.join("track.bin");
        fs::write(&track, b"data").unwrap();
        let cue = root.join("game.cue");
        fs::write(&cue, "FILE \"track.bin\" BINARY\n  TRACK 01 MODE1/2352\n").unwrap();
        fs::write(parent.join("game.m3u"), "game.cue\n").unwrap();

        assert_eq!(
            resolve_dropped(&track).unwrap(),
            cue,
            "a playlist outside the folder took the drop"
        );
    }

    /// A folder with more playlists than the cap. The playlists that sort
    /// first do not name the sheet, and the one that does has its name. In
    /// directory order we would reach the cap before it.
    #[test]
    fn the_playlist_that_names_a_sheet_is_found_among_more_decoys_than_the_cap() {
        let root = fixture("playlist-cap");
        let track = root.join("Sonic (Track 3).bin");
        fs::write(&track, b"track").unwrap();
        fs::write(
            root.join("Sonic.cue"),
            "FILE \"Sonic (Track 3).bin\" BINARY\n  TRACK 03 AUDIO\n",
        )
        .unwrap();
        let playlist = root.join("Sonic.m3u");
        fs::write(&playlist, "Sonic.cue\n").unwrap();
        for index in 0..SHEET_MATCH_LIMIT {
            let decoy = format!("Sonic (Track 3) extra {index:03}.m3u");
            fs::write(root.join(decoy), "nobody.cue\n").unwrap();
        }

        assert_eq!(
            resolve_dropped(&track).unwrap(),
            playlist,
            "the playlist that names the sheet was not chosen"
        );
    }

    /// A folder with more sheets than the cap. The sheets that sort first do
    /// not name the track. The one that does has the track's name without
    /// `(Track 3)`. In directory order we would reach the cap before it.
    #[test]
    fn the_sheet_that_names_a_track_is_found_among_more_decoys_than_the_cap() {
        let root = fixture("sheet-cap");
        let track = root.join("Sonic (Track 3).bin");
        fs::write(&track, b"track").unwrap();
        let sheet = root.join("Sonic.gdi");
        fs::write(&sheet, "1\n1 0 4 2352 \"Sonic (Track 3).bin\" 0\n").unwrap();
        for index in 0..SHEET_MATCH_LIMIT {
            let decoy = format!("Sonic (Track 3) extra {index:03}.gdi");
            fs::write(root.join(decoy), "1\n1 0 4 2352 \"nobody.bin\" 0\n").unwrap();
        }

        assert_eq!(
            resolve_dropped(&track).unwrap(),
            sheet,
            "the sheet that names the track was not chosen"
        );
    }

    /// A disc image renamed to `.cue` is not a sheet. We do not read a file past
    /// the limit, so we never take such an image into the drop.
    #[test]
    fn a_sheet_larger_than_the_limit_does_not_name_the_track() {
        let root = fixture("huge-sheet");
        let track = root.join("track03.bin");
        fs::write(&track, b"track").unwrap();
        let sheet = root.join("game.cue");
        fs::write(
            &sheet,
            cue_of_length("track03.bin", sheet_bytes(SHEET_BYTE_LIMIT) + 1),
        )
        .unwrap();

        assert_eq!(
            resolve_dropped(&track).unwrap(),
            track,
            "a sheet past the size limit was read"
        );
    }

    /// We still read a sheet whose size is exactly the limit.
    #[test]
    fn a_sheet_at_the_size_limit_names_the_track() {
        let root = fixture("sheet-at-limit");
        let track = root.join("track03.bin");
        fs::write(&track, b"track").unwrap();
        let sheet = root.join("game.cue");
        fs::write(
            &sheet,
            cue_of_length("track03.bin", sheet_bytes(SHEET_BYTE_LIMIT)),
        )
        .unwrap();

        assert_eq!(resolve_dropped(&track).unwrap(), sheet);
    }

    /// The cue is a link to a file in the same folder. We follow such a link
    /// when collecting, so we also read that sheet when someone drops a track.
    #[cfg(unix)]
    #[test]
    fn a_symlinked_sheet_inside_the_folder_names_the_track() {
        let root = fixture("symlink-inside");
        let track = root.join("track03.bin");
        fs::write(&track, b"track").unwrap();
        let body = root.join("body");
        fs::write(&body, "FILE \"track03.bin\" BINARY\n  TRACK 03 AUDIO\n").unwrap();
        let sheet = root.join("game.cue");
        std::os::unix::fs::symlink(&body, &sheet).unwrap();

        assert_eq!(
            resolve_dropped(&track).unwrap(),
            sheet,
            "a sheet linked inside the folder was not read"
        );
    }

    /// A link to a file in another folder is not a sheet in this folder.
    #[cfg(unix)]
    #[test]
    fn a_symlinked_sheet_outside_the_folder_is_not_read() {
        let root = fixture("symlink-outside");
        let elsewhere = fixture("symlink-elsewhere");
        let track = root.join("track03.bin");
        fs::write(&track, b"track").unwrap();
        let body = elsewhere.join("body");
        fs::write(&body, "FILE \"track03.bin\" BINARY\n  TRACK 03 AUDIO\n").unwrap();
        let sheet = root.join("game.cue");
        std::os::unix::fs::symlink(&body, &sheet).unwrap();

        assert_eq!(
            resolve_dropped(&track).unwrap(),
            track,
            "a sheet linked outside the folder was read"
        );
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
        assert!(
            !names.iter().any(|name| name.ends_with(".sub")),
            "{names:?}"
        );
    }

    fn sheet_bytes(limit: u64) -> usize {
        usize::try_from(limit).expect("the sheet limit fits in a file length")
    }

    fn cue_of_length(track: &str, bytes: usize) -> String {
        text_of_length(
            &format!("FILE \"{track}\" BINARY\n  TRACK 01 MODE1/2352\n"),
            bytes,
        )
    }

    fn text_of_length(head: &str, bytes: usize) -> String {
        assert!(
            head.len() <= bytes,
            "sheet text is longer than the size under test"
        );
        let mut text = head.to_owned();
        text.extend(std::iter::repeat('\n').take(bytes - text.len()));
        text
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
