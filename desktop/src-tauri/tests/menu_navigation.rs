//! Arrow keys, pointer and focus on every composed menu screen, headless.
//!
//! We compose every design in the registry and every hypothetical design
//! under `tests/fixtures/designs/` with `themes::compose_menu` for a few
//! pads, and run the menu C++ on each composition in `menu_nav_driver`
//! (built in the `navigation` scope, found through
//! `ROMINABOX_NAVIGATION_DRIVER`). We load the document in the driver with a
//! fake RetroArch host: no window, no GL, no sound. Then we press keys, move
//! the pointer and record, after every step, the element the player sees
//! highlighted, the screen and the sounds played.
//!
//! The case tables, `scripts/fixtures/navigation/*.json`, list what each
//! step should highlight, per design and from the picture a person sees.
//! There is no second spatial search here: a table contains ids.
//!
//! A case can have a `red` marker for a design: the bug, why it happens, and
//! the current result (`now`). We accept the case while the result is
//! exactly `now`, and fail it once the result is the expected one ("fixed:
//! remove the red marker"), so we report a red case that goes wrong for
//! another reason instead of hiding it.
//!
//! Without the driver variable we fail the test: it runs only the driver.

use rominabox_desktop::{controls::Controls, repo, shaders::ShaderSelection, themes};
use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

/// The pads available to a case, by name: the system and the controller we
/// export the composition with.
const MENUS: [(&str, &str, Option<&str>); 4] = [
    ("md3", "megadrive", None),
    ("md6", "megadrive", Some("megadrive6")),
    ("gb", "gbc", None),
    ("ps1-analog", "ps1", Some("ps1-analog")),
];

fn copy_tree(from: &Path, to: &Path) {
    fs::create_dir_all(to).unwrap();
    for entry in fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let destination = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_tree(&entry.path(), &destination);
        } else {
            fs::copy(entry.path(), destination).unwrap();
        }
    }
}

fn fixture_designs() -> PathBuf {
    repo::at("desktop/src-tauri/tests/fixtures/designs")
}

/// The designs a table must cover: registered ones, then hypothetical ones.
fn all_designs() -> (Vec<String>, Vec<String>) {
    let registered = themes::registry()
        .unwrap()
        .designs
        .into_iter()
        .map(|design| design.id)
        .collect();
    let mut hypothetical: Vec<String> = fs::read_dir(fixture_designs())
        .unwrap()
        .map(|entry| entry.unwrap())
        .filter(|entry| entry.file_type().unwrap().is_dir())
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect();
    hypothetical.sort();
    (registered, hypothetical)
}

/// A kit as an export uses it. We look for Native beside the design when we
/// compose, so we copy a hypothetical design next to the actual ones.
fn kit(root: &Path, hypothetical: &[String]) -> PathBuf {
    let kit = root.join("runtime-kit");
    copy_tree(&repo::at("integrations/designs"), &kit.join("designs"));
    for name in hypothetical {
        let target = kit.join("designs").join(name);
        copy_tree(&fixture_designs().join(name), &target);
        // A style-only fixture contains only its changes to the Native
        // stylesheet. The design it represents has the whole sheet.
        let changes = target.join("changes.rcss");
        if changes.is_file() {
            let mut sheet =
                fs::read_to_string(repo::at("integrations/designs/native/menu.rcss")).unwrap();
            sheet.push_str("\n/* The hypothetical design's own rules. */\n");
            sheet.push_str(&fs::read_to_string(&changes).unwrap());
            fs::write(target.join("menu.rcss"), sheet).unwrap();
            fs::remove_file(changes).unwrap();
        }
    }
    copy_tree(
        &repo::at("desktop/assets/controllers"),
        &kit.join("menu-assets"),
    );
    kit
}

