//! The agent skill, as someone installs it in Claude Code from this repository. We
//! name the plugin in the marketplace, declare in the plugin and its skill the
//! details for Claude Code, and check that every rominabox-cli command in the
//! skill exists in the command line and takes the request fields the skill shows
//! for it.
//!
//! In the skill, a command appears as `rominabox-cli <command>`, or as ``
//! `<command>` `` followed directly by the request it takes.

use serde_json::Value;
use std::{
    fs,
    path::PathBuf,
    process::{Command, Stdio},
};

fn at(relative: &str) -> PathBuf {
    rominabox_engine::repo::at(relative)
}

fn json(path: &std::path::Path) -> Value {
    let text = fs::read_to_string(path).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    serde_json::from_str(&text).unwrap_or_else(|error| panic!("{} is not JSON: {error}", path.display()))
}

/// A valid Claude Code name for a plugin or a skill, made of lower-case
/// letters, digits and hyphens.
fn is_name(text: &str) -> bool {
    !text.is_empty()
        && text.len() <= 64
        && text.bytes().all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
        && !text.starts_with('-')
        && !text.ends_with('-')
}

/// The plugin folders listed in the marketplace, by plugin name.
fn plugins() -> Vec<(String, PathBuf)> {
    let marketplace = json(&at(".claude-plugin/marketplace.json"));
    marketplace["plugins"]
        .as_array()
        .expect("the marketplace lists its plugins")
        .iter()
        .map(|plugin| {
            let source = plugin["source"].as_str().expect("a plugin in this repository has a path for its source");
            let relative = source.strip_prefix("./").expect("a plugin's path starts with ./");
            (plugin["name"].as_str().unwrap_or_default().to_string(), at(relative))
        })
        .collect()
}

/// Each skill of the plugin in `folder`, as its folder's name and its SKILL.md.
fn skills(folder: &std::path::Path) -> Vec<(String, String)> {
    let mut found: Vec<(String, String)> = fs::read_dir(folder.join("skills"))
        .expect("the plugin has a skills folder")
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.is_dir())
        .map(|path| {
            let text = fs::read_to_string(path.join("SKILL.md"))
                .unwrap_or_else(|error| panic!("{}: {error}", path.join("SKILL.md").display()));
            (path.file_name().unwrap().to_string_lossy().into_owned(), text)
        })
        .collect();
    found.sort();
    found
}

/// The skill's front matter, `key: value` between its opening `---` lines.
fn front_matter(skill: &str) -> Vec<(String, String)> {
    let mut lines = skill.lines();
    assert_eq!(lines.next(), Some("---"), "a skill opens with its front matter");
    lines
        .take_while(|line| *line != "---")
        .filter_map(|line| line.split_once(": "))
        .map(|(key, value)| (key.to_string(), value.trim().to_string()))
        .collect()
}

#[test]
fn the_marketplace_names_the_plugin_in_this_repository() {
    let marketplace = json(&at(".claude-plugin/marketplace.json"));
    assert!(is_name(marketplace["name"].as_str().unwrap_or_default()), "{marketplace}");
    assert!(!marketplace["owner"]["name"].as_str().unwrap_or_default().is_empty(), "{marketplace}");
    let listed = plugins();
    assert!(!listed.is_empty(), "the marketplace lists no plugin");
    for (name, folder) in listed {
        let manifest = json(&folder.join(".claude-plugin/plugin.json"));
        assert_eq!(manifest["name"].as_str(), Some(name.as_str()), "{}", folder.display());
    }
}

#[test]
fn the_plugin_and_its_skill_declare_what_claude_code_reads() {
    for (_, folder) in plugins() {
        let manifest = json(&folder.join(".claude-plugin/plugin.json"));
        assert!(is_name(manifest["name"].as_str().unwrap_or_default()), "{manifest}");
        assert!(!manifest["description"].as_str().unwrap_or_default().is_empty(), "{manifest}");
        let found = skills(&folder);
        assert!(!found.is_empty(), "{} has no skill", folder.display());
        for (folder_name, skill) in found {
            let declared = front_matter(&skill);
            let value = |key: &str| declared.iter().find(|(name, _)| name == key).map(|(_, value)| value.as_str());
            assert_eq!(value("name"), Some(folder_name.as_str()), "a skill is named for its folder");
            assert!(is_name(&folder_name), "{folder_name}");
            let description = value("description").unwrap_or_default();
            assert!(!description.is_empty() && description.len() <= 1024, "{folder_name}: {description}");
        }
    }
}

