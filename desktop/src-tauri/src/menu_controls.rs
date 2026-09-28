//! MENU CONTROLS, the inputs that control the in-game menu. Each menu action
//! (open and close the menu, confirm, go back) has a list of bindings, and
//! each binding is a key or several pad inputs pressed together.
//!
//! We read the actions, the bindings each must keep and the binding format
//! from `menu_controls.inc` of the player. The defaults of the author come
//! from the export request, or else from `desktop/defaults.json`. At export
//! we write them into `menu-controls-defaults.cfg` in the menu folder, with
//! the word for each pad input in the menu. The changes that the player makes
//! go to the game's data (`menu-controls.cfg`, which we write only from the
//! menu), so a later export with other defaults does not replace them.

use crate::menu::{file_name, key};
use serde::{de, Deserialize, Deserializer, Serialize, Serializer};
use std::collections::BTreeMap;
use std::fmt;

const SOURCE: &str =
    include_str!("../../../vendor/retroarch/menu/drivers/rmlui/menu_controls.inc");

/// The file in the menu's folder that we write the defaults into on export.
pub const DEFAULTS_FILE: &str = file_name!(MenuControlsDefaults);

/// Every `macro_name(...)` declaration's fields, in order.
fn declared(macro_name: &'static str) -> impl Iterator<Item = Vec<&'static str>> {
    crate::menu::inc::declarations(SOURCE)
        .filter(move |declaration| declaration.macro_name() == macro_name)
        .map(|declaration| declaration.fields())
}

/// The one field in a single declaration `macro_name(value)`.
fn only(macro_name: &'static str, field: usize) -> &'static str {
    declared(macro_name)
        .next()
        .and_then(|fields| fields.get(field).copied())
        .unwrap_or_else(|| panic!("menu_controls.inc declares no {macro_name}"))
}

/// One of the menu actions, by its name in `menu_controls.inc`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Action {
    Menu,
    Confirm,
    Back,
}

/// The bindings an action always keeps, so that no one locks themselves out.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Keeps {
    /// At least one binding of any kind.
    Binding,
    /// At least one keyboard key.
    Key,
}

impl Action {
    pub const ALL: [Action; 3] = [Action::Menu, Action::Confirm, Action::Back];

    fn declaration(self) -> Vec<&'static str> {
        let name = format!("{self:?}");
        declared("RIB_MENU_ACTION")
            .find(|fields| fields.first() == Some(&name.as_str()))
            .unwrap_or_else(|| panic!("menu_controls.inc declares no RIB_MENU_ACTION({name}, ...)"))
    }

    /// The id of the action in the files, the document and a request.
    pub fn id(self) -> &'static str {
        self.declaration()[1]
    }

    pub fn keeps(self) -> Keeps {
        match self.declaration()[2] {
            "Key" => Keeps::Key,
            "Binding" => Keeps::Binding,
            other => panic!("menu_controls.inc: {self:?} keeps '{other}', which is neither Key nor Binding"),
        }
    }

    fn named(id: &str) -> Option<Action> {
        Action::ALL.into_iter().find(|action| action.id() == id)
    }

    /// Whether two actions may have the same input.
    pub fn shares_with(self, other: Action) -> bool {
        declared("RIB_MENU_ACTIONS_SHARE").any(|pair| {
            let (one, two) = (format!("{self:?}"), format!("{other:?}"));
            (pair[0] == one && pair[1] == two) || (pair[0] == two && pair[1] == one)
        })
    }
}

/// A pad input in a binding: a position of the standard pad, by its bind
/// name in RetroArch, or the menu button on the pad.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PadInput {
    Position(String),
    Home,
}

impl PadInput {
    fn home_field(field: usize) -> &'static str {
        only("RIB_MENU_PAD_HOME", field)
    }

    pub fn id(&self) -> &str {
        match self {
            PadInput::Position(id) => id,
            PadInput::Home => PadInput::home_field(0),
        }
    }

    fn read(id: &str) -> Result<PadInput, String> {
        if id == PadInput::home_field(0) {
            return Ok(PadInput::Home);
        }
        let positions = crate::controls::pad_positions()?;
        if positions.iter().any(|position| position.id == id) {
            return Ok(PadInput::Position(id.to_string()));
        }
        let known: Vec<String> = positions.into_iter().map(|position| position.id).collect();
        Err(format!(
            "'{id}' is no pad input; the pad inputs are {} and {}",
            known.join(", "),
            PadInput::home_field(0)
        ))
    }
}

/// One binding: a key by its name in the RetroArch config, or pad inputs
/// held together.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Binding {
    Key(String),
    Pad(Vec<PadInput>),
}

