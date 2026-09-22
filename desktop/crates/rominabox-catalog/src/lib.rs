//! Load, validate and resolve ROM-in-a-Box console packages.
//!
//! We keep Tauri, networking and changes to files out of this crate. From a
//! package root we return either a typed, immutable catalog or a list of
//! diagnostics. So the builder, the CLI, the exporter and the tests all have
//! the same answers, and a test can point the loader at a temporary directory
//! that contains a made-up console.
//!
//! We load a package only when it is complete, and otherwise report why it is
//! not. We never use a default in place of a missing declaration.

pub mod model;

use model::{
    Console, ControllerProfile, CoreComponent, Presentation, SheetParser, CONTROL_IDS,
    SCHEMA_VERSION,
};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::path::{Component, Path, PathBuf};

/// The folder of the packages, relative to the repository root.
pub const PACKAGE_ROOT: &str = "integrations/consoles";

/// The checkout whose packages we read in this process.
///
/// Checkouts can share one cargo target directory, and whether we rebuild
/// there depends on a fingerprint of the sources. Two checkouts at the same
/// commit have the same sources but different paths, and
/// `env!("CARGO_MANIFEST_DIR")` puts the path into the binary. So we could
/// run a binary built in one checkout in another and read the wrong packages.
///
/// The root is the checkout the process runs in. We set `ROMINABOX_REPO` in
/// `scripts/test.py`, and fall back to the compiled path for a run by hand.
///
/// This crate is standalone so that it is never part of a parent workspace,
/// and for that reason it cannot import `repo.rs` from the desktop crate. In
/// `scripts/test_repo_root.py` we reject any file with code that resolves
/// paths outside its crate without reading the environment first.
pub fn repo_root() -> std::path::PathBuf {
    match std::env::var("ROMINABOX_REPO") {
        Ok(declared) if !declared.is_empty() => std::path::PathBuf::from(declared),
        // desktop/crates/rominabox-catalog -> repository root
        _ => std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../.."),
    }
}

/// The generic profile that every catalog contains.
pub const BUILTIN_GENERIC_PROFILE: &str = "retropad";

/// A problem with a declaration, for the person who wrote it.
///
/// `code` is stable, so we can check it in tests and tools. `package` and
/// `field` name where to look, and `message` is the text for a person.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    pub code: &'static str,
    pub package: String,
    pub field: String,
    pub message: String,
}

impl Diagnostic {
    fn new(
        code: &'static str,
        package: impl Into<String>,
        field: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            code,
            package: package.into(),
            field: field.into(),
            message: message.into(),
        }
    }
}

impl fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} [{}] {}: {}",
            self.package, self.code, self.field, self.message
        )
    }
}

/// A console together with the directory of its package.
#[derive(Debug, Clone)]
pub struct ConsoleEntry {
    pub console: Console,
    pub directory: PathBuf,
}

/// A validated, immutable view of every package under one root.
#[derive(Debug, Clone, Default)]
pub struct Catalog {
    consoles: BTreeMap<String, ConsoleEntry>,
    profiles: BTreeMap<String, (ControllerProfile, PathBuf)>,
    components: BTreeMap<String, (CoreComponent, PathBuf)>,
}

impl Catalog {
    /// Read every package under `root`, or return every problem found.
    ///
    /// We collect all diagnostics and do not stop at the first, so that a
    /// contributor sees everything wrong with a package in one run.
    pub fn load(root: &Path) -> Result<Self, Vec<Diagnostic>> {
        let mut catalog = Catalog::default();
        let mut problems = Vec::new();
        catalog.seed_builtin_profiles();

        let mut directories: Vec<PathBuf> = match std::fs::read_dir(root) {
            Ok(entries) => entries
                .filter_map(|entry| entry.ok())
                .map(|entry| entry.path())
                .filter(|path| path.is_dir())
                .collect(),
            Err(error) => {
                return Err(vec![Diagnostic::new(
                    "root.unreadable",
                    root.display().to_string(),
                    "",
                    format!("could not read the package root: {error}"),
                )])
            }
        };
        // We sort, so that the diagnostics and the generated files do not
        // depend on the order in which the filesystem lists the entries.
        directories.sort();

        for directory in directories {
            catalog.load_package(&directory, &mut problems);
        }

        catalog.validate(&mut problems);

        if problems.is_empty() {
            Ok(catalog)
        } else {
            problems.sort_by(|a, b| {
                (a.package.as_str(), a.field.as_str()).cmp(&(&b.package, &b.field))
            });
            Err(problems)
        }
    }

