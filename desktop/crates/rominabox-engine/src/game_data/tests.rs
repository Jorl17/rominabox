use super::*;
use rominabox_scratch::Scratch;
use std::fs;
use std::io::Write;

const SONIC: &str = "Sonic & Knuckles + Sonic The Hedgehog 3 (USA) (Lock-on Combination)";

fn game(identity: &str, title: &str, system: &str, content: &str) -> Game {
    Game {
        identity: identity.into(),
        title: title.into(),
        system: system.into(),
        console: if system == "megadrive" { "Mega Drive / Genesis" } else { "Game Boy Color" }.into(),
        content: content.into(),
        app: "/Applications/Sonic 3.app".into(),
        made_with: "0.3.0".into(),
        player_files: vec!["volume.cfg".into(), "background-play.cfg".into()],
    }
}

/// Write `files`, each (path from the folder, contents), into `folder`.
fn put(folder: &Path, files: &[(&str, &str)]) {
    for (path, contents) in files {
        let path = folder.join(path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, contents).unwrap();
    }
}

/// A game's data folder, as a game leaves it after some play.
fn played(root: &Path, name: &str, game: &Game) -> PathBuf {
    let folder = root.join(name);
    fs::create_dir_all(&folder).unwrap();
    write_manifest(&folder, game).unwrap();
    let content = &game.content;
    put(
        &folder,
        &[
            (&format!("saves/{content}.srm"), "battery save"),
            (&format!("states/{content}.state1"), "state one"),
            (&format!("states/{content}.state1.png"), "state picture"),
            (&format!("states/{content}.state.auto"), "auto state"),
            (&format!("screenshots/{content}-251009-101010.png"), "screenshot"),
            (&format!("remaps/Genesis Plus GX/{content}.rmp"), "remap"),
            ("applied/remaps/Genesis Plus GX/x.rmp", "applied"),
            ("controls.cfg", "input_player1_a = \"x\"\n"),
            ("shader-choice", "crt-lottes\n"),
            ("volume.cfg", "audio_volume = \"-3.0\"\n"),
            ("background-play.cfg", "pause_nonactive = \"false\"\n"),
            // Not the player's own: the login, the BIOS, the config we write
            // on every launch, caches and logs.
            ("achievements.session", "user\ntoken\n"),
            ("system/bios.bin", "bios"),
            ("retroarch.cfg", "video_driver = \"gl\"\n"),
            ("cache/x", "cache"),
            ("logs/launch.log", "log"),
            ("achievements-badges/1.png", "badge"),
        ],
    );
    folder
}

/// Every file under `folder`, by its path from there with `/`.
fn files(folder: &Path) -> Vec<String> {
    let mut found = Vec::new();
    let mut pending = vec![folder.to_path_buf()];
    while let Some(current) = pending.pop() {
        for entry in fs::read_dir(&current).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                pending.push(path);
            } else {
                found.push(path.strip_prefix(folder).unwrap().to_string_lossy().replace('\\', "/"));
            }
        }
    }
    found.sort();
    found
}

/// A zip with `entries`, each (name, contents), written with the zip crate,
/// as someone could make by hand.
fn hand_made_zip(path: &Path, entries: &[(&str, &str)]) {
    let mut writer = zip::ZipWriter::new(fs::File::create(path).unwrap());
    for (name, contents) in entries {
        writer.start_file(*name, zip::write::SimpleFileOptions::default()).unwrap();
        writer.write_all(contents.as_bytes()).unwrap();
    }
    writer.finish().unwrap();
}

fn manifest_text(game: &Game) -> String {
    let root = Scratch::dir("rominabox-game-data-manifest-text");
    write_manifest(&root, game).unwrap();
    fs::read_to_string(root.join("game.json")).unwrap()
}

/// We read every field and player file of a manifest back as we wrote it.
/// Quotes in a title cannot add a member, and a line break in it becomes a
/// space.
#[test]
fn a_manifest_reads_back_as_written() {
    let root = Scratch::dir("rominabox-game-data-manifest");
    let mut written = game("03f5379ee2aa7a47a3449acd", "Sonic 3", "megadrive", SONIC);
    write_manifest(&root, &written).unwrap();
    assert_eq!(read_manifest(&root), Some(written.clone()));
    written.title = "Sonic\", \"system\": \"gbc\\\n3".into();
    write_manifest(&root, &written).unwrap();
    let read = read_manifest(&root).unwrap();
    assert_eq!((read.title.as_str(), read.system.as_str()), ("Sonic\", \"system\": \"gbc\\ 3", "megadrive"));
    assert_eq!(read_manifest(&root.join("absent")), None);
}

