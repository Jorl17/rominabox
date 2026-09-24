//! Composing a whole menu and writing it.

use super::{declarations, document, manifest::Manifest, scene, tokens, Content, ScreenPlace};
use crate::controls::Controls;
use crate::shaders::ShaderSelection;
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
};

/// The settings for the in-game menu of a game.
#[derive(Clone, Debug)]
pub struct MenuRequest {
    /// The design's package, with Native beside it.
    pub design: PathBuf,
    /// The controller artwork every design shares.
    pub artwork: PathBuf,
    pub palette: String,
    pub background: Option<PathBuf>,
    pub system: String,
    pub controls: Controls,
    /// The full menu. Without it the game has only the splash, if any.
    pub show_menu: bool,
    pub splash: bool,
    pub include_achievements: bool,
    /// The Options entries that the author chose, or the design's defaults
    /// when absent.
    pub menu_entries: Option<Vec<String>>,
    pub shaders: ShaderSelection,
    /// How many discs the game has. The disc list, and its Options entry,
    /// exist only for more than one.
    pub discs: usize,
}

impl MenuRequest {
    /// The design's default menu for a one-disc Mega Drive game in Blue.
    pub fn new(design: impl Into<PathBuf>, artwork: impl Into<PathBuf>) -> MenuRequest {
        MenuRequest {
            design: design.into(),
            artwork: artwork.into(),
            palette: "blue".into(),
            background: None,
            system: "megadrive".into(),
            controls: Controls::default(),
            show_menu: true,
            splash: false,
            include_achievements: false,
            menu_entries: None,
            shaders: ShaderSelection::default(),
            discs: 1,
        }
    }
}

/// Every file of a composed menu, by its path under the menu's directory.
#[derive(Clone, Debug, Default)]
pub struct Composition {
    files: Vec<(PathBuf, Content)>,
}

impl Composition {
    fn put(&mut self, name: impl Into<PathBuf>, content: Content) {
        let name = name.into();
        self.files.retain(|(existing, _)| *existing != name);
        self.files.push((name, content));
    }

    /// A composed text file, such as `menu.rml` or `design.cfg`.
    pub fn text(&self, name: &str) -> Option<&str> {
        self.files.iter().find_map(|(path, content)| match content {
            Content::Text(text) if path == Path::new(name) => Some(text.as_str()),
            _ => None,
        })
    }

    /// Every path we write in the composition.
    pub fn names(&self) -> Vec<&Path> {
        self.files.iter().map(|(name, _)| name.as_path()).collect()
    }

    /// Write every file under `destination`.
    pub fn write(&self, destination: &Path) -> Result<(), String> {
        for (name, content) in &self.files {
            let to = destination.join(name);
            if let Some(parent) = to.parent() {
                fs::create_dir_all(parent)
                    .map_err(|e| format!("Could not create {}: {e}", parent.display()))?;
            }
            match content {
                Content::Text(text) => fs::write(&to, text),
                Content::Bytes(bytes) => fs::write(&to, bytes),
                Content::Copy(from) => {
                    // Copying a file onto itself truncates it to nothing, so
                    // staging into the directory we read from would destroy
                    // the artwork we are about to use.
                    let same = matches!(
                        (from.canonicalize(), to.canonicalize()),
                        (Ok(a), Ok(b)) if a == b
                    );
                    if same {
                        return Err(format!(
                            "refusing to stage {} onto itself: source and destination are the \
                             same file, which would truncate it",
                            name.display()
                        ));
                    }
                    fs::copy(from, &to).map(|_| ()).map_err(|e| {
                        std::io::Error::new(e.kind(), format!("from {}: {e}", from.display()))
                    })
                }
            }
            .map_err(|e| format!("Could not stage {}: {e}", name.display()))?;
        }
        Ok(())
    }
}

fn palette(id: &str) -> Result<crate::themes::Palette, String> {
    crate::themes::registry()?
        .palettes
        .into_iter()
        .find(|palette| palette.id == id)
        .ok_or_else(|| "Choose an available colour palette.".to_string())
}

/// The fonts declared in the design and their licences, which we ship along.
fn fonts(composition: &mut Composition, manifest: &Manifest) {
    for font in &manifest.fonts {
        for name in [&font.file, &font.license] {
            composition.put(name.as_str(), Content::Copy(manifest.fragment_path(name)));
        }
    }
}

