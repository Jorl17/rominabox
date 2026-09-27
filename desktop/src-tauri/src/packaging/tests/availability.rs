//! We give the reason when we cannot offer a console.
//!
//! We leave a declared system whose core was never prepared out of the
//! build, and we give the reason.

use super::*;
use crate::packaging::availability::core_readiness;

/// A kit containing exactly the named cores and licence texts.
/// A kit prepared on the machine running the tests.
fn kit(cores: &[(&str, bool, bool)]) -> rominabox_scratch::Scratch {
    kit_for(Target::host().expect("tests run on a target the builder builds for"), cores)
}

/// A kit prepared for `target`, whatever machine runs the tests.
fn kit_for(target: Target, cores: &[(&str, bool, bool)]) -> rominabox_scratch::Scratch {
    let root = rominabox_scratch::Scratch::dir("rominabox-availability");
    fs::create_dir_all(root.join("cores")).unwrap();
    fs::create_dir_all(root.join("licenses")).unwrap();
    for (system, artifact, licence) in cores {
        let core = crate::systems::find(system)
            .expect("known system")
            .cores
            .first()
            .expect("declared core");
        if *artifact {
            fs::write(
                root.join("cores")
                    .join(core.artifact_for(target).expect("an artifact for this target")),
                [],
            )
            .unwrap();
        }
        if *licence {
            fs::write(root.join("licenses").join(&core.license_file), []).unwrap();
        }
    }
    root
}

fn entry(root: &Path, id: &str) -> SystemAvailability {
    system_availability(root)
        .into_iter()
        .find(|entry| entry.id == id)
        .expect("every declared console is reported")
}

#[test]
fn a_core_in_the_first_boot_cache_is_enough() {
    let kit_root = kit(&[]);
    let cache = kit(&[("megadrive", true, true)]);
    let megadrive =
        system_availability_in(&kit_root, Some(&cache), Target::host())
            .into_iter()
            .find(|entry| entry.id == "megadrive")
            .expect("every declared console is reported");
    assert_eq!(megadrive.unavailable, None);
    assert_eq!(megadrive.component.as_deref(), Some("genesis_plus_gx"));
}

#[test]
fn a_prepared_console_names_the_component_that_will_run_it() {
    let root = kit(&[("megadrive", true, true)]);
    let megadrive = entry(&root, "megadrive");
    assert_eq!(megadrive.unavailable, None);
    assert_eq!(megadrive.component.as_deref(), Some("genesis_plus_gx"));
    assert!(available_systems(&root).contains(&"megadrive".to_string()));
}

#[test]
fn a_missing_artifact_is_reported_as_a_missing_artifact() {
    let root = kit(&[("megadrive", false, true)]);
    let megadrive = entry(&root, "megadrive");
    match megadrive.unavailable {
        Some(Unavailable::NoPreparedCore { ref tried }) => {
            assert_eq!(tried.len(), 1);
            assert!(
                tried[0].contains("genesis_plus_gx") && tried[0].contains("artifact"),
                "{tried:?}"
            );
        }
        other => panic!("expected a missing artifact, got {other:?}"),
    }
    assert!(!available_systems(&root).contains(&"megadrive".to_string()));
}

/// We must not ship a core whose binary is present without its licence
/// text, because we are obliged to distribute the licence with it.
#[test]
fn an_artifact_without_its_licence_is_still_unavailable() {
    let root = kit(&[("megadrive", true, false)]);
    match entry(&root, "megadrive").unavailable {
        Some(Unavailable::NoPreparedCore { ref tried }) => assert!(
            tried[0].contains("licence"),
            "the reason should name the missing licence: {tried:?}"
        ),
        other => panic!("expected a missing licence, got {other:?}"),
    }
}

/// A declared target whose file is not in this kit is a missing file. A
/// target not named in the component is a different problem, and we must
/// say which one it is in the report.
#[test]
fn a_console_with_no_artifact_for_a_target_says_exactly_that() {
    let root = kit_for(Target::MacosArm64, &[("megadrive", true, true)]);
    let windows = system_availability_for(&root, Target::WindowsX86_64)
        .into_iter()
        .find(|entry| entry.id == "megadrive")
        .expect("every console is reported for every target");
    match windows.unavailable {
        Some(Unavailable::NoPreparedCore { ref tried }) => assert!(
            tried[0].contains("artifact genesis_plus_gx_libretro.dll missing"),
            "windows is declared, so a macOS kit is missing the file: {tried:?}"
        ),
        other => panic!("expected a missing windows artifact, got {other:?}"),
    }
    // We declare every target for every shipped core, so we make the
    // component that lists only one here.
    let megadrive = crate::systems::find("megadrive").unwrap();
    let shipped = megadrive.preferred_core().unwrap();
    let named_once = crate::systems::Core {
        artifacts: [(
            Target::MacosArm64.key().to_string(),
            shipped.artifact_for(Target::MacosArm64).unwrap().to_string(),
        )]
        .into(),
        component: shipped.component.clone(),
        license: shipped.license.clone(),
        license_file: shipped.license_file.clone(),
        capabilities: Vec::new(),
        library_name: None,
        pixels: Vec::new(),
    };
    let undeclared =
        core_readiness(&root, None, &named_once, Some(Target::WindowsX86_64)).unwrap_err();
    assert!(
        undeclared.contains("no windows-x86_64 artifact declared"),
        "a target with nothing declared is a different problem from a missing file: {undeclared}"
    );
}

#[test]
fn the_same_kit_resolves_for_the_target_it_was_built_for() {
    let root = kit_for(Target::MacosArm64, &[("megadrive", true, true)]);
    let macos = system_availability_for(&root, Target::MacosArm64)
        .into_iter()
        .find(|entry| entry.id == "megadrive")
        .expect("reported");
    assert_eq!(macos.component.as_deref(), Some("genesis_plus_gx"));
    assert_eq!(macos.unavailable, None);
}

#[test]
fn every_shipped_component_declares_an_artifact_for_the_target_it_claims() {
    // A console enabled for a target with no artifact name for it would
    // be a declaration that could never resolve.
    for system in crate::systems::registry() {
        for core in &system.cores {
            assert!(
                !core.artifacts.is_empty(),
                "{} declares component {} with no artifact for any target",
                system.id,
                core.component
            );
        }
    }
}

#[test]
fn every_declared_console_is_accounted_for_either_way() {
    let root = kit(&[("megadrive", true, true)]);
    let reported = system_availability(&root);
    assert_eq!(
        reported.len(),
        crate::systems::registry().len(),
        "a console must never simply vanish from the report"
    );
    for entry in reported {
        assert_eq!(
            entry.component.is_some(),
            entry.unavailable.is_none(),
            "{} must either resolve a component or give a reason",
            entry.id
        );
    }
}
