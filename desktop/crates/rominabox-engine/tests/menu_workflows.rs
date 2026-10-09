//! The menu workflow cases, replayed headlessly.
//!
//! Each case in `scripts/fixtures/menu-workflows.json` is a script in the
//! menu script grammar of the player, and the export it runs in. The baseline
//! of a case is the checkpoints and files that the launched player wrote when
//! we ran the case, one launch each. Here we compose the menu of the export
//! with the export staging (`packaging::stage_menu`), and run the script
//! through the script driver and report code of the fork in the menu C++,
//! with the fake RetroArch host and a fake clock (`menu_workflow_driver`,
//! which we build in the `workflows` tests and name in
//! `ROMINABOX_WORKFLOW_DRIVER`). We use no window, GL or sound, and run every
//! case in one process.
//!
//! We compare every checkpoint and every file with the baseline, except what
//! the `headless` section of the table lists as unknown to the fake host,
//! with the reason. No case can pass without testing anything: a declared
//! case that did not run, a script that did not reach its end, a missing or
//! extra checkpoint, an id in the baseline that the composed document does
//! not have, a baseline entry that no case declares, or an open menu with
//! nothing focused where the case does not allow it, all fail.
//!
//! With `ROMINABOX_WORKFLOW_OBSERVED=<file>` we write what we observed in
//! every case, in the format of the baseline. With
//! `ROMINABOX_WORKFLOW_RECORD=<key>,<key>` we replace the baseline entries of
//! those cases with what we observed. Name each one, and why, in the commit.

mod support;

use rominabox_engine::{menu, packaging, player_settings, repo, shaders, themes};
use serde_json::{json, Map, Value};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

/// One case, for one design and palette.
#[derive(Clone)]
struct Case {
    /// `design/palette/name`, its key in the baseline.
    key: String,
    baseline: String,
    /// The export request's settings, merged in the table's order.
    export: Value,
    /// The script, without the table's `end`.
    script: Vec<String>,
    /// The key of the case whose game data this one starts from.
    continues: Option<String>,
    /// Checkpoints that may have nothing focused while the menu is open.
    unfocused: BTreeSet<String>,
}

fn read_json(relative: &str) -> Value {
    let path = repo::at(relative);
    serde_json::from_slice(&fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display())))
        .unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

/// `base` with every field of `over` replacing its own, as one level.
fn merged(base: &Value, over: Option<&Value>) -> Value {
    let mut out = base.as_object().cloned().unwrap_or_default();
    if let Some(Value::Object(over)) = over {
        for (key, value) in over {
            out.insert(key.clone(), value.clone());
        }
    }
    Value::Object(out)
}

fn strings(value: &Value) -> Vec<String> {
    value
        .as_array()
        .unwrap_or_else(|| panic!("not a list: {value}"))
        .iter()
        .map(|item| {
            item.as_str()
                .unwrap_or_else(|| panic!("not a string: {item}"))
                .to_owned()
        })
        .collect()
}

/// A script from the table, for one design: a list, or one per design, with
/// each `@name` replaced by that design's sequence.
fn script_for(table: &Value, given: &Value, design: &str, label: &str) -> Vec<String> {
    let steps = given
        .as_array()
        .or_else(|| given.get(design).and_then(Value::as_array))
        .unwrap_or_else(|| panic!("{label}: no script for {design}"));
    let mut out = Vec::new();
    for step in steps {
        let step = step.as_str().unwrap();
        match step.strip_prefix('@') {
            Some(name) => out.extend(strings(
                table["sequences"][name]
                    .get(design)
                    .unwrap_or_else(|| panic!("{label}: no sequence {name} for {design}")),
            )),
            None => out.push(step.to_owned()),
        }
    }
    out
}