/// The shared part stylesheets, filled in and linked before the design's own.
fn parts(
    composition: &mut Composition,
    manifest: &Manifest,
    values: &tokens::Tokens,
    document: &str,
) -> Result<String, String> {
    let sheets = document::part_sheets(&manifest.design)?;
    let mut names = Vec::new();
    for (name, text) in sheets {
        composition.put(
            Path::new("parts").join(&name),
            Content::Text(tokens::substitute(&text, values)?),
        );
        names.push(name);
    }
    document::link_parts(document, &names)
}

/// The Options entries this game has: the author's (or the design's
/// defaults), achievements when the game has them, and the disc list exactly
/// when the game has more than one disc.
fn entries(
    manifest: &Manifest,
    request: &MenuRequest,
    lists: &[crate::lists::List],
) -> Result<Vec<String>, String> {
    let mut entries = crate::achievements::entries_in(
        &manifest.screens,
        request.include_achievements,
        request.show_menu,
        request.menu_entries.as_deref(),
    )?;
    if request.discs <= 1 {
        entries.retain(|entry| {
            !manifest
                .screens
                .iter()
                .any(|screen| screen.id == *entry && screen.is_disc_list())
        });
    }
    for list in lists {
        if list.screen.option_label.is_some() && !entries.contains(&list.screen.id) {
            entries.push(list.screen.id.clone());
        }
    }
    Ok(entries)
}