/// A member we do not know, even an object or a list, is left out, and a
/// manifest that is not JSON, or has no identity, is no manifest.
#[test]
fn a_manifest_skips_what_it_does_not_know() {
    let root = Scratch::dir("rominabox-game-data-manifest-unknown");
    let manifest = root.join("game.json");
    fs::write(&manifest, r#"{"format": 1, "later": {"a": [1, {"b": 2}]}, "identity": "abc", "title": 7,
        "system": "megadrive", "player_files": ["volume.cfg", 3, ["x"]], "extra": [true, null]}"#).unwrap();
    let read = read_manifest(&root).unwrap();
    assert_eq!((read.identity.as_str(), read.title.as_str(), read.player_files.clone()), ("abc", "", vec!["volume.cfg".to_string()]));
    for broken in [r#"{"format": 1, "identity": "abc", "system": "megadrive""#, r#"{"format": 1, "system": "megadrive"}"#,
                   r#"{"format": 2, "identity": "abc", "system": "megadrive"}"#, "format\t1\nidentity\tabc\nsystem\tmegadrive\n"] {
        fs::write(&manifest, broken).unwrap();
        assert_eq!(read_manifest(&root), None, "{broken}");
    }
}

/// A backup contains the manifest and the player's own files, and none of
/// the login, the BIOS, the config we write on every launch, caches, logs or
/// badges. Imported into a fresh copy of the same game, the player's files
/// arrive unchanged.
#[test]
fn a_backup_round_trips_the_players_data_and_nothing_else() {
    let root = Scratch::dir("rominabox-game-data-round-trip");
    let sonic = game("03f5379ee2aa7a47a3449acd", "Sonic 3", "megadrive", SONIC);
    let source = played(&root, "source", &sonic);
    let zip = root.join("Sonic 3.zip");
    export(&[source.clone()], &zip).unwrap();
    assert_eq!(list(&zip).unwrap(), [sonic.clone()]);

    let target = root.join("target");
    fs::create_dir_all(&target).unwrap();
    write_manifest(&target, &sonic).unwrap();
    assert_eq!(check(&zip, 0, &target), Check::SameGame);
    import(&zip, 0, &target).unwrap();
    let expected: Vec<String> = files(&source)
        .into_iter()
        .filter(|path| {
            !["achievements.session", "retroarch.cfg"].contains(&path.as_str())
                && !["system/", "cache/", "logs/", "achievements-badges/"].iter().any(|folder| path.starts_with(folder))
        })
        .collect();
    assert_eq!(files(&target), expected);
    for path in &expected {
        assert_eq!(fs::read(target.join(path)).unwrap(), fs::read(source.join(path)).unwrap(), "{path}");
    }
}

/// An import replaces the player's data: a state the backup does not have is
/// gone afterwards. The game's login stays, because a backup never has one.
#[test]
fn an_import_replaces_the_players_data_and_keeps_the_login() {
    let root = Scratch::dir("rominabox-game-data-replace");
    let sonic = game("03f5379ee2aa7a47a3449acd", "Sonic 3", "megadrive", SONIC);
    let source = played(&root, "source", &sonic);
    let zip = root.join("backup.zip");
    export(&[source], &zip).unwrap();
    let target = played(&root, "target", &sonic);
    put(&target, &[(&format!("states/{SONIC}.state9"), "newer state"), ("achievements.session", "mine\n")]);
    import(&zip, 0, &target).unwrap();
    assert!(!target.join(format!("states/{SONIC}.state9")).exists());
    assert_eq!(fs::read_to_string(target.join("achievements.session")).unwrap(), "mine\n");
}

/// The data of another game for the same console can go into this one after
/// a warning, and we name its saves, states, screenshots and per-game remap
/// after this game's file, so the game finds them.
#[test]
fn another_games_data_is_renamed_to_this_games_file() {
    let root = Scratch::dir("rominabox-game-data-rename");
    let source_game = game("aaaaaaaaaaaaaaaaaaaaaaaa", "Sonic 3", "megadrive", SONIC);
    let source = played(&root, "source", &source_game);
    let zip = root.join("backup.zip");
    export(&[source], &zip).unwrap();
    let target = root.join("target");
    fs::create_dir_all(&target).unwrap();
    write_manifest(&target, &game("bbbbbbbbbbbbbbbbbbbbbbbb", "Sonic 3 (patched)", "megadrive", "Sonic 3 Patched")).unwrap();
    assert_eq!(check(&zip, 0, &target), Check::OtherGame(source_game));
    import(&zip, 0, &target).unwrap();
    let imported = files(&target);
    for path in [
        "saves/Sonic 3 Patched.srm",
        "states/Sonic 3 Patched.state1",
        "states/Sonic 3 Patched.state1.png",
        "states/Sonic 3 Patched.state.auto",
        "screenshots/Sonic 3 Patched-251009-101010.png",
        "remaps/Genesis Plus GX/Sonic 3 Patched.rmp",
    ] {
        assert!(imported.contains(&path.to_string()), "{path} is not in {imported:?}");
    }
    assert!(!imported.iter().any(|path| path.contains(SONIC)), "{imported:?}");
}

