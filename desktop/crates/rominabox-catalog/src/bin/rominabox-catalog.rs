//! Developer tool for the console catalog.
//!
//!   rominabox-catalog validate [ROOT]   report every problem, exit non-zero if any
//!   rominabox-catalog list     [ROOT]   what is declared, and what each build claims
//!   rominabox-catalog assets   [ROOT]   controller illustrations that must be staged
//!   rominabox-catalog generate [ROOT]   write the compatibility registries
//!
//! With `generate` we write `desktop/systems.json` and `desktop/controls.json`
//! from the packages. The output must be byte for byte the files checked in,
//! and we check this in the parity test.

use rominabox_catalog::{Catalog, PACKAGE_ROOT};
use std::path::PathBuf;
use std::process::ExitCode;

fn repository_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../..")
        .canonicalize()
        .unwrap_or_else(|_| PathBuf::from("."))
}

fn main() -> ExitCode {
    let mut arguments = std::env::args().skip(1);
    let command = arguments.next().unwrap_or_else(|| "validate".to_string());
    let root = arguments
        .next()
        .map(PathBuf::from)
        .unwrap_or_else(|| repository_root().join(PACKAGE_ROOT));

    let catalog = match Catalog::load(&root) {
        Ok(catalog) => catalog,
        Err(problems) => {
            eprintln!("{} problem(s) in {}:\n", problems.len(), root.display());
            for problem in &problems {
                eprintln!("  {problem}");
            }
            return ExitCode::FAILURE;
        }
    };

    match command.as_str() {
        "validate" => {
            println!(
                "{} consoles, {} controller profiles, {} components — all valid",
                catalog.consoles().count(),
                catalog.profiles().count(),
                catalog.components().count()
            );
            ExitCode::SUCCESS
        }
        "list" => {
            for (id, entry) in catalog.consoles() {
                let intent = entry
                    .console
                    .support
                    .iter()
                    .map(|(target, intent)| format!("{target}={intent:?}"))
                    .collect::<Vec<_>>()
                    .join(" ");
                println!(
                    "{id:<20}{:<34}{intent}",
                    entry.console.controllers.default
                );
            }
            ExitCode::SUCCESS
        }
        // In the build scripts we read the files to stage from the catalog.
        "assets" => {
            let mut names: Vec<&str> = catalog
                .profiles()
                .filter_map(|(_, profile)| match &profile.presentation {
                    rominabox_catalog::model::Presentation::Illustrated { image } => {
                        Some(image.as_str())
                    }
                    rominabox_catalog::model::Presentation::Generic => None,
                })
                .collect();
            names.sort_unstable();
            names.dedup();
            for name in names {
                println!("{name}");
            }
            ExitCode::SUCCESS
        }
        "generate" => match rominabox_catalog::compatibility_registries(&catalog) {
            Ok(rendered) => {
                let desktop = repository_root().join("desktop");
                for (name, text) in rendered {
                    let path = desktop.join(name);
                    if let Err(error) = std::fs::write(&path, text) {
                        eprintln!("could not write {}: {error}", path.display());
                        return ExitCode::FAILURE;
                    }
                    println!("wrote {}", path.display());
                }
                ExitCode::SUCCESS
            }
            Err(error) => {
                eprintln!("could not generate: {error}");
                ExitCode::FAILURE
            }
        },
        other => {
            eprintln!("unknown command '{other}'; expected validate, list, assets or generate");
            ExitCode::FAILURE
        }
    }
}