/// Every case the table declares, for every registered design and every
/// palette it names.
fn declared_cases(table: &Value, designs: &[String], palettes: &[String]) -> Vec<Case> {
    let shots = read_json("scripts/fixtures/menu-shots.json")["shots"].clone();
    let mut cases = Vec::new();
    for group in table["groups"].as_array().expect("groups") {
        let baseline = group["baseline"]
            .as_str()
            .expect("a group names its baseline");
        let entries = group["cases"].as_array().expect("a group lists cases");
        assert!(!entries.is_empty(), "{baseline}: a group with no cases");
        for entry in entries {
            let name = entry["name"].as_str().expect("a case has a name");
            let shot = entry.get("shot").map(|shot| {
                let shot = shot.as_str().unwrap();
                let found = shots
                    .get(shot)
                    .unwrap_or_else(|| panic!("{name}: no shot {shot}"));
                // A shot is its steps, or its steps beside its export settings.
                match found {
                    Value::Array(_) => json!({ "script": found }),
                    other => other.clone(),
                }
            });
            let palettes_named = entry.get("palettes").or_else(|| group.get("palettes"));
            let chosen: Vec<String> = match palettes_named {
                Some(Value::String(all)) if all == "*" => palettes.to_vec(),
                Some(list @ Value::Array(_)) => strings(list),
                other => panic!("{name}: palettes must be \"*\" or a list, not {other:?}"),
            };
            for design in designs {
                let script = entry
                    .get("script")
                    .or_else(|| shot.as_ref().map(|shot| &shot["script"]))
                    .unwrap_or_else(|| panic!("{name}: neither a script nor a shot"));
                let script = script_for(table, script, design, name);
                let mut export = merged(&table["export"], table["designs"][design].get("export"));
                export = merged(&export, group.get("export"));
                if let Some(Value::Object(settings)) = &shot {
                    let settings: Map<String, Value> = settings
                        .iter()
                        .filter(|(key, _)| {
                            !matches!(key.as_str(), "script" | "config" | "inMotion")
                        })
                        .map(|(key, value)| (key.clone(), value.clone()))
                        .collect();
                    export = merged(&export, Some(&Value::Object(settings)));
                }
                export = merged(&export, entry.get("export"));
                for palette in &chosen {
                    assert!(
                        palettes.contains(palette),
                        "{name}: {palette} is not a declared palette"
                    );
                    let mut export = export.clone();
                    export["theme"] = json!(design);
                    export["palette"] = json!(palette);
                    cases.push(Case {
                        key: format!("{design}/{palette}/{name}"),
                        baseline: baseline.to_owned(),
                        export,
                        script: script.clone(),
                        continues: entry["continues"]
                            .as_str()
                            .map(|other| format!("{design}/{palette}/{other}")),
                        unfocused: entry
                            .get("unfocused")
                            .map(strings)
                            .unwrap_or_default()
                            .into_iter()
                            .collect(),
                    });
                }
            }
        }
    }
    cases
}

/// The menu that we stage for one export, in the place where an export has it.
fn compose(kit: &Path, table: &Value, export: &Value, to: &Path) -> packaging::ExportRequest {
    let mut request = export.clone();
    request["rom"] = json!(to.join("game"));
    request["outputDir"] = json!(to.join("exported"));
    request["target"] = json!("macos");
    request["runtimeKit"] = json!(kit);
    let request: packaging::ExportRequest = serde_json::from_value(request)
        .unwrap_or_else(|error| panic!("{export}: not an export request: {error}"));
    let discs = table["game"]["discs"].as_u64().expect("game.discs") as usize;
    packaging::stage_menu(&request, discs, &[], to)
        .unwrap_or_else(|error| panic!("{export}: staging the menu failed: {error}"));
    request
}

/// What we pass from the launcher to RetroArch at launch and read back in the
/// menu: the value of each player setting from the player's file, or the
/// export default, and the shader that the game starts with.
fn launch_state(request: &packaging::ExportRequest, assets: &Path, data: &Path) -> String {
    let mut lines = String::new();
    for setting in player_settings::declared(request.game.player_defaults()) {
        let value = setting.chosen(data).unwrap_or(setting.default);
        lines.push_str(&format!("setting {} {value}\n", setting.key.name()));
    }
    // We keep the player's filter by its id, with the preset in this game's
    // shaders.cfg for it. An id that is no longer there is no choice.
    let config = fs::read_to_string(assets.join("shaders.cfg")).unwrap_or_default();
    let chosen = fs::read_to_string(data.join("shader-choice"))
        .ok()
        .and_then(|text| text.lines().next().map(str::to_owned))
        .and_then(|id| {
            let key = format!("shader_preset_{id} = \"");
            config
                .lines()
                .find_map(|line| line.strip_prefix(key.as_str())?.strip_suffix('"').map(str::to_owned))
        })
        .map(|preset| match preset.as_str() {
            "" => preset,
            relative => assets.join(relative).display().to_string(),
        });
    let initial = if request.game.show_menu {
        let destination = shaders::Destination {
            platform: request.game.target,
            library: shaders::kit_library(&request.runtime_kit),
        };
        shaders::launch_preset(&request.game.shaders, &destination)
            .unwrap()
            .map(|relative| assets.join(relative).display().to_string())
    } else {
        None
    };
    if let Some(shader) = chosen.or(initial).filter(|shader| !shader.is_empty()) {
        lines.push_str(&format!("shader {shader}\n"));
    }
    lines
}

