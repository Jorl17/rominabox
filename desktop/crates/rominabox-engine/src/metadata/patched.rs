//! The lookup of a game that comes with patches (crate::patches).
//!
//! A patched game is a different game, so we look it up first. When the
//! catalogue has it, we use its name and picture from the catalogue. When it
//! does not, we take the name from the patch, either the name of the output
//! file in the xdelta header or the file name of the patch, and we use the
//! picture of the original game, because the patched game is still that game.
//!
//! We make the patched game once, in the metadata cache, under the hash of
//! the bytes of the original and the patches, and look it up there each time.

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
    let patches = set.patches();
    let Some(first_patch) = patches.first() else {
        return Ok(original);
    };
    // We recognise a disc by its serial, which a patch rarely changes, so the
    // patched disc would match the original. We do not make it to find that
    // out, and we take its name from the patch.
    if systems::find(&original.system).is_some_and(|system| system.category == "disc") {
        return Ok(Inspection {
            title: patch_title(&first_patch.file_name().unwrap_or_default().to_string_lossy()),
            source: MetadataSource::Filename,
            ..original
        });
    }
    let named = set
        .patched_name
        .clone()
        .or_else(|| first_patch.file_name().map(|name| name.to_string_lossy().into_owned()))
        .unwrap_or_else(|| original.filename.clone());
    // With the extension of the original, because we find the console by it.
    let extension = Path::new(&original.filename)
        .extension()
        .map(|extension| extension.to_string_lossy().into_owned())
        .unwrap_or_default();
    let stem = Path::new(&named).file_stem().map(|stem| stem.to_string_lossy().into_owned());
    let file_name = format!("{}.{extension}", stem.unwrap_or_else(|| "patched".into()));
    // We report a failing patch at export, and keep the lookup.
    let Ok(file) = made_once(&rom, &patches, &file_name, cache) else {
        return Ok(original);
    };
    // We read the patched game as we read any dropped game.
    let patched = inspect_file(&file, cache, online, Some(&original.system))?;
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
            title: patch_title(&named),
            source: MetadataSource::Filename,
            matched: false,
            catalog_name: None,
            ..original
        }
    })
}

/// The title from the name of a patch, made as from a catalogue name, without
/// the region and language tags, which describe the original.
fn patch_title(name: &str) -> String {
    display_title(&filename_title(name))
}

/// The patched game in the cache, at `patched/<hash>/<file_name>`, which we
/// make when it is not there yet. We hash the bytes of the original and the
/// patches, and the name.
fn made_once(rom: &Path, patches: &[PathBuf], file_name: &str, cache: &Path) -> io::Result<PathBuf> {
    use sha2::Digest;
    let mut hash = sha2::Sha256::new();
    crate::patches::feed_patched(&mut hash, rom, patches).map_err(|(_, error)| error)?;
    hash.update(file_name.as_bytes());
    let folder = cache.join("patched").join(format!("{:x}", hash.finalize()));
    let file = folder.join(file_name);
    if file.is_file() {
        return Ok(file);
    }
    fs::create_dir_all(&folder)?;
    // We write it under another name and then rename it, so if a lookup
    // stops part way, there is no file that looks finished.
    let making = folder.join(format!(
        "making-{}-{}",
        std::process::id(),
        PATCHED_LOOKUPS.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    let paths: Vec<&Path> = patches.iter().map(PathBuf::as_path).collect();
    crate::patching::apply_files(rom, &paths, &making).map_err(|(_, failure)| match failure {
        crate::patching::FileFailure::Patch(error) => io::Error::other(format!("{error:?}")),
        crate::patching::FileFailure::Io(error) => error,
    })?;
    fs::rename(&making, &file)?;
    Ok(file)
}

/// We write a separate temporary file first for each patched game we make.
static PATCHED_LOOKUPS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
