//! What a composed menu must contain, from the contract in the player.
//!
//! `document_contract.inc` contains the names we look up in the player. It
//! is grouped by screen role, so "required" means "required whenever we ship
//! that screen". We check every composed menu against it before we write
//! anything, and tell the author which design, file and id are at fault.

use super::manifest::{Manifest, Screen, ScreenRole};

const SOURCE: &str =
    include_str!("../../../../vendor/retroarch/menu/drivers/rmlui/document_contract.inc");

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Element,
    Class,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scope {
    /// Every document, the splash included.
    Shared,
    /// Every menu document.
    Menu,
    /// The logo-only document.
    Splash,
    /// The numbered save slots, `<prefix><1..=count>`.
    Slots,
    /// A prefix the exporter fills in.
    Generated,
    /// A class we set in the player.
    State,
    /// One screen, whenever we ship it.
    Screen(ScreenRole),
}

#[derive(Clone, Debug)]
pub struct Entry {
    pub kind: Kind,
    pub name: String,
    pub value: String,
    pub scope: Scope,
    pub required: bool,
}

fn scope(word: &str) -> Result<Scope, String> {
    Ok(match word {
        "Shared" => Scope::Shared,
        "Menu" => Scope::Menu,
        "Splash" => Scope::Splash,
        "Slots" => Scope::Slots,
        "Generated" => Scope::Generated,
        "State" => Scope::State,
        other => Scope::Screen(
            ScreenRole::ALL
                .into_iter()
                .find(|role| format!("{role:?}") == other)
                .ok_or_else(|| format!("document_contract.inc names an unknown scope '{other}'"))?,
        ),
    })
}

/// Every `macro_name(...)` declaration in the contract, in order.
fn declarations(macro_name: &'static str) -> impl Iterator<Item = super::inc::Declaration> {
    super::inc::declarations(SOURCE)
        .filter(move |declaration| declaration.macro_name() == macro_name)
}

/// The id, class or attribute declared under `name` in the contract: the
/// value of the element, class, attribute or fact, as in
/// `document_contract::<name>` in the player. Read it with `contract!(Name)`,
/// so that we read it when the exporter compiles.
pub const fn value(name: &str) -> &'static str {
    super::inc::declared(
        SOURCE,
        &["RIB_ELEMENT", "RIB_CLASS", "RIB_ATTRIBUTE", "RIB_FACT"],
        name,
        1,
    )
}

/// `contract!(Name)`: the value declared under `Name` in
/// `document_contract.inc`, the same name we use in the C++ of the player.
/// The build fails on a name that the contract does not declare.
macro_rules! contract {
    ($name:ident) => {
        const { $crate::menu::contract::value(stringify!($name)) }
    };
}
pub(crate) use contract;

/// The word for `role` in the contract, design.json and design.cfg. Every
/// role with special handling in the exporter has a declaration in the player.
pub fn role_word(role: ScreenRole) -> &'static str {
    let name = format!("{role:?}");
    super::inc::find(SOURCE, &["RIB_ROLE"], &name)
        .and_then(|declaration| declaration.field(1))
        .unwrap_or_else(|| panic!("document_contract.inc declares no RIB_ROLE({name}, ...)"))
}

/// The reference canvas every design is laid out on, in dp: width, height.
pub fn canvas() -> (u32, u32) {
    declarations("RIB_CANVAS")
        .find_map(|declaration| match declaration.fields()[..] {
            [width, height] => Some((width.parse().ok()?, height.parse().ok()?)),
            _ => None,
        })
        .expect("document_contract.inc declares the canvas as RIB_CANVAS(width, height)")
}

/// Every declaration in the contract.
pub fn entries() -> Result<Vec<Entry>, String> {
    let mut entries = Vec::new();
    for declaration in super::inc::declarations(SOURCE) {
        let kind = match declaration.macro_name() {
            "RIB_ELEMENT" => Kind::Element,
            "RIB_CLASS" => Kind::Class,
            _ => continue,
        };
        let fields = declaration.fields();
        let [name, value, scope_word, presence] = fields[..] else {
            return Err(format!(
                "a contract declaration needs four fields: {}({})",
                declaration.macro_name(),
                fields.join(", ")
            ));
        };
        if value.is_empty() {
            return Err(format!("the contract declares {name} with no id"));
        }
        let required = match presence {
            "Required" => true,
            "Optional" => false,
            other => return Err(format!("'{other}' is neither Required nor Optional: {name}")),
        };
        entries.push(Entry {
            kind,
            name: name.to_string(),
            value: value.to_string(),
            scope: scope(scope_word)?,
            required,
        });
    }
    if entries.is_empty() {
        return Err("document_contract.inc declares nothing".into());
    }
    Ok(entries)
}