/// The ids a baseline entry names, and the elements its script clicks.
fn named_ids(expected: &Value, script: &[String]) -> BTreeSet<String> {
    let mut ids = BTreeSet::new();
    for report in expected["reports"]
        .as_object()
        .into_iter()
        .flat_map(Map::values)
    {
        for field in [
            "focused",
            "selected",
            "capturing",
            "disabled",
            "on",
            "showing",
            "leaving",
        ] {
            for id in report[field].as_array().into_iter().flatten() {
                ids.insert(id.as_str().unwrap().to_owned());
            }
        }
        for id in report["text"].as_object().into_iter().flat_map(Map::keys) {
            ids.insert(id.clone());
        }
    }
    for step in script {
        if let Some(id) = step.strip_prefix("hover:") {
            ids.insert(id.to_owned());
        } else if !step.contains(':') && !menu::script::commands().contains(&step.as_str()) {
            // Every other bare step clicks an element, or sets a slider (id@fraction).
            ids.insert(step.split('@').next().unwrap().to_owned());
        }
    }
    ids
}

/// Files we keep for the menu in the game's data, by the table's patterns: a
/// name, a name with one `*`, or `dir/**/pattern` for any depth under `dir`.
fn persisted(
    data: &Path,
    patterns: &[String],
    assets: &Path,
    staged_at: &str,
) -> BTreeMap<String, String> {
    fn matches(pattern: &str, name: &str) -> bool {
        match pattern.split_once('*') {
            Some((before, after)) => {
                name.len() >= before.len() + after.len()
                    && name.starts_with(before)
                    && name.ends_with(after)
            }
            None => pattern == name,
        }
    }
    fn walk(directory: &Path, found: &mut Vec<PathBuf>) {
        for entry in fs::read_dir(directory).into_iter().flatten().flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk(&path, found);
            } else {
                found.push(path);
            }
        }
    }
    let mut files = Vec::new();
    walk(data, &mut files);
    // We write paths into the composed menu as we do in the player: into
    // the menu that we stage in the app of an export.
    let prefix = format!("{}/", assets.display());
    let mut out = BTreeMap::new();
    for path in files {
        let relative = path
            .strip_prefix(data)
            .unwrap()
            .to_string_lossy()
            .into_owned();
        let wanted = patterns
            .iter()
            .any(|pattern| match pattern.split_once("/**/") {
                Some((directory, name)) => {
                    relative.starts_with(&format!("{directory}/"))
                        && matches(name, path.file_name().unwrap().to_str().unwrap())
                }
                None => matches(pattern, &relative),
            });
        if wanted {
            let text = fs::read_to_string(&path).unwrap();
            out.insert(
                relative,
                text.replace(&prefix, &format!("$APP/{staged_at}/")),
            );
        }
    }
    out
}

/// A report as we compare it in the headless runner: each field in the table
/// reduced to whether it is there and not empty.
fn reduced_report(report: &Value, reductions: &Map<String, Value>) -> Value {
    let mut report = report.clone();
    for path in reductions.keys() {
        let (parent, field) = match path.split_once('.') {
            Some((parent, field)) => (report.get_mut(parent), field),
            None => (Some(&mut report), path.as_str()),
        };
        if let Some(Value::Object(parent)) = parent {
            if let Some(value) = parent.get_mut(field) {
                let shown = match value {
                    Value::Array(items) => !items.is_empty(),
                    Value::String(text) => !text.is_empty(),
                    Value::Bool(recorded) => *recorded,
                    Value::Null => false,
                    _ => true,
                };
                *value = json!(shown);
            }
        }
    }
    report
}

/// The files as we compare them in the headless runner: without the ones that
/// RetroArch writes, according to the table.
fn reduced_files(files: &Value, rules: &Map<String, Value>) -> Value {
    let mut out = Map::new();
    for (name, text) in files.as_object().into_iter().flatten() {
        let mut text = text.as_str().unwrap().to_owned();
        let mut dropped = false;
        for (rule, what) in rules {
            if rule.ends_with('/') && name.starts_with(rule.as_str()) {
                dropped = true;
            } else if rule == name {
                let prefix = what["lines"].as_str().expect("a file rule names its lines");
                text = text
                    .lines()
                    .filter(|line| !line.starts_with(prefix))
                    .map(|line| format!("{line}\n"))
                    .collect();
            }
        }
        if !dropped {
            out.insert(name.clone(), json!(text));
        }
    }
    Value::Object(out)
}

