//! Composing a whole menu and writing it.

use super::{
    contract, declarations, document, file_name, manifest::Manifest, scene, tokens, Content,
    ScreenPlace,
};
use crate::controls::Controls;
use crate::menu::EntryPlace;
use crate::shaders::ShaderSelection;
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
};

/// The page we write in a composition and open in the player.
pub const DOCUMENT: &str = file_name!(Menu);

/// The settings for the in-game menu of a game.
#[derive(Clone, Debug)]
pub struct MenuRequest {
    /// The design's package, with Native beside it.
    pub design: PathBuf,
    /// The controller artwork every design shares.
    pub artwork: PathBuf,
    pub palette: String,
    pub background: Option<PathBuf>,
    /// The background picture drawn in the screen's colour.
    pub tint_background: bool,
    pub system: String,
    pub controls: Controls,
    /// What opens the menu, and confirms and goes back in it, until the
    /// player changes them.
    pub hotkeys: crate::hotkeys::Hotkeys,
    /// The full menu. Without it the game has only the splash, if any.
    pub show_menu: bool,
    pub splash: bool,
    pub include_achievements: bool,
    /// The Options entries that the author chose, or the design's defaults
    /// when absent.
    pub menu_entries: Option<Vec<String>>,
    pub shaders: ShaderSelection,
    /// The runtime kit's shader library, which libretro presets come from.
    pub shader_library: PathBuf,
    /// How many discs the game has. The disc list, and its Options entry,
    /// exist only for more than one.
    pub discs: usize,
    /// The export's defaults for the settings the player changes in Options.
    pub settings: crate::player_settings::Defaults,
    /// The game has a menu sound pack, and we play its movement cue when the
    /// volume changes. Without a pack we ship a tick for the volume.
    pub sound_pack: bool,
    /// DATA, for a game without a list of its entries: the author left it
    /// on.
    pub game_data: bool,
    /// The platform we make the game for, and so the screens it has.
    pub target: crate::packaging::ExportTarget,
    /// The components we ship in the game, which we list on its ABOUT screen.
    pub licences: Vec<crate::licences::Row>,
}

