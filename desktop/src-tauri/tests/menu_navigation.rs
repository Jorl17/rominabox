//! Arrow keys, pointer and focus on every composed menu screen, headless.
//!
//! We compose every design in the registry and every hypothetical design
//! under `tests/fixtures/designs/` with `menu::compose_menu` for a few
//! pads, and run the menu C++ on each composition in `menu_nav_driver`
//! (built in the `navigation` scope, found through
//! `ROMINABOX_NAVIGATION_DRIVER`). We load the document in the driver with a
//! fake RetroArch host: no window, no GL, no sound. Then we press keys, move
//! the pointer and record, after every step, the element the player sees
//! highlighted, what shows that a binding is being captured, the screen,
//! the sounds played and, for the elements a case lists under `text`, the
//! words on them.
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
//! With the same driver we also open Pause in each design for a 4:3, a 16:9
//! and a 10:9 game and measure the picture of the first save slot, where a
//! design can give an element the aspect ratio of the running game.
//!
//! Without the driver variable we fail the tests: they run only the driver.

mod support;

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

/// The designs a table must cover: registered ones, then hypothetical ones.
fn all_designs() -> (Vec<String>, Vec<String>) {
    let registered = themes::registry()
        .unwrap()
        .designs
        .into_iter()
        .map(|design| design.id)
        .collect();
    (registered, support::hypothetical_designs())
}

