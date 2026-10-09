use super::*;
use rominabox_scratch::Scratch;

const LAYOUTS: [Layout; 2] = [Layout::Macos, Layout::Windows];
const SONIC: &str = "03f5379ee2aa7a47a3449acd";
const KNUCKLES: &str = "1b2c3d4e5f60718293a4b5c6";
const GOLD: &str = "cccccccccccccccccccccccc";

fn manifest(identity: &str, title: &str, system: &str, app: &Path) -> Game {
    Game {
        identity: identity.into(),
        title: title.into(),
        system: system.into(),
        console: if system == "megadrive" { "Mega Drive / Genesis" } else { "Game Boy Color" }.into(),
        content: format!("{title} (USA)"),
        app: app.to_string_lossy().into_owned(),
        made_with: "0.3.0".into(),
        player_files: vec!["volume.cfg".into()],
    }
}

/// A game's data folder in `library`, as its launcher leaves it, with a save
/// that says `save`, and its app at `app` when `present`.
fn installed(library: &Library, game: &Game, save: &str, present: bool) -> PathBuf {
    let data = library.data_dir(&game.identity);
    fs::create_dir_all(data.join("saves")).unwrap();
    game_data::write_manifest(&data, game).unwrap();
    fs::write(data.join(format!("saves/{}.srm", game.content)), save).unwrap();
    if present {
        fs::create_dir_all(&game.app).unwrap();
    }
    data
}

fn save(library: &Library, game: &Game) -> String {
    fs::read_to_string(library.data_dir(&game.identity).join(format!("saves/{}.srm", game.content))).unwrap()
}

/// On each platform's layout, we list the games whose sandbox has our prefix
/// and whose data folder has a manifest, by title, with the icon when there
/// is one and whether the app is still there. We leave out other apps'
/// sandboxes, a sandbox without a manifest, a name that is not an identity
/// and a file.
#[test]
fn the_games_with_a_manifest_in_a_sandbox_of_ours_are_listed() {
    for layout in LAYOUTS {
        let root = Scratch::dir("rominabox-game-library-list");
        let library = Library::at(root.to_path_buf(), layout);
        let sonic = manifest(SONIC, "Sonic 3", "megadrive", &root.join("apps/Sonic 3.app"));
        let gold = manifest(GOLD, "Pokemon Gold", "gbc", &root.join("apps/Gold.app"));
        let sonic_data = installed(&library, &sonic, "rings", true);
        fs::write(sonic_data.join("game-icon.png"), b"png").unwrap();
        let gold_data = installed(&library, &gold, "badges", false);
        let sandboxes = library.sandboxes();
        fs::create_dir_all(library.sandbox(KNUCKLES)).unwrap();
        fs::create_dir_all(sandboxes.join("com.apple.TextEdit")).unwrap();
        fs::create_dir_all(sandboxes.join(layout.sandbox_name("NOT-AN-IDENTITY"))).unwrap();
        fs::write(sandboxes.join(layout.sandbox_name("abc")), b"a file").unwrap();

        let games = library.games();

        assert_eq!(
            games,
            [
                InstalledGame {
                    file_name: "Pokemon Gold data.zip".into(),
                    game: gold,
                    icon: None,
                    app_present: false,
                    running: false,
                    data: gold_data,
                },
                InstalledGame {
                    game: sonic,
                    icon: Some(sonic_data.join("game-icon.png")),
                    app_present: true,
                    running: false,
                    file_name: "Sonic 3 data.zip".into(),
                    data: sonic_data,
                },
            ],
            "{layout:?}"
        );
    }
}

/// On Windows, the folder of a sandbox in Packages has a lower-case name, and
/// we accept the prefix in any case, as in the uninstaller.
#[test]
fn a_windows_sandbox_is_found_whatever_the_case_of_its_prefix() {
    let root = Scratch::dir("rominabox-game-library-case");
    let library = Library::at(root.to_path_buf(), Layout::Windows);
    let sonic = manifest(SONIC, "Sonic 3", "megadrive", &root.join("Sonic 3.exe"));
    let data = library.data_dir(SONIC);
    assert!(data.starts_with(root.join("Packages").join(format!("rominabox.game.{SONIC}"))), "{}", data.display());
    installed(&library, &sonic, "rings", false);
    fs::rename(library.sandbox(SONIC), root.join("Packages").join(format!("ROMinaBox.Game.{SONIC}"))).unwrap();
    assert_eq!(library.games().len(), 1);
}

