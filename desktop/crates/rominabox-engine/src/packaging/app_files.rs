//! The files of the app that we stage the same way on every platform: the
//! menu, the content, remaps, core options, firmware, controller profiles
//! and legal materials.

use super::{resolve_cached, ErrorStage, ExportError, ExportRequest};
use crate::content;
use crate::controls;
use std::fs;
use std::path::{Path, PathBuf};

/// The defaults of the player's settings that the author chose in `request`.
pub(super) fn player_defaults(request: &ExportRequest) -> crate::player_settings::Defaults {
    crate::player_settings::Defaults {
        keep_playing_in_background: request.game.keep_playing_in_background,
    }
}

/// The in-game menu that we ship in an export of `request`, for a game with
/// `discs` discs. We use this one mapping in the tests too, so that we test
/// exactly what an export would stage.
pub fn menu_request(
    request: &ExportRequest,
    discs: usize,
    licences: &[crate::licences::Row],
) -> crate::menu::MenuRequest {
    let kit = &request.runtime_kit;
    crate::menu::MenuRequest {
        palette: request.game.palette.clone(),
        background: request.game.background.clone(),
        tint_background: request.game.tint_background,
        system: request.game.system.clone(),
        controls: request.game.controls.clone(),
        hotkeys: request.game.hotkeys.clone(),
        show_menu: request.game.show_menu,
        splash: request.game.splash,
        include_achievements: request.game.include_achievements,
        menu_entries: request.game.menu_entries.clone(),
        shaders: request.game.shaders.clone(),
        shader_library: crate::shaders::kit_library(kit),
        discs,
        settings: player_defaults(request),
        sound_pack: request.game.menu_sounds != "off",
        target: request.game.target,
        licences: licences.to_vec(),
        // Controller artwork is not part of a design. We show the same pads in
        // every design, from the shared menu-assets in the kit.
        ..crate::menu::MenuRequest::new(
            crate::themes::staged_design(kit, &request.game.theme),
            kit.join("menu-assets"),
        )
    }
}

/// Write everything for the menu into `menu_assets`: the composed design,
/// the control defaults, and the logo of the splash when there is a splash.
/// Returns the controller profile we wrote the defaults for.
pub fn stage_menu(
    request: &ExportRequest,
    discs: usize,
    licences: &[crate::licences::Row],
    menu_assets: &Path,
) -> Result<controls::ControlProfile, ExportError> {
    crate::menu::compose_menu(&menu_request(request, discs, licences))
        .and_then(|menu| menu.write(menu_assets))
        .map_err(|message| ExportError::new(ErrorStage::Stage, message))?;
    fs::create_dir_all(menu_assets)
        .map_err(|error| ExportError::io(ErrorStage::Stage, menu_assets, error))?;
    let profile = controls::write_defaults_config(
        &request.game.system,
        &request.game.controls,
        &menu_assets.join(controls::DEFAULTS_FILE),
    )
    .map_err(|message| ExportError::new(ErrorStage::Stage, message))?;
    if request.game.splash {
        copy_file(
            &request.runtime_kit.join("branding/logo.png"),
            &menu_assets.join("splash-logo.png"),
        )?;
    }
    Ok(profile)
}

