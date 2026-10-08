//! HOTKEYS: the inputs for the game's menu, and the ones for use during play.
//! Each hotkey (open and close the menu, confirm, go back, quick save, quick
//! load, the previous and the next slot) has a list of bindings. A binding is
//! a key, or pad inputs held together.
//!
//! In the player's `hotkeys.inc` we declare the hotkeys, what each must keep,
//! when the player can use each, and how we write a binding, and we read
//! that file here. The author's defaults come from the export request, or
//! else from `desktop/defaults.json`. On export we write them into
//! `hotkeys-defaults.cfg` in the menu's folder, with the words we show in
//! the menu for each pad input. We keep the player's changes in `hotkeys.cfg`
//! in the game's data and write it only from the menu, never on export.

use crate::controls::GameInput;
use crate::menu::{file_name, key};
use serde::{de, Deserialize, Deserializer, Serialize, Serializer};
use std::collections::BTreeMap;
use std::fmt;
use std::sync::OnceLock;

const SOURCE: &str = include_str!("../../../../vendor/retroarch/menu/drivers/rmlui/hotkeys.inc");

/// The file in the menu's folder that we write the defaults into on export.
pub const DEFAULTS_FILE: &str = file_name!(HotkeysDefaults);

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
        .unwrap_or_else(|| panic!("hotkeys.inc declares no {macro_name}"))
}

/// What a hotkey must always keep, so that nobody can lock themselves out.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Keeps {
    /// At least one binding of any kind.
    Binding,
    /// At least one keyboard key.
    Key,
    /// Nothing: it may be left unbound.
    Nothing,
}

/// When the player can use a hotkey.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Acts {
    /// While the menu is open.
    InMenu,
    /// While the game plays.
    InGame,
    Both,
}

impl Acts {
    /// Whether the player can use it during play, so that a key it shares
    /// with the game would do both actions.
    pub fn in_game(self) -> bool {
        self != Acts::InMenu
    }
}

/// A `RIB_HOTKEY(name, "id", keeps, acts)` line.
struct Declaration {
    name: &'static str,
    id: &'static str,
    keeps: Keeps,
    acts: Acts,
}

fn declarations() -> &'static [Declaration] {
    static READ: OnceLock<Vec<Declaration>> = OnceLock::new();
    READ.get_or_init(|| {
        let read: Vec<Declaration> = declared("RIB_HOTKEY")
            .map(|fields| match fields[..] {
                [name, id, keeps, acts] => Declaration {
                    name,
                    id,
                    keeps: match keeps {
                        "Binding" => Keeps::Binding,
                        "Key" => Keeps::Key,
                        "Nothing" => Keeps::Nothing,
                        other => panic!("hotkeys.inc: {name} keeps '{other}', which is not Binding, Key or Nothing"),
                    },
                    acts: match acts {
                        "InMenu" => Acts::InMenu,
                        "InGame" => Acts::InGame,
                        "Both" => Acts::Both,
                        other => panic!("hotkeys.inc: {name} acts '{other}', which is not InMenu, InGame or Both"),
                    },
                },
                ref other => panic!("hotkeys.inc: RIB_HOTKEY({}) is not (name, \"id\", keeps, acts)", other.join(", ")),
            })
            .collect();
        assert!(!read.is_empty(), "hotkeys.inc declares no hotkey");
        read
    })
}

/// The hotkey that a game has only with fast forward on.
pub const FAST_FORWARD: &str = "fast-forward";

/// The hotkey settings of an export beyond the bindings: the hotkeys the
/// game does not have, and the starting way of each hotkey with ways.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct GameHotkeys {
    pub absent: Vec<Hotkey>,
    pub modes: Vec<(Hotkey, &'static str)>,
}

impl GameHotkeys {
    /// The settings of a game, from the defaults of its player settings. Fast
    /// forward is absent when off, and works while held (its first way) or
    /// from one press to the next (its second).
    pub fn of(defaults: &crate::player_settings::Defaults) -> GameHotkeys {
        let Some(fast_forward) = Hotkey::named(FAST_FORWARD) else {
            return GameHotkeys::default();
        };
        let ways = fast_forward.modes();
        GameHotkeys {
            absent: if defaults.fast_forward { Vec::new() } else { vec![fast_forward] },
            modes: ways
                .get(if defaults.fast_forward_hold { 0 } else { 1 })
                .map(|way| vec![(fast_forward, *way)])
                .unwrap_or_default(),
        }
    }