/// The number of save slots in the player.
pub fn slot_count() -> Result<usize, String> {
    declarations("RIB_SLOT_COUNT")
        .find_map(|declaration| declaration.field(0)?.parse().ok())
        .ok_or_else(|| "document_contract.inc declares no slot count".to_string())
}

fn has_id(document: &str, id: &str) -> bool {
    document.contains(&format!("id=\"{id}\""))
}

fn has_class(document: &str, class: &str) -> bool {
    document.split("class=\"").skip(1).any(|rest| {
        rest.split('"')
            .next()
            .unwrap_or("")
            .split_whitespace()
            .any(|token| token == class)
    })
}

/// The opening tag of the element with `id`.
fn opening_tag<'a>(document: &'a str, id: &str) -> Option<&'a str> {
    let at = document.find(&format!("id=\"{id}\""))?;
    let start = document[..at].rfind('<')?;
    let end = document[at..].find('>')? + at;
    Some(&document[start..end])
}

/// The files of a screen, in the order in which an author would look.
fn screen_files(screen: &Screen) -> Vec<String> {
    let mut files = vec![
        format!("screen-{}.rml", screen.id),
        format!("actions-{}.rml", screen.id),
    ];
    files.extend(screen.dialogs.iter().map(|dialog| format!("dialog-{dialog}.rml")));
    if screen.role == Some(ScreenRole::Pause) {
        files.push("save-slots.rml".into());
    }
    files
}

/// Which of those files come from this design instead of from Native.
fn own_files(manifest: &Manifest, screen: &Screen) -> Vec<String> {
    if manifest.design == manifest.base {
        return Vec::new();
    }
    screen_files(screen)
        .into_iter()
        .filter(|name| manifest.design.join(name).is_file())
        .collect()
}

fn missing(manifest: &Manifest, screen: &Screen, what: &str) -> String {
    let role = screen.role.map(ScreenRole::name).unwrap_or(&screen.id);
    let own = own_files(manifest, screen);
    if !own.is_empty() {
        return format!(
            "{} in design '{}' omits {what}, required by the {role} screen",
            own.join(" / "),
            manifest.id
        );
    }
    let native: Vec<String> = screen_files(screen)
        .into_iter()
        .filter(|name| manifest.base.join(name).is_file())
        .collect();
    let source = if native.is_empty() {
        "the exporter generates this screen from what the game bundles".to_string()
    } else {
        format!("it comes from Native's {}", native.join(", "))
    };
    format!(
        "design '{}': the {role} screen ships without {what}, which the player requires; {source}",
        manifest.id
    )
}

/// We refuse a design file for a screen, an action row or a dialog that no
/// one declares, because otherwise we would ignore it silently.
fn unknown_files(manifest: &Manifest) -> Result<(), String> {
    let Ok(listing) = std::fs::read_dir(&manifest.design) else {
        return Ok(());
    };
    for entry in listing.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        let Some(stem) = name.strip_suffix(".rml") else {
            continue;
        };
        let known = if let Some(id) = stem
            .strip_prefix("screen-")
            .or_else(|| stem.strip_prefix("actions-"))
        {
            // screen-order.rml contains the order of screens and is not one.
            stem == "screen-order" || manifest.screens.iter().any(|screen| screen.id == id)
        } else if let Some(dialog) = stem.strip_prefix("dialog-") {
            manifest
                .screens
                .iter()
                .any(|screen| screen.dialogs.iter().any(|declared| declared == dialog))
        } else {
            true
        };
        if !known {
            let declared: Vec<&str> = manifest.screens.iter().map(|s| s.id.as_str()).collect();
            return Err(format!(
                "{name} in design '{}' names no screen or dialog the menu has; its screens are {}",
                manifest.id,
                declared.join(", ")
            ));
        }
    }
    Ok(())
}

