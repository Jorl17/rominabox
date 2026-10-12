//! The picture filters we recommend for the games of a console. We check that
//! each list has at most five filters, each once, and that we ship each one,
//! that is, that the shader catalog beside the packages has a preset with
//! that id.

use crate::{Catalog, Diagnostic};
use std::collections::BTreeSet;
use std::path::Path;

/// The most filters we recommend for a game, so that the choice stays short.
pub const MOST: usize = 5;

/// The shader catalog, beside the folder of the packages.
fn shader_catalog(root: &Path) -> std::path::PathBuf {
    root.join("../shaders/catalog.json")
}

/// The ids of the presets in the shader catalog of `root`.
fn shipped(root: &Path) -> Result<BTreeSet<String>, String> {
    let path = shader_catalog(root);
    let text = std::fs::read_to_string(&path).map_err(|error| format!("could not read {}: {error}", path.display()))?;
    let catalog: serde_json::Value =
        serde_json::from_str(&text).map_err(|error| format!("could not read {}: {error}", path.display()))?;
    Ok(catalog["presets"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|preset| preset["id"].as_str().map(str::to_string))
        .collect())
}

/// Check the recommended filters of every console in `catalog`, whose
/// packages are in `root`.
pub(crate) fn check(catalog: &Catalog, root: &Path, problems: &mut Vec<Diagnostic>) {
    let declared: Vec<_> = catalog
        .consoles
        .iter()
        .filter_map(|(id, entry)| entry.console.recommended_filters.as_ref().map(|filters| (id, filters)))
        .collect();
    if declared.is_empty() {
        return;
    }
    let shipped = match shipped(root) {
        Ok(shipped) => shipped,
        Err(message) => {
            problems.push(Diagnostic::new("filters.no_shader_catalog", root.display().to_string(), "", message));
            return;
        }
    };
    for (package, filters) in declared {
        if filters.default.is_empty() {
            problems.push(Diagnostic::new(
                "filters.empty",
                package.as_str(),
                "recommendedFilters.default",
                "a console with recommended filters recommends at least one for every game",
            ));
        }
        for (field, list) in [("default", &filters.default), ("pal", &filters.pal)] {
            let field = format!("recommendedFilters.{field}");
            if list.len() > MOST {
                problems.push(Diagnostic::new(
                    "filters.too_many",
                    package.as_str(),
                    &field,
                    format!("{} filters, and we recommend at most {MOST}", list.len()),
                ));
            }
            let mut seen = BTreeSet::new();
            for filter in list {
                if !seen.insert(filter) {
                    problems.push(Diagnostic::new(
                        "filters.repeated",
                        package.as_str(),
                        &field,
                        format!("'{filter}' is listed twice"),
                    ));
                }
                if !shipped.contains(filter) {
                    problems.push(Diagnostic::new(
                        "filters.unknown",
                        package.as_str(),
                        &field,
                        format!("'{filter}' is not a preset in {}", shader_catalog(root).display()),
                    ));
                }
            }
        }
    }
}