    fn load_package(&mut self, directory: &Path, problems: &mut Vec<Diagnostic>) {
        let package = directory
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();

        let manifest = directory.join("console.json");
        if !manifest.is_file() {
            problems.push(Diagnostic::new(
                "package.no_manifest",
                &package,
                "console.json",
                "a package directory must contain console.json",
            ));
            return;
        }

        match read_json::<Console>(&manifest) {
            Ok(console) => {
                if console.schema_version != SCHEMA_VERSION {
                    problems.push(Diagnostic::new(
                        "schema.unsupported_version",
                        &package,
                        "schemaVersion",
                        format!(
                            "declares version {} but this build understands {SCHEMA_VERSION}",
                            console.schema_version
                        ),
                    ));
                    return;
                }
                if let Some(existing) = self.consoles.get(&console.id) {
                    problems.push(Diagnostic::new(
                        "id.duplicate",
                        &package,
                        "id",
                        format!(
                            "console id '{}' is already declared by {}",
                            console.id,
                            existing.directory.display()
                        ),
                    ));
                    return;
                }
                self.consoles.insert(
                    console.id.clone(),
                    ConsoleEntry {
                        console,
                        directory: directory.to_path_buf(),
                    },
                );
            }
            Err(error) => problems.push(Diagnostic::new(
                "parse.invalid_json",
                &package,
                "console.json",
                error,
            )),
        }

        self.load_owned(
            directory,
            &package,
            "controllers",
            problems,
            |catalog, value: ControllerProfile, path, package, problems| {
                if value.schema_version != SCHEMA_VERSION {
                    problems.push(Diagnostic::new(
                        "schema.unsupported_version",
                        package,
                        "schemaVersion",
                        format!(
                            "controller profile '{}' declares version {}",
                            value.id, value.schema_version
                        ),
                    ));
                    return;
                }
                if let Some((_, existing)) = catalog.profiles.get(&value.id) {
                    problems.push(Diagnostic::new(
                        "id.duplicate",
                        package,
                        "controllers",
                        format!(
                            "controller profile '{}' is already declared by {}",
                            value.id,
                            existing.display()
                        ),
                    ));
                    return;
                }
                catalog.profiles.insert(value.id.clone(), (value, path));
            },
        );

        self.load_owned(
            directory,
            &package,
            "components",
            problems,
            |catalog, value: CoreComponent, path, package, problems| {
                if value.schema_version != SCHEMA_VERSION {
                    problems.push(Diagnostic::new(
                        "schema.unsupported_version",
                        package,
                        "schemaVersion",
                        format!(
                            "component '{}' declares version {}",
                            value.id, value.schema_version
                        ),
                    ));
                    return;
                }
                if let Some((_, existing)) = catalog.components.get(&value.id) {
                    problems.push(Diagnostic::new(
                        "id.duplicate",
                        package,
                        "components",
                        format!(
                            "component '{}' is already declared by {}",
                            value.id,
                            existing.display()
                        ),
                    ));
                    return;
                }
                catalog.components.insert(value.id.clone(), (value, path));
            },
        );
    }

    /// The generic RetroPad, available to every console without a package
    /// that declares it.
    ///
    /// It is the device model of the runtime and of no particular console, so
    /// it is in no package and no console declares a copy of it. A package
    /// may not declare this id again. Doing so is a duplicate-id error like
    /// any other.
    fn seed_builtin_profiles(&mut self) {
        let controls = [
            ("up", "Up", "up"),
            ("down", "Down", "down"),
            ("left", "Left", "left"),
            ("right", "Right", "right"),
            ("b", "B", "z"),
            ("a", "A", "x"),
            ("y", "Y", "a"),
            ("x", "X", "s"),
            ("l", "L", "d"),
            ("r", "R", "c"),
            ("l2", "L2", "w"),
            ("r2", "R2", "e"),
            ("select", "Select", "rshift"),
            ("start", "Start", "enter"),
            ("l3", "L3", "1"),
            ("r3", "R3", "2"),
        ];
        let profile = ControllerProfile {
            schema_version: SCHEMA_VERSION,
            id: BUILTIN_GENERIC_PROFILE.to_string(),
            name: "RetroPad".to_string(),
            presentation: Presentation::Generic,
            presentation_order: Some(u32::MAX - 1),
            core_device: None,
            controls: controls
                .iter()
                .map(|(id, label, key)| model::Control {
                    id: (*id).to_string(),
                    label: (*label).to_string(),
                    key: (*key).to_string(),
                    group: None,
                    x: None,
                    y: None,
                    callout_x: None,
                    callout_y: None,
                })
                .collect(),
        };
        self.profiles
            .insert(profile.id.clone(), (profile, PathBuf::from("<built-in>")));
    }