impl MenuRequest {
    /// The design's default menu for a one-disc Mega Drive game in Blue.
    pub fn new(design: impl Into<PathBuf>, artwork: impl Into<PathBuf>) -> MenuRequest {
        MenuRequest {
            design: design.into(),
            artwork: artwork.into(),
            palette: "blue".into(),
            background: None,
            tint_background: false,
            system: "megadrive".into(),
            controls: Controls::default(),
            hotkeys: crate::builder::unstated::hotkeys(),
            show_menu: true,
            splash: false,
            include_achievements: false,
            menu_entries: None,
            shaders: ShaderSelection::default(),
            shader_library: PathBuf::new(),
            discs: 1,
            settings: crate::player_settings::Defaults {
                video: Some(crate::player_settings::Video {
                    brightness: crate::builder::unstated::brightness(),
                    contrast: crate::builder::unstated::contrast(),
                }),
                ..crate::player_settings::Defaults::default()
            },
            sound_pack: false,
            game_data: crate::builder::unstated::game_data(),
            target: crate::packaging::ExportTarget::of_host()
                .expect("the builder runs on a platform it makes games for"),
            licences: Vec::new(),
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

    /// A composed text file, such as `DOCUMENT` or `design.cfg`.
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
                        (dunce::canonicalize(&from), dunce::canonicalize(&to)),
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

/// The fonts declared in the design and their licences, which we ship along.
fn fonts(composition: &mut Composition, manifest: &Manifest) {
    for font in &manifest.fonts {
        for name in [&font.file, &font.license] {
            composition.put(name.as_str(), Content::Copy(manifest.fragment_path(name)));
        }
    }
}

/// The pictures in the design's menu, which a game with only the splash
/// does not contain.
fn pictures(composition: &mut Composition, manifest: &Manifest) {
    for name in &manifest.pictures {
        composition.put(name.as_str(), Content::Copy(manifest.fragment_path(name)));
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
            Path::new(document::PARTS).join(&name),
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
    // Without a list of the entries, we give a game VIDEO when the author left
    // it on.
    if request.menu_entries.is_none() && request.settings.video.is_none() {
        entries.retain(|entry| entry != crate::player_settings::VIDEO_ENTRY);
    }
    // And DATA when the author left it on.
    if request.menu_entries.is_none() && !request.game_data {
        entries.retain(|entry| {
            !manifest.screens.iter().any(|screen| screen.id == *entry && screen.role == Some(crate::menu::ScreenRole::Data))
        });
    }
    if request.discs <= 1 {
        entries.retain(|entry| {
            !manifest
                .screens
                .iter()
                .any(|screen| screen.id == *entry && screen.is_disc_list())
        });
    }
    // A screen we fill in the exporter is an entry when we filled it.
    entries.retain(|entry| {
        manifest.screens.iter().all(|screen| screen.id != *entry || screen.rows.is_none())
            || lists.iter().any(|list| list.screen.id == *entry)
    });
    for list in lists {
        if list.screen.option_label.is_some() && !entries.contains(&list.screen.id) {
            entries.push(list.screen.id.clone());
        }
    }
    for entry in &entries {
        let Some(screen) = manifest.screens.iter().find(|screen| screen.id == *entry) else {
            continue;
        };
        let Some(opener) = screen.opener.as_ref().filter(|_| screen.entry_place == EntryPlace::Opener) else {
            continue;
        };
        if !entries.contains(opener) {
            let heading = |id: &str| {
                manifest.screens.iter().find(|screen| screen.id == id).map_or(id.to_string(), |screen| screen.heading.clone())
            };
            return Err(format!(
                "{} opens from {}, so a game with {} needs {} in its Options too",
                screen.heading,
                heading(opener),
                screen.heading,
                heading(opener)
            ));
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
    let palette = crate::themes::palette(&request.palette)?;
    let values = tokens::design(&manifest, &palette);
    fonts(&mut composition, &manifest);
    let stylesheet = tokens::substitute(&manifest.stylesheet()?, &values)?;
    // The hotkeys, whether or not the game has HOTKEYS to change them. A hint
    // names a hotkey by the words of its bindings.
    let game_hotkeys = crate::hotkeys::GameHotkeys::of(&request.settings);
    let hotkeys = game_hotkeys.offered(&request.hotkeys);
    let hint = |text: &str| hotkeys.hint(text);

    if !request.show_menu {
        let splash = document::place_parts(&manifest, &manifest.fragment(&manifest.documents.splash)?)?;
        super::contract::validate_splash(&manifest, &splash)?;
        let staged = document::staged_screens(&manifest.screens, None, request.discs, request.target)?;
        let cfg = declarations::write(&manifest, &staged, &[], &[], &splash, &hint)?;
        let splash = parts(&mut composition, &manifest, &values, &splash)?;
        composition.put(DOCUMENT, Content::Text(splash));
        composition.put(document::STYLESHEET, Content::Text(stylesheet));
        composition.put(file_name!(Design), Content::Text(cfg));
        return Ok(composition);
    }

    pictures(&mut composition, &manifest);
    // The settings of VIDEO only in a menu with VIDEO among its entries.
    let settings = crate::player_settings::Defaults {
        video: request.settings.video.and_then(|video| video.in_menu(request.menu_entries.as_deref())),
        ..request.settings
    };
    // We compose data lists and the live account screen with the same code.
    let mut lists: Vec<crate::lists::List> = Vec::new();
    if request.discs > 1 {
        lists.extend(crate::disc_menu::list(&manifest));
    }
    let destination = crate::shaders::Destination {
        platform: request.target,
        library: request.shader_library.clone(),
    };
    let shaders = crate::shaders::stage(
        &manifest,
        &request.shaders,
        &destination,
        settings.video.is_some(),
        crate::systems::frames_of(&request.system),
    )?;
    for (name, content) in shaders.files {
        composition.put(name, content);
    }
    if !shaders.config.is_empty() {
        composition.put(file_name!(Shaders), Content::Text(shaders.config));
    }
    lists.extend(shaders.list);
    let achievements = crate::achievements::included(request.include_achievements, true);
    if achievements {
        lists.push(crate::achievements::screen(&manifest)?);
        lists.extend(crate::achievements::accounts_screen(&manifest));
    }
    lists.extend(crate::licences::list(&manifest, &request.licences, request.target));
    let entries = entries(&manifest, request, &lists)?;
    let staged = document::staged_screens(&manifest.screens, Some(&entries), request.discs, request.target)?;

    let scene = scene::compose(
        &request.artwork,
        &manifest,
        &request.system,
        &request.controls,
    )?;
    for (name, content) in scene.files {
        composition.put(name, content);
    }
    let menu = document::opening_screen(&manifest, &document::skeleton(&manifest, &staged)?, &hint)?;
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
    let settings = crate::player_settings::declared(settings);
    let menu = document::apply_options(&manifest, &menu, &staged, &settings)?;
    // The rest of the player's settings are parts of the Options screen that
    // we just built.
    let menu = document::install_settings(
        &menu,
        &document::SettingsPlace {
            design: &manifest.design,
            words: &manifest.words,
            options_panel: staged
                .iter()
                .find(|screen| screen.place == ScreenPlace::Options)
                .map(|screen| screen.panel.as_str()),
        },
        &settings,
    )?;
    let mut menu = tokens::substitute(&menu, &tokens::product())?;
    if !staged
        .iter()
        .any(|screen| screen.place == ScreenPlace::Options)
    {
        menu = document::add_class(&menu, "actions", "no-options");
    }
    let (menu, installed) = crate::lists::install(&manifest, &menu, &staged, &lists)?;
    super::contract::validate(&manifest, &menu, &staged.iter().collect::<Vec<_>>())?;
    // Where the game has HOTKEYS, each row must show every default binding
    // for its hotkey.
    crate::hotkeys::fit(&hotkeys, &menu, &manifest.id)?;
    composition.put(crate::hotkeys::DEFAULTS_FILE, Content::Text(hotkeys.defaults_config(&game_hotkeys)?));
    let cfg = declarations::write(&manifest, &staged, &installed, &settings, &menu, &hint)?;
    // We play a sound for a change of volume in every game with a volume
    // control: the movement cue of the pack, or in a game without a pack the
    // tick for the volume, which we ship only then.
    let volume = crate::player_settings::volume();
    if !request.sound_pack && menu.contains(&format!("id=\"{}\"", volume.control())) {
        let tick = crate::volume::tick_file();
        composition.put(
            tick,
            Content::Copy(document::parts_root(&manifest.design)?.join(tick)),
        );
    }
    // After the parts, which can name a hotkey as the design's own markup does.
    let mut menu = hotkeys.bound_words(&parts(&mut composition, &manifest, &values, &menu)?)?;
    // The author picks the picture and the design places it (by default
    // behind #screen, with the shared part, unless the design restyles it,
    // in the screen's colour where the author asked for a tint).
    if let Some(image_path) = &request.background {
        let image = crate::icons::read_image(image_path).map_err(|e| e.sentence())?;
        let mut png = Vec::new();
        image
            .resize(1920, 1200, image::imageops::FilterType::Lanczos3)
            .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
            .map_err(|e| e.to_string())?;
        composition.put("background.png", Content::Bytes(png));
        menu = document::add_class(&menu, contract!(Screen), "with-background");
        if request.tint_background {
            menu = document::add_class(&menu, contract!(Screen), "tinted-background");
        }
    }

    let mut stylesheet = stylesheet;
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

    composition.put(DOCUMENT, Content::Text(menu));
    composition.put(document::STYLESHEET, Content::Text(stylesheet));
    composition.put(file_name!(Design), Content::Text(cfg));
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
    #[serde(default)]
    pub tint_background: bool,
    pub width: u32,
    pub height: u32,
    /// A folder of pictures rendered before, each under its `Preview::name`.
    /// We take the picture from there when the folder holds it.
    #[serde(default)]
    pub rendered: Option<PathBuf>,
}

/// A picture of the menu, and its name among pictures rendered before.
pub struct Preview {
    pub path: PathBuf,
    pub name: String,
}

/// Render the menu for an export of this design with the same windowless
/// RmlUi helper as in an export, or take the picture from `request.rendered`.
/// We keep the intermediate files in the given output directory for
/// inspection.
pub fn render_preview(request: &PreviewRequest) -> Result<Preview, String> {
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
        tint_background: request.tint_background,
        ..MenuRequest::new(design, &request.assets)
    })?
    .write(&request.output_dir)?;
    let name = picture_name(&request.output_dir, request.width, request.height)?;
    if let Some(found) = request.rendered.as_ref().map(|folder| folder.join(&name)).filter(|path| path.is_file()) {
        return Ok(Preview { path: found, name });
    }
    let output = request.output_dir.join("preview.png");
    let run = crate::helper::command(&request.renderer)
        .arg(request.output_dir.join(DOCUMENT))
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
    Ok(Preview { path: output, name })
}

/// The name of the picture of the menu composed in `composed`: a digest of
/// every composed file, by its path inside the folder, and of the size. The
/// composed files contain the design, the palette, the artwork, the
/// background and the version. The renderer is not in the digest, because we
/// render the pictures we bundle with the renderer of the same kit.
fn picture_name(composed: &Path, width: u32, height: u32) -> Result<String, String> {
    use sha2::{Digest, Sha256};
    fn files(folder: &Path, found: &mut Vec<PathBuf>) -> Result<(), String> {
        for entry in fs::read_dir(folder).map_err(|e| format!("{}: {e}", folder.display()))? {
            let path = entry.map_err(|e| e.to_string())?.path();
            if path.is_dir() {
                files(&path, found)?;
            } else {
                found.push(path);
            }
        }
        Ok(())
    }
    let mut found = Vec::new();
    files(composed, &mut found)?;
    let mut named: Vec<(String, PathBuf)> = found
        .into_iter()
        .map(|path| {
            let relative = path.strip_prefix(composed).unwrap_or(&path);
            let parts: Vec<_> = relative.components().map(|part| part.as_os_str().to_string_lossy().into_owned()).collect();
            (parts.join("/"), path)
        })
        .collect();
    named.sort();
    let mut hash = Sha256::new();
    hash.update(format!("{width}x{height}\n"));
    for (relative, path) in named {
        let bytes = fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        hash.update(format!("{relative}\n{}\n", bytes.len()));
        hash.update(&bytes);
    }
    Ok(format!("{:x}.png", hash.finalize()))
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

    /// The Controls button is inside Options, with the screen for removing
    /// what we store for the game on its platform: UNINSTALL on Windows, RESET
    /// on a Mac. A game with nothing enabled has no button that opens an empty
    /// screen.
    #[test]
    fn options_lists_only_the_entries_a_game_enables() {
        let composed = compose(MenuRequest {
            target: crate::packaging::ExportTarget::Windows,
            ..request("native")
        });
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
            cfg.contains("screens = \"pause options controls hotkeys video restart uninstall data\""),
            "{cfg}"
        );
        let mac = compose(MenuRequest {
            target: crate::packaging::ExportTarget::Macos,
            ..request("native")
        });
        let mac = mac.text("design.cfg").unwrap();
        assert!(mac.contains("screens = \"pause options controls hotkeys video restart reset data\""), "{mac}");
        assert!(cfg.contains("screen_button_options = \"options video-back\""));
        assert!(cfg.contains("screen_button_controls = \"controls\""));

        let empty = compose(MenuRequest {
            menu_entries: entries(&[]),
            ..request("native")
        });
        let empty = empty.text("menu.rml").unwrap();
        assert!(!empty.contains("id=\"options\""), "no options button");
        assert!(!empty.contains("id=\"options-panel\""));
        assert!(
            !empty.contains("id=\"hotkeys-panel\""),
            "a screen the game does not get is not drawn"
        );
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
            menu_entries: entries(&["controls", "video", "shaders"]),
            shaders: one_shader(),
            ..request("native")
        });
        let both = both.text("menu.rml").unwrap();
        let video_at = both.find("id=\"video\"").unwrap();
        let controls_at = both.find("id=\"controls\"").unwrap();
        assert!(
            controls_at < video_at,
            "entries follow the design's order"
        );
        // SHADERS is on VIDEO, where it is in the design, and not a row in
        // Options.
        let shaders_at = both.find("id=\"shaders\"").unwrap();
        let video_panel = both.find("id=\"video-panel\"").unwrap();
        let options_panel = both.find("id=\"options-panel\"").unwrap();
        assert!(video_panel < shaders_at && (shaders_at < options_panel || options_panel < video_panel));
        assert_eq!(both.matches("id=\"shaders\"").count(), 1, "one SHADERS button");

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
            "<button id=\"BUTTON\" class=\"menu-action option-entry list-row\">LABEL</button>",
        )
        .unwrap();
        let templated = compose(MenuRequest {
            menu_entries: entries(&["video", "shaders"]),
            shaders: one_shader(),
            ..MenuRequest::new(
                designs.join("disc"),
                crate::repo::at("desktop/assets/controllers"),
            )
        });
        let templated = templated.text("menu.rml").unwrap();
        assert!(
            templated.contains("<button id=\"video\" class=\"menu-action option-entry list-row\">VIDEO</button>"),
            "the design's entry template is what gets filled"
        );
    }