fn prefix(kind: &str) -> &'static str {
    declared("RIB_MENU_BINDING")
        .find(|fields| fields[0] == kind)
        .map(|fields| fields[1])
        .unwrap_or_else(|| panic!("menu_controls.inc declares no RIB_MENU_BINDING({kind}, ...)"))
}

fn chord() -> &'static str {
    only("RIB_MENU_PAD_CHORD", 0)
}

impl Binding {
    /// The binding as the player reads one.
    pub fn text(&self) -> String {
        match self {
            Binding::Key(name) => format!("{}{name}", prefix("Key")),
            Binding::Pad(inputs) => format!(
                "{}{}",
                prefix("Pad"),
                inputs.iter().map(PadInput::id).collect::<Vec<_>>().join(chord())
            ),
        }
    }

    pub fn is_key(&self) -> bool {
        matches!(self, Binding::Key(_))
    }

    /// `text` as a binding, after we check that it is a key known to RetroArch
    /// or pad inputs of the standard pad, with none twice.
    pub fn read(text: &str) -> Result<Binding, String> {
        if let Some(name) = text.strip_prefix(prefix("Key")) {
            return match crate::controls::retroarch_key(name) {
                Some(_) => Ok(Binding::Key(name.to_string())),
                None => Err(format!("'{name}' in '{text}' is no key RetroArch reads")),
            };
        }
        let Some(inputs) = text.strip_prefix(prefix("Pad")) else {
            return Err(format!(
                "'{text}' is no binding: a binding is {}<key> or {}<pad input>[{}<pad input>...]",
                prefix("Key"),
                prefix("Pad"),
                chord()
            ));
        };
        let mut read = Vec::new();
        for id in inputs.split(chord()) {
            let input = PadInput::read(id)?;
            if read.contains(&input) {
                return Err(format!("'{text}' holds {id} twice"));
            }
            read.push(input);
        }
        Ok(Binding::Pad(read))
    }
}

impl Serialize for Binding {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.text())
    }
}

impl<'de> Deserialize<'de> for Binding {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Binding::read(&String::deserialize(deserializer)?).map_err(de::Error::custom)
    }
}

/// The bindings of each action, in order, as the defaults of an export.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MenuControls {
    lists: BTreeMap<Action, Vec<Binding>>,
}

impl MenuControls {
    pub fn of(&self, action: Action) -> &[Binding] {
        self.lists.get(&action).map(Vec::as_slice).unwrap_or(&[])
    }

    /// The rules for every change in the menu of the game. Each action keeps
    /// the bindings it must keep, and an input belongs to one action, or to
    /// two that share it.
    pub fn check(&self) -> Result<(), String> {
        for action in Action::ALL {
            let list = self.of(action);
            if list.is_empty() {
                return Err(format!("menu controls: {} has no binding, and must keep one", action.id()));
            }
            if action.keeps() == Keeps::Key && !list.iter().any(Binding::is_key) {
                return Err(format!("menu controls: {} has no key, and must keep one", action.id()));
            }
            for (index, binding) in list.iter().enumerate() {
                if list[..index].contains(binding) {
                    return Err(format!("menu controls: {} holds {} twice", action.id(), binding.text()));
                }
                for other in Action::ALL {
                    if other != action && !action.shares_with(other) && self.of(other).contains(binding) {
                        return Err(format!(
                            "menu controls: {} is bound to both {} and {}, which cannot share an input",
                            binding.text(),
                            action.id(),
                            other.id()
                        ));
                    }
                }
            }
        }
        Ok(())
    }

    /// `menu-controls-defaults.cfg`, with the list of each action and then the
    /// word for every pad input in the menu.
    pub fn defaults_config(&self) -> Result<String, String> {
        self.check()?;
        let mut text = String::new();
        for action in Action::ALL {
            let list: Vec<String> = self.of(action).iter().map(Binding::text).collect();
            text.push_str(&format!("{} = \"{}\"\n", key!(MenuControl, action.id()), list.join(" ")));
        }
        for position in crate::controls::pad_positions()? {
            text.push_str(&format!("{} = \"{}\"\n", key!(PadWord, position.id), position.name));
        }
        text.push_str(&format!(
            "{} = \"{}\"\n",
            key!(PadWord, PadInput::home_field(0)),
            PadInput::home_field(2)
        ));
        Ok(text)
    }
}

impl Serialize for MenuControls {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let named: BTreeMap<&str, &Vec<Binding>> =
            self.lists.iter().map(|(action, list)| (action.id(), list)).collect();
        named.serialize(serializer)
    }
}