/// The menu composed from one design and pad, with every Options entry in
/// the design and the bundled shaders and achievements, as in a full export.
fn compose(kit: &Path, design: &str, system: &str, profile: Option<&str>, to: &Path) {
    let staged = themes::staged_design(kit, design);
    let entries: Vec<String> = themes::declared_screens(&staged)
        .unwrap()
        .into_iter()
        .filter(|screen| screen.option_label.is_some())
        .map(|screen| screen.id)
        .collect();
    let catalog = rominabox_desktop::shaders::catalog().unwrap();
    let shaders = ShaderSelection {
        bundled: catalog.iter().map(|entry| entry.id.clone()).collect(),
        custom: Vec::new(),
        initial: None,
    };
    let controls = Controls {
        profile: profile.map(str::to_owned),
        ..Controls::default()
    };
    fs::create_dir_all(to).unwrap();
    themes::compose_menu(
        &themes::MenuRequest {
            kit,
            design,
            palette: "blue",
            background: None,
            system,
            controls: &controls,
            show_menu: true,
            splash: false,
            include_achievements: true,
            menu_entries: Some(&entries),
            shaders: &shaders,
        },
        to,
    )
    .unwrap_or_else(|error| panic!("{design}/{system}: composition failed: {error}"));
    rominabox_desktop::controls::write_defaults_config_with_advanced_access(
        system,
        &controls,
        &to.join("controls-defaults.cfg"),
        false,
    )
    .unwrap();
}

/// One case table, `scripts/fixtures/navigation/<name>.json`.
struct Table {
    name: String,
    designs: Vec<String>,
    cases: Vec<Value>,
}

fn tables() -> Vec<Table> {
    let directory = repo::at("scripts/fixtures/navigation");
    let mut found: Vec<PathBuf> = fs::read_dir(&directory)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension == "json")
        })
        .collect();
    found.sort();
    assert!(
        !found.is_empty(),
        "no case tables in {}",
        directory.display()
    );
    found
        .into_iter()
        .map(|path| {
            let name = path.file_stem().unwrap().to_string_lossy().into_owned();
            let table: Value = serde_json::from_slice(&fs::read(&path).unwrap())
                .unwrap_or_else(|error| panic!("{name}: {error}"));
            let designs = table["designs"]
                .as_array()
                .unwrap_or_else(|| panic!("{name}: no designs list"))
                .iter()
                .map(|design| design.as_str().unwrap().to_owned())
                .collect();
            let cases = table["cases"]
                .as_array()
                .unwrap_or_else(|| panic!("{name}: no cases"))
                .clone();
            Table {
                name,
                designs,
                cases,
            }
        })
        .collect()
}

/// A per-design value from a case field: the design's own entry, else `*`.
fn for_design<'a>(field: &'a Value, design: &str) -> Option<&'a Value> {
    field.get(design).or_else(|| field.get("*"))
}

/// The steps of a case for a design: one list for all designs, or one per
/// design where the designs reach the same screen differently.
fn steps_for<'a>(case: &'a Value, design: &str) -> &'a Vec<Value> {
    let steps = &case["steps"];
    steps
        .as_array()
        .or_else(|| for_design(steps, design).and_then(Value::as_array))
        .unwrap_or_else(|| panic!("{}: no steps for {design}", case["name"]))
}

/// Every element id in a case for a design, so that we can report the ones
/// missing from the composed document. Otherwise an expected id that does not
/// exist would match "nothing there" in the wrong places.
fn named_ids(case: &Value, design: &str) -> BTreeSet<String> {
    let mut ids = BTreeSet::new();
    let mut add = |value: Option<&Value>| {
        if let Some(Value::Array(steps)) = value {
            for step in steps {
                // One id, or several highlighted at once.
                let named = match step {
                    Value::Array(several) => several.iter().collect(),
                    one => vec![one],
                };
                for id in named.into_iter().filter_map(Value::as_str) {
                    ids.insert(id.to_owned());
                }
            }
        }
    };
    add(for_design(&case["focused"], design));
    for step in steps_for(case, design) {
        let step = step.as_str().unwrap();
        if let Some(id) = step.strip_prefix("hover:") {
            ids.insert(id.to_owned());
        } else if !step.contains(':') {
            ids.insert(step.to_owned());
        }
    }
    ids
}

/// The step results expected in a case for a design, in the driver's format.
fn expected(case: &Value, design: &str) -> Value {
    let mut out = serde_json::Map::new();
    for field in ["focused", "screen", "sounds"] {
        if let Some(value) = case.get(field).and_then(|value| for_design(value, design)) {
            out.insert(field.to_owned(), value.clone());
        }
    }
    Value::Object(out)
}