/// A record as we compare it in both runners and record it in this one,
/// with the facts that only a launched player has (the `headless` section
/// of the table), reduced. Only the launched runner has a picture.
fn reduced(record: &Value, table: &Value) -> Value {
    let reductions = table["headless"]["reports"].as_object().unwrap();
    let rules = table["headless"]["files"].as_object().unwrap();
    let reports: Map<String, Value> = record["reports"]
        .as_object()
        .into_iter()
        .flatten()
        .map(|(label, report)| (label.clone(), reduced_report(report, reductions)))
        .collect();
    json!({
        "script": record["script"],
        "reports": reports,
        "files": reduced_files(&record["files"], rules),
    })
}

/// Where two JSON values differ, as dotted paths.
fn differences(expected: &Value, observed: &Value, at: &str, out: &mut Vec<String>) {
    match (expected, observed) {
        (Value::Object(left), Value::Object(right)) => {
            for key in left.keys().chain(right.keys()).collect::<BTreeSet<_>>() {
                let path = if at.is_empty() {
                    key.clone()
                } else {
                    format!("{at}.{key}")
                };
                match (left.get(key), right.get(key)) {
                    (Some(a), Some(b)) => differences(a, b, &path, out),
                    (a, b) => out.push(format!("{path}: {} -> {}", short(a), short(b))),
                }
            }
        }
        (left, right) if left != right => out.push(format!("{at}: {left} -> {right}")),
        _ => {}
    }
}

fn short(value: Option<&Value>) -> String {
    value.map_or_else(|| "(absent)".to_owned(), Value::to_string)
}