/// The menu composed from one design and pad, with every Options entry in
/// the design and the bundled shaders and achievements, as in a full export.
fn compose(kit: &Path, design: &str, system: &str, profile: Option<&str>, to: &Path) {
    let staged = themes::staged_design(kit, design);
    let entries: Vec<String> = rominabox_desktop::menu::declared_screens(&staged)
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
    let request = rominabox_desktop::menu::MenuRequest {
        palette: "blue".into(),
        system: system.into(),
        controls: controls.clone(),
        include_achievements: true,
        menu_entries: Some(entries),
        shaders,
        // We compose the disc list only for a game of several discs, and the
        // fake host gives the number of discs of the running game, even one.
        discs: 7,
        ..rominabox_desktop::menu::MenuRequest::new(&staged, kit.join("menu-assets"))
    };
    rominabox_desktop::menu::compose_menu(&request)
        .and_then(|menu| menu.write(to))
        .unwrap_or_else(|error| panic!("{design}/{system}: composition failed: {error}"));
    rominabox_desktop::controls::write_defaults_config(
        system,
        &controls,
        &to.join("controls-defaults.cfg"),
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
    add(for_design(&case["capturing"], design));
    ids.extend(text_ids(case, design));
    for step in steps_for(case, design) {
        let step = step.as_str().unwrap();
        if let Some(id) = step
            .strip_prefix("hover:")
            .or_else(|| step.strip_prefix("press:"))
        {
            ids.insert(id.to_owned());
        } else if !step.contains(':') {
            ids.insert(step.to_owned());
        }
    }
    ids
}

/// The elements whose words we check in a case for a design: the keys of its
/// `text` expectation, with the same elements at every step.
fn text_ids(case: &Value, design: &str) -> Vec<String> {
    for_design(&case["text"], design)
        .and_then(Value::as_array)
        .and_then(|steps| steps.first())
        .and_then(Value::as_object)
        .map(|words| words.keys().cloned().collect())
        .unwrap_or_default()
}

/// The step results expected in a case for a design, in the driver's format.
fn expected(case: &Value, design: &str) -> Value {
    let mut out = serde_json::Map::new();
    for field in ["focused", "capturing", "screen", "sounds", "text"] {
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
#[ignore = "needs the headless driver: python3 scripts/test.py navigation"]
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
    let kit = support::kit_with_hypothetical(&root);

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
                let text = text_ids(case, design);
                if !text.is_empty() {
                    script.push_str(&format!("text {}\n", text.join(" ")));
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
            let marker = case.get("red").and_then(|red| for_design(red, design));
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

/// The aspect ratios we check a game's picture in: a 4:3 console, a 16:9 one
/// and the Game Boy's 10:9, as width over height, as the document states each.
const SHAPES: [(&str, f64, &str); 3] = [
    ("4:3", 4.0 / 3.0, "1.333"),
    ("16:9", 16.0 / 9.0, "1.778"),
    ("10:9", 10.0 / 9.0, "1.111"),
];

/// Where Native and Disc draw the picture of a slot when we size it in the
/// player, as intended: the game's aspect ratio, as large as 230 × 138 dp
/// allows, centred vertically in that space. In Native it is centred across
/// the 252 dp slot, inside its 3 dp edge and below its 26 dp label, in a 2 dp
/// bevel. In Disc its left edge is 8 dp inside the 1 dp edge of the slot, in
/// a 1 dp bevel. Each is the content box, as [x, y, width, height] in dp from
/// the corner of the slot's border box.
fn picture_as_before(design: &str, aspect: f64) -> [f64; 4] {
    let width = (138.0 * aspect).min(230.0);
    let height = (230.0 / aspect).min(138.0);
    let down = (138.0 - height) / 2.0;
    match design {
        "native" => [
            3.0 + (252.0 - (width + 4.0)) / 2.0 + 2.0,
            3.0 + 26.0 + 2.0 + down,
            width,
            height,
        ],
        "disc" => [1.0 + 8.0 + 1.0, 1.0 + 8.0 + 1.0 + down, width, height],
        other => panic!("{other} drew no slot picture before"),
    }
}

/// The picture box of the unmarked fixture, the same for every game.
const UNSHAPED_BOX: (f64, f64) = (200.0, 100.0);

fn rect(value: &Value) -> Option<[f64; 4]> {
    let numbers: Vec<f64> = value.as_array()?.iter().filter_map(Value::as_f64).collect();
    numbers.try_into().ok()
}

fn near(a: [f64; 4], b: [f64; 4]) -> bool {
    a.iter().zip(b).all(|(x, y)| (x - y).abs() <= 1.0)
}

fn show(found: Option<[f64; 4]>) -> String {
    found
        .map(|[x, y, w, h]| format!("{w:.1}x{h:.1} at {x:.1},{y:.1}"))
        .unwrap_or_else(|| "not laid out".into())
}

/// What we observe of the first slot: the aspect ratio in the document, the
/// content box of the picture and the box of the drawn image, both from the
/// slot's corner.
struct SlotSeen {
    shape: Option<String>,
    frame: Option<[f64; 4]>,
    drawn: Option<[f64; 4]>,
}

const SLOT: &str = "#slot-1";
const PICTURE: &str = "#slot-1 .slot-picture";
const IMAGE: &str = "#slot-image-1";

fn slot_seen(step: &Value) -> SlotSeen {
    let boxes = &step["boxes"];
    let slot = rect(&boxes[SLOT]["border"]);
    let from_slot = |found: Option<[f64; 4]>| {
        found
            .zip(slot)
            .map(|(b, s)| [b[0] - s[0], b[1] - s[1], b[2], b[3]])
    };
    SlotSeen {
        shape: step["document"]["data-game-shape"]
            .as_str()
            .map(str::to_owned),
        frame: from_slot(rect(&boxes[PICTURE]["content"])),
        drawn: from_slot(rect(&boxes[IMAGE]["border"])),
    }
}

/// Return what is wrong with the first slot's picture in `design` for a game
/// of `aspect`. In a marked design it has the game's aspect ratio where the
/// design puts it, filled by the image. The unmarked fixture has a fixed box.
fn picture_problems(design: &str, label: &str, aspect: f64, seen: &SlotSeen) -> Vec<String> {
    let mut problems = Vec::new();
    if design == "unshaped" {
        let kept = seen.frame.is_some_and(|[_, _, w, h]| {
            (w - UNSHAPED_BOX.0).abs() <= 1.0 && (h - UNSHAPED_BOX.1).abs() <= 1.0
        });
        if !kept {
            problems.push(format!(
                "{design} {label}: it marks nothing, so its picture keeps its own {}x{} box; it is {}",
                UNSHAPED_BOX.0,
                UNSHAPED_BOX.1,
                show(seen.frame)
            ));
        }
        return problems;
    }
    let before = picture_as_before(design, aspect);
    if !seen.frame.is_some_and(|frame| near(frame, before)) {
        problems.push(format!(
            "{design} {label}: the picture is {}; in the game's shape, as it was before, it is {}",
            show(seen.frame),
            show(Some(before))
        ));
    }
    if !seen
        .drawn
        .zip(seen.frame)
        .is_some_and(|(drawn, frame)| near(drawn, frame))
    {
        problems.push(format!(
            "{design} {label}: the image is {} in a picture of {}, so it has bars",
            show(seen.drawn),
            show(seen.frame)
        ));
    }
    problems
}

/// The picture of every save slot has the aspect ratio of the running game
/// where the design marks it, so there are never bars, and we follow a game
/// that changes ratio when the menu opens again. The document contains the
/// ratio. In a design without marks, the box is the one from its stylesheet.
#[test]
#[ignore = "needs the headless driver: python3 scripts/test.py navigation"]
fn a_slot_picture_takes_the_games_shape_where_the_design_marks_it() {
    let driver = std::env::var_os("ROMINABOX_NAVIGATION_DRIVER")
        .map(PathBuf::from)
        .expect("ROMINABOX_NAVIGATION_DRIVER names the driver; run `python3 scripts/test.py navigation`");
    let scratch = rominabox_scratch::Scratch::dir("rominabox-game-shape");
    let root = scratch.to_path_buf();
    let kit = support::kit_with_hypothetical(&root);
    let (_, system, profile) = MENUS[0];
    let (four_three, wide) = (SHAPES[0], SHAPES[1]);
    let mut failures = Vec::new();
    for design in ["native", "disc", "unshaped"] {
        let assets = root.join("composed").join(design);
        compose(&kit, design, system, profile, &assets);
        let mut script = String::new();
        let mut case = |name: &str, aspect: f64, steps: &[String]| {
            let data = root.join("data").join(design).join(name.replace(':', "x"));
            fs::create_dir_all(&data).unwrap();
            script.push_str(&format!(
                "case {name}\nassets {}\ndata {}\nset load 1\nset aspect {aspect}\n",
                assets.display(),
                data.display()
            ));
            for selector in [SLOT, PICTURE, IMAGE] {
                script.push_str(&format!("box {selector}\n"));
            }
            for step in steps {
                script.push_str(&format!("step {step}\n"));
            }
            script.push_str("run\n");
        };
        for (name, aspect, _) in SHAPES {
            case(name, aspect, &["wait-ms:16".to_owned()]);
        }
        // Open the menu on a 4:3 picture. Then the game turns 16:9 as it runs.
        let reshaped = [
            "wait-ms:16",
            "menu:close",
            &format!("aspect:{}", wide.1),
            "menu:open",
        ]
        .map(str::to_owned);
        case("reshaped", four_three.1, &reshaped);
        let results = run_driver(&driver, &script);
        assert_eq!(results.len(), SHAPES.len() + 1, "{design}: {results:?}");

        for ((name, aspect, stated), result) in SHAPES.into_iter().zip(&results) {
            let seen = slot_seen(&result["steps"][0]);
            if seen.shape.as_deref() != Some(stated) {
                failures.push(format!(
                    "{design} {name}: the document states the game's shape as {:?}, not {stated}",
                    seen.shape
                ));
            }
            failures.extend(picture_problems(design, name, aspect, &seen));
        }

        let steps: Vec<SlotSeen> = results[SHAPES.len()]["steps"]
            .as_array()
            .unwrap()
            .iter()
            .map(slot_seen)
            .collect();
        let stated: Vec<Option<&str>> = steps.iter().map(|seen| seen.shape.as_deref()).collect();
        // We read the ratio when the menu opens, not while it is closed.
        let expected = [four_three.2, four_three.2, four_three.2, wide.2].map(Some);
        if stated != expected {
            failures.push(format!(
                "{design}: a game that turns 16:9 while the menu is closed is stated as {stated:?} \
                 through opening, closing, the change and opening again, not {expected:?}"
            ));
        }
        failures.extend(picture_problems(
            design,
            "turned 16:9 and reopened",
            wide.1,
            &steps[3],
        ));
    }
    assert!(failures.is_empty(), "\n{}", failures.join("\n"));
}
