//! `design.json`, typed, read once and merged with Native's.
//!
//! A design is a package beside Native. We merge its screens with Native's
//! by id, and take each other field from the design when present, else from
//! Native. An unknown key is an error, so we refuse a misspelt field instead
//! of ignoring it.

use serde::{Deserialize, Deserializer};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};

/// A field that may be absent (keep Native's) or explicitly `null` (none).
fn present<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    T::deserialize(deserializer).map(Some)
}

#[derive(Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct File {
    // For people and the registry. We do not need them to compose the menu.
    #[allow(dead_code)]
    schema_version: Option<u32>,
    id: Option<String>,
    #[allow(dead_code)]
    name: Option<String>,
    #[allow(dead_code)]
    description: Option<String>,
    documents: Option<DocumentsFile>,
    fonts: Option<Vec<Font>>,
    metrics: Option<MetricsFile>,
    list: Option<ListFile>,
    binds: Option<BindsFile>,
    overlays: Option<Vec<OverlayFile>>,
    #[serde(default)]
    screens: Vec<ScreenFile>,
    #[serde(default)]
    screen_order: Vec<String>,
    #[serde(default)]
    tokens: BTreeMap<String, String>,
    /// The design's wording for the words we write in the player, by id.
    #[serde(default)]
    words: BTreeMap<String, String>,
}

#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct DocumentsFile {
    menu: Option<String>,
    splash: Option<String>,
    style: Option<String>,
}

/// A font in the design, and its licence, which we ship with it.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Font {
    pub file: String,
    pub license: String,
    pub family: String,
}

#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct MetricsFile {
    scene: Option<SizeFile>,
    marker: Option<MarkerFile>,
    callout: Option<CalloutFile>,
    group: Option<GroupFile>,
}

#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct SizeFile {
    width: Option<i32>,
    height: Option<i32>,
}

#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct MarkerFile {
    diameter: Option<i32>,
}

#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct CalloutFile {
    width: Option<i32>,
    height: Option<i32>,
    border: Option<i32>,
}

#[derive(Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct GroupFile {
    width: Option<i32>,
    height: Option<i32>,
    border: Option<i32>,
    gap: Option<i32>,
    bottom_margin: Option<i32>,
}

#[derive(Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ListFile {
    page_size: Option<usize>,
    row_height: Option<usize>,
    row_gap: Option<usize>,
}

#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct BindsFile {
    after_ms: u32,
    hover_after_ms: u32,
    width: u32,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct OverlayFile {
    id: String,
    #[serde(default)]
    follows: String,
    after_ms: u32,
    hold_ms: u32,
    leave_ms: u32,
    #[serde(default)]
    needs: String,
}

#[derive(Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ScreenFile {
    id: Option<String>,
    role: Option<ScreenRole>,
    panel: Option<String>,
    heading: Option<String>,
    footer: Option<String>,
    button: Option<String>,
    label: Option<String>,
    back: Option<String>,
    page_size: Option<usize>,
    place: Option<PlaceFile>,
    #[serde(default, deserialize_with = "present")]
    option: Option<Option<OptionFile>>,
    images: Option<String>,
    dialogs: Option<Vec<String>>,
    from: Option<String>,
}

#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "lowercase")]
enum PlaceFile {
    Options,
}

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct OptionFile {
    label: String,
    #[serde(default)]
    default: bool,
}

impl ScreenFile {
    /// This entry with every field declared in `over` laid over it.
    fn merged(mut self, over: &ScreenFile) -> ScreenFile {
        macro_rules! take {
            ($($field:ident),*) => {$(
                if over.$field.is_some() {
                    self.$field = over.$field.clone();
                }
            )*};
        }
        take!(
            panel, heading, footer, button, label, back, page_size, place, option, images,
            dialogs, from
        );
        self
    }
}

/// The role of a screen. Only the screens that we handle specially in the
/// player have one, and only the screens in Native have roles. A design
/// replaces a screen by its id and keeps the role. The player's contract
/// contains the roles and their words (`RIB_ROLE` in `document_contract.inc`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum ScreenRole {
    Pause,
    Options,
    Controls,
    Shaders,
    Achievements,
    Discs,
    Accounts,
}