/// Where the command line is in the macOS builder bundle, from
/// tauri.conf.json. It is `<productName>.app/Contents/Resources/`, the folder
/// that `resources/bin/` maps to, and the name of the command line.
fn command_line_in_the_mac_bundle() -> String {
    let config = json(&at("desktop/src-tauri/tauri.conf.json"));
    let product = config["productName"].as_str().expect("tauri.conf.json has a productName");
    let folder = config["bundle"]["resources"]["resources/bin/"]
        .as_str()
        .expect("the bundle maps resources/bin/ to a folder of its resources");
    let program = std::path::Path::new(env!("CARGO_BIN_EXE_rominabox-cli"));
    let name = program.file_stem().unwrap().to_string_lossy();
    format!("{product}.app/Contents/Resources/{folder}{name}")
}

#[test]
fn every_skill_gives_the_command_line_inside_the_mac_app() {
    // Someone puts the Mac builder in Applications, and we do not add its
    // command line to the path, so the skill must say where it is.
    let expected = format!("/Applications/{}", command_line_in_the_mac_bundle());
    for (_, folder) in plugins() {
        for (name, skill) in skills(&folder) {
            assert!(skill.contains(&format!("`{expected}`")), "{name} does not give `{expected}` for a Mac");
        }
    }
}

/// Every command `skill` names, with the request it shows for it, if any.
fn commands(skill: &str) -> Vec<(String, Option<String>)> {
    let mut named = Vec::new();
    for (at, _) in skill.match_indices("rominabox-cli ") {
        let rest = &skill[at + "rominabox-cli ".len()..];
        let command: String = rest
            .chars()
            .take_while(|character| character.is_ascii_lowercase() || *character == '-')
            .collect();
        if !command.is_empty() {
            named.push((command, None));
        }
    }
    // `command` `{…}`
    for (at, _) in skill.match_indices("` `{") {
        let before = &skill[..at];
        let command = &before[before.rfind('`').expect("an opening backtick") + 1..];
        let request = &skill[at + 3..];
        let request = &request[..request.find('`').expect("a closing backtick")];
        named.push((command.to_string(), Some(request.to_string())));
    }
    named
}

/// The fields a request written in the skill names: each `"field":`.
fn fields(request: &str) -> Vec<String> {
    // After a split at the quotes, every other part, from the second, is quoted.
    let parts: Vec<&str> = request.split('"').collect();
    parts
        .windows(2)
        .skip(1)
        .step_by(2)
        .filter(|pair| pair[1].starts_with(':'))
        .map(|pair| pair[0].to_string())
        .collect()
}

fn cli(arguments: &[&str]) -> (bool, String) {
    let output = Command::new(env!("CARGO_BIN_EXE_rominabox-cli"))
        .args(arguments)
        .stdin(Stdio::null())
        .output()
        .expect("run rominabox-cli");
    (output.status.success(), String::from_utf8_lossy(&output.stdout).into_owned())
}

#[test]
fn every_command_the_skill_names_is_one_rominabox_cli_answers_to() {
    let (_, printed) = cli(&["schemas"]);
    let schemas: Value = serde_json::from_str(&printed).expect("schemas prints JSON");
    for (_, folder) in plugins() {
        for (name, skill) in skills(&folder) {
            let named = commands(&skill);
            assert!(
                named.iter().any(|(_, request)| request.is_none()) && named.iter().any(|(_, request)| request.is_some()),
                "{name} names commands both ways: {named:?}"
            );
            for (command, request) in named {
                let (_, printed) = cli(&[&command]);
                assert!(
                    !printed.contains(&format!("unknown command: {command}")),
                    "{name} tells an agent to use `rominabox-cli {command}`, which does not exist"
                );
                let Some(request) = request else { continue };
                let takes: Vec<String> = schemas[&command]["request"]
                    .as_array()
                    .unwrap_or_else(|| panic!("schemas lists no request for {command}"))
                    .iter()
                    .map(|field| field.as_str().unwrap().trim_end_matches('?').to_string())
                    .collect();
                for field in fields(&request) {
                    assert!(takes.contains(&field), "{name} gives {command} `{field}`, which it does not take: {takes:?}");
                }
            }
            // The request shown in the skill for export.
            let heredoc = skill.split("rominabox-cli export <<'JSON'\n").nth(1).expect("an export request");
            let request = heredoc.split("\nJSON").next().unwrap();
            let takes = schemas["export"]["request"].as_array().unwrap();
            for field in fields(request) {
                assert!(
                    takes.iter().any(|taken| taken.as_str().unwrap().trim_end_matches('?') == field),
                    "{name}'s export request has `{field}`, which export does not take"
                );
            }
        }
    }
}