/// Check a composed menu document against the contract.
///
/// `shipped` is every screen in the game. Each must have its panel, and
/// every panel except the Pause panel has `.screen-panel` and starts hidden.
/// Every entry required in every menu, and every entry required by the role
/// of a shipped screen, must be present.
pub fn validate(manifest: &Manifest, document: &str, shipped: &[&Screen]) -> Result<(), String> {
    unknown_files(manifest)?;
    let entries = entries()?;
    let slots = slot_count()?;
    let pause = shipped
        .iter()
        .copied()
        .find(|screen| screen.role == Some(ScreenRole::Pause));
    for entry in entries.iter().filter(|entry| entry.required) {
        let owner: Option<&Screen> = match entry.scope {
            Scope::Shared | Scope::Menu => None,
            Scope::Slots => pause,
            Scope::Screen(role) => match shipped.iter().find(|s| s.role == Some(role)) {
                Some(screen) => Some(screen),
                None => continue,
            },
            Scope::Splash | Scope::Generated | Scope::State => continue,
        };
        let absent: Vec<String> = match (entry.kind, entry.scope) {
            (Kind::Element, Scope::Slots) => (1..=slots)
                .map(|slot| format!("{}{slot}", entry.value))
                .filter(|id| !has_id(document, id))
                .map(|id| format!("#{id}"))
                .collect(),
            (Kind::Element, _) if !has_id(document, &entry.value) => {
                vec![format!("#{}", entry.value)]
            }
            (Kind::Class, _) if !has_class(document, &entry.value) => {
                vec![format!("an element of class .{}", entry.value)]
            }
            _ => Vec::new(),
        };
        if let Some(what) = absent.first() {
            return Err(match owner {
                Some(screen) => missing(manifest, screen, what),
                None => format!(
                    "the menu composed from design '{}' has no {what}, which every menu requires",
                    manifest.id
                ),
            });
        }
    }
    // HOTKEYS has a row for every hotkey declared in the player.
    if let Some(screen) = shipped
        .iter()
        .find(|screen| screen.role == Some(ScreenRole::Hotkeys))
    {
        if let Some(id) = crate::hotkeys::required_ids()
            .into_iter()
            .find(|id| !has_id(document, id))
        {
            return Err(missing(manifest, screen, &format!("#{id}")));
        }
    }
    for screen in shipped {
        let Some(tag) = opening_tag(document, &screen.panel) else {
            return Err(missing(manifest, screen, &format!("its panel #{}", screen.panel)));
        };
        if screen.role == Some(ScreenRole::Pause) {
            continue;
        }
        if !has_class(tag, contract!(ScreenPanel)) {
            return Err(missing(
                manifest,
                screen,
                &format!("class .{} on #{}", contract!(ScreenPanel), screen.panel),
            ));
        }
        if !tag.replace(' ', "").contains("display:none") {
            return Err(missing(
                manifest,
                screen,
                &format!("display:none on #{}, so it starts hidden", screen.panel),
            ));
        }
    }
    Ok(())
}

/// Check the logo-only document.
pub fn validate_splash(manifest: &Manifest, document: &str) -> Result<(), String> {
    for entry in entries()? {
        if entry.required
            && entry.kind == Kind::Element
            && matches!(entry.scope, Scope::Shared | Scope::Splash)
            && !has_id(document, &entry.value)
        {
            return Err(format!(
                "{} in design '{}' omits #{}, which the logo-only document requires",
                manifest.documents.splash, manifest.id, entry.value
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_contract_reads_and_every_scope_is_known() {
        let entries = entries().unwrap();
        assert!(entries.iter().any(|entry| entry.value == "achievements-login"
            && entry.scope == Scope::Screen(ScreenRole::Achievements)
            && entry.required));
        // Every name, of elements and classes alike, is in one C++ namespace.
        let mut names = std::collections::BTreeSet::new();
        for entry in &entries {
            assert!(names.insert(entry.name.clone()), "{} is declared twice", entry.name);
        }
        assert_eq!(slot_count().unwrap(), 6);
    }

    #[test]
    fn a_role_goes_by_its_declared_word_and_the_canvas_has_its_size() {
        assert_eq!(ScreenRole::Achievements.name(), "achievements");
        assert_eq!(canvas(), (960, 600));
    }
}