/// We export the chosen games, or every game, to one zip, and refuse a game
/// that is not here.
#[test]
fn chosen_games_or_every_game_export_to_one_zip() {
    let root = Scratch::dir("rominabox-game-library-export");
    let library = Library::at(root.to_path_buf(), Layout::Macos);
    let sonic = manifest(SONIC, "Sonic 3", "megadrive", &root.join("Sonic 3.app"));
    let gold = manifest(GOLD, "Pokemon Gold", "gbc", &root.join("Gold.app"));
    installed(&library, &sonic, "rings", true);
    installed(&library, &gold, "badges", true);

    let one = root.join("one.zip");
    assert_eq!(library.export(Some(&[SONIC.into()]), &one).unwrap(), [sonic.clone()]);
    assert_eq!(game_data::list(&one).unwrap(), [sonic.clone()]);
    let every = root.join("every.zip");
    assert_eq!(library.export(None, &every).unwrap(), [gold.clone(), sonic.clone()]);
    assert_eq!(game_data::list(&every).unwrap(), [gold, sonic]);

    let error = library.export(Some(&[KNUCKLES.into()]), &root.join("none.zip")).unwrap_err();
    assert_eq!(error, format!("There is no game {KNUCKLES} on this computer."));
    assert!(!root.join("none.zip").exists());
}

/// A backup of another game for the same console goes into the game we
/// choose after the check names its game, and its save takes this game's
/// name. A game that is not here is refused.
#[test]
fn one_backup_goes_into_the_game_chosen() {
    let root = Scratch::dir("rominabox-game-library-import");
    let library = Library::at(root.to_path_buf(), Layout::Macos);
    let sonic = manifest(SONIC, "Sonic 3", "megadrive", &root.join("Sonic 3.app"));
    let knuckles = manifest(KNUCKLES, "Knuckles", "megadrive", &root.join("Knuckles.app"));
    installed(&library, &sonic, "rings", true);
    installed(&library, &knuckles, "emeralds", true);
    let zip = root.join("sonic.zip");
    library.export(Some(&[SONIC.into()]), &zip).unwrap();

    assert_eq!(library.open(&zip).unwrap(), [BackupGame { game: sonic.clone(), here: true }]);
    assert_eq!(library.check(&zip, 0, SONIC), Check::SameGame);
    assert_eq!(library.check(&zip, 0, KNUCKLES), Check::OtherGame(sonic));
    library.import(&zip, 0, KNUCKLES).unwrap();
    assert_eq!(save(&library, &knuckles), "rings");

    let absent = format!("There is no game {GOLD} on this computer.");
    assert_eq!(library.check(&zip, 0, GOLD), Check::Refused(absent.clone()));
    assert_eq!(library.import(&zip, 0, GOLD), Err(absent));
}

/// A bulk backup goes into each game with the same identity here. We report
/// the games that are not here, and refuse one whose data is for another
/// console than the game with its identity here.
#[test]
fn a_bulk_backup_goes_into_every_game_it_contains_that_is_here() {
    let root = Scratch::dir("rominabox-game-library-bulk");
    let elsewhere = Library::at(root.join("elsewhere"), Layout::Macos);
    let sonic = manifest(SONIC, "Sonic 3", "megadrive", &root.join("Sonic 3.app"));
    let knuckles = manifest(KNUCKLES, "Knuckles", "megadrive", &root.join("Knuckles.app"));
    let gold = manifest(GOLD, "Pokemon Gold", "gbc", &root.join("Gold.app"));
    installed(&elsewhere, &sonic, "rings", false);
    installed(&elsewhere, &knuckles, "emeralds", false);
    installed(&elsewhere, &gold, "badges", false);
    let zip = root.join("every.zip");
    elsewhere.export(None, &zip).unwrap();

    let library = Library::at(root.join("here"), Layout::Windows);
    installed(&library, &sonic, "no rings", false);
    let mut gold_here = gold.clone();
    gold_here.system = "megadrive".into();
    gold_here.console = "Mega Drive / Genesis".into();
    installed(&library, &gold_here, "no badges", false);

    let opened = library.open(&zip).unwrap();
    let here: Vec<(&str, bool)> = opened.iter().map(|game| (game.game.identity.as_str(), game.here)).collect();
    assert_eq!(here, [(KNUCKLES, false), (GOLD, true), (SONIC, true)]);

    let report = library.import_all(&zip).unwrap();

    assert_eq!(report.imported, [sonic.clone()]);
    assert_eq!(report.not_here, [knuckles]);
    assert_eq!(report.refused.len(), 1, "{report:?}");
    assert_eq!(report.refused[0].game, gold);
    assert!(report.refused[0].reason.contains("Game Boy Color"), "{}", report.refused[0].reason);
    assert_eq!(save(&library, &sonic), "rings");
    assert_eq!(save(&library, &gold_here), "no badges");
}