pub(super) fn stage_legal_materials(
    runtime_kit: &Path,
    cache: Option<&Path>,
    destination: &Path,
    core: &crate::systems::Core,
    licence: &Path,
    rows: &[super::legal::Row],
) -> Result<(), ExportError> {
    let licenses = destination.join("Licenses");
    fs::create_dir_all(&licenses)
        .map_err(|error| ExportError::io(ErrorStage::Stage, &licenses, error))?;
    copy_file(
        &runtime_kit.join("licenses/NATIVE-DEPENDENCIES.txt"),
        &licenses.join("NATIVE-DEPENDENCIES.txt"),
    )?;
    copy_optional_tree(
        &runtime_kit.join("provenance/native-rmlui"),
        &destination.join("Source-Provenance/native-rmlui"),
    )?;

    // The entries in the kit's index for this game (we copy them from
    // licenses/ in the repository with scripts/build_kit.py, one file per
    // component), then the licence text of the core, when we fetched it too.
    let mut rows = rows.to_vec();
    super::legal::copy_entries(runtime_kit, &licenses, &mut rows)?;
    let core_licence = resolve_cached(runtime_kit, cache, licence);
    if core_licence.is_file() {
        copy_file(&core_licence, &licenses.join(&core.license_file))?;
    } else if let Some(row) = rows.iter_mut().find(|row| row.group == super::legal::Group::Cores) {
        row.file.clear();
    }
    // The licence of the controller profiles, which we stage with them in
    // scripts/prepare_runtime.py and name in components.json.
    let joypad_licence = runtime_kit.join("licenses/retroarch-joypad-autoconfig.txt");
    if joypad_licence.is_file() {
        copy_file(&joypad_licence, &licenses.join("retroarch-joypad-autoconfig.txt"))?;
    }
    super::legal::write_index(&licenses, &rows)?;

    let manifest_path = runtime_kit.join("manifest.json");
    let mut manifest: serde_json::Value = serde_json::from_slice(
        &fs::read(&manifest_path)
            .map_err(|error| ExportError::io(ErrorStage::Stage, &manifest_path, error))?,
    )
    .map_err(|error| {
        ExportError::new(
            ErrorStage::Stage,
            format!("invalid {}: {error}", manifest_path.display()),
        )
    })?;
    let components = manifest
        .get_mut("components")
        .and_then(serde_json::Value::as_array_mut)
        .ok_or_else(|| {
            ExportError::new(
                ErrorStage::Stage,
                "runtime manifest has no components array",
            )
        })?;
    components.retain(|component| {
        component
            .get("name")
            .and_then(serde_json::Value::as_str)
            .is_some_and(|name| {
                name == "RetroArch"
                    || name == "RmlUi"
                    || name == "retroarch-joypad-autoconfig"
                    || name == core.component.as_str()
            })
    });
    fs::write(
        destination.join("components.json"),
        serde_json::to_vec_pretty(&manifest).unwrap(),
    )
    .map_err(|error| {
        ExportError::io(
            ErrorStage::Stage,
            &destination.join("components.json"),
            error,
        )
    })?;
    Ok(())
}

pub(super) fn copy_content_file(
    file: &content::ContentFile,
    destination_root: &Path,
) -> Result<(), ExportError> {
    let destination = destination_root.join(&file.relative);
    let parent = destination.parent().ok_or_else(|| {
        ExportError::new(
            ErrorStage::Stage,
            "game content destination has no parent directory",
        )
    })?;
    fs::create_dir_all(parent)
        .map_err(|error| ExportError::io(ErrorStage::Stage, parent, error))?;
    match &file.staging {
        content::Staging::Copy => copy_file(&file.source, &destination),
        content::Staging::Bytes(bytes) => fs::write(&destination, bytes)
            .map_err(|error| ExportError::io(ErrorStage::Stage, &destination, error)),
        content::Staging::Patched(patches) => stage_patched(&file.source, patches, &destination),
        content::Staging::Unpacked(unpacked) => stage_unpacked(&file.source, unpacked, parent),
    }
}

/// Write out a compressed disc next to where it would have gone: its sheet
/// and each track, patched when it has patches. For a patched track, we first
/// write the unpatched track next to it and remove that after patching.
fn stage_unpacked(chd: &Path, unpacked: &content::Unpacked, folder: &Path) -> Result<(), ExportError> {
    let sheet = folder.join(&unpacked.disc.sheet_name);
    fs::write(&sheet, unpacked.disc.sheet()).map_err(|error| ExportError::io(ErrorStage::Stage, &sheet, error))?;
    for (track, patches) in unpacked.disc.tracks.iter().zip(&unpacked.patches) {
        let destination = folder.join(&track.name);
        let written = if patches.is_empty() { destination.clone() } else { folder.join(format!("{}.unpacked", track.name)) };
        let write = || -> std::io::Result<()> {
            let mut out = std::io::BufWriter::new(fs::File::create(&written)?);
            crate::chd_disc::write_track(chd, track, &mut out)?;
            std::io::Write::flush(&mut out)
        };
        write().map_err(|error| ExportError::io(ErrorStage::Stage, &written, error))?;
        if !patches.is_empty() {
            let patched = stage_patched(&written, patches, &destination);
            let _ = fs::remove_file(&written);
            patched?;
        }
    }
    Ok(())
}