impl ScreenRole {
    pub const ALL: [ScreenRole; 7] = [
        ScreenRole::Pause,
        ScreenRole::Options,
        ScreenRole::Controls,
        ScreenRole::Shaders,
        ScreenRole::Achievements,
        ScreenRole::Discs,
        ScreenRole::Accounts,
    ];

    /// The word in `design.json` and `design.cfg`, as in the contract.
    pub fn name(self) -> &'static str {
        super::contract::role_word(self)
    }
}

impl<'de> Deserialize<'de> for ScreenRole {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let word = String::deserialize(deserializer)?;
        ScreenRole::ALL
            .into_iter()
            .find(|role| role.name() == word)
            .ok_or_else(|| {
                let known: Vec<&str> = ScreenRole::ALL.iter().map(|role| role.name()).collect();
                serde::de::Error::custom(format!(
                    "unknown role '{word}'; the roles are {}",
                    known.join(", ")
                ))
            })
    }
}

/// A screen in the in-game menu, as the design declares it.
///
/// The heading and the footer hint are the design's words. A design may word
/// them differently, and a design in another language must be able to.
#[derive(Clone, Debug)]
pub struct Screen {
    pub id: String,
    /// The role of this screen, when we handle it specially in the player.
    pub role: Option<ScreenRole>,
    pub panel: String,
    pub heading: String,
    pub footer: String,
    /// The button that opens this screen. A back button is the button that
    /// opens the screen behind, so we need no separate kind for it.
    pub button: String,
    /// The pause-row label, when this screen is opened from another screen.
    pub label: Option<String>,
    /// The words on this screen's own back button.
    pub back_label: Option<String>,
    /// Optional capacity for screens with content above their list.
    pub list_page_size: Option<usize>,
    pub place: ScreenPlace,
    /// Set when this screen is an entry inside Options. The words are the
    /// design's, on the button that opens it.
    pub option_label: Option<String>,
    /// We ship the entry in a game that does not name a set.
    pub option_default: bool,
    /// The screen to open instead of this one when the game has more than one
    /// disc, as the disc list opens instead of the disc column.
    pub images: Option<String>,
    /// Dialogs that open over the menu from this screen, each from
    /// `dialog-<name>.rml` in the design or Native. We compose them beside the
    /// screen, so a design can replace either without copying the other.
    pub dialogs: Vec<String>,
    /// The screen whose button opens this one, when that is not Options or
    /// Pause. BACK on this screen leads there.
    pub opener: Option<String>,
}

impl Screen {
    /// The list we fill with the discs from the core once the game has loaded.
    pub fn is_disc_list(&self) -> bool {
        self.role == Some(ScreenRole::Discs)
    }
}

/// Where a declared screen is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScreenPlace {
    Plain,
    Options,
}

/// Something we draw over the running game for a moment, from the design.
/// It is not a screen, because nobody opens it by name and it takes no input.
#[derive(Clone, Debug)]
pub struct Overlay {
    /// The element in the design's markup, which is also the overlay's name.
    pub id: String,
    /// An overlay declared before this one that has to finish first. Empty
    /// means that we wait for the start of the game instead.
    pub follows: String,
    pub after_ms: u32,
    pub hold_ms: u32,
    /// How long the overlay takes to leave. The animation in the stylesheet
    /// lasts exactly this long, from the same declaration.
    pub leave_ms: u32,
    /// A staged file that we need to draw the overlay. Empty means none.
    pub needs: String,
}

/// The frame of the controller scene in a design.
#[derive(Clone, Copy, Debug)]
pub struct SceneMetrics {
    pub scene_width: i32,
    pub scene_height: i32,
    pub callout_width: i32,
    pub callout_height: i32,
    /// How far a callout is drawn beyond its declared size. The leader meets
    /// the drawn edge, not the content edge.
    pub callout_border: i32,
    pub marker: i32,
    pub group_width: i32,
    pub group_height: i32,
    /// How far a stick's box is drawn beyond its declared size.
    pub group_border: i32,
    pub group_gap: i32,
    pub group_bottom_margin: i32,
}

/// When the bind list appears, and how wide it is.
#[derive(Clone, Copy, Debug)]
pub struct Binds {
    /// After a keyboard or pad selection rests on a control.
    pub after_ms: u32,
    /// After the pointer rests on a control.
    pub hover_after_ms: u32,
    pub width: u32,
}

/// A design's documents by role.
#[derive(Clone, Debug)]
pub struct Documents {
    /// Native's page skeleton. A design cannot replace it.
    pub menu: String,
    pub splash: String,
    pub style: String,
}