/// What we observed in the driver, reduced to the fields in the expectation.
fn observed(steps: &[Value], wanted: &Value) -> Value {
    let mut out = serde_json::Map::new();
    for field in wanted.as_object().unwrap().keys() {
        let values: Vec<Value> = steps.iter().map(|step| step[field].clone()).collect();
        let value = match field.as_str() {
            // The screen the case ends on.
            "screen" => values.last().cloned().unwrap_or(Value::Null),
            _ => Value::Array(values),
        };
        out.insert(field.clone(), value);
    }
    Value::Object(out)
}

/// Run every case in the tables for a design in one driver process per design.
fn run_driver(driver: &Path, script: &str) -> Vec<Value> {
    let mut child = Command::new(driver)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap_or_else(|error| panic!("could not start {}: {error}", driver.display()));
    child
        .stdin
        .take()
        .unwrap()
        .write_all(script.as_bytes())
        .unwrap();
    let output = child.wait_with_output().unwrap();
    if std::env::var_os("ROMINABOX_NAVIGATION_DUMP").is_some() {
        eprint!("{}", String::from_utf8_lossy(&output.stderr));
    }
    assert!(
        output.status.success(),
        "the driver failed\n{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter(|line| line.starts_with('{'))
        .map(|line| serde_json::from_str(line).unwrap_or_else(|error| panic!("{error}: {line}")))
        .collect()
}

#[test]
fn arrows_pointer_and_focus_follow_every_composed_layout() {
    let driver = std::env::var_os("ROMINABOX_NAVIGATION_DRIVER")
        .map(PathBuf::from)
        .expect("ROMINABOX_NAVIGATION_DRIVER names the driver; run `python3 scripts/test.py navigation`");
    let (registered, hypothetical) = all_designs();
    let tables = tables();

    // Every registered design must be in every table, so that we write
    // expectations for a new design instead of running none for it.
    for table in &tables {
        for design in &registered {
            assert!(
                table.designs.contains(design),
                "{}: the registry's design {design} has no expectations",
                table.name
            );
        }
        for design in &table.designs {
            assert!(
                registered.contains(design) || hypothetical.contains(design),
                "{}: {design} is neither registered nor a fixture",
                table.name
            );
        }
        assert!(
            !table.cases.is_empty(),
            "{}: the table is empty",
            table.name
        );
    }

    let scratch = rominabox_scratch::Scratch::dir("rominabox-navigation");
    let keep = std::env::var_os("ROMINABOX_NAVIGATION_KEEP").map(PathBuf::from);
    let root = keep.clone().unwrap_or_else(|| scratch.to_path_buf());
    let kit = kit(&root, &hypothetical);

    // Compose only what the tables ask for.
    let mut wanted: BTreeSet<(String, String)> = BTreeSet::new();
    for table in &tables {
        for case in &table.cases {
            let menu = case["menu"].as_str().unwrap_or("md3").to_owned();
            assert!(
                MENUS.iter().any(|(name, _, _)| *name == menu),
                "{}: unknown menu {menu}",
                table.name
            );
            for design in &table.designs {
                wanted.insert((design.clone(), menu.clone()));
            }
        }
    }
    let mut composed: BTreeMap<(String, String), PathBuf> = BTreeMap::new();
    for (design, menu) in wanted {
        let (_, system, profile) = MENUS.iter().find(|(name, _, _)| *name == menu).unwrap();
        let to = root.join("composed").join(format!("{design}-{menu}"));
        compose(&kit, &design, system, *profile, &to);
        composed.insert((design, menu), to);
    }

    let mut failures = Vec::new();
    // What we observed in every failed case, by table, case and design, so
    // that we can write `now` into a red marker from what the menu does.
    let mut unmet: BTreeMap<String, Value> = BTreeMap::new();
    let mut red = Vec::new();
    let mut ran = 0usize;
    let mut checkpoints = 0usize;
    let designs: BTreeSet<&String> = tables.iter().flat_map(|table| &table.designs).collect();
    for design in designs {
        let mut script = String::new();
        let mut index: Vec<(&Table, &Value)> = Vec::new();
        for table in &tables {
            if !table.designs.contains(design) {
                continue;
            }
            for case in &table.cases {
                let name = case["name"].as_str().unwrap();
                let menu = case["menu"].as_str().unwrap_or("md3");
                assert!(
                    !expected(case, design).as_object().unwrap().is_empty(),
                    "{}: {name}: no expectation for {design}",
                    table.name
                );
                let data = root
                    .join("data")
                    .join(design)
                    .join(format!("{}", index.len()));
                fs::create_dir_all(&data).unwrap();
                script.push_str(&format!("case {}/{name}\n", table.name));
                script.push_str(&format!(
                    "assets {}\n",
                    composed[&(design.clone(), menu.to_owned())].display()
                ));
                script.push_str(&format!("data {}\n", data.display()));
                if let Some(setup) = case.get("setup").and_then(Value::as_object) {
                    for (key, value) in setup {
                        let value = match value {
                            Value::String(text) => text.clone(),
                            other => other.to_string(),
                        };
                        script.push_str(&format!("set {key} {value}\n"));
                    }
                }
                let ids: Vec<String> = named_ids(case, design).into_iter().collect();
                if !ids.is_empty() {
                    script.push_str(&format!("ids {}\n", ids.join(" ")));
                }
                for step in steps_for(case, design) {
                    script.push_str(&format!("step {}\n", step.as_str().unwrap()));
                }
                script.push_str("run\n");
                index.push((table, case));
            }
        }
        let results = run_driver(&driver, &script);
        assert_eq!(
            results.len(),
            index.len(),
            "{design}: the driver reported {} cases of {}",
            results.len(),
            index.len()
        );
        for ((table, case), result) in index.into_iter().zip(results) {
            let name = format!("{}/{}", table.name, case["name"].as_str().unwrap());
            assert_eq!(
                result["case"].as_str(),
                Some(name.as_str()),
                "{design}: cases out of order"
            );
            let label = format!("{design}: {name}");
            let steps = result["steps"].as_array().cloned().unwrap_or_default();
            let wanted_steps = steps_for(case, design).len();
            ran += 1;
            checkpoints += steps.len();
            if let Some(missing) = result["missing"]
                .as_array()
                .filter(|missing| !missing.is_empty())
            {
                failures.push(format!("{label}: the composed document has no {missing:?}"));
                continue;
            }
            if steps.len() != wanted_steps || steps.is_empty() {
                failures.push(format!(
                    "{label}: {} checkpoints for {wanted_steps} steps",
                    steps.len()
                ));
                continue;
            }
            let expect = expected(case, design);
            let seen = observed(&steps, &expect);
            let marker = case.get("red").and_then(|red| red.get(design));
            if seen != expect {
                unmet.insert(format!("{name}|{design}"), seen.clone());
            }
            match marker {
                None if seen == expect => {}
                None => failures.push(format!(
                    "{label}\n    expected {expect}\n    observed {seen}"
                )),
                Some(marker) => {
                    let bug = marker["bug"].as_str().unwrap_or("?");
                    let mut now = expect.clone();
                    for (field, value) in marker["now"].as_object().unwrap_or_else(|| {
                        panic!("{label}: a red marker needs `now`, what the menu does today")
                    }) {
                        now[field] = value.clone();
                    }
                    if seen == expect {
                        failures.push(format!("{label}: fixed ({bug}): remove the red marker"));
                    } else if seen == now {
                        red.push(format!(
                            "{label}: red {bug}: {}",
                            marker["reason"].as_str().unwrap_or("")
                        ));
                    } else {
                        failures.push(format!(
                            "{label}: red {bug} but not for the stated reason\n    expected {expect}\n    today    {now}\n    observed {seen}"
                        ));
                    }
                }
            }
        }
    }
    for line in &red {
        eprintln!("{line}");
    }
    eprintln!(
        "navigation: {ran} cases, {checkpoints} checkpoints, {} red, {} failed",
        red.len(),
        failures.len()
    );
    if let Some(path) = std::env::var_os("ROMINABOX_NAVIGATION_OBSERVED") {
        fs::write(path, serde_json::to_vec_pretty(&unmet).unwrap()).unwrap();
    }
    assert!(ran > 0, "no case ran");
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