    /// `hotkeys` with every hotkey the game does not have bound to nothing,
    /// so we neither check nor write a binding kept for it.
    pub fn offered(&self, hotkeys: &Hotkeys) -> Hotkeys {
        self.absent.iter().fold(hotkeys.clone(), |kept, hotkey| kept.without(*hotkey))
    }
}

/// One hotkey, a `RIB_HOTKEY` declaration in `hotkeys.inc`, by its position
/// there. There are no other hotkeys. Get one from `Hotkey::all` or
/// `Hotkey::named`.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Hotkey(usize);

impl Hotkey {
    /// Every hotkey, in the order of `hotkeys.inc`.
    pub fn all() -> impl Iterator<Item = Hotkey> {
        (0..declarations().len()).map(Hotkey)
    }

    /// The hotkey with this id, as used in the files, the document and a
    /// request.
    pub fn named(id: &str) -> Option<Hotkey> {
        Hotkey::all().find(|hotkey| hotkey.id() == id)
    }

    fn declaration(self) -> &'static Declaration {
        &declarations()[self.0]
    }

    pub fn id(self) -> &'static str {
        self.declaration().id
    }

    /// The ways it can work, which the player chooses on its row, as declared
    /// in `hotkeys.inc` (`RIB_HOTKEY_MODES`). Most hotkeys have none.
    pub fn modes(self) -> Vec<&'static str> {
        declared("RIB_HOTKEY_MODES")
            .find(|fields| fields.first() == Some(&self.declaration().name))
            .map(|fields| fields[1..].to_vec())
            .unwrap_or_default()
    }

    pub fn keeps(self) -> Keeps {
        self.declaration().keeps
    }

    pub fn acts(self) -> Acts {
        self.declaration().acts
    }

    /// Whether two hotkeys may have the same input.
    pub fn shares_with(self, other: Hotkey) -> bool {
        let (one, two) = (self.declaration().name, other.declaration().name);
        declared("RIB_HOTKEYS_SHARE")
            .any(|pair| (pair[0] == one && pair[1] == two) || (pair[0] == two && pair[1] == one))
    }

    fn ids() -> String {
        Hotkey::all().map(Hotkey::id).collect::<Vec<_>>().join(", ")
    }
}

impl fmt::Debug for Hotkey {
    fn fmt(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str(self.id())
    }
}

impl Serialize for Hotkey {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.id())
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
        only("RIB_HOTKEY_PAD_HOME", field)
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
        let positions = button_positions()?;
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

/// The pad positions allowed in a binding. In the player we read a pad
/// input as a button, so a stick's directions are not among them.
fn button_positions() -> Result<Vec<crate::controls::PadPosition>, String> {
    Ok(crate::controls::pad_positions()?.into_iter().filter(|position| position.is_button()).collect())
}

/// One binding: a key by its name in the RetroArch config, or pad inputs
/// held together.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Binding {
    Key(String),
    Pad(Vec<PadInput>),
}

fn prefix(kind: &str) -> &'static str {
    declared("RIB_HOTKEY_BINDING")
        .find(|fields| fields[0] == kind)
        .map(|fields| fields[1])
        .unwrap_or_else(|| panic!("hotkeys.inc declares no RIB_HOTKEY_BINDING({kind}, ...)"))
}

fn chord() -> &'static str {
    only("RIB_HOTKEY_PAD_CHORD", 0)
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

/// Hotkeys that break a rule of the player's menu, by their ids. We word it
/// for the author in the builder, and refuse the export with its sentence.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum Refusal {
    NoBinding { hotkey: Hotkey },
    NoKey { hotkey: Hotkey },
    Twice { hotkey: Hotkey, binding: Binding },
    /// `binding` is in both `hotkey` and `other`, which may not share one.
    Shared { binding: Binding, hotkey: Hotkey, other: Hotkey },
    /// `binding`, a key or pad button of `hotkey`, which the player uses
    /// during play, is also an input for the game's `control`, which we show
    /// as `label`.
    GameInput { binding: Binding, hotkey: Hotkey, control: String, label: String },
    /// We could not read the game's inputs, for the reason in `message`.
    Controls { message: String },
}