/// A design, resolved against Native.
#[derive(Clone, Debug)]
pub struct Manifest {
    /// The design's package.
    pub design: PathBuf,
    /// Native's package, beside it.
    pub base: PathBuf,
    /// The design's id, for messages.
    pub id: String,
    pub documents: Documents,
    pub fonts: Vec<Font>,
    pub scene: SceneMetrics,
    /// Rows on one page of a list.
    pub list_page_size: usize,
    /// The height of one row, so we can move up the actions of a short list.
    pub list_row_step: usize,
    pub binds: Binds,
    pub overlays: Vec<Overlay>,
    pub screens: Vec<Screen>,
    /// The design's own named values, which a palette may override.
    pub tokens: BTreeMap<String, String>,
    /// The words we write in the player, in this design's wording, by id:
    /// Native's, then the design's over them, or else the English word.
    pub words: BTreeMap<String, String>,
}

/// The base must come from the same source tree or frozen kit as the design.
/// With a fallback to a path in the repository, an export could mix
/// different versions of the menu without anyone noticing.
pub fn base_design(design: &Path) -> Result<PathBuf, String> {
    let base = if design.file_name().is_some_and(|name| name == "native") {
        design.to_path_buf()
    } else {
        design
            .parent()
            .ok_or_else(|| format!("Design has no package parent: {}", design.display()))?
            .join("native")
    };
    if !base.is_dir() {
        return Err(format!(
            "Native base design is missing beside {}: {}",
            design.display(),
            base.display()
        ));
    }
    Ok(base)
}

fn read(package: &Path) -> Result<File, String> {
    let path = package.join("design.json");
    let text =
        fs::read_to_string(&path).map_err(|e| format!("Could not read {}: {e}", path.display()))?;
    serde_json::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))
}

fn missing(base: &Path, what: &str) -> String {
    format!(
        "{} declares no {what}, and a design without it has nothing to inherit",
        base.join("design.json").display()
    )
}

