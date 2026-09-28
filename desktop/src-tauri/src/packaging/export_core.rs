//! The core that we ship in a game: which file for each target of the app,
//! where each one comes from, and how we bring each into its cache before
//! we build the game.

use super::{ErrorStage, ExportError, ExportProgress, ExportRequest, ExportStage};
use crate::target::Target;
use std::path::{Path, PathBuf};

/// The core that we will ship in this export, named for its platform.
///
/// A Windows export contains `flycast_libretro.dll` even on a Mac builder,
/// where `Core::artifact` is `flycast_libretro.dylib`. We use this name to
/// check presence, download, copy and find the licence. `Target::host` is the
/// platform of the builder.
pub(super) struct ExportCore<'a> {
    pub system_name: &'a str,
    pub core: &'a crate::systems::Core,
    /// One for each target of the app, with the platform's target first.
    pub builds: Vec<CoreBuild<'a>>,
}

/// The file of the core for one target, and the cache we fetch it into.
pub(super) struct CoreBuild<'a> {
    pub platform: Target,
    pub artifact_name: &'a str,
    pub cache: Option<PathBuf>,
}

/// The core that we ship for `request`, or `None` when its console, its core
/// or an artifact for one of its targets is unknown. We name which in validation.
pub(super) fn export_core(
    request: &ExportRequest,
    targets: &[Target],
) -> Result<Option<ExportCore<'static>>, ExportError> {
    let Some(system) = crate::systems::find(&request.system) else {
        return Ok(None);
    };
    let Some(core) = system.preferred_core() else {
        return Ok(None);
    };
    let mut builds = Vec::new();
    for &platform in targets {
        let Some(artifact_name) = core.artifact_for(platform) else {
            return Ok(None);
        };
        let cache = match (request.core_cache.as_deref(), targets.first()) {
            (Some(cache), Some(&own)) => Some(
                crate::export_cores::cache_for(cache, own, platform)
                    .map_err(|message| ExportError::new(ErrorStage::Validate, message))?,
            ),
            _ => None,
        };
        builds.push(CoreBuild {
            platform,
            artifact_name,
            cache,
        });
    }
    Ok(Some(ExportCore {
        system_name: &system.name,
        core,
        builds,
    }))
}

impl ExportCore<'_> {
    pub(super) fn licence_relative(&self) -> PathBuf {
        Path::new("licenses").join(&self.core.license_file)
    }
}

impl CoreBuild<'_> {
    fn artifact_relative(&self) -> PathBuf {
        Path::new("cores").join(self.artifact_name)
    }
}

/// Where the core binary is for each target of the export. An explicit
/// `request.core` is a development override for every target. Otherwise each
/// is the file named in `export_core`, from its cache or the kit.
pub(super) fn shipped_cores(
    request: &ExportRequest,
    resolved: Option<&ExportCore<'_>>,
    targets: &[Target],
) -> Vec<(Target, PathBuf)> {
    if let Some(explicit) = &request.core {
        return targets.iter().map(|&target| (target, explicit.clone())).collect();
    }
    match resolved {
        Some(resolved) => resolved
            .builds
            .iter()
            .map(|build| {
                (
                    build.platform,
                    resolve_cached(
                        &request.runtime_kit,
                        build.cache.as_deref(),
                        &build.artifact_relative(),
                    ),
                )
            })
            .collect(),
        None => targets
            .iter()
            .map(|&target| (target, request.runtime_kit.join("cores")))
            .collect(),
    }
}

pub(super) fn prepare_core<F>(
    request: &ExportRequest,
    resolved: Option<&ExportCore<'_>>,
    progress: &mut F,
    transport: &dyn crate::cores::Transport,
) -> Result<(), ExportError>
where
    F: FnMut(ExportProgress),
{
    let Some(resolved) = resolved else {
        return Ok(());
    };
    let wanted: Vec<_> = resolved
        .builds
        .iter()
        .filter_map(|build| {
            let cache = build.cache.as_deref()?;
            let present = [build.artifact_relative(), resolved.licence_relative()]
                .iter()
                .all(|relative| resolve_cached(&request.runtime_kit, Some(cache), relative).is_file());
            Some(crate::export_cores::Wanted {
                component: &resolved.core.component,
                platform: build.platform,
                cache,
                present,
            })
        })
        .collect();
    if wanted.is_empty() {
        return Ok(());
    }
    // We write the words in the builder, and the event contains the facts for
    // them. Neither contains a URL.
    crate::export_cores::prepare(&wanted, transport, |activity| {
        progress(ExportProgress {
            stage: ExportStage::Validate,
            fraction: 0.04,
            message: activity.message(),
            cores: Some(activity.clone()),
        })
    })
    .map_err(|_| {
        ExportError::new(
            ErrorStage::Cores,
            format!(
                "The {} core could not be downloaded. Try again later.",
                resolved.system_name
            ),
        )
    })
}

/// A file that we ship in the export, from the cache or the kit. We look in
/// the cache first, because it contains what this builder downloaded, and we
/// put a newer nightly there, not in the kit.
pub(super) fn resolve_cached(kit: &Path, cache: Option<&Path>, relative: &Path) -> PathBuf {
    if let Some(cache) = cache {
        let fetched = cache.join(relative);
        if fetched.is_file() {
            return fetched;
        }
    }
    kit.join(relative)
}