impl fmt::Display for Refusal {
    fn fmt(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Refusal::NoBinding { hotkey } => {
                write!(formatter, "hotkeys: {hotkey:?} has no binding, and must keep one")
            }
            Refusal::NoKey { hotkey } => write!(formatter, "hotkeys: {hotkey:?} has no key, and must keep one"),
            Refusal::Twice { hotkey, binding } => {
                write!(formatter, "hotkeys: {hotkey:?} holds {} twice", binding.text())
            }
            Refusal::Shared { binding, hotkey, other } => write!(
                formatter,
                "hotkeys: {} is bound to both {hotkey:?} and {other:?}, which cannot share an input",
                binding.text()
            ),
            Refusal::GameInput { binding, hotkey, control, label } => write!(
                formatter,
                "hotkeys: {} is bound to {hotkey:?}, which acts while the game plays, and is the \
                 game's {} for {control} ({label})",
                binding.text(),
                if binding.is_key() { "key" } else { "button" }
            ),
            Refusal::Controls { message } => write!(formatter, "hotkeys: {message}"),
        }
    }
}

/// Each hotkey's bindings, in order, as the defaults for an export.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Hotkeys {
    lists: BTreeMap<Hotkey, Vec<Binding>>,
}

impl Hotkeys {
    pub fn of(&self, hotkey: Hotkey) -> &[Binding] {
        self.lists.get(&hotkey).map(Vec::as_slice).unwrap_or(&[])
    }

    /// The same hotkeys, with `hotkey` bound to nothing.
    pub fn without(&self, hotkey: Hotkey) -> Hotkeys {
        let mut lists = self.lists.clone();
        lists.insert(hotkey, Vec::new());
        Hotkeys { lists }
    }

    /// The rules we apply in the menu to every change: each hotkey keeps
    /// what it must, and an input belongs to one hotkey, or to two that may
    /// share it.
    pub fn check(&self) -> Result<(), Refusal> {
        for hotkey in Hotkey::all() {
            let list = self.of(hotkey);
            match hotkey.keeps() {
                Keeps::Binding | Keeps::Key if list.is_empty() => return Err(Refusal::NoBinding { hotkey }),
                Keeps::Key if !list.iter().any(Binding::is_key) => return Err(Refusal::NoKey { hotkey }),
                _ => {}
            }
            for (index, binding) in list.iter().enumerate() {
                if list[..index].contains(binding) {
                    return Err(Refusal::Twice { hotkey, binding: binding.clone() });
                }
                for other in Hotkey::all() {
                    if other != hotkey && !hotkey.shares_with(other) && self.of(other).contains(binding) {
                        return Err(Refusal::Shared { binding: binding.clone(), hotkey, other });
                    }
                }
            }
        }
        Ok(())
    }

    /// `check`, then the rule for the game's inputs: a hotkey used during play
    /// may have none of the keys in `game` and none of the pad positions for
    /// the game, or a press would do both. Keys are the same when they are one
    /// key in RetroArch. We check a pad binding only when it is one position,
    /// so a chord of several, such as MENU's L3+R3, and Home are allowed.
    pub fn check_with(&self, game: &[GameInput]) -> Result<(), Refusal> {
        self.check()?;
        for hotkey in Hotkey::all().filter(|hotkey| hotkey.acts().in_game()) {
            for binding in self.of(hotkey) {
                let taken = match binding {
                    Binding::Key(name) => {
                        let read = crate::controls::retroarch_key(name);
                        game.iter().find(|input| crate::controls::retroarch_key(&input.key) == read)
                    }
                    Binding::Pad(inputs) => match inputs.as_slice() {
                        [PadInput::Position(position)] => game.iter().find(|input| &input.position == position),
                        _ => None,
                    },
                };
                if let Some(taken) = taken {
                    return Err(Refusal::GameInput {
                        binding: binding.clone(),
                        hotkey,
                        control: taken.control.clone(),
                        label: taken.label.clone(),
                    });
                }
            }
        }
        Ok(())
    }

    /// `check_with` the inputs of the game for `system`, with `controls`.
    pub fn check_for(&self, system: &str, controls: &crate::controls::Controls) -> Result<(), Refusal> {
        let game = crate::controls::game_inputs(system, controls)
            .map_err(|message| Refusal::Controls { message })?;
        self.check_with(&game)
    }