impl Manifest {
    pub fn load(design: &Path) -> Result<Manifest, String> {
        let base = base_design(design)?;
        let native = read(&base)?;
        let own = if design == base {
            File::default()
        } else {
            read(design)?
        };
        let id = own
            .id
            .clone()
            .or_else(|| {
                design
                    .file_name()
                    .map(|name| name.to_string_lossy().into_owned())
            })
            .unwrap_or_default();

        let documents = {
            let native_documents = native.documents.as_ref();
            let own_documents = own.documents.as_ref();
            let pick = |key: fn(&DocumentsFile) -> &Option<String>, what: &str| {
                own_documents
                    .and_then(|documents| key(documents).clone())
                    .or_else(|| native_documents.and_then(|documents| key(documents).clone()))
                    .ok_or_else(|| missing(&base, what))
            };
            Documents {
                menu: native_documents
                    .and_then(|documents| documents.menu.clone())
                    .ok_or_else(|| missing(&base, "documents.menu"))?,
                splash: pick(|documents| &documents.splash, "documents.splash")?,
                style: pick(|documents| &documents.style, "documents.style")?,
            }
        };
        let fonts = own
            .fonts
            .clone()
            .or_else(|| native.fonts.clone())
            .ok_or_else(|| missing(&base, "fonts"))?;
        // We draw every word with these in the player, and start no menu
        // without one.
        if fonts.is_empty() {
            return Err(format!(
                "design '{id}' lists no fonts. The menu writes every word in the fonts a design \
                 lists, and a game whose design lists none would have no menu; list at least \
                 one in design.json \"fonts\", or leave \"fonts\" out to use Native's"
            ));
        }

        let metrics = |pick: &dyn Fn(&MetricsFile) -> Option<i32>, what: &str| {
            own.metrics
                .as_ref()
                .and_then(pick)
                .or_else(|| native.metrics.as_ref().and_then(pick))
                .ok_or_else(|| missing(&base, what))
        };
        let scene = SceneMetrics {
            scene_width: metrics(&|m| m.scene.as_ref()?.width, "metrics.scene.width")?,
            scene_height: metrics(&|m| m.scene.as_ref()?.height, "metrics.scene.height")?,
            callout_width: metrics(&|m| m.callout.as_ref()?.width, "metrics.callout.width")?,
            callout_height: metrics(&|m| m.callout.as_ref()?.height, "metrics.callout.height")?,
            callout_border: metrics(&|m| m.callout.as_ref()?.border, "metrics.callout.border")?,
            marker: metrics(&|m| m.marker.as_ref()?.diameter, "metrics.marker.diameter")?,
            group_width: metrics(&|m| m.group.as_ref()?.width, "metrics.group.width")?,
            group_height: metrics(&|m| m.group.as_ref()?.height, "metrics.group.height")?,
            group_border: metrics(&|m| m.group.as_ref()?.border, "metrics.group.border")?,
            group_gap: metrics(&|m| m.group.as_ref()?.gap, "metrics.group.gap")?,
            group_bottom_margin: metrics(
                &|m| m.group.as_ref()?.bottom_margin,
                "metrics.group.bottomMargin",
            )?,
        };

        let list = |pick: &dyn Fn(&ListFile) -> Option<usize>| {
            own.list
                .as_ref()
                .and_then(pick)
                .or_else(|| native.list.as_ref().and_then(pick))
        };
        let list_page_size =
            list(&|l| l.page_size).ok_or_else(|| missing(&base, "list.pageSize"))?;
        if list_page_size == 0 {
            return Err("list.pageSize must be at least 1".into());
        }
        // Both or neither, because with a height and no gap we would move the
        // actions up slightly too little, which looks like a mistake.
        let list_row_step = match list(&|l| l.row_height) {
            None | Some(0) => 0,
            Some(height) => height + list(&|l| l.row_gap).unwrap_or(0),
        };

        let binds = own
            .binds
            .or(native.binds)
            .ok_or_else(|| missing(&base, "binds"))?;
        let binds = Binds {
            after_ms: binds.after_ms,
            hover_after_ms: binds.hover_after_ms,
            width: binds.width,
        };
        if binds.width == 0 {
            return Err("binds.width must be a width in dp".into());
        }

        let overlays = overlays(
            own.overlays.as_ref().map(|_| design).unwrap_or(&base),
            own.overlays.or(native.overlays).unwrap_or_default(),
        )?;

        let mut tokens = native.tokens.clone();
        tokens.extend(own.tokens.clone());

        let mut words = native.words.clone();
        words.extend(own.words.clone());
        super::words::check(&id, &words)?;

        let screens = screens(design, &base, native.screens, own.screens, own.screen_order)?;
        Ok(Manifest {
            design: design.to_path_buf(),
            base,
            id,
            documents,
            fonts,
            scene,
            list_page_size,
            list_row_step,
            binds,
            overlays,
            screens,
            tokens,
            words,
        })
    }

    pub fn screen(&self, role: ScreenRole) -> Option<&Screen> {
        self.screens.iter().find(|screen| screen.role == Some(role))
    }

    /// A fragment from the design, or Native's when the design has none.
    pub fn fragment(&self, name: &str) -> Result<String, String> {
        let source = self.fragment_path(name);
        fs::read_to_string(&source).map_err(|e| format!("Could not read {}: {e}", source.display()))
    }

    /// Whether `name` is in the design or in Native.
    pub fn has_fragment(&self, name: &str) -> bool {
        self.fragment_path(name).is_file()
    }

    /// Where `name` comes from: the design's own file when it has one.
    pub fn fragment_path(&self, name: &str) -> PathBuf {
        let selected = self.design.join(name);
        if selected.is_file() {
            selected
        } else {
            self.base.join(name)
        }
    }
}

fn overlays(declaration: &Path, listed: Vec<OverlayFile>) -> Result<Vec<Overlay>, String> {
    let mut overlays: Vec<Overlay> = Vec::new();
    for entry in listed {
        // Only an overlay declared earlier, so that no design can make two
        // overlays wait for each other forever.
        if !entry.follows.is_empty() && !overlays.iter().any(|before| before.id == entry.follows) {
            return Err(format!(
                "overlay '{}' follows '{}', which {} does not declare before it",
                entry.id,
                entry.follows,
                declaration.join("design.json").display()
            ));
        }
        overlays.push(Overlay {
            id: entry.id,
            follows: entry.follows,
            after_ms: entry.after_ms,
            hold_ms: entry.hold_ms,
            leave_ms: entry.leave_ms,
            needs: entry.needs,
        });
    }
    Ok(overlays)
}

