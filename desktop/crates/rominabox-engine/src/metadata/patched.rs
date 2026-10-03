//! The lookup of a game that comes with patches (crate::patches).
//!
//! A patched game is a different game, so we look it up first. When the
//! catalogue has it, we use its name and picture from the catalogue. When it
//! does not, we take the name from the patch, either the name of the output
//! file in the xdelta header or the file name of the patch, and we use the
//! picture of the original game, because the patched game is still that game.

use super::*;

/// `original`, the lookup of the dropped game, combined with the lookup of
/// the patched game when the game comes with patches.
pub(super) fn join(
    original: Inspection,
    rom: &Path,
    cache: &Path,
    online: bool,
    files: &content::GameFiles,
) -> Result<Inspection, InspectionError> {
    if original.system.is_empty() {
        return Ok(original);
    }
    let Ok(rom) = content::resolve_dropped(rom) else {
        return Ok(original);
    };
    // We report at export that a game cannot be exported, and keep the lookup.
    let Ok(set) = content::collect_with(&rom, Some(&original.system), files) else {
        return Ok(original);
    };
    let Some(bytes) = set.files.first().and_then(|game| game.staged_bytes.as_deref()) else {
        return Ok(original);
    };
    let Some(first_patch) = set.patches.first() else {
        return Ok(original);
    };
    let named = set
        .patched_name
        .clone()
        .or_else(|| first_patch.file_name().map(|name| name.to_string_lossy().into_owned()))
        .unwrap_or_else(|| original.filename.clone());
    let patched = inspect_patched(bytes, &named, &original, cache, online)?;
    Ok(if patched.matched {
        // With no picture of the patched game, we use the original's picture.
        let warnings = match (&patched.icon_path, &original.icon_path) {
            (None, Some(_)) => patched.warnings.iter().filter(|warning| *warning != NO_COVER).cloned().collect(),
            _ => patched.warnings.clone(),
        };
        Inspection {
            icon_path: patched.icon_path.or(original.icon_path),
            filename: original.filename,
            warnings,
            ..patched
        }
    } else {
        Inspection {
            title: filename_title(&named),
            source: MetadataSource::Filename,
            matched: false,
            catalog_name: None,
            ..original
        }
    })
}

/// The lookup of the patched game, from a file of its bytes under its given
/// name, which we read as we read any dropped game.
fn inspect_patched(
    bytes: &[u8],
    named: &str,
    original: &Inspection,
    cache: &Path,
    online: bool,
) -> Result<Inspection, InspectionError> {
    // With the extension of the original, because we find the console by it.
    let extension = Path::new(&original.filename)
        .extension()
        .map(|extension| extension.to_string_lossy().into_owned())
        .unwrap_or_default();
    let stem = Path::new(named).file_stem().map(|stem| stem.to_string_lossy().into_owned());
    let file_name = format!("{}.{extension}", stem.unwrap_or_else(|| "patched".into()));
    let folder = std::env::temp_dir().join(format!(
        "rominabox-patched-{}-{}",
        std::process::id(),
        PATCHED_LOOKUPS.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    fs::create_dir_all(&folder)?;
    let file = folder.join(&file_name);
    let looked_up = fs::write(&file, bytes)
        .map_err(InspectionError::from)
        .and_then(|()| inspect_file(&file, cache, online, Some(&original.system)));
    // We remove the one file we wrote and then the folder we made for it,
    // because nothing else is in that folder.
    let _ = fs::remove_file(&file);
    let _ = fs::remove_dir(&folder);
    looked_up
}

/// We write a separate folder for each lookup of a patched game.
static PATCHED_LOOKUPS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