    /// In each design, RESTART is on the pause screen, after OPTIONS and
    /// before QUIT, and we keep it there and add no row for it in Options. In
    /// a game without RESTART we leave out its button and its screen.
    #[test]
    fn restart_stays_where_the_design_places_it() {
        for name in ["native", "disc", "rominabox"] {
            let staged = compose(request(name));
            let menu = staged.text("menu.rml").unwrap();
            let at = |id: &str| menu.find(&format!("id=\"{id}\"")).unwrap_or_else(|| panic!("{name}: no #{id}"));
            assert!(
                at("actions") < at("options") && at("options") < at("restart") && at("restart") < at("quit"),
                "{name}: RESTART is on the pause screen, between OPTIONS and QUIT"
            );
            assert!(at("restart") < at("options-panel"), "{name}: RESTART is not a row of Options");
            assert_eq!(menu.matches("id=\"restart\"").count(), 1, "{name}: one RESTART button");

            let without = compose(MenuRequest {
                menu_entries: entries(&["controls", "hotkeys"]),
                ..request(name)
            });
            let menu = without.text("menu.rml").unwrap();
            assert!(!menu.contains("id=\"restart"), "{name}: a game without RESTART has none of it");
        }
    }

    /// In a design without a RESTART button on the pause screen, RESTART is a
    /// row of Options, after the switches and before ABOUT.
    #[test]
    fn restart_is_a_row_of_options_in_a_design_that_places_none() {
        let root = rominabox_scratch::Scratch::dir("rominabox-restart-row");
        let designs = root.join("designs");
        for package in ["native", "disc"] {
            copy(&design(package), &designs.join(package));
        }
        copy(&crate::repo::at("integrations/parts"), &root.join("parts"));
        let pause = designs.join("disc/screen-pause.rml");
        let markup = std::fs::read_to_string(&pause).unwrap();
        let button = "<button class=\"menu-action\" id=\"restart\">RESTART</button>";
        assert!(markup.contains(button), "the Disc pause screen has RESTART to take out");
        std::fs::write(&pause, markup.replace(button, "")).unwrap();
        let staged = compose(MenuRequest {
            licences: vec![crate::licences::Row {
                group: crate::licences::Group::Native,
                title: "RetroArch".into(),
                version: String::new(),
                licence: "GPL-3.0".into(),
                file: String::new(),
                copyright: String::new(),
            }],
            ..MenuRequest::new(designs.join("disc"), crate::repo::at("desktop/assets/controllers"))
        });
        let menu = staged.text("menu.rml").unwrap();
        let at = |id: &str| menu.find(&format!("id=\"{id}\"")).unwrap_or_else(|| panic!("no #{id}"));
        assert!(at("options-panel") < at("restart"), "RESTART is a row of Options");
        assert!(at("restart") < at("about"), "RESTART comes before ABOUT");
        assert_eq!(menu.matches("id=\"restart\"").count(), 1, "one RESTART button");
    }

    fn one_shader() -> ShaderSelection {
        ShaderSelection {
            bundled: vec!["scanlines".into()],
            ..ShaderSelection::default()
        }
    }

    /// We refuse an entry for a screen that is not in the menu. For example,
    /// Shaders with no bundled shader would be a button that opens nothing.
    #[test]
    fn an_entry_whose_screen_is_not_drawn_is_refused() {
        let error = compose_menu(&MenuRequest {
            menu_entries: entries(&["controls", "video", "shaders"]),
            ..request("native")
        })
        .unwrap_err();
        assert!(error.contains("#shaders-panel"), "{error}");
        assert!(error.contains("design 'native'"), "{error}");
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
        // We place the entries with the design's stylesheet and write no
        // position, so a hidden entry leaves no gap.
        let controls = button_bounds(staged, "controls").expect("controls");
        assert!(!staged[controls.0..controls.1].contains("top:"));
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
        let error =
            document::staged_screens(
                &screens,
                Some(&["controls".into()]),
                1,
                crate::packaging::ExportTarget::of_host().unwrap(),
            )
            .unwrap_err();
        assert!(
            error.contains("Native base must declare an Options screen"),
            "{error}"
        );
    }
}