/// Every base screen comes from Native. A design may replace the fields of a
/// screen by id or add a screen, and for any other id we keep Native's.
fn screens(
    design: &Path,
    base: &Path,
    native: Vec<ScreenFile>,
    own: Vec<ScreenFile>,
    order: Vec<String>,
) -> Result<Vec<Screen>, String> {
    if native.is_empty() {
        return Err(format!("Native declares no screens: {}", base.display()));
    }
    let mut overrides: Vec<(String, ScreenFile)> = Vec::new();
    for entry in own {
        let id = entry
            .id
            .clone()
            .ok_or_else(|| format!("Screen override in {} declares no id", design.display()))?;
        if overrides.iter().any(|(seen, _)| *seen == id) {
            return Err(format!(
                "Duplicate screen override '{id}' in {}",
                design.display()
            ));
        }
        if entry.role.is_some() {
            return Err(format!(
                "screen '{id}' in {} declares a role; only Native assigns roles, and a design \
                 takes one by replacing that screen's id",
                design.join("design.json").display()
            ));
        }
        overrides.push((id, entry));
    }
    let mut listed: Vec<ScreenFile> = Vec::new();
    for entry in native {
        let id = entry
            .id
            .clone()
            .ok_or_else(|| format!("Native screen in {} declares no id", base.display()))?;
        let merged = match overrides.iter().find(|(selected, _)| *selected == id) {
            Some((_, over)) => entry.merged(over),
            None => entry,
        };
        listed.push(merged);
    }
    for (id, entry) in &overrides {
        if !listed.iter().any(|listed| listed.id.as_deref() == Some(id)) {
            listed.push(entry.clone());
        }
    }
    // For a different declaration order, a design must name it explicitly.
    if !order.is_empty() {
        let mut ordered = Vec::new();
        let mut seen = BTreeSet::new();
        for id in order {
            if !seen.insert(id.clone()) {
                return Err(format!(
                    "Duplicate screen '{id}' in screenOrder for {}",
                    design.display()
                ));
            }
            let Some(index) = listed
                .iter()
                .position(|entry| entry.id.as_deref() == Some(&id))
            else {
                return Err(format!(
                    "Unknown screen '{id}' in screenOrder for {}",
                    design.display()
                ));
            };
            ordered.push(listed.remove(index));
        }
        ordered.extend(listed);
        listed = ordered;
    }
    let declaration = design.join("design.json");
    let mut screens: Vec<Screen> = Vec::new();
    for (index, entry) in listed.into_iter().enumerate() {
        let at = |value: Option<String>, key: &str| -> Result<String, String> {
            value.ok_or_else(|| {
                format!(
                    "screen {index} in {} declares no {key}",
                    declaration.display()
                )
            })
        };
        let option = entry.option.flatten();
        if let Some(option) = &option {
            if option.label.is_empty() {
                return Err(format!(
                    "screen {index} in {} has an empty option label",
                    declaration.display()
                ));
            }
        }
        if entry.page_size == Some(0) {
            return Err(format!(
                "screen {index} pageSize must be a positive integer"
            ));
        }
        let screen = Screen {
            id: at(entry.id, "id")?,
            role: entry.role,
            panel: at(entry.panel, "panel")?,
            heading: at(entry.heading, "heading")?,
            footer: at(entry.footer, "footer")?,
            // Optional, because a screen we open only in code has no button.
            button: entry.button.unwrap_or_default(),
            label: entry.label,
            back_label: entry.back,
            list_page_size: entry.page_size,
            place: match entry.place {
                None => ScreenPlace::Plain,
                Some(PlaceFile::Options) => ScreenPlace::Options,
            },
            option_label: option.as_ref().map(|option| option.label.clone()),
            option_default: option.is_some_and(|option| option.default),
            images: entry.images,
            dialogs: entry.dialogs.unwrap_or_default(),
            opener: entry.from,
        };
        if let Some(role) = screen.role {
            if let Some(other) = screens.iter().find(|other| other.role == Some(role)) {
                return Err(format!(
                    "screens '{}' and '{}' in {} both take the {} role",
                    other.id,
                    screen.id,
                    base.join("design.json").display(),
                    role.name()
                ));
            }
        }
        screens.push(screen);
    }
    Ok(screens)
}

/// The screens a design declares, merged with Native's.
pub fn declared_screens(design: &Path) -> Result<Vec<Screen>, String> {
    Ok(Manifest::load(design)?.screens)
}