    /// `hotkeys-defaults.cfg`: each hotkey's list, then the words the menu
    /// shows for every pad input.
    pub fn defaults_config(&self, game: &GameHotkeys) -> Result<String, String> {
        self.check().map_err(|refusal| refusal.to_string())?;
        let mut text = String::new();
        for hotkey in Hotkey::all() {
            let list: Vec<String> = self.of(hotkey).iter().map(Binding::text).collect();
            text.push_str(&format!("{} = \"{}\"\n", key!(HotkeyList, hotkey.id()), list.join(" ")));
        }
        if !game.absent.is_empty() {
            let ids: Vec<&str> = game.absent.iter().map(|hotkey| hotkey.id()).collect();
            text.push_str(&format!("{} = \"{}\"\n", key!(HotkeysAbsent), ids.join(" ")));
        }
        for (hotkey, mode) in &game.modes {
            text.push_str(&format!("{} = \"{mode}\"\n", key!(HotkeyMode, hotkey.id())));
        }
        for position in button_positions()? {
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

impl Serialize for Hotkeys {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let named: BTreeMap<&str, &Vec<Binding>> =
            self.lists.iter().map(|(hotkey, list)| (hotkey.id(), list)).collect();
        named.serialize(serializer)
    }
}

/// Each hotkey's list, in a map by the hotkeys' ids. We refuse an id with no
/// hotkey declared in `hotkeys.inc`.
fn read_lists<'de, M: de::MapAccess<'de>>(mut map: M) -> Result<BTreeMap<Hotkey, Vec<Binding>>, M::Error> {
    let mut lists = BTreeMap::new();
    while let Some(id) = map.next_key::<String>()? {
        let hotkey = Hotkey::named(&id).ok_or_else(|| {
            de::Error::custom(format!("'{id}' is no hotkey; the hotkeys are {}", Hotkey::ids()))
        })?;
        lists.insert(hotkey, map.next_value()?);
    }
    Ok(lists)
}

/// A request lists the hotkeys it changes by their ids. For a hotkey that
/// it leaves out, we use the builder's defaults (`desktop/defaults.json`).
impl<'de> Deserialize<'de> for Hotkeys {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Lists;
        impl<'de> de::Visitor<'de> for Lists {
            type Value = BTreeMap<Hotkey, Vec<Binding>>;
            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                write!(formatter, "each hotkey's bindings, by the hotkey's id")
            }
            fn visit_map<M: de::MapAccess<'de>>(self, map: M) -> Result<Self::Value, M::Error> {
                read_lists(map)
            }
        }
        let given = deserializer.deserialize_map(Lists)?;
        let mut lists = crate::builder::defaults().hotkeys.lists.clone();
        lists.extend(given);
        Ok(Hotkeys { lists })
    }
}

/// We read the builder's defaults with nothing below them, and they include
/// every hotkey.
pub(crate) fn read_defaults<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Hotkeys, D::Error> {
    struct Whole;
    impl<'de> de::Visitor<'de> for Whole {
        type Value = Hotkeys;
        fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
            write!(formatter, "every hotkey's bindings, by the hotkey's id")
        }
        fn visit_map<M: de::MapAccess<'de>>(self, map: M) -> Result<Self::Value, M::Error> {
            let lists = read_lists(map)?;
            if let Some(missing) = Hotkey::all().find(|hotkey| !lists.contains_key(hotkey)) {
                return Err(de::Error::custom(format!(
                    "desktop/defaults.json gives {missing:?} no bindings"
                )));
            }
            Ok(Hotkeys { lists })
        }
    }
    deserializer.deserialize_map(Whole)
}

/// The pad bindings we add to the default keys in the builder for saving,
/// loading and changing the slot, by the shoulder buttons with a meaning on
/// the pads of a console. Save is on the left and load on the right. In each
/// case we name the positions that must be in use and the ones that must be
/// free, and we take the first case that fits. When none of L1, R1, L2 and R2
/// has a meaning on the pads, we put save and load on L1 and R1, and the
/// previous and the next slot on L2 and R2. When L1 and R1 have a meaning and
/// L2 and R2 do not, we put save and load on L2 and R2. Otherwise we add no
/// pad binding. We read every pad in the console's picker, because the player
/// can switch pad in the game and a default must be free on each.
const SHOULDER_DEFAULTS: [ShoulderCase; 2] = [
    ShoulderCase {
        in_use: &[],
        free: &["l", "r", "l2", "r2"],
        placed: &[("quick-save", "l"), ("quick-load", "r"), ("previous-slot", "l2"), ("next-slot", "r2")],
    },
    ShoulderCase { in_use: &["l", "r"], free: &["l2", "r2"], placed: &[("quick-save", "l2"), ("quick-load", "r2")] },
];