/// We refuse the data of a game for another console, and say which game and
/// consoles they are.
#[test]
fn another_consoles_data_is_refused() {
    let root = Scratch::dir("rominabox-game-data-console");
    let source = played(&root, "source", &game("aaaaaaaaaaaaaaaaaaaaaaaa", "Sonic 3", "megadrive", SONIC));
    let zip = root.join("backup.zip");
    export(&[source], &zip).unwrap();
    let target = root.join("target");
    fs::create_dir_all(&target).unwrap();
    write_manifest(&target, &game("cccccccccccccccccccccccc", "Pokemon Gold", "gbc", "Pokemon - Gold Version")).unwrap();
    let refusal = "This is the data of \u{201c}Sonic 3\u{201d}, a Mega Drive / Genesis game, and this game is for the Game Boy Color.";
    assert_eq!(check(&zip, 0, &target), Check::Refused(refusal.into()));
    assert_eq!(import(&zip, 0, &target), Err(refusal.into()));
    assert_eq!(files(&target), ["game.json"]);
}

/// A zip is untrusted. We refuse the whole of one that climbs out of a
/// folder, contains anything that is not a game's data, or has no manifest,
/// before we change anything.
#[test]
fn a_zip_with_anything_but_a_games_data_is_refused_whole() {
    let root = Scratch::dir("rominabox-game-data-untrusted");
    let sonic = game("03f5379ee2aa7a47a3449acd", "Sonic 3", "megadrive", SONIC);
    let target = played(&root, "target", &sonic);
    let before = files(&target);
    let manifest = manifest_text(&sonic);
    for (name, entries, refusal) in [
        ("climbs.zip", vec![("game.json", manifest.as_str()), ("saves/../../escape.txt", "x")], "saves/../../escape.txt"),
        ("bios.zip", vec![("game.json", manifest.as_str()), ("system/bios.bin", "x")], "system/bios.bin"),
        ("absolute.zip", vec![("game.json", manifest.as_str()), ("/etc/hosts", "x")], "/etc/hosts"),
        ("login.zip", vec![("game.json", manifest.as_str()), ("achievements.session", "x")], "achievements.session"),
        ("bare.zip", vec![("saves/x.srm", "x")], "is not the data of a ROM-in-a-Box game"),
    ] {
        let zip = root.join(name);
        hand_made_zip(&zip, &entries);
        let error = import(&zip, 0, &target).unwrap_err();
        assert!(error.contains(refusal), "{name}: {error}");
        assert!(list(&zip).is_err(), "{name}");
        assert_eq!(files(&target), before, "{name} changed the game's data");
    }
    fs::write(root.join("not.zip"), "not a zip").unwrap();
    assert_eq!(list(&root.join("not.zip")), Err("This file is not a zip we can read.".into()));
}

/// A bulk backup contains each game in a folder named after its title and
/// identity, and we import any one of them into its game.
#[test]
fn a_bulk_backup_holds_each_game_in_its_own_folder() {
    let root = Scratch::dir("rominabox-game-data-bulk");
    let sonic = game("03f5379ee2aa7a47a3449acd", "Sonic 3", "megadrive", SONIC);
    let gold = game("dddddddddddddddddddddddd", "Pok\u{e9}mon: Gold", "gbc", "Pokemon - Gold Version");
    let folders = [played(&root, "sonic", &sonic), played(&root, "gold", &gold)];
    let zip = root.join("everything.zip");
    export(&folders, &zip).unwrap();
    assert_eq!(list(&zip).unwrap(), [sonic.clone(), gold.clone()]);
    let names: Vec<String> = {
        let mut archive = zip::ZipArchive::new(fs::File::open(&zip).unwrap()).unwrap();
        (0..archive.len()).map(|index| archive.by_index(index).unwrap().name().to_string()).collect()
    };
    assert!(names.contains(&"Sonic 3 [03f5379e]/game.json".to_string()), "{names:?}");
    assert!(names.contains(&"Pok\u{e9}mon- Gold [dddddddd]/game.json".to_string()), "{names:?}");
    let target = root.join("gold-again");
    fs::create_dir_all(&target).unwrap();
    write_manifest(&target, &gold).unwrap();
    assert_eq!(check(&zip, 1, &target), Check::SameGame);
    assert!(matches!(check(&zip, 0, &target), Check::Refused(_)));
    import(&zip, 1, &target).unwrap();
    assert!(target.join("saves/Pokemon - Gold Version.srm").is_file());
    assert!(!target.join(format!("saves/{SONIC}.srm")).exists());
}

/// We export a game only from a folder with a manifest, which the launcher
/// writes when the game starts.
#[test]
fn a_game_without_a_manifest_is_not_exported() {
    let root = Scratch::dir("rominabox-game-data-no-manifest");
    put(&root, &[("saves/x.srm", "x")]);
    let error = export(&[root.to_path_buf()], &root.join("out.zip")).unwrap_err();
    assert!(error.contains("has no manifest yet"), "{error}");
    assert!(!root.join("out.zip").exists() && !root.join("out.zip.partial").exists());
}