pub fn declared_overlays(design: &Path) -> Result<Vec<Overlay>, String> {
    Ok(Manifest::load(design)?.overlays)
}

pub fn scene_metrics(design: &Path) -> Result<SceneMetrics, String> {
    Ok(Manifest::load(design)?.scene)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn package(root: &Path, name: &str, json: &str) -> PathBuf {
        let design = root.join(name);
        fs::create_dir_all(&design).unwrap();
        fs::write(design.join("design.json"), json).unwrap();
        design
    }

    fn with_native(root: &Path) {
        let native = root.join("native");
        fs::create_dir_all(&native).unwrap();
        fs::copy(
            crate::repo::at("integrations/designs/native/design.json"),
            native.join("design.json"),
        )
        .unwrap();
    }

    #[test]
    fn an_unknown_key_is_refused_with_the_file_that_holds_it() {
        let root = rominabox_scratch::Scratch::dir("rominabox-manifest-unknown");
        with_native(&root);
        let design = package(&root, "typo", r#"{"screnes": []}"#);
        let error = Manifest::load(&design).unwrap_err();
        assert!(error.contains("screnes"), "{error}");
        assert!(error.contains("typo"), "{error}");
    }

    #[test]
    fn a_design_inherits_every_field_it_does_not_declare() {
        let root = rominabox_scratch::Scratch::dir("rominabox-manifest-inherit");
        with_native(&root);
        let design = package(
            &root,
            "bare",
            r#"{"metrics": {"marker": {"diameter": 30}}}"#,
        );
        let bare = Manifest::load(&design).unwrap();
        let native = Manifest::load(&root.join("native")).unwrap();
        assert_eq!(bare.scene.marker, 30);
        assert_eq!(bare.scene.scene_width, native.scene.scene_width);
        assert_eq!(bare.binds.hover_after_ms, native.binds.hover_after_ms);
        assert_eq!(bare.screens.len(), native.screens.len());
        assert_eq!(bare.fonts.len(), native.fonts.len());
        assert_eq!(bare.documents.style, "menu.rcss");
    }

    #[test]
    fn a_null_option_removes_the_inherited_entry() {
        let root = rominabox_scratch::Scratch::dir("rominabox-manifest-null");
        with_native(&root);
        let design = package(
            &root,
            "nulled",
            r#"{"screens": [{"id": "discs", "option": null}]}"#,
        );
        let discs = declared_screens(&design)
            .unwrap()
            .into_iter()
            .find(|screen| screen.id == "discs")
            .unwrap();
        assert_eq!(discs.option_label, None);
        assert_eq!(discs.role, Some(ScreenRole::Discs), "the role is inherited");
    }

    #[test]
    fn a_design_words_only_what_the_player_writes() {
        let root = rominabox_scratch::Scratch::dir("rominabox-manifest-words");
        with_native(&root);
        let worded = package(&root, "worded", r#"{"words": {"slot": "BLOCK {slot}"}}"#);
        assert_eq!(Manifest::load(&worded).unwrap().words["slot"], "BLOCK {slot}");
        let misworded = package(&root, "misworded", r#"{"words": {"slots": "BLOCK"}}"#);
        let error = Manifest::load(&misworded).unwrap_err();
        assert!(error.contains("'slots'") && error.contains("misworded"), "{error}");
    }

    /// We draw every word in the player with the fonts in a design, and do not
    /// start a menu without fonts. An export of such a design would contain a
    /// game with no menu at all.
    #[test]
    fn a_design_that_lists_no_font_is_refused() {
        let root = rominabox_scratch::Scratch::dir("rominabox-manifest-fontless");
        with_native(&root);
        let design = package(&root, "fontless", r#"{"fonts": []}"#);
        let error = Manifest::load(&design).expect_err("a design with no font was accepted");
        assert!(error.contains("fontless") && error.contains("font"), "{error}");
    }

    #[test]
    fn only_native_assigns_roles() {
        let root = rominabox_scratch::Scratch::dir("rominabox-manifest-role");
        with_native(&root);
        let design = package(
            &root,
            "claims",
            r#"{"screens": [{"id": "mine", "role": "pause", "panel": "mine-panel", "heading": "M", "footer": "F"}]}"#,
        );
        let error = Manifest::load(&design).unwrap_err();
        assert!(error.contains("only Native assigns roles"), "{error}");
    }
}