/// We remove the sandbox of a game whose app is gone, and on Windows its
/// unpacked copy, and leave every other game alone.
#[test]
fn a_game_whose_app_is_gone_is_removed() {
    for layout in LAYOUTS {
        let root = Scratch::dir("rominabox-game-library-remove");
        let library = Library::at(root.to_path_buf(), layout);
        let sonic = manifest(SONIC, "Sonic 3", "megadrive", &root.join("Sonic 3.app"));
        let gold = manifest(GOLD, "Pokemon Gold", "gbc", &root.join("Gold.app"));
        installed(&library, &sonic, "rings", false);
        installed(&library, &gold, "badges", false);
        let unpacked = root.join(runtime_folder(SONIC));
        fs::create_dir_all(&unpacked).unwrap();

        library.remove(SONIC).unwrap();

        assert!(!library.sandbox(SONIC).exists(), "{layout:?}");
        assert_eq!(unpacked.exists(), layout == Layout::Macos, "{layout:?}");
        assert_eq!(library.games().len(), 1, "{layout:?}");
        assert_eq!(save(&library, &gold), "badges");
    }
}

/// We refuse to remove a game whose app is still there, a game that is not
/// here, and a game whose sandbox is a link, which we never follow.
#[test]
fn a_game_is_removed_only_when_its_app_is_gone() {
    let root = Scratch::dir("rominabox-game-library-keep");
    let library = Library::at(root.to_path_buf(), Layout::Macos);
    let sonic = manifest(SONIC, "Sonic 3", "megadrive", &root.join("Sonic 3.app"));
    installed(&library, &sonic, "rings", true);

    let error = library.remove(SONIC).unwrap_err();
    assert_eq!(error, format!("\u{201c}Sonic 3\u{201d} is still at {}.", sonic.app));
    assert_eq!(save(&library, &sonic), "rings");
    assert_eq!(library.remove("../../x").unwrap_err(), "There is no game ../../x on this computer.");

    #[cfg(unix)]
    {
        let gold = manifest(GOLD, "Pokemon Gold", "gbc", &root.join("Gold.app"));
        let real = Library::at(root.join("real"), Layout::Macos);
        installed(&real, &gold, "badges", false);
        fs::create_dir_all(library.sandboxes()).unwrap();
        std::os::unix::fs::symlink(real.sandbox(GOLD), library.sandbox(GOLD)).unwrap();
        let error = library.remove(GOLD).unwrap_err();
        assert!(error.ends_with("it is not a folder"), "{error}");
        assert_eq!(save(&real, &gold), "badges");
    }
}

/// While a game runs, we refuse to import into it or remove it, and list it
/// as running. In this test, the running game is a copy of the system's
/// sleep inside a Mac app.
#[cfg(target_os = "macos")]
#[test]
fn a_running_game_is_neither_imported_into_nor_removed() {
    let root = Scratch::dir("rominabox-game-library-running");
    let library = Library::at(root.to_path_buf(), Layout::Macos);
    let app = root.join("Sonic 3.app");
    let sonic = manifest(SONIC, "Sonic 3", "megadrive", &app);
    installed(&library, &sonic, "rings", true);
    let zip = root.join("sonic.zip");
    library.export(None, &zip).unwrap();
    let program = app.join("Contents/MacOS/sleep");
    fs::create_dir_all(program.parent().unwrap()).unwrap();
    fs::copy("/bin/sleep", &program).unwrap();
    let mut child = std::process::Command::new(&program).arg("30").spawn().unwrap();

    let games = library.games();
    let check = library.check(&zip, 0, SONIC);
    let imported = library.import(&zip, 0, SONIC);
    let removed = library.remove(SONIC);
    child.kill().unwrap();
    child.wait().unwrap();

    assert!(games[0].running);
    let refusal = "\u{201c}Sonic 3\u{201d} is open. Quit it, then try again.".to_string();
    assert_eq!(check, Check::Refused(refusal.clone()));
    assert_eq!(imported, Err(refusal.clone()));
    assert_eq!(removed, Err(refusal));
    assert!(!library.games()[0].running);
}

/// The same on Windows, where the running game is a copy of Windows' own
/// ping, pinging the loopback address.
#[cfg(windows)]
#[test]
fn a_running_windows_game_is_neither_imported_into_nor_removed() {
    let root = Scratch::dir("rominabox-game-library-running");
    let library = Library::at(root.to_path_buf(), Layout::Windows);
    let program = root.join("Sonic 3.exe");
    let sonic = manifest(SONIC, "Sonic 3", "megadrive", &program);
    installed(&library, &sonic, "rings", false);
    let zip = root.join("sonic.zip");
    library.export(None, &zip).unwrap();
    let system = std::env::var("SystemRoot").expect("Windows names its folder");
    fs::copy(Path::new(&system).join("System32/PING.EXE"), &program).unwrap();
    let mut child = std::process::Command::new(&program)
        .args(["-n", "30", "127.0.0.1"])
        .stdout(std::process::Stdio::null())
        .spawn()
        .unwrap();

    let games = library.games();
    let imported = library.import(&zip, 0, SONIC);
    child.kill().unwrap();
    child.wait().unwrap();

    assert!(games[0].running);
    assert_eq!(imported, Err("\u{201c}Sonic 3\u{201d} is open. Quit it, then try again.".to_string()));
}