    fn load_owned<T, F>(
        &mut self,
        directory: &Path,
        package: &str,
        subdirectory: &str,
        problems: &mut Vec<Diagnostic>,
        mut insert: F,
    ) where
        T: serde::de::DeserializeOwned,
        F: FnMut(&mut Catalog, T, PathBuf, &str, &mut Vec<Diagnostic>),
    {
        let root = directory.join(subdirectory);
        if !root.is_dir() {
            return;
        }
        let mut files: Vec<PathBuf> = std::fs::read_dir(&root)
            .into_iter()
            .flatten()
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.path())
            .filter(|path| path.extension().is_some_and(|ext| ext == "json"))
            .collect();
        files.sort();
        for file in files {
            match read_json::<T>(&file) {
                Ok(value) => insert(self, value, file, package, problems),
                Err(error) => problems.push(Diagnostic::new(
                    "parse.invalid_json",
                    package,
                    format!(
                        "{subdirectory}/{}",
                        file.file_name().unwrap_or_default().to_string_lossy()
                    ),
                    error,
                )),
            }
        }
    }

    /// Checks across declarations. We put here every check across more
    /// than one file.
    fn validate(&self, problems: &mut Vec<Diagnostic>) {
        let mut aliases: BTreeMap<String, String> = BTreeMap::new();
        // Each extension has one parser. Two consoles may share `cue`, but we
        // read the track names in a cue sheet the same way for both.
        let mut parsers: BTreeMap<String, (String, SheetParser)> = BTreeMap::new();

        for (id, entry) in &self.consoles {
            let console = &entry.console;
            let package = &id[..];

            if console.content.extensions.is_empty() {
                problems.push(Diagnostic::new(
                    "content.no_extensions",
                    package,
                    "content.extensions",
                    "a console must declare at least one content extension",
                ));
            }
            for sheet in &console.content.sheets {
                if !console
                    .content
                    .extensions
                    .iter()
                    .any(|extension| extension.eq_ignore_ascii_case(&sheet.extension))
                {
                    problems.push(Diagnostic::new(
                        "content.sheet_not_recognised",
                        package,
                        "content.sheets",
                        format!(
                            "{} is followed as a sheet but is not an extension this console recognises",
                            sheet.extension
                        ),
                    ));
                }
                if console.content.recognize_only.iter().any(|extension| {
                    extension.eq_ignore_ascii_case(&sheet.extension)
                }) {
                    problems.push(Diagnostic::new(
                        "content.sheet_is_recognise_only",
                        package,
                        "content.sheets",
                        format!(
                            "{} is followed as a sheet and also declared recognise-only",
                            sheet.extension
                        ),
                    ));
                }
                let key = sheet.extension.to_ascii_lowercase();
                if let Some((owner, parser)) = parsers.get(&key) {
                    if *parser != sheet.parser {
                        problems.push(Diagnostic::new(
                            "content.sheet_parser_conflict",
                            package,
                            "content.sheets",
                            format!(
                                "{} is a {} sheet here and a {} sheet on {owner}",
                                sheet.extension,
                                sheet.parser.as_str(),
                                parser.as_str()
                            ),
                        ));
                    }
                } else {
                    parsers.insert(key, (id.clone(), sheet.parser));
                }
            }
            for companion in &console.content.companions {
                let Some(when) = &companion.when else {
                    continue;
                };
                if !console
                    .content
                    .extensions
                    .iter()
                    .any(|extension| extension.eq_ignore_ascii_case(when))
                {
                    problems.push(Diagnostic::new(
                        "content.companion_host",
                        package,
                        "content.companions",
                        format!(
                            ".{} travels with a .{} file, which this console does not recognise",
                            companion.extension, when
                        ),
                    ));
                }
            }

            // If an alias named two consoles, recognition would depend on
            // order, and nobody could reach the console that lost.
            for alias in std::iter::once(id).chain(console.aliases.iter()) {
                let key = alias.to_ascii_lowercase();
                if let Some(owner) = aliases.get(&key) {
                    if owner != id {
                        problems.push(Diagnostic::new(
                            "alias.duplicate",
                            package,
                            "aliases",
                            format!("'{alias}' is already used by console '{owner}'"),
                        ));
                    }
                } else {
                    aliases.insert(key, id.clone());
                }
            }

            for binding in &console.cores {
                if !self.components.contains_key(&binding.component) {
                    problems.push(Diagnostic::new(
                        "reference.missing_component",
                        package,
                        "cores",
                        format!("no package declares component '{}'", binding.component),
                    ));
                }
            }

            // We state this in one direction only. A console lists its
            // profiles, and a profile never lists the consoles that use it, so
            // a typo here cannot pass for deliberate generic support.
            let mut variants: BTreeSet<&str> = console
                .controllers
                .variants
                .iter()
                .map(String::as_str)
                .collect();
            variants.insert(console.controllers.default.as_str());
            for variant in &variants {
                if !self.profiles.contains_key(*variant) {
                    problems.push(Diagnostic::new(
                        "reference.missing_profile",
                        package,
                        "controllers",
                        format!("no package declares controller profile '{variant}'"),
                    ));
                }
            }
            for (index, window) in console.recognition.header_titles.iter().enumerate() {
                if window.length == 0 {
                    problems.push(Diagnostic::new(
                        "recognition.empty_title",
                        package,
                        format!("recognition.headerTitle[{index}].length"),
                        "a title window has to cover at least one byte",
                    ));
                }
                if let Some(anchor) = &window.anchor {
                    if anchor.is_empty() {
                        problems.push(Diagnostic::new(
                            "recognition.empty_anchor",
                            package,
                            format!("recognition.headerTitle[{index}].anchor"),
                            "an anchor is the signature the offset is measured from",
                        ));
                    }
                }
                if let Some(magic) = &window.magic {
                    let text = magic.text.as_deref().filter(|value| !value.is_empty());
                    let hex_ok = magic.hex.as_deref().is_some_and(hex_bytes_ok);
                    if text.is_none() && !hex_ok {
                        problems.push(Diagnostic::new(
                            "recognition.magic",
                            package,
                            format!("recognition.headerTitle[{index}].magic"),
                            "magic needs a text or an even-length hex string",
                        ));
                    }
                }
            }
            if !console.controllers.variants.is_empty()
                && !console
                    .controllers
                    .variants
                    .contains(&console.controllers.default)
            {
                problems.push(Diagnostic::new(
                    "controller.default_not_offered",
                    package,
                    "controllers.default",
                    format!(
                        "default profile '{}' is not among the declared variants",
                        console.controllers.default
                    ),
                ));
            }
        }

        for (id, (profile, path)) in &self.profiles {
            let package = package_name(path);
            let mut seen: BTreeSet<&str> = BTreeSet::new();
            for control in &profile.controls {
                if !CONTROL_IDS.contains(&control.id.as_str()) {
                    problems.push(Diagnostic::new(
                        "control.unknown_id",
                        &package,
                        format!("{id}.controls"),
                        format!("'{}' is not a control the runtime can bind", control.id),
                    ));
                }
                if !seen.insert(control.id.as_str()) {
                    problems.push(Diagnostic::new(
                        "control.duplicate_id",
                        &package,
                        format!("{id}.controls"),
                        format!("control '{}' is declared twice", control.id),
                    ));
                }
            }

            match &profile.presentation {
                Presentation::Illustrated { image } => {
                    let directory = path.parent().unwrap_or(Path::new(""));
                    match resolve_asset(directory, image) {
                        Err(reason) => problems.push(Diagnostic::new(
                            "asset.escapes_package",
                            &package,
                            format!("{id}.presentation.image"),
                            reason,
                        )),
                        Ok(resolved) if !resolved.is_file() => problems.push(Diagnostic::new(
                            "controller.illustration_missing",
                            &package,
                            format!("{id}.presentation.image"),
                            format!("declares illustration '{image}', which does not exist"),
                        )),
                        Ok(_) => {}
                    }
                    // Without anchors we would draw an illustrated pad's rings
                    // at the origin, which looks like a rendering bug and not
                    // like an error in the declaration.
                    // A group is one object on the pad, so it has one anchor
                    // for all its members. A stick's four directions and its
                    // click all point at the same drawn stick.
                    let mut anchored_groups: BTreeSet<&str> = BTreeSet::new();
                    for control in &profile.controls {
                        if control.x.is_some() && control.y.is_some() {
                            if let Some(group) = &control.group {
                                anchored_groups.insert(group.as_str());
                            }
                            continue;
                        }
                        if control.group.is_none() {
                            problems.push(Diagnostic::new(
                                "controller.anchor_missing",
                                &package,
                                format!("{id}.controls.{}", control.id),
                                "an illustrated profile needs x and y for every ungrouped control",
                            ));
                        }
                    }
                    let declared_groups: BTreeSet<&str> = profile
                        .controls
                        .iter()
                        .filter_map(|control| control.group.as_deref())
                        .collect();
                    for group in declared_groups.difference(&anchored_groups) {
                        problems.push(Diagnostic::new(
                            "controller.group_unanchored",
                            &package,
                            format!("{id}.controls[group={group}]"),
                            "a group on an illustrated profile needs exactly one member carrying x and y",
                        ));
                    }
                }
                Presentation::Generic => {}
            }
        }
    }

    pub fn consoles(&self) -> impl Iterator<Item = (&String, &ConsoleEntry)> {
        self.consoles.iter()
    }

    /// Consoles in the order an author should see them.
    ///
    /// First the declared order, then the rest by name, so the list is the
    /// same whatever the order of the directory listing.
    pub fn consoles_in_presentation_order(&self) -> Vec<(&String, &ConsoleEntry)> {
        let mut ordered: Vec<(&String, &ConsoleEntry)> = self.consoles.iter().collect();
        ordered.sort_by(|(_, a), (_, b)| {
            a.console
                .presentation_order
                .unwrap_or(u32::MAX)
                .cmp(&b.console.presentation_order.unwrap_or(u32::MAX))
                .then_with(|| a.console.name.cmp(&b.console.name))
        });
        ordered
    }

    pub fn console(&self, id: &str) -> Option<&ConsoleEntry> {
        self.consoles.get(id)
    }

    /// Resolve a console by its id or any declared alias, ignoring case.
    ///
    /// To keep the result, store the returned `console.id`, never the
    /// spelling you were given, because we hash that exact string into the
    /// name of the exported save directory.
    pub fn find(&self, needle: &str) -> Option<&ConsoleEntry> {
        let needle = needle.trim().to_ascii_lowercase();
        self.consoles.values().find(|entry| {
            entry.console.id.to_ascii_lowercase() == needle
                || entry
                    .console
                    .aliases
                    .iter()
                    .any(|alias| alias.to_ascii_lowercase() == needle)
        })
    }

    pub fn profile(&self, id: &str) -> Option<&ControllerProfile> {
        self.profiles.get(id).map(|(profile, _)| profile)
    }

    /// The package directory of a profile, for resolving its declared assets.
    pub fn profile_directory(&self, id: &str) -> Option<&Path> {
        self.profiles.get(id).and_then(|(_, path)| path.parent())
    }

    pub fn profiles(&self) -> impl Iterator<Item = (&String, &ControllerProfile)> {
        self.profiles.iter().map(|(id, (profile, _))| (id, profile))
    }

    /// Profiles in the order an author should see them, by the same rule as
    /// consoles: the declared order first, then the rest by name.
    /// Where we put a control's callout in the asset-free grid.
    ///
    /// In the generic presentation we lay the callouts out in four columns and
    /// do not point at an illustration. The positions follow a rule, so we
    /// derive them here and do not store them sixteen times per profile.
    pub fn generic_callout(index: usize) -> (i32, i32) {
        (32 + (index as i32 % 4) * 232, 20 + (index as i32 / 4) * 84)
    }

    pub fn profiles_in_presentation_order(&self) -> Vec<(&String, &ControllerProfile)> {
        let mut ordered: Vec<(&String, &ControllerProfile)> = self.profiles().collect();
        ordered.sort_by(|(_, a), (_, b)| {
            a.presentation_order
                .unwrap_or(u32::MAX)
                .cmp(&b.presentation_order.unwrap_or(u32::MAX))
                .then_with(|| a.name.cmp(&b.name))
        });
        ordered
    }

    pub fn component(&self, id: &str) -> Option<&CoreComponent> {
        self.components.get(id).map(|(component, _)| component)
    }

    pub fn components(&self) -> impl Iterator<Item = (&String, &CoreComponent)> {
        self.components
            .iter()
            .map(|(id, (component, _))| (id, component))
    }

    /// Every profile on offer for a console, default first.
    pub fn controller_variants(&self, console: &str) -> Vec<&ControllerProfile> {
        let Some(entry) = self.consoles.get(console) else {
            return Vec::new();
        };
        let mut ordered = Vec::new();
        if let Some(profile) = self.profile(&entry.console.controllers.default) {
            ordered.push(profile);
        }
        for variant in &entry.console.controllers.variants {
            if variant == &entry.console.controllers.default {
                continue;
            }
            if let Some(profile) = self.profile(variant) {
                ordered.push(profile);
            }
        }
        ordered
    }
}

