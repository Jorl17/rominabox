//! One download for both platforms. We make the game for Mac and for Windows,
//! each with the ordinary export, and zip both into `<title>.zip`, with
//! `Mac/<title>.app` and `Windows/<title>`. In the zip, the Mac programs stay
//! executable on whichever system we make it (`archive`).

use super::{archive, emit, export_game, ErrorStage, ExportError, ExportProgress, ExportRequest, ExportResult};
use super::{ExportStage, ExportTarget, OwnedStaging};
use crate::target::Target;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;

/// Each platform, and the folder its game is under in the zip.
const PLATFORMS: [(ExportTarget, &str); 2] = [(ExportTarget::Macos, "Mac"), (ExportTarget::Windows, "Windows")];

/// Make the game of `request` for both platforms and zip the two. `kit_for`
/// is the kit for each platform (`crate::kits`), and `core_cache_for` the
/// download folder for the cores of each target. We ignore the platform,
/// kit and core in the request, and use `intel_macs` only for the Mac game.
pub fn export_for_both<F: FnMut(ExportProgress)>(
    request: &ExportRequest,
    kit_for: &dyn Fn(&ExportTarget) -> Result<PathBuf, String>,
    core_cache_for: &dyn Fn(Target) -> Option<PathBuf>,
    cancelled: &AtomicBool,
    mut progress: F,
) -> Result<ExportResult, ExportError> {
    let name = crate::publish::download_name(&request.game.title);
    let destination = request.output_dir.join(&name);
    if !request.replace {
        crate::publish::refuse_existing(&destination)?;
    }
    fs::create_dir_all(&request.output_dir)
        .map_err(|error| ExportError::io(ErrorStage::Stage, &request.output_dir, error))?;
    let staging = OwnedStaging::create(&request.output_dir)?;
    let (mut runtime_bytes, mut content_bytes) = (0, 0);
    // We give each game a share of the progress, and the rest to the zip.
    let share = 0.47;
    for (index, (platform, folder)) in PLATFORMS.into_iter().enumerate() {
        let mut game = request.clone();
        game.runtime_kit = kit_for(&platform).map_err(|message| ExportError::new(ErrorStage::Refused, message))?;
        game.core = None;
        game.core_cache = core_cache_for(platform.target());
        game.game.target = platform;
        game.output_dir = staging.path().join(folder);
        game.replace = false;
        game.zip = Some(false);
        let start = share * index as f32;
        let made = export_game(&game, cancelled, |event| {
            progress(ExportProgress {
                fraction: start + share * event.fraction,
                message: format!("{folder}: {}", event.message),
                ..event
            })
        })?;
        runtime_bytes += made.runtime_bytes;
        content_bytes = made.content_bytes;
    }
    emit(&mut progress, ExportStage::Complete, 2.0 * share, "Zipping the Mac and Windows games");
    let zip = staging.path().join(&name);
    let (mac, windows) = (staging.path().join("Mac"), staging.path().join("Windows"));
    archive::write_zip(&[(&mac, "Mac"), (&windows, "Windows")], &zip)?;
    let installed_bytes = fs::metadata(&zip).map_or(0, |metadata| metadata.len());
    crate::publish::put_in_place(&zip, &destination, request.replace)?;
    staging.cleanup()?;
    emit(&mut progress, ExportStage::Complete, 1.0, "Export complete");
    Ok(ExportResult {
        app_path: destination,
        installed_bytes,
        runtime_bytes,
        content_bytes,
    })
}