/// Compose the menu of a game: the full menu when it has one, the splash
/// alone when it has only that, nothing otherwise.
pub fn compose_menu(request: &MenuRequest) -> Result<Composition, String> {
    let mut composition = Composition::default();
    if !request.show_menu && !request.splash {
        return Ok(composition);
    }
    let manifest = Manifest::load(&request.design)?;
    let palette = palette(&request.palette)?;
    let values = tokens::design(&manifest, &palette);
    fonts(&mut composition, &manifest);
    let stylesheet = tokens::substitute(&manifest.fragment(&manifest.documents.style)?, &values)?;

    if !request.show_menu {
        let splash = manifest.fragment(&manifest.documents.splash)?;
        let staged = document::staged_screens(&manifest.screens, None)?;
        let cfg = declarations::write(&manifest, &staged, &[], &splash)?;
        let splash = parts(&mut composition, &manifest, &values, &splash)?;
        composition.put("menu.rml", Content::Text(splash));
        composition.put("menu.rcss", Content::Text(stylesheet));
        composition.put("design.cfg", Content::Text(cfg));
        return Ok(composition);
    }

    // We compose data lists and the live account screen with the same code.
    let mut lists: Vec<crate::lists::List> = Vec::new();
    if request.discs > 1 {
        lists.extend(crate::disc_menu::list(&manifest));
    }
    let shaders = crate::shaders::stage(&manifest, &request.shaders)?;
    for (name, content) in shaders.files {
        composition.put(name, content);
    }
    if !shaders.config.is_empty() {
        composition.put("shaders.cfg", Content::Text(shaders.config));
    }
    lists.extend(shaders.list);
    let achievements = crate::achievements::included(request.include_achievements, true);
    if achievements {
        lists.push(crate::achievements::screen(&manifest)?);
    }
    let entries = entries(&manifest, request, &lists)?;
    let staged = document::staged_screens(&manifest.screens, Some(&entries))?;

    let scene = scene::compose(
        &request.artwork,
        &manifest,
        &request.system,
        &request.controls,
    )?;
    for (name, content) in scene.files {
        composition.put(name, content);
    }
    let menu = document::skeleton(&manifest, &staged)?;
    // The picker and the bind list are siblings of the scene, not children,
    // because we replace the scene when someone swaps pads and they must stay.
    // We refuse a design with no place for them, instead of exporting it
    // without a way to change controller.
    if !scene.picker.is_empty() && !menu.contains(scene::PICKER_SLOT) {
        return Err(format!(
            "this design has no {} for the controller picker, and {} offers more than one \
             controller. Add the slot to screen-controls.rml, outside #controller-scene.",
            scene::PICKER_SLOT,
            request.system
        ));
    }
    if !menu.contains(scene::CONTROLS_SLOT) {
        return Err(format!(
            "this design has no {} in #controller-scene, so no controller would be drawn",
            scene::CONTROLS_SLOT
        ));
    }
    if !menu.contains(scene::BINDS_SLOT) {
        return Err(format!(
            "this design has no {} for the binds on a control. Add the slot to \
             screen-controls.rml, outside #controller-scene.",
            scene::BINDS_SLOT
        ));
    }
    let menu = menu
        .replace(scene::CONTROLS_SLOT, &scene.markup)
        .replace(scene::PICKER_SLOT, &scene.picker)
        .replace(scene::BINDS_SLOT, &scene.binds);
    let menu = document::apply_options(&manifest, &menu, &staged)?;
    // Volume is part of the Options screen that we just built.
    let menu = document::install_volume_control(&menu, &manifest.design)?;
    let mut menu = tokens::substitute(&menu, &tokens::product())?;
    if !staged
        .iter()
        .any(|screen| screen.place == ScreenPlace::Options)
    {
        menu = document::add_class(&menu, "actions", "no-options");
    }
    let (menu, installed) = crate::lists::install(&manifest, &menu, &staged, &lists)?;
    let cfg = declarations::write(&manifest, &staged, &installed, &menu)?;
    let menu = parts(&mut composition, &manifest, &values, &menu)?;

    let mut stylesheet = stylesheet;
    if let Some(image_path) = &request.background {
        let image = crate::icons::read_image(image_path).map_err(|e| e.to_string())?;
        let mut png = Vec::new();
        image
            .resize(1920, 1200, image::imageops::FilterType::Lanczos3)
            .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
            .map_err(|e| e.to_string())?;
        composition.put("background.png", Content::Bytes(png));
        stylesheet.push_str("\n#screen { decorator: image(\"background.png\" cover); }\n");
    }
    if achievements {
        // The account screen's rules: Native's, then the design's own.
        let mut sheets = vec![manifest.base.join("achievements.rcss")];
        let own = manifest.design.join("achievements.rcss");
        if manifest.design != manifest.base && own.is_file() {
            sheets.push(own);
        }
        for path in sheets {
            let rules = fs::read_to_string(&path)
                .map_err(|e| format!("Could not read {}: {e}", path.display()))?;
            stylesheet.push('\n');
            stylesheet.push_str(&tokens::substitute(&rules, &values)?);
        }
    }

    composition.put("menu.rml", Content::Text(menu));
    composition.put("menu.rcss", Content::Text(stylesheet));
    composition.put("design.cfg", Content::Text(cfg));
    Ok(composition)
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewRequest {
    /// The design to draw, as a separate package, in the same directory that
    /// we stage an export from.
    #[serde(default)]
    pub design: PathBuf,
    /// Where the controller artwork is. It is not part of any design.
    pub assets: PathBuf,
    pub renderer: PathBuf,
    pub output_dir: PathBuf,
    pub palette: String,
    pub background: Option<PathBuf>,
    pub width: u32,
    pub height: u32,
}

/// Render the menu for an export of this design with the same windowless
/// RmlUi helper as in an export. We keep the intermediate files in the given
/// output directory for inspection.
pub fn render_preview(request: &PreviewRequest) -> Result<PathBuf, String> {
    if !(320..=3840).contains(&request.width) || !(200..=2400).contains(&request.height) {
        return Err("Preview dimensions are outside the supported range.".into());
    }
    // When there is no design in the call, we use the artwork directory for
    // both. The call then fails by name, as an export would, instead of
    // drawing something that no game contains.
    let design = if request.design.as_os_str().is_empty() {
        request.assets.clone()
    } else {
        request.design.clone()
    };
    compose_menu(&MenuRequest {
        palette: request.palette.clone(),
        background: request.background.clone(),
        ..MenuRequest::new(design, &request.assets)
    })?
    .write(&request.output_dir)?;
    let output = request.output_dir.join("preview.png");
    let run = std::process::Command::new(&request.renderer)
        .arg(request.output_dir.join("menu.rml"))
        .arg(&output)
        .arg(request.width.to_string())
        .arg(request.height.to_string())
        .output()
        .map_err(|e| e.to_string())?;
    if !run.status.success() {
        return Err(format!(
            "Menu renderer failed: {}",
            String::from_utf8_lossy(&run.stderr)
        ));
    }
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::menu::document::button_bounds;

    fn design(name: &str) -> PathBuf {
        crate::repo::at("integrations/designs").join(name)
    }

    fn compose(request: MenuRequest) -> Composition {
        compose_menu(&request).unwrap_or_else(|error| panic!("{error}"))
    }

    fn request(name: &str) -> MenuRequest {
        MenuRequest::new(design(name), crate::repo::at("desktop/assets/controllers"))
    }

    fn entries(ids: &[&str]) -> Option<Vec<String>> {
        Some(ids.iter().map(|id| id.to_string()).collect())
    }

    /// A design may draw a screen of its own, and BACK on it must lead
    /// somewhere. The disc design has `disc-back`, which we must declare, or
    /// the button can be focused and does nothing when someone presses it.
    #[test]
    fn a_designs_own_screen_has_a_back_that_goes_somewhere() {
        let composed = compose(request("disc"));
        assert!(composed
            .text("menu.rml")
            .unwrap()
            .contains("id=\"disc-back\""));
        let cfg = composed.text("design.cfg").unwrap();
        let pause = cfg
            .lines()
            .find(|line| line.starts_with("screen_button_pause = "))
            .expect("the pause screen is declared");
        assert!(pause.contains("disc-back"), "{pause}");
        // BACK on an Options entry leads to Options.
        assert!(!pause.contains("controls-back"), "{pause}");

        let native = compose(request("native"));
        assert!(native
            .text("design.cfg")
            .unwrap()
            .contains("screen_button_pause = \"options-back\""));
    }

    /// The footer contains the version instead of a prototype label, with a
    /// token like the tokens for colours in a stylesheet.
    #[test]
    fn an_exported_game_says_its_version_and_not_that_it_is_unfinished() {
        let composed = compose(MenuRequest {
            // A console with no drawing, because we check only the document.
            system: "atari2600".into(),
            ..request("native")
        });
        let menu = composed.text("menu.rml").unwrap();
        assert!(!menu.contains("PROTOTYPE"));
        assert!(
            !menu.contains("design(version)"),
            "the token was left unsubstituted"
        );
        assert!(
            menu.contains(&format!("ROM-IN-A-BOX / {}", env!("CARGO_PKG_VERSION"))),
            "the footer should read the version"
        );
    }

    /// We move the Controls button from the pause row into Options. A game with
    /// nothing enabled has no button that opens an empty screen.
    #[test]
    fn options_lists_only_the_entries_a_game_enables() {
        let composed = compose(request("native"));
        let staged = composed.text("menu.rml").unwrap();
        let panel = staged.find("id=\"options-panel\"").expect("options panel");
        let controls = staged.find("id=\"controls\"").expect("controls entry");
        let actions = staged.find("id=\"actions\"").expect("pause actions");
        assert!(
            actions < panel && panel < controls,
            "controls sits inside options"
        );
        assert!(staged.contains(">OPTIONS<") && staged.contains(">CONTROLS<"));
        let cfg = composed.text("design.cfg").unwrap();
        assert!(
            cfg.contains("screens = \"pause options controls\""),
            "{cfg}"
        );
        assert!(cfg.contains("screen_button_options = \"options\""));
        assert!(cfg.contains("screen_button_controls = \"controls\""));

        let empty = compose(MenuRequest {
            menu_entries: entries(&[]),
            ..request("native")
        });
        let empty = empty.text("menu.rml").unwrap();
        assert!(!empty.contains("id=\"options\""), "no options button");
        assert!(!empty.contains("id=\"options-panel\""));
        assert!(
            button_bounds(empty, "controls").is_none(),
            "controls is not left behind"
        );
        assert!(
            empty.contains("<div id=\"actions\" class=\"no-options\">"),
            "the design draws its four-button row from this state"
        );
    }

    /// We leave out an entry unless the export enables it, refuse an unknown
    /// id instead of dropping it, and fill the design's entry template.
    #[test]
    fn an_entry_appears_only_when_that_game_enables_it() {
        let defaults = compose(request("native"));
        assert!(!defaults.text("menu.rml").unwrap().contains(">SHADERS<"));

        let both = compose(MenuRequest {
            menu_entries: entries(&["controls", "shaders"]),
            ..request("native")
        });
        let both = both.text("menu.rml").unwrap();
        let shaders_at = both.find("id=\"shaders\"").unwrap();
        let controls_at = both.find("id=\"controls\"").unwrap();
        assert!(
            controls_at < shaders_at,
            "entries follow the design's order"
        );

        let refused = compose_menu(&MenuRequest {
            menu_entries: entries(&["nope"]),
            ..request("native")
        })
        .unwrap_err();
        assert!(refused.contains("nope"), "{refused}");

        let root = rominabox_scratch::Scratch::dir("rominabox-entry-template");
        let designs = root.join("designs");
        for package in ["native", "disc"] {
            copy(&design(package), &designs.join(package));
        }
        copy(&crate::repo::at("integrations/parts"), &root.join("parts"));
        std::fs::write(
            designs.join("disc/option-entry.rml"),
            "<button id=\"BUTTON\" class=\"menu-action option-entry list-row\" style=\"top: TOPdp;\">LABEL</button>",
        )
        .unwrap();
        let templated = compose(MenuRequest {
            menu_entries: entries(&["shaders"]),
            ..MenuRequest::new(
                designs.join("disc"),
                crate::repo::at("desktop/assets/controllers"),
            )
        });
        let templated = templated.text("menu.rml").unwrap();
        assert!(
            templated.contains("<button id=\"shaders\" class=\"menu-action option-entry list-row\" style=\"top: 0dp;\">FILTERS</button>"),
            "the design's entry template is what gets filled"
        );
    }

    fn copy(from: &Path, to: &Path) {
        fs::create_dir_all(to).unwrap();
        for entry in fs::read_dir(from).unwrap() {
            let entry = entry.unwrap();
            if entry.file_type().unwrap().is_dir() {
                copy(&entry.path(), &to.join(entry.file_name()));
            } else {
                fs::copy(entry.path(), to.join(entry.file_name())).unwrap();
            }
        }
    }

    /// We put the disc list in the document before the core loads, because we
    /// cannot create a button in the player afterwards. It starts hidden and
    /// disabled, and it is the last Options entry, so it leaves no gap and
    /// does not move Controls. It is never on the pause row.
    #[test]
    fn the_disc_list_is_hidden_until_the_core_has_several_images() {
        let composed = compose(MenuRequest {
            system: "ps1".into(),
            discs: 2,
            ..request("native")
        });
        let staged = composed.text("menu.rml").unwrap();
        let bounds = button_bounds(staged, "discs").expect("the disc entry is in options");
        let button = &staged[bounds.0..bounds.1];
        assert!(button.contains("display: none"), "{button}");
        assert!(button.contains("disabled"), "{button}");
        let controls = button_bounds(staged, "controls").expect("controls");
        assert!(staged[controls.0..controls.1].contains("top: 0dp"));
        let step = Manifest::load(&design("native")).unwrap().option_entry_step;
        assert!(button.contains(&format!("top: {step}dp")), "{button}");
        let actions_at = staged.find("id=\"actions\"").expect("pause row");
        let panel_at = staged.find("id=\"options-panel\"").expect("options");
        assert!(!staged[actions_at..panel_at].contains("id=\"discs\""));

        let disc = compose(MenuRequest {
            system: "ps1".into(),
            discs: 2,
            ..request("disc")
        });
        let disc = disc.text("menu.rml").unwrap();
        assert!(
            disc.contains("id=\"disc-face\""),
            "one disc still opens the circle"
        );
        assert!(
            button_bounds(disc, "disc").is_some(),
            "the column keeps its DISC button"
        );
    }

    #[test]
    fn an_enabled_entry_requires_the_base_options_screen() {
        let mut screens = crate::menu::declared_screens(&design("native")).unwrap();
        screens.retain(|screen| screen.id != "options");
        let error = document::staged_screens(&screens, Some(&["controls".into()])).unwrap_err();
        assert!(
            error.contains("Native base must declare an Options screen"),
            "{error}"
        );
    }
}