fn package_name(path: &Path) -> String {
    // .../integrations/consoles/<package>/<subdir>/<file>.json
    path.parent()
        .and_then(Path::parent)
        .and_then(Path::file_name)
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// Join a declared asset path to its package, refusing any path out of it.
///
/// A declared path must be relative and stay inside the package. We reject
/// absolute paths, `..` and symlinks that point outside, so a manifest cannot
/// name a file in the repository or elsewhere on the person's disk.
pub fn resolve_asset(directory: &Path, declared: &str) -> Result<PathBuf, String> {
    let candidate = Path::new(declared);
    if candidate.is_absolute() {
        return Err(format!("'{declared}' must be relative to its package"));
    }
    for component in candidate.components() {
        match component {
            Component::ParentDir => {
                return Err(format!(
                    "'{declared}' must not escape its package with '..'"
                ))
            }
            Component::Prefix(_) | Component::RootDir => {
                return Err(format!("'{declared}' must be relative to its package"))
            }
            _ => {}
        }
    }
    let joined = directory.join(candidate);
    // A symlink can point outside, so compare canonical paths when both exist.
    if let (Ok(real), Ok(root)) = (joined.canonicalize(), directory.canonicalize()) {
        if !real.starts_with(&root) {
            return Err(format!("'{declared}' resolves outside its package"));
        }
    }
    Ok(joined)
}

fn read_json<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T, String> {
    let text = std::fs::read_to_string(path).map_err(|error| error.to_string())?;
    serde_json::from_str(&text).map_err(|error| error.to_string())
}

/// Generate the registries from the packages.
///
/// Keep the field order and formatting of the checked-in files exactly, so a
/// diff after generating them again shows a change in content, not noise.
fn hex_bytes_ok(text: &str) -> bool {
    !text.is_empty() && text.len() % 2 == 0 && text.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn header_title_json(header: &model::HeaderTitle) -> serde_json::Value {
    use serde_json::json;
    let mut window = json!({ "offset": header.offset, "length": header.length });
    if let Some(anchor) = &header.anchor {
        window["anchor"] = json!(anchor);
    }
    if let Some(at) = header.complement_at {
        window["complementAt"] = json!(at);
    }
    if let Some(magic) = &header.magic {
        let mut rendered = json!({ "offset": magic.offset });
        if let Some(text) = &magic.text {
            rendered["text"] = json!(text);
        }
        if let Some(hex) = &magic.hex {
            rendered["hex"] = json!(hex);
        }
        window["magic"] = rendered;
    }
    window
}

pub fn compatibility_registries(catalog: &Catalog) -> Result<Vec<(&'static str, String)>, String> {
    use serde_json::{json, Map, Value};

    // systems.json has one entry per console, in declaration order.
    let mut systems = Vec::new();
    for (id, entry) in catalog.consoles_in_presentation_order() {
        let console = &entry.console;
        let mut system = Map::new();
        system.insert("id".into(), json!(id));
        system.insert("name".into(), json!(console.name));
        system.insert("aliases".into(), json!(console.aliases));
        system.insert("extensions".into(), json!(console.content.extensions));
        if !console.content.sheets.is_empty() {
            system.insert("sheets".into(), json!(console.content.sheets));
        }
        if !console.content.companions.is_empty() {
            system.insert("companions".into(), json!(console.content.companions));
        }
        if !console.content.recognize_only.is_empty() {
            system.insert(
                "recognizeOnly".into(),
                json!(console.content.recognize_only),
            );
        }
        // For a console with no checksum catalogue we write no field at all.
        if let Some(reference) = &console.metadata.catalog {
            system.insert("catalog".into(), json!(reference.name));
        }
        let cores: Vec<Value> = console
            .cores
            .iter()
            .filter_map(|binding| {
                let component = catalog.component(&binding.component)?;
                Some({
                    let mut core = json!({
                        // We write every declared target, so the registry does not
                        // depend on the machine where we generated it. We pick the
                        // target to run or build for when we read the registry.
                        "artifacts": component.artifacts,
                        "component": component.id,
                        "license": component.license.spdx,
                        "licenseFile": component.license.file,
                        "capabilities": component.capabilities,
                        // The paths of per-core files in RetroArch contain this
                        // exact string: the controller remap, and the options for
                        // an unfiltered picture. So we need it in the player too.
                        "libraryName": component.library_name,
                    });
                    // We leave this out when the core's defaults already keep
                    // the pixels intact, so a console with nothing to override
                    // has no empty list.
                    if !component.pixels.is_empty() {
                        core["pixels"] = json!(component.pixels);
                    }
                    core
                })
            })
            .collect();
        system.insert("cores".into(), Value::Array(cores));
        if !console.firmware.is_empty() {
            let firmware: Vec<Value> = console
                .firmware
                .iter()
                .map(|group| {
                    json!({
                        "id": group.id,
                        "acceptedNames": group.accepted_names,
                        "minimum": group.minimum,
                        "help": group.help,
                    })
                })
                .collect();
            system.insert("firmware".into(), Value::Array(firmware));
        }
        system.insert(
            "controllerProfile".into(),
            json!(console.controllers.default),
        );
        system.insert("category".into(), json!(console.content.category));
        // We treat a bounded offset and a length as data, and a list of them
        // when the title is not at one address. A branch per console is code.
        if let Some(size) = console.recognition.copier_header {
            system.insert("copierHeader".into(), json!(size));
        }
        if !console.recognition.header_titles.is_empty() {
            let windows: Vec<Value> = console
                .recognition
                .header_titles
                .iter()
                .map(header_title_json)
                .collect();
            let value = if windows.len() == 1 {
                windows.into_iter().next().expect("one window")
            } else {
                Value::Array(windows)
            };
            system.insert("headerTitle".into(), value);
        }
        systems.push(Value::Object(system));
    }

    // controls.json has the profiles. We derive their system lists from the
    // consoles, so the two always match.
    let mut used_by: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for (id, entry) in catalog.consoles_in_presentation_order() {
        let mut offered: Vec<&str> = entry
            .console
            .controllers
            .variants
            .iter()
            .map(String::as_str)
            .collect();
        if offered.is_empty() {
            offered.push(entry.console.controllers.default.as_str());
        }
        for profile in offered {
            used_by.entry(profile).or_default().push(id.as_str());
        }
    }

    let mut profiles = Vec::new();
    for (id, profile) in catalog.profiles_in_presentation_order() {
        // We offer the built-in generic pad for every console. It is in no
        // console's package, so its system list is empty.
        let systems_for_profile: Vec<&str> = if id == BUILTIN_GENERIC_PROFILE {
            Vec::new()
        } else {
            used_by.get(id.as_str()).cloned().unwrap_or_default()
        };
        let mut entry = Map::new();
        entry.insert("id".into(), json!(id));
        entry.insert("name".into(), json!(profile.name));
        entry.insert("systems".into(), json!(systems_for_profile));
        entry.insert(
            "image".into(),
            match &profile.presentation {
                Presentation::Illustrated { image } => json!(image),
                Presentation::Generic => json!(""),
            },
        );
        let generic = matches!(profile.presentation, Presentation::Generic);
        let controls: Vec<Value> = profile
            .controls
            .iter()
            .enumerate()
            .map(|(index, control)| {
                let (callout_x, callout_y) = if generic {
                    Catalog::generic_callout(index)
                } else {
                    (
                        control.callout_x.unwrap_or(0),
                        control.callout_y.unwrap_or(0),
                    )
                };
                let mut rendered = json!({
                    "id": control.id,
                    "label": control.label,
                    "key": control.key,
                    "x": control.x.unwrap_or(0),
                    "y": control.y.unwrap_or(0),
                    "calloutX": callout_x,
                    "calloutY": callout_y,
                });
                // We leave the field out when it is absent, so the registry
                // entry of a profile without groups has no group field.
                if let Some(group) = &control.group {
                    rendered["group"] = json!(group);
                }
                rendered
            })
            .collect();
        entry.insert("controls".into(), Value::Array(controls));
        if let Some(device) = profile.core_device {
            entry.insert("coreDevice".into(), json!(device));
        }
        profiles.push(Value::Object(entry));
    }

    let mut rendered = Vec::new();
    for (name, value) in [
        ("systems.json", json!({ "version": 1, "systems": systems })),
        ("controls.json", json!({ "profiles": profiles })),
    ] {
        let mut text = serde_json::to_string_pretty(&value).map_err(|e| e.to_string())?;
        text.push('\n');
        rendered.push((name, text));
    }
    rendered.push(("core-pins.json", core_pins(catalog)?));
    Ok(rendered)
}

/// The bytes we accept at a first boot.
///
/// libretro replaces `latest` on the buildbot in place, so we reject a file
/// whose hash differs from the pinned one. We compile Gambatte and Genesis
/// Plus GX locally on macOS when the recipe matches. At first boot we fetch
/// the pinned file when the kit lacks that compiled file.
fn core_pins(catalog: &Catalog) -> Result<String, String> {
    use serde_json::{json, Map, Value};

    let mut cores = Vec::new();
    let mut components: Vec<_> = catalog.components().collect();
    components.sort_by(|left, right| left.0.cmp(right.0));
    for (id, component) in components {
        let Some(provenance) = &component.provenance else {
            continue;
        };
        if provenance.downloads.is_empty() {
            continue;
        }
        let Some(license_sha256) = &provenance.license_sha256 else {
            continue;
        };
        let Some(license_path) = provenance.license_candidates.first() else {
            continue;
        };
        let mut artifacts = Map::new();
        for (target, pin) in &provenance.downloads {
            let Some(filename) = component.artifacts.get(target) else {
                continue;
            };
            artifacts.insert(
                target.clone(),
                json!({
                    "filename": filename,
                    "archiveSha256": pin.archive_sha256,
                    "binarySha256": pin.binary_sha256,
                    "archiveBytes": pin.archive_bytes,
                    "binaryBytes": pin.binary_bytes,
                }),
            );
        }
        cores.push(json!({
            "component": id,
            "repository": provenance.repository,
            "revision": provenance.revision,
            "licenseFile": component.license.file,
            "licensePath": license_path,
            "licenseSha256": license_sha256,
            "artifacts": Value::Object(artifacts),
        }));
    }
    let value = json!({
        "schemaVersion": 1,
        "measured": "2026-09-22",
        "coreMirrors": [
            "https://buildbot.libretro.com/nightly",
            "https://bot.libretro.com/nightly"
        ],
        "targets": {
            "linux-x86_64": "linux/x86_64",
            "macos-arm64": "apple/osx/arm64",
            "macos-x86_64": "apple/osx/x86_64",
            "windows-x86_64": "windows/x86_64"
        },
        "licenseMirrors": [
            "https://raw.githubusercontent.com/{repository}/{revision}/{path}",
            "https://cdn.jsdelivr.net/gh/{repository}@{revision}/{path}"
        ],
        "cores": cores
    });
    let mut text = serde_json::to_string_pretty(&value).map_err(|error| error.to_string())?;
    text.push('\n');
    Ok(text)
}