/// One case of `SHOULDER_DEFAULTS`. We name positions by their ids in
/// `desktop/controls.json` and hotkeys by their ids in `hotkeys.inc`.
struct ShoulderCase {
    in_use: &'static [&'static str],
    free: &'static [&'static str],
    placed: &'static [(&'static str, &'static str)],
}

/// The hotkeys at the start of a game for `system`, which are the keys in
/// `desktop/defaults.json` and the pad bindings of `SHOULDER_DEFAULTS` for
/// the positions of the console's pads as we declare them.
pub fn defaults_for(system: &str) -> Result<Hotkeys, String> {
    let used: Vec<String> = crate::controls::game_inputs(system, &Default::default())?
        .into_iter()
        .map(|input| input.position)
        .collect();
    let in_use = |position: &&str| used.iter().any(|seen| seen == position);
    let mut hotkeys = crate::builder::defaults().hotkeys.clone();
    let case = SHOULDER_DEFAULTS
        .iter()
        .find(|case| case.in_use.iter().all(in_use) && !case.free.iter().any(in_use));
    for (id, position) in case.map(|case| case.placed).unwrap_or_default() {
        let hotkey = Hotkey::named(id).ok_or_else(|| format!("hotkeys.inc declares no hotkey {id}"))?;
        hotkeys
            .lists
            .entry(hotkey)
            .or_default()
            .push(Binding::Pad(vec![PadInput::read(position)?]));
    }
    Ok(hotkeys)
}

/// Give each hotkey missing from `request` the default for the console in
/// `request` (`defaults_for`). For a request without a console we change
/// nothing, and when we read it we fill in the default keys of the builder.
pub fn complete(request: &mut serde_json::Map<String, serde_json::Value>) -> Result<(), String> {
    let Some(system) = request.get("system").and_then(serde_json::Value::as_str) else {
        return Ok(());
    };
    let defaults = serde_json::to_value(defaults_for(system)?).map_err(|error| error.to_string())?;
    let stated = request
        .entry("hotkeys")
        .or_insert_with(|| serde_json::Value::Object(Default::default()));
    // We leave hotkeys that are no map as they are, and report them when we
    // read the request.
    if let (serde_json::Value::Object(stated), serde_json::Value::Object(defaults)) = (stated, defaults) {
        for (id, list) in defaults {
            stated.entry(id).or_insert(list);
        }
    }
    Ok(())
}

/// The one meta line we keep in a shipped controller profile. It contains the name
/// of the button that is the menu button on the pad, which is Home in the menu.
pub fn home_button_key() -> String {
    format!("input_{}_btn", PadInput::home_field(1))
}

/// The ids a HOTKEYS row is found by: `<prefix><hotkey>`, then its suffix,
/// or `-<n>` for its chips.
fn row_id(hotkey: Hotkey) -> String {
    format!("{}{}", crate::menu::contract!(HotkeyPrefix), hotkey.id())
}

/// The parts of every row that must be in the screen when we ship it: its +
/// and its first chip.
pub fn required_ids() -> Vec<String> {
    Hotkey::all()
        .flat_map(|hotkey| {
            [
                format!("{}{}", row_id(hotkey), crate::menu::contract!(AddSuffix)),
                format!("{}-1", row_id(hotkey)),
            ]
        })
        .collect()
}

/// How many bindings we can show in the row of `hotkey` in `document`, by its
/// chips numbered from 1. None when the document has no such row.
pub fn room(document: &str, hotkey: Hotkey) -> Option<usize> {
    let add = format!("id=\"{}{}\"", row_id(hotkey), crate::menu::contract!(AddSuffix));
    if !document.contains(&add) {
        return None;
    }
    Some(
        (1..)
            .take_while(|chip| document.contains(&format!("id=\"{}-{chip}\"", row_id(hotkey))))
            .count(),
    )
}

/// Refuse defaults that the HOTKEYS screen of a design cannot show in full.
/// A row has room for as many bindings as it has chips.
pub fn fit(hotkeys: &Hotkeys, document: &str, design: &str) -> Result<(), String> {
    for hotkey in Hotkey::all() {
        if let Some(room) = room(document, hotkey) {
            let wanted = hotkeys.of(hotkey).len();
            if wanted > room {
                return Err(format!(
                    "design '{design}' shows {room} bindings for {hotkey:?} on HOTKEYS, and the \
                     game's defaults give it {wanted}"
                ));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests;