fn run_driver(driver: &Path, script: &str) -> Vec<Value> {
    let mut child = Command::new(driver)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap_or_else(|error| panic!("could not start {}: {error}", driver.display()));
    // Write while we read, so that the driver never waits on a full pipe when
    // it answers before it has read every case.
    let mut input = child.stdin.take().unwrap();
    let script = script.to_owned();
    let writer = std::thread::spawn(move || input.write_all(script.as_bytes()));
    let output = child.wait_with_output().unwrap();
    writer.join().unwrap().unwrap();
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

/// Split the input of every case across a few drivers that run side by side,
/// and return the results in the order of the cases.
fn run_drivers(driver: &Path, cases: &[String]) -> Vec<Value> {
    let parallel = std::thread::available_parallelism()
        .map_or(4, usize::from)
        .min(8);
    let chunk = cases.len().div_ceil(parallel).max(1);
    std::thread::scope(|scope| {
        let running: Vec<_> = cases
            .chunks(chunk)
            .map(|part| scope.spawn(move || run_driver(driver, &part.concat())))
            .collect();
        running
            .into_iter()
            .flat_map(|thread| thread.join().unwrap())
            .collect()
    })
}

#[test]
#[ignore = "needs the headless driver: uv run python scripts/test.py workflows"]
fn every_workflow_case_reports_what_the_launched_player_recorded() {
    let driver = std::env::var_os("ROMINABOX_WORKFLOW_DRIVER")
        .map(PathBuf::from)
        .expect(
            "ROMINABOX_WORKFLOW_DRIVER names the driver; run `uv run python scripts/test.py workflows`",
        );
    let table = read_json("scripts/fixtures/menu-workflows.json");
    let registry = themes::registry().unwrap();
    let designs: Vec<String> = registry
        .designs
        .iter()
        .map(|design| design.id.clone())
        .collect();
    let palettes: Vec<String> = registry
        .palettes
        .iter()
        .map(|palette| palette.id.clone())
        .collect();
    for design in &designs {
        assert!(
            table["designs"].get(design).is_some(),
            "the registry's design {design} has no entry in menu-workflows.json"
        );
    }
    for design in table["designs"].as_object().unwrap().keys() {
        assert!(
            designs.contains(design),
            "menu-workflows.json names {design}, which is not registered"
        );
    }
    let cases = declared_cases(&table, &designs, &palettes);
    assert!(!cases.is_empty(), "the table declares no case");
    let keys: BTreeSet<&str> = cases.iter().map(|case| case.key.as_str()).collect();
    assert_eq!(keys.len(), cases.len(), "two cases share a key");

    let mut failures: Vec<String> = Vec::new();
    let mut baselines: BTreeMap<String, Value> = BTreeMap::new();
    for case in &cases {
        baselines
            .entry(case.baseline.clone())
            .or_insert_with(|| read_json(&case.baseline));
    }
    for (file, baseline) in &baselines {
        for key in baseline.as_object().unwrap().keys() {
            if !cases
                .iter()
                .any(|case| &case.key == key && &case.baseline == file)
            {
                failures.push(format!(
                    "{file}: {key} is recorded, but no case declares it"
                ));
            }
        }
    }

    let scratch = rominabox_scratch::Scratch::dir("rominabox-workflows");
    let keep = std::env::var_os("ROMINABOX_WORKFLOW_KEEP").map(PathBuf::from);
    let root = keep.unwrap_or_else(|| scratch.to_path_buf());
    let kit = support::kit(&root);
    let end = strings(&table["end"]);
    let frame = &table["frame"];
    let staged_at = table["menuAssets"].as_str().expect("menuAssets");
    let patterns = strings(&table["persisted"]);

    let started = std::time::Instant::now();
    // One composition per distinct export.
    let mut menus: BTreeMap<String, (PathBuf, packaging::ExportRequest)> = BTreeMap::new();
    for case in &cases {
        let key = case.export.to_string();
        if !menus.contains_key(&key) {
            let to = root.join("menus").join(menus.len().to_string());
            let request = compose(&kit, &table, &case.export, &to);
            menus.insert(key, (to, request));
        }
    }

    // Run a case that continues another after it, in a later driver run, so
    // that it starts from what the other case left.
    let mut data: BTreeMap<String, PathBuf> = BTreeMap::new();
    let mut observed: BTreeMap<String, Value> = BTreeMap::new();
    let mut pending: Vec<&Case> = cases.iter().collect();
    let (mut ran, mut checkpoints, mut identical, mut directories) =
        (0usize, 0usize, 0usize, 0usize);
    while !pending.is_empty() {
        let (ready, waiting): (Vec<&Case>, Vec<&Case>) = pending.into_iter().partition(|case| {
            case.continues
                .as_ref()
                .is_none_or(|other| data.contains_key(other) && observed.contains_key(other))
        });
        assert!(
            !ready.is_empty(),
            "a case continues one that never runs: {:?}",
            waiting.iter().map(|c| &c.key).collect::<Vec<_>>()
        );
        let mut scripts: Vec<String> = Vec::new();
        let mut wave_data = Vec::new();
        for case in &ready {
            let mut script = String::new();
            let (assets, request) = &menus[&case.export.to_string()];
            let directory = match &case.continues {
                Some(other) => data[other].clone(),
                None => {
                    directories += 1;
                    let directory = root.join("data").join(directories.to_string());
                    fs::create_dir_all(&directory).unwrap();
                    directory
                }
            };
            wave_data.push(directory.clone());
            let expected = &baselines[&case.baseline][&case.key];
            let ids: Vec<String> = named_ids(expected, &case.script).into_iter().collect();
            script.push_str(&format!("case {}\n", case.key));
            script.push_str(&format!("assets {}\n", assets.display()));
            script.push_str(&format!("data {}\n", directory.display()));
            script.push_str(&format!("frame {} {}\n", frame["width"], frame["height"]));
            script.push_str(&format!("open {}\n", u8::from(request.game.start_at_menu)));
            script.push_str(&launch_state(request, assets, &directory));
            if !ids.is_empty() {
                script.push_str(&format!("ids {}\n", ids.join(" ")));
            }
            let steps: Vec<&str> = case.script.iter().chain(&end).map(String::as_str).collect();
            script.push_str(&format!("script {}\n", steps.join(",")));
            script.push_str("run\n");
            scripts.push(script);
        }
        let results = run_drivers(&driver, &scripts);
        assert_eq!(
            results.len(),
            ready.len(),
            "the driver reported {} cases of {}",
            results.len(),
            ready.len()
        );
        for ((case, result), directory) in ready.iter().zip(results).zip(wave_data) {
            assert_eq!(
                result["case"].as_str(),
                Some(case.key.as_str()),
                "cases out of order"
            );
            ran += 1;
            let (assets, _) = &menus[&case.export.to_string()];
            let expected = &baselines[&case.baseline][&case.key];
            let label = &case.key;
            let mut problems = Vec::new();
            if result["loaded"] != json!(true) {
                problems.push("the menu did not load".to_owned());
            }
            if result["finished"] != json!(true) {
                problems.push(format!(
                    "the script did not reach its end in {} frames",
                    result["frames"]
                ));
            }
            if result["quit"] == json!(true) || !result["errors"].as_str().unwrap_or("").is_empty()
            {
                problems.push(format!(
                    "the player stopped: {}",
                    result["errors"].as_str().unwrap_or("")
                ));
            }
            if let Some(missing) = result["missing"]
                .as_array()
                .filter(|missing| !missing.is_empty())
            {
                problems.push(format!("the composed document has no {missing:?}"));
            }
            if expected.is_null() {
                problems.push(format!("no baseline entry in {}", case.baseline));
            } else if expected["script"] != json!(case.script) {
                problems.push(format!(
                    "the baseline recorded another script: {}",
                    expected["script"]
                ));
            }
            let wanted: Vec<String> = case
                .script
                .iter()
                .chain(&end)
                .filter_map(|step| step.strip_prefix("report:").map(str::to_owned))
                .collect();
            let points = result["checkpoints"]
                .as_array()
                .cloned()
                .unwrap_or_default();
            let labels: Vec<String> = points
                .iter()
                .map(|point| point["label"].as_str().unwrap().to_owned())
                .collect();
            checkpoints += points.len();
            if labels != wanted || points.is_empty() {
                problems.push(format!("checkpoints {labels:?}, not {wanted:?}"));
            }
            let mut reports = Map::new();
            for point in &points {
                let label = point["label"].as_str().unwrap();
                let report = &point["report"];
                if report["menuOpen"] == json!(true)
                    && report["focused"].as_array().is_none_or(Vec::is_empty)
                    && !case.unfocused.contains(label)
                {
                    problems.push(format!("{label}: the menu is open and nothing is focused"));
                }
                reports.insert(label.to_owned(), report.clone());
            }
            let files = persisted(&directory, &patterns, assets, staged_at);
            let seen = reduced(
                &json!({ "script": case.script, "reports": reports, "files": files }),
                &table,
            );
            let mut changed = Vec::new();
            if !expected.is_null() {
                let before = reduced(expected, &table);
                differences(
                    &before["reports"],
                    &seen["reports"],
                    "reports",
                    &mut changed,
                );
                differences(&before["files"], &seen["files"], "files", &mut changed);
            }
            if problems.is_empty() && changed.is_empty() {
                identical += 1;
            }
            if !changed.is_empty() {
                problems.push(format!(
                    "differs from its baseline:\n      {}",
                    changed.join("\n      ")
                ));
            }
            if !problems.is_empty() {
                failures.push(format!("{label}: {}", problems.join("\n    ")));
            }
            data.insert(case.key.clone(), directory);
            observed.insert(case.key.clone(), seen);
        }
        pending = waiting;
    }

    if let Some(path) = std::env::var_os("ROMINABOX_WORKFLOW_OBSERVED") {
        fs::write(path, serde_json::to_vec_pretty(&observed).unwrap()).unwrap();
    }
    if let Ok(named) = std::env::var("ROMINABOX_WORKFLOW_RECORD") {
        let named: Vec<&str> = named.split(',').filter(|key| !key.is_empty()).collect();
        for (file, baseline) in &mut baselines {
            let mut changed = false;
            for key in &named {
                let Some(case) = cases
                    .iter()
                    .find(|case| case.key == *key && &case.baseline == file)
                else {
                    continue;
                };
                baseline[&case.key] = observed[&case.key].clone();
                changed = true;
            }
            if changed {
                let text = serde_json::to_string_pretty(baseline).unwrap() + "\n";
                fs::write(repo::at(file), text).unwrap();
            }
        }
        for key in &named {
            assert!(
                cases.iter().any(|case| case.key == *key),
                "no case {key} to record"
            );
        }
    }
    eprintln!(
        "workflows: {ran} cases, {checkpoints} checkpoints on {} composed menus in {:.1} s; \
         {identical} identical to their baseline, {} failed",
        menus.len(),
        started.elapsed().as_secs_f64(),
        failures.len()
    );
    assert_eq!(
        ran,
        cases.len(),
        "{} declared cases did not run",
        cases.len() - ran
    );
    assert!(checkpoints > 0, "no checkpoint was taken");
    assert!(failures.is_empty(), "\n{}", failures.join("\n"));
}