/// Make the game with its patches applied, at its place in the exported game.
/// When a patch fails, we refuse the export and name the patch.
fn stage_patched(game: &Path, patches: &[PathBuf], destination: &Path) -> Result<(), ExportError> {
    let paths: Vec<&Path> = patches.iter().map(PathBuf::as_path).collect();
    crate::patching::apply_files(game, &paths, destination).map_err(|(index, failure)| {
        let patch = &patches[index];
        match failure {
            crate::patching::FileFailure::Patch(error) => {
                let name = |path: &Path| path.file_name().unwrap_or_default().to_string_lossy().into_owned();
                ExportError::new(ErrorStage::Refused, error.explain(&name(patch), &name(game))).about(patch)
            }
            crate::patching::FileFailure::Io(error) => ExportError::io(ErrorStage::Stage, destination, error),
        }
    })
}

/// Copy the kit's controller profiles, one folder per driver, into the app.
/// In the launcher we then copy them into `$data_dir/autoconfig/<driver>/`,
/// the only directory that we tell RetroArch to search. We copy nothing from
/// a kit that has not been staged.
pub(super) fn stage_bundled_autoconfig(runtime_kit: &Path, destination: &Path) -> Result<(), ExportError> {
    let source = runtime_kit.join("autoconfig");
    if !source.exists() {
        return Ok(());
    }
    copy_optional_tree(&source, destination)
}

pub(super) fn stage_firmware(request: &ExportRequest, destination: &Path) -> Result<(), ExportError> {
    fs::create_dir_all(destination)
        .map_err(|error| ExportError::io(ErrorStage::Stage, destination, error))?;
    let system = crate::systems::find(&request.game.system).ok_or_else(|| {
        ExportError::new(
            ErrorStage::Stage,
            format!("unsupported system: {}", request.game.system),
        )
    })?;
    for source in &request.game.firmware {
        let name = firmware_destination_name(source, system).ok_or_else(|| {
            ExportError::new(
                ErrorStage::Stage,
                format!("firmware has no filename: {}", source.display()),
            )
        })?;
        copy_file(source, &destination.join(name))?;
    }
    Ok(())
}

pub(super) fn firmware_destination_name(source: &Path, system: &crate::systems::System) -> Option<String> {
    let source_name = source.file_name()?.to_str()?;
    system
        .firmware
        .iter()
        .flat_map(|requirement| &requirement.accepted_names)
        .find(|accepted| accepted.eq_ignore_ascii_case(source_name))
        .cloned()
        .or_else(|| Some(source_name.to_string()))
}

/// Write the emulated controller, and the controls that the author moved on
/// the pad, to the remap file for RetroArch.
///
/// `input_libretro_device_p1` looks like an ordinary setting, but the only
/// read of it in `configuration.c` is inside `input_remapping_load_file`,
/// not in the loading of `retroarch.cfg`. A device in the config has no
/// effect, and the default device of the core stays in use.
///
/// So we write the file to the path in `config_load_remap`,
/// `<remap dir>/<library name>/<library name>.rmp`, with the libretro library
/// name of the artifact. It contains the lines that
/// `pad_positions::remap_file` returns for `profile`, the author's controls
/// as `placed`, and whether every pad plays as player 1, which also has an
/// effect only in a remap.
pub(super) fn stage_controller_remap(
    profile: &controls::ControlProfile,
    placed: &[crate::pad_positions::Placed],
    every_pad_is_player_one: bool,
    core: &crate::systems::Core,
    remaps: &Path,
) -> Result<(), ExportError> {
    let contents = controls::pad_positions()
        .and_then(|positions| {
            crate::pad_positions::remap_file(profile, placed, &positions, every_pad_is_player_one)
        })
        .map_err(|error| ExportError::new(ErrorStage::Stage, error))?;
    if contents.is_empty() {
        // A pad that is the core's default device, with nothing moved and
        // each pad a separate player, gets no remap at all.
        return Ok(());
    }
    let Some(library) = core.library_name.as_deref() else {
        return Err(ExportError::new(
            ErrorStage::Stage,
            format!(
                "{} needs a remap, but component '{}' does not declare its libraryName, so \
                 there is nowhere to write the remap RetroArch reads",
                profile.id, core.component
            ),
        ));
    };
    let directory = remaps.join(library);
    fs::create_dir_all(&directory)
        .map_err(|error| ExportError::io(ErrorStage::Stage, &directory, error))?;
    let path = directory.join(format!("{library}.rmp"));
    fs::write(&path, contents).map_err(|error| ExportError::io(ErrorStage::Stage, &path, error))?;
    Ok(())
}