/// A request lists the actions it changes by their ids. An action missing
/// from it keeps the builder defaults (`desktop/defaults.json`), and we
/// reject an id that matches no action in the menu.
impl<'de> Deserialize<'de> for MenuControls {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Lists;
        impl<'de> de::Visitor<'de> for Lists {
            type Value = BTreeMap<Action, Vec<Binding>>;
            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                write!(formatter, "each menu action's bindings, by the action's id")
            }
            fn visit_map<M: de::MapAccess<'de>>(self, mut map: M) -> Result<Self::Value, M::Error> {
                let mut lists = BTreeMap::new();
                while let Some(id) = map.next_key::<String>()? {
                    let action = Action::named(&id).ok_or_else(|| {
                        let ids: Vec<&str> = Action::ALL.iter().map(|action| action.id()).collect();
                        de::Error::custom(format!(
                            "'{id}' is no menu action; the actions are {}",
                            ids.join(", ")
                        ))
                    })?;
                    lists.insert(action, map.next_value()?);
                }
                Ok(lists)
            }
        }
        let given = deserializer.deserialize_map(Lists)?;
        Ok(MenuControls::with(given))
    }
}

impl MenuControls {
    /// `given`, applied over the builder defaults.
    fn with(given: BTreeMap<Action, Vec<Binding>>) -> MenuControls {
        let mut lists = crate::builder::defaults().menu_controls.lists.clone();
        lists.extend(given);
        MenuControls { lists }
    }

    /// Only the values in `given`, for the builder defaults, which we apply
    /// over nothing.
    pub(crate) fn declared(given: BTreeMap<Action, Vec<Binding>>) -> MenuControls {
        MenuControls { lists: given }
    }
}

/// We read the builder defaults with nothing under them.
pub(crate) fn read_defaults<'de, D: Deserializer<'de>>(deserializer: D) -> Result<MenuControls, D::Error> {
    struct Whole;
    impl<'de> de::Visitor<'de> for Whole {
        type Value = MenuControls;
        fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
            write!(formatter, "every menu action's bindings, by the action's id")
        }
        fn visit_map<M: de::MapAccess<'de>>(self, mut map: M) -> Result<Self::Value, M::Error> {
            let mut lists = BTreeMap::new();
            while let Some(id) = map.next_key::<String>()? {
                let action = Action::named(&id)
                    .ok_or_else(|| de::Error::custom(format!("'{id}' is no menu action")))?;
                lists.insert(action, map.next_value()?);
            }
            for action in Action::ALL {
                if !lists.contains_key(&action) {
                    return Err(de::Error::custom(format!(
                        "desktop/defaults.json gives {} no bindings",
                        action.id()
                    )));
                }
            }
            Ok(MenuControls::declared(lists))
        }
    }
    deserializer.deserialize_map(Whole)
}

/// The one meta line we keep in a shipped controller profile. It contains the name
/// of the button that is the menu button on the pad, which is Home in the menu.
pub fn home_button_key() -> String {
    format!("input_{}_btn", PadInput::home_field(1))
}

/// The ids by which we find a MENU CONTROLS row, `<prefix><action>` followed
/// by its suffix, or by `-<n>` for its chips.
fn row_id(action: Action) -> String {
    format!("{}{}", crate::menu::contract!(MenuControlPrefix), action.id())
}

/// The parts of every row that must be in the screen when we ship it: its +
/// and its first chip.
pub fn required_ids() -> Vec<String> {
    Action::ALL
        .into_iter()
        .flat_map(|action| {
            [
                format!("{}{}", row_id(action), crate::menu::contract!(AddSuffix)),
                format!("{}-1", row_id(action)),
            ]
        })
        .collect()
}

/// The number of bindings in the row of `action` in `document`, which is the
/// number of its chips, numbered from 1. None when there is no such row.
pub fn room(document: &str, action: Action) -> Option<usize> {
    let add = format!("id=\"{}{}\"", row_id(action), crate::menu::contract!(AddSuffix));
    if !document.contains(&add) {
        return None;
    }
    Some(
        (1..)
            .take_while(|chip| document.contains(&format!("id=\"{}-{chip}\"", row_id(action))))
            .count(),
    )
}

/// Reject defaults that do not fit in MENU CONTROLS of a design, because a
/// row has room for as many bindings as it has chips.
pub fn fit(controls: &MenuControls, document: &str, design: &str) -> Result<(), String> {
    for action in Action::ALL {
        if let Some(room) = room(document, action) {
            let wanted = controls.of(action).len();
            if wanted > room {
                return Err(format!(
                    "design '{design}' shows {room} bindings for {} on MENU CONTROLS, and the \
                     game's defaults give it {wanted}",
                    action.id()
                ));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests;
