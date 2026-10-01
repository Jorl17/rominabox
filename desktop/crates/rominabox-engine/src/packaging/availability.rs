//! Which consoles we can export with a runtime kit, and why not the others.

use super::resolve_cached;
use crate::target::Target;
use serde::Serialize;
use std::path::Path;

/// Return the canonical systems that we can export with this runtime kit.
/// We judge from the declared core files, without searching any global
/// RetroArch path or loading any core.
/// Why we cannot offer a declared console in this build.
///
/// For each console we leave out, we give a reason that a developer can act
/// on, even though the list that people see stays short.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase", tag = "reason")]
pub enum Unavailable {
    /// We declare no core at all, so the game cannot run.
    NoCoreDeclared,
    /// The artifact is missing for every declared core.
    NoPreparedCore { tried: Vec<String> },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemAvailability {
    pub id: String,
    /// The component that we will use, when we find one.
    pub component: Option<String>,
    pub unavailable: Option<Unavailable>,
}

/// Resolve every declared console against a prepared kit, with reasons.
///
/// We try every core in the declared order of preference, not only the
/// first, so we do not report a console as missing when its preferred core
/// is absent and another core works.
pub fn system_availability(runtime_kit: &Path) -> Vec<SystemAvailability> {
    system_availability_in(runtime_kit, None, Target::host())
}

/// Resolve availability for a named target.
///
/// We take the target as an argument instead of using the running one, so on
/// macOS we can answer "would this console work on Windows?", and we can
/// test that question at all.
pub fn system_availability_for(runtime_kit: &Path, target: Target) -> Vec<SystemAvailability> {
    system_availability_in(runtime_kit, None, Some(target))
}

/// Resolve availability, looking also in the cache of cores fetched at export.
///
/// We search the cache first. It contains the cores we download while making
/// an app, in the same `cores/` and `licenses/` layout, and it is not a
/// global RetroArch directory. `target` is `None` for a machine that we do
/// not build for, where no core is ready.
pub fn system_availability_in(
    runtime_kit: &Path,
    cache: Option<&Path>,
    target: Option<Target>,
) -> Vec<SystemAvailability> {
    crate::systems::registry()
        .iter()
        .map(|system| {
            if system.cores.is_empty() {
                return SystemAvailability {
                    id: system.id.clone(),
                    component: None,
                    unavailable: Some(Unavailable::NoCoreDeclared),
                };
            }
            let mut tried = Vec::new();
            for core in &system.cores {
                match core_readiness(runtime_kit, cache, core, target) {
                    Ok(()) => {
                        return SystemAvailability {
                            id: system.id.clone(),
                            component: Some(core.component.clone()),
                            unavailable: None,
                        };
                    }
                    Err(missing) => tried.push(missing),
                }
            }
            SystemAvailability {
                id: system.id.clone(),
                component: None,
                unavailable: Some(Unavailable::NoPreparedCore { tried }),
            }
        })
        .collect()
}

/// Whether the file of one core is in the cache or the kit for a target, or
/// the name of what is missing.
pub(super) fn core_readiness(
    runtime_kit: &Path,
    cache: Option<&Path>,
    core: &crate::systems::Core,
    target: Option<Target>,
) -> Result<(), String> {
    let Some(filename) = target.and_then(|target| core.artifact_for(target)) else {
        // The component exists, but we declare nothing in it for this target,
        // which is a different problem from a missing file.
        return Err(match target {
            Some(target) => format!("{} (no {target} artifact declared)", core.component),
            None => format!("{} (this machine is not one the builder builds for)", core.component),
        });
    };
    // We check only the core, because a missing licence text never makes a
    // console unavailable.
    let artifact = resolve_cached(runtime_kit, cache, &Path::new("cores").join(filename));
    if artifact.is_file() {
        return Ok(());
    }
    // We name what is missing, so the author can tell "this console is gone"
    // from "this core was never prepared".
    Err(format!("{} (artifact {filename} missing)", core.component))
}

pub fn available_systems(runtime_kit: &Path) -> Vec<String> {
    system_availability(runtime_kit)
        .into_iter()
        .filter(|entry| entry.unavailable.is_none())
        .map(|entry| entry.id)
        .collect()
}