/// Write the options that show the pixels of this core as they are.
///
/// We write them to `<config dir>/<library name>/<library name>.opt`, the
/// path for per-core options in RetroArch, with the library name of the
/// remap. For a core with nothing declared we write no file, so the core
/// defaults apply. In the launcher, we copy the file into the game's config
/// directory on first launch, and keep a file the player already changed.
pub(super) fn stage_pixel_options(core: &crate::systems::Core, destination: &Path) -> Result<(), ExportError> {
    if core.pixels.is_empty() {
        return Ok(());
    }
    let Some(library) = core.library_name.as_deref() else {
        return Err(ExportError::new(
            ErrorStage::Stage,
            format!(
                "component '{}' keeps its picture with core options, but does not declare its \
                 libraryName, so there is nowhere to write the options file RetroArch reads",
                core.component
            ),
        ));
    };
    if library.is_empty()
        || library.contains(['/', '\\'])
        || core.pixels.iter().any(|option| {
            option.key.is_empty()
                || option.value.is_empty()
                || option.key.contains([' ', '=', '"', '\n', '/'])
                || option.value.contains(['"', '\n'])
        })
    {
        return Err(ExportError::new(
            ErrorStage::Stage,
            format!(
                "component '{}' has a picture option that cannot be written as a core options line",
                core.component
            ),
        ));
    }
    let directory = destination.join(library);
    fs::create_dir_all(&directory)
        .map_err(|error| ExportError::io(ErrorStage::Stage, &directory, error))?;
    let path = directory.join(format!("{library}.opt"));
    let mut contents = String::new();
    for option in &core.pixels {
        contents.push_str(&format!("{} = \"{}\"\n", option.key, option.value));
    }
    fs::write(&path, contents).map_err(|error| ExportError::io(ErrorStage::Stage, &path, error))?;
    Ok(())
}

pub(super) fn copy_file(source: &Path, destination: &Path) -> Result<(), ExportError> {
    let source_metadata = fs::symlink_metadata(source)
        .map_err(|error| ExportError::io(ErrorStage::Stage, source, error))?;
    if source_metadata.file_type().is_symlink() || !source_metadata.is_file() {
        return Err(ExportError::new(
            ErrorStage::Stage,
            format!("refusing to stage non-regular file: {}", source.display()),
        ));
    }
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| ExportError::io(ErrorStage::Stage, parent, error))?;
    }
    fs::copy(source, destination)
        .map_err(|error| ExportError::io(ErrorStage::Stage, destination, error))?;
    Ok(())
}

fn copy_optional_tree(source: &Path, destination: &Path) -> Result<(), ExportError> {
    let source_metadata = fs::symlink_metadata(source)
        .map_err(|error| ExportError::io(ErrorStage::Stage, source, error))?;
    if source_metadata.file_type().is_symlink() {
        return Err(ExportError::new(
            ErrorStage::Stage,
            format!(
                "refusing to stage symlinked directory: {}",
                source.display()
            ),
        ));
    }
    if !source_metadata.is_dir() {
        return Ok(());
    }
    fs::create_dir_all(destination)
        .map_err(|error| ExportError::io(ErrorStage::Stage, destination, error))?;
    for entry in
        fs::read_dir(source).map_err(|error| ExportError::io(ErrorStage::Stage, source, error))?
    {
        let entry = entry.map_err(|error| ExportError::io(ErrorStage::Stage, source, error))?;
        let target = destination.join(entry.file_name());
        let file_type = entry
            .file_type()
            .map_err(|error| ExportError::io(ErrorStage::Stage, &entry.path(), error))?;
        if file_type.is_symlink() {
            return Err(ExportError::new(
                ErrorStage::Stage,
                format!("refusing to stage symlink: {}", entry.path().display()),
            ));
        }
        if file_type.is_dir() {
            copy_optional_tree(&entry.path(), &target)?;
        } else {
            copy_file(&entry.path(), &target)?;
        }
    }
    Ok(())
}

pub(super) fn make_executable(path: &Path) -> Result<(), ExportError> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut permissions = fs::metadata(path)
            .map_err(|error| ExportError::io(ErrorStage::Stage, path, error))?
            .permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(path, permissions)
            .map_err(|error| ExportError::io(ErrorStage::Stage, path, error))?;
    }
    // Windows has no executable bit. A file runs because of its extension.
    #[cfg(windows)]
    let _ = path;
    Ok(())
}

pub(super) fn tree_size(path: &Path) -> Result<u64, ExportError> {
    if !path.exists() {
        return Ok(0);
    }
    let metadata = fs::symlink_metadata(path)
        .map_err(|error| ExportError::io(ErrorStage::Measure, path, error))?;
    if metadata.is_file() {
        return Ok(metadata.len());
    }
    let mut total = 0;
    for entry in
        fs::read_dir(path).map_err(|error| ExportError::io(ErrorStage::Measure, path, error))?
    {
        let entry = entry.map_err(|error| ExportError::io(ErrorStage::Measure, path, error))?;
        total += tree_size(&entry.path())?;
    }
    Ok(total)
}

/// Why we do not export a game with patches for a compressed disc until the
/// author has chosen whether to include them.
pub(super) fn compressed_refusal(compressed: &content::CompressedPatches, game: &Path) -> ExportError {
    let name = |path: &Path| path.file_name().unwrap_or_default().to_string_lossy().into_owned();
    let patches: Vec<String> = compressed.patches.iter().map(|patch| format!("\"{}\"", name(patch))).collect();
    ExportError::new(
        ErrorStage::Refused,
        format!(
            "{} {} \"{}\", which is compressed. Choose whether to include the patch, which decompresses the game, or leave it out.",
            patches.join(", "),
            if patches.len() == 1 { "changes" } else { "change" },
            name(game)
        ),
    )
}

/// We merge the PPF patches of a PlayStation disc into the one PPF file for
/// the emulator and ship it. In the launcher we copy it into the game's data,
/// named after the serial of the disc as required by the emulator
/// (crate::discs::playstation_patch_name).
pub(super) fn stage_played_patches(patches: &[PathBuf], game: &Path, folder: &Path) -> Result<(), ExportError> {
    let Some(first) = patches.first() else {
        return Ok(());
    };
    let name = |path: &Path| path.file_name().unwrap_or_default().to_string_lossy().into_owned();
    let serial = crate::discs::playstation_patch_name(game).ok_or_else(|| {
        ExportError::new(
            ErrorStage::Refused,
            format!(
                "\"{}\" is found by the disc's serial while the game runs, and the serial of \"{}\" could not be read.",
                name(first),
                name(game)
            ),
        )
        .about(first)
    })?;
    let records = patches
        .iter()
        .map(|patch| {
            let bytes = fs::read(patch).map_err(|error| ExportError::io(ErrorStage::Stage, patch, error))?;
            crate::ppf::records(&bytes).map_err(|why| {
                ExportError::new(ErrorStage::Refused, format!("\"{}\" is {why}.", name(patch))).about(patch)
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    fs::create_dir_all(folder).map_err(|error| ExportError::io(ErrorStage::Stage, folder, error))?;
    let destination = folder.join(serial);
    fs::write(&destination, crate::ppf::merged(&records)).map_err(|error| ExportError::io(ErrorStage::Stage, &destination, error))
}
