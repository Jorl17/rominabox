use super::*;

fn fixture(name: &str) -> rominabox_scratch::Scratch {
    rominabox_scratch::Scratch::dir(&format!("rominabox-content-{name}"))
}

#[test]
fn cue_collects_each_referenced_track_with_relative_layout() {
    let root = fixture("multi-track");
    fs::create_dir(root.join("audio")).unwrap();
    fs::write(root.join("disc.bin"), b"data").unwrap();
    fs::write(root.join("audio/track 02.bin"), b"audio").unwrap();
    let cue = root.join("game.cue");
    fs::write(
        &cue,
        "FILE \"disc.bin\" BINARY\n  TRACK 01 MODE1/2352\nFILE \"audio\\track 02.bin\" BINARY\n  TRACK 02 AUDIO\n",
    )
    .unwrap();

    let content = collect(&cue).unwrap();
    let relative = content
        .files
        .iter()
        .map(|file| file.relative.clone())
        .collect::<Vec<_>>();
    assert_eq!(
        relative,
        ["game.cue", "disc.bin", "audio/track 02.bin"]
            .map(PathBuf::from)
            .to_vec()
    );
}

#[test]
fn a_refusal_names_the_file_the_way_the_author_would_write_it() {
    // A canonical path on Windows starts with the verbatim prefix \\?\,
    // which an author should not see in a message.
    let root = fixture("missing-sub");
    let ccd = root.join("game.ccd");
    fs::write(&ccd, "[CloneCD]\n").unwrap();
    fs::write(root.join("game.img"), b"data").unwrap();

    let error = collect_for(&ccd, Some("pcecd")).unwrap_err();
    assert!(error.contains("game.ccd"), "{error}");
    assert!(!error.contains(r"\\?\"), "{error}");
}

#[test]
fn cue_rejects_parent_traversal() {
    let root = fixture("traversal");
    let cue = root.join("game.cue");
    fs::write(&cue, "FILE \"../secret.bin\" BINARY\n").unwrap();

    let error = collect(&cue).unwrap_err();
    assert!(error.contains("must stay within"), "{error}");
}

#[test]
fn cue_reports_a_missing_track_instead_of_exporting_only_the_sheet() {
    let root = fixture("missing");
    let cue = root.join("game.cue");
    fs::write(&cue, "FILE \"missing.bin\" BINARY\n").unwrap();

    let error = collect(&cue).unwrap_err();
    assert!(error.contains("missing track file"), "{error}");
    assert!(error.contains("missing.bin"), "{error}");
}

/// We refuse a playlist with a missing disc and name that disc in the
/// message, as we do for a cue.
#[test]
fn a_playlist_names_the_disc_it_is_missing() {
    let root = fixture("playlist-missing");
    let playlist = root.join("game.m3u");
    fs::write(&playlist, b"disc1.cue\n").unwrap();

    let refusal = collect(&playlist).expect_err("the disc is not there");
    assert!(refusal.contains("disc1.cue"), "{refusal}");
}

/// A Dreamcast multi-disc game is a playlist of GD-ROM layouts, and each
/// layout lists tracks. We follow both kinds of sheet in one pass, or we
/// would leave out the tracks of the second disc.
#[test]
fn a_playlist_of_gd_rom_layouts_collects_every_track() {
    let root = fixture("playlist-gdi");
    fs::write(root.join("track.bin"), b"data").unwrap();
    fs::write(root.join("game.gdi"), "1\n1 0 4 2352 track.bin 0\n").unwrap();
    let playlist = root.join("game.m3u");
    fs::write(&playlist, "game.gdi\n").unwrap();

    let names = relative_names(&collect(&playlist).unwrap());
    assert!(names.iter().any(|name| name == "game.m3u"), "{names:?}");
    assert!(names.iter().any(|name| name == "game.gdi"), "{names:?}");
    assert!(names.iter().any(|name| name == "track.bin"), "{names:?}");
}

fn relative_names(content: &ContentSet) -> Vec<String> {
    content
        .files
        .iter()
        .map(|file| file.relative.to_string_lossy().replace('\\', "/"))
        .collect()
}

/// The files that go with a dropped game, according to each console's
/// package. We list them here apart from the code that collects files,
/// so that we notice when a package stops declaring a sheet.
#[test]
fn every_console_declares_what_travels_with_its_games() {
    let sheets = |id: &str| -> Vec<String> {
        systems::find(id)
            .unwrap()
            .sheets
            .iter()
            .map(|sheet| format!("{}:{}", sheet.extension, sheet.parser.as_str()))
            .collect()
    };
    let required = |id: &str| -> Vec<String> {
        systems::find(id)
            .unwrap()
            .companions
            .iter()
            .filter(|companion| companion.required)
            .map(|companion| {
                format!(
                    "{}->{}",
                    companion.when.as_deref().unwrap_or("*"),
                    companion.extension
                )
            })
            .collect()
    };
    let optional = |id: &str| -> Vec<String> {
        systems::find(id)
            .unwrap()
            .companions
            .iter()
            .filter(|companion| !companion.required)
            .map(|companion| {
                format!(
                    "{}->{}",
                    companion.when.as_deref().unwrap_or("*"),
                    companion.extension
                )
            })
            .collect()
    };
    assert_eq!(sheets("dreamcast"), ["gdi:gdi", "m3u:playlist", "cue:cue"]);
    assert!(required("dreamcast").is_empty());
    assert_eq!(sheets("ps1"), ["cue:cue", "m3u:playlist", "toc:toc"]);
    assert_eq!(required("ps1"), ["ccd->img"]);
    assert_eq!(optional("ps1"), ["*->sbi", "ccd->sub"]);
    assert_eq!(sheets("pcecd"), ["cue:cue", "m3u:playlist", "toc:toc"]);
    assert_eq!(required("pcecd"), ["ccd->img", "ccd->sub"]);
    assert_eq!(optional("pcecd"), ["*->sbi"]);
    assert_eq!(sheets("segacd"), ["cue:cue", "m3u:playlist"]);
    assert!(required("segacd").is_empty() && optional("segacd").is_empty());
    assert_eq!(sheets("ps2"), ["cue:cue", "m3u:playlist"]);
    assert_eq!(sheets("gamecube"), ["m3u:playlist"]);
    for id in [
        "atari2600",
        "atari5200",
        "atari7800",
        "gamegear",
        "gb",
        "gba",
        "gbc",
        "lynx",
        "mastersystem",
        "megadrive",
        "n64",
        "neogeopocket",
        "neogeopocketcolor",
        "nes",
        "pce",
        "sg1000",
        "snes",
        "wonderswan",
        "wonderswancolor",
    ] {
        assert!(
            sheets(id).is_empty(),
            "{id} is a cartridge: {}",
            sheets(id).join(",")
        );
        assert!(required(id).is_empty() && optional(id).is_empty(), "{id}");
    }
    let declared: Vec<&str> = systems::registry()
        .iter()
        .flat_map(|system| system.recognize_only.iter().map(String::as_str))
        .collect();
    assert!(
        declared.is_empty(),
        "a sheet that is followed is not recognise-only: {declared:?}"
    );
    let mut ids: Vec<&str> = systems::registry()
        .iter()
        .map(|system| system.id.as_str())
        .collect();
    ids.sort_unstable();
    assert_eq!(
        ids,
        [
            "atari2600",
            "atari5200",
            "atari7800",
            "dreamcast",
            "gamecube",
            "gamegear",
            "gb",
            "gba",
            "gbc",
            "lynx",
            "mastersystem",
            "megadrive",
            "n64",
            "neogeopocket",
            "neogeopocketcolor",
            "nes",
            "pce",
            "pcecd",
            "ps1",
            "ps2",
            "segacd",
            "sg1000",
            "snes",
            "wonderswan",
            "wonderswancolor",
        ]
    );
}

/// When someone drops the folder or one track of a GD-ROM, we use the
/// layout, whose name does not contain `(Track 3)`. We collect the audio
/// track (type 0) too. Without every track, the game does not start.
#[test]
fn a_gd_rom_folder_track_and_layout_are_one_disc() {
    let root = fixture("gd-rom-folder");
    let tracks = [
        "Tiny Disc (Track 1).bin",
        "Tiny Disc (Track 2).bin",
        "Tiny Disc (Track 3).bin",
    ];
    for name in tracks {
        fs::write(root.join(name), b"track").unwrap();
    }
    let layout = root.join("Tiny Disc.gdi");
    fs::write(
        &layout,
        "3\n\
         1 0 4 2352 \"Tiny Disc (Track 1).bin\" 0\n\
         2 450 0 2352 \"Tiny Disc (Track 2).bin\" 0\n\
         3 2250 4 2352 \"Tiny Disc (Track 3).bin\" 0\n",
    )
    .unwrap();

    let from_folder = resolve_dropped(&root).unwrap();
    let from_layout = resolve_dropped(&layout).unwrap();
    let from_track = resolve_dropped(&root.join(tracks[2])).unwrap();
    assert_eq!(from_folder, layout, "dropping the folder must select the layout");
    assert_eq!(from_track, from_layout, "dropping the track is not dropping the layout");

    let names = relative_names(&collect(&from_layout).unwrap());
    assert!(names.iter().any(|name| name.ends_with(".gdi")), "{names:?}");
    for track in tracks {
        assert!(
            names.iter().any(|name| name == track),
            "{track} was not collected: {names:?}"
        );
    }
    assert_eq!(relative_names(&collect(&from_track).unwrap()), names);

    let from_track = crate::traveling::files_for(&root.join(tracks[2]), Some("dreamcast"))
        .unwrap();
    let from_layout = crate::traveling::files_for(&layout, Some("dreamcast")).unwrap();
    assert_eq!(from_track.entry, from_layout.entry);
    assert_eq!(from_track.files, from_layout.files);
}

/// When someone drops a folder, we use its layout. A multi-disc folder
/// also contains the discs the playlist lists, and we use the playlist.
#[test]
fn a_dropped_folder_resolves_to_the_playlist_not_one_disc() {
    let root = fixture("folder-playlist");
    fs::write(root.join("track.bin"), b"data").unwrap();
    fs::write(root.join("game.gdi"), "1\n1 0 4 2352 track.bin 0\n").unwrap();
    fs::write(root.join("game.m3u"), "game.gdi\n").unwrap();

    let resolved = resolve_dropped(&root).unwrap();
    assert_eq!(
        resolved.file_name().and_then(|name| name.to_str()),
        Some("game.m3u")
    );
    let names = relative_names(&collect(&resolved).unwrap());
    assert!(names.iter().any(|name| name == "track.bin"), "{names:?}");
}

#[test]
fn every_declared_sheet_is_followed_and_a_missing_file_is_named() {
    for system in systems::registry() {
        for sheet in &system.sheets {
            let root = fixture(&format!("{}-{}", system.id, sheet.extension));
            let entry = root.join(format!("game.{}", sheet.extension));
            fs::write(root.join("track.bin"), b"data").unwrap();
            fs::write(&entry, sheet_text(sheet.parser, "track.bin")).unwrap();
            let collected = collect_for(&entry, Some(&system.id))
                .unwrap_or_else(|error| panic!("{} {}: {error}", system.id, sheet.extension));
            let names = relative_names(&collected);
            assert!(
                names.iter().any(|name| name == "track.bin"),
                "{} {} did not bring track.bin: {names:?}",
                system.id,
                sheet.extension
            );

            let missing_root = fixture(&format!("{}-{}-missing", system.id, sheet.extension));
            let missing = missing_root.join(format!("game.{}", sheet.extension));
            fs::write(&missing, sheet_text(sheet.parser, "absent.bin")).unwrap();
            let error = collect_for(&missing, Some(&system.id)).unwrap_err();
            assert!(
                error.contains("absent.bin"),
                "{} {} did not name the missing file: {error}",
                system.id,
                sheet.extension
            );
        }
        for extension in &system.extensions {
            if system
                .sheets
                .iter()
                .any(|sheet| sheet.extension == *extension)
                || extension == "ccd"
            {
                continue;
            }
            let root = fixture(&format!("{}-bare-{extension}", system.id));
            let file = root.join(format!("game.{extension}"));
            fs::write(&file, b"data").unwrap();
            let collected = collect_for(&file, Some(&system.id)).unwrap_or_else(|error| {
                panic!("{} .{extension} is one file: {error}", system.id)
            });
            assert_eq!(
                collected.files.len(),
                1,
                "{} .{extension} collected {:?}",
                system.id,
                relative_names(&collected)
            );
        }
    }
}

/// A track file need not have the sheet's name. We use a cue with another
/// name for a dropped `track03.bin` when the text of the cue lists it.
#[test]
fn a_dropped_track_is_the_sheet_that_names_it() {
    let root = fixture("track03");
    let track = root.join("track03.bin");
    fs::write(&track, b"track").unwrap();
    let sheet = root.join("anything.cue");
    fs::write(&sheet, "FILE \"track03.bin\" BINARY\n  TRACK 03 AUDIO\n").unwrap();

    let resolved = resolve_dropped(&track).unwrap();
    assert_eq!(resolved, sheet, "the track was not the disc");
    assert_eq!(
        relative_names(&collect(&resolved).unwrap()),
        relative_names(&collect(&sheet).unwrap()),
        "dropping the track collected a different set than dropping the sheet"
    );
}

/// The common case of a cue and the bin it lists.
#[test]
fn a_dropped_bin_beside_its_cue_is_that_cue() {
    let root = fixture("cue-bin");
    let track = root.join("disc.bin");
    fs::write(&track, b"data").unwrap();
    let cue = root.join("game.cue");
    fs::write(&cue, "FILE \"disc.bin\" BINARY\n  TRACK 01 MODE1/2352\n").unwrap();

    assert_eq!(resolve_dropped(&track).unwrap(), cue);
}

/// A CHD does not contain the subchannel file of a game such as Ape
/// Escape. In the PlayStation package we declare that a `.sbi` goes with
/// the disc of the same name, as if a sheet named it as a track.
#[test]
fn a_subchannel_file_beside_its_disc_is_that_disc() {
    let root = fixture("sbi");
    let disc = root.join("Ape Escape.chd");
    let subchannel = root.join("Ape Escape.sbi");
    fs::write(&disc, b"not a real disc").unwrap();
    fs::write(&subchannel, b"subchannel").unwrap();

    assert_eq!(resolve_dropped(&subchannel).unwrap(), disc);
}

/// We use the `.ccd` as the CloneCD image. The `.img` is a companion listed
/// in the package, and we do not treat it as a cartridge in the folder.
#[test]
fn a_clonecd_image_beside_its_sheet_is_that_sheet() {
    let root = fixture("ccd-img");
    let sheet = root.join("game.ccd");
    let image = root.join("game.img");
    fs::write(&sheet, b"[CloneCD]\n").unwrap();
    fs::write(&image, b"data").unwrap();

    assert_eq!(resolve_dropped(&image).unwrap(), sheet);
}

/// Mega Drive dumps and disc tracks both often end in `.bin`. When no
/// sheet in the folder lists a dropped `.bin`, we use that file itself.
#[test]
fn a_bin_beside_an_unrelated_sheet_stays_that_file() {
    let root = fixture("loose-bin");
    let cartridge = root.join("Sonic.bin");
    fs::write(&cartridge, b"SEGA").unwrap();
    fs::write(root.join("notes.gdi"), "1\n1 0 4 2352 \"other.bin\" 0\n").unwrap();

    assert_eq!(resolve_dropped(&cartridge).unwrap(), cartridge);
}

/// When two sheets name one track, we report both and the author chooses
/// between them.
#[test]
fn two_sheets_that_name_one_file_ask_for_one() {
    let root = fixture("two-sheets");
    let track = root.join("track.bin");
    fs::write(&track, b"data").unwrap();
    fs::write(
        root.join("Alpha.cue"),
        "FILE \"track.bin\" BINARY\n  TRACK 01 MODE1/2352\n",
    )
    .unwrap();
    fs::write(root.join("Beta.gdi"), "1\n1 0 4 2352 \"track.bin\" 0\n").unwrap();

    let resolved = resolve_dropped(&track);
    let Err(error) = resolved else {
        panic!(
            "two sheets name the track but it resolved to {}",
            resolved.unwrap().display()
        );
    };
    assert!(error.contains("Alpha.cue"), "{error}");
    assert!(error.contains("Beta.gdi"), "{error}");
    assert!(
        error.to_ascii_lowercase().contains("drop"),
        "the error does not ask for one of them: {error}"
    );
}

/// The playlist lists the layout, and the layout lists the track. We use
/// the playlist for a dropped track, so the player can change discs.
#[test]
fn a_track_is_the_playlist_that_names_the_sheet_that_names_it() {
    let root = fixture("track-playlist");
    let track = root.join("track.bin");
    fs::write(&track, b"data").unwrap();
    let gdi = root.join("game.gdi");
    fs::write(&gdi, "1\n1 0 4 2352 \"track.bin\" 0\n").unwrap();
    let playlist = root.join("game.m3u");
    fs::write(&playlist, "game.gdi\n").unwrap();

    assert_eq!(
        resolve_dropped(&track).unwrap(),
        playlist,
        "dropping the track is not dropping the playlist"
    );
}

/// A multi-disc game of three cue/bin pairs and a playlist that lists the
/// three cues. Dropping the playlist, a cue or a track of another disc
/// gives the same entry and the same files.
#[test]
fn dropping_any_file_of_a_multi_disc_game_is_the_playlist() {
    let root = fixture("final-fantasy-vii");
    let playlist = root.join("Final Fantasy VII.m3u");
    let mut lines = Vec::new();
    for disc in 1..=3 {
        let cue_name = format!("Final Fantasy VII (Disc {disc}).cue");
        let bin_name = format!("Final Fantasy VII (Disc {disc}).bin");
        fs::write(root.join(&bin_name), b"data").unwrap();
        fs::write(
            root.join(&cue_name),
            format!("FILE \"{bin_name}\" BINARY\n  TRACK 01 MODE1/2352\n"),
        )
        .unwrap();
        lines.push(cue_name);
    }
    fs::write(&playlist, lines.join("\n") + "\n").unwrap();

    let drops = [
        playlist.clone(),
        root.join("Final Fantasy VII (Disc 2).cue"),
        root.join("Final Fantasy VII (Disc 3).bin"),
    ];
    let listed: Vec<_> = drops
        .iter()
        .map(|dropped| {
            let resolved = resolve_dropped(dropped).unwrap();
            let set = collect(&resolved).unwrap();
            let files = relative_names(&set);
            (resolved, set.entrypoint, files)
        })
        .collect();
    assert_eq!(listed[0].0, playlist, "dropping the playlist");
    assert_eq!(listed[1].0, listed[0].0, "dropping disc 2's cue");
    assert_eq!(listed[2].0, listed[0].0, "dropping disc 3's track");
    assert_eq!(
        listed[1].1, listed[0].1,
        "disc 2's cue is a different entry"
    );
    assert_eq!(
        listed[2].1, listed[0].1,
        "disc 3's track is a different entry"
    );
    assert_eq!(
        listed[1].2, listed[0].2,
        "disc 2's cue collects different files"
    );
    assert_eq!(
        listed[2].2, listed[0].2,
        "disc 3's track collects different files"
    );
    assert_eq!(collect(&playlist).unwrap().discs, 3, "the playlist names three discs");
    assert_eq!(
        collect(&root.join("Final Fantasy VII (Disc 2).cue")).unwrap().discs,
        1,
        "one sheet on its own is one disc"
    );
    for name in [
        "Final Fantasy VII.m3u",
        "Final Fantasy VII (Disc 1).cue",
        "Final Fantasy VII (Disc 1).bin",
        "Final Fantasy VII (Disc 2).cue",
        "Final Fantasy VII (Disc 2).bin",
        "Final Fantasy VII (Disc 3).cue",
        "Final Fantasy VII (Disc 3).bin",
    ] {
        assert!(
            listed[0].2.iter().any(|file| file == name),
            "{name} is not in the game: {:?}",
            listed[0].2
        );
    }
}

/// Two playlists name the sheet. Whether someone drops the track or the
/// sheet, we ask them to choose one playlist, in the same words.
#[test]
fn two_playlists_that_name_one_sheet_ask_for_one() {
    let root = fixture("two-playlists");
    let track = root.join("track.bin");
    fs::write(&track, b"data").unwrap();
    let cue = root.join("game.cue");
    fs::write(&cue, "FILE \"track.bin\" BINARY\n  TRACK 01 MODE1/2352\n").unwrap();
    fs::write(root.join("Alpha.m3u"), "game.cue\n").unwrap();
    fs::write(root.join("Beta.m3u"), "game.cue\n").unwrap();

    let from_track = resolve_dropped(&track);
    let Err(from_track) = from_track else {
        panic!(
            "two playlists name the sheet but the track resolved to {}",
            from_track.unwrap().display()
        );
    };
    let from_sheet = resolve_dropped(&cue).expect_err("two playlists name the sheet");
    assert_eq!(from_track, from_sheet);
    assert!(from_track.contains("Alpha.m3u"), "{from_track}");
    assert!(from_track.contains("Beta.m3u"), "{from_track}");
    assert!(from_track.contains("game.cue"), "{from_track}");
    assert!(
        from_track.to_ascii_lowercase().contains("drop"),
        "the error does not ask for one of them: {from_track}"
    );
}

/// We do not read a playlist past the sheet size limit, so we use the
/// sheet that lists the track.
#[test]
fn a_playlist_larger_than_the_limit_does_not_take_the_sheet() {
    let root = fixture("huge-playlist");
    let track = root.join("track03.bin");
    fs::write(&track, b"track").unwrap();
    let cue = root.join("game.cue");
    fs::write(&cue, "FILE \"track03.bin\" BINARY\n  TRACK 03 AUDIO\n").unwrap();
    fs::write(
        root.join("game.m3u"),
        text_of_length("game.cue\n", sheet_bytes(SHEET_BYTE_LIMIT) + 1),
    )
    .unwrap();

    assert_eq!(
        resolve_dropped(&track).unwrap(),
        cue,
        "a playlist past the size limit was read"
    );
}

/// We still read a playlist whose size is exactly the limit, and use it.
#[test]
fn a_playlist_at_the_size_limit_takes_the_sheet() {
    let root = fixture("playlist-at-limit");
    let track = root.join("track03.bin");
    fs::write(&track, b"track").unwrap();
    fs::write(
        root.join("game.cue"),
        "FILE \"track03.bin\" BINARY\n  TRACK 03 AUDIO\n",
    )
    .unwrap();
    let playlist = root.join("game.m3u");
    fs::write(
        &playlist,
        text_of_length("game.cue\n", sheet_bytes(SHEET_BYTE_LIMIT)),
    )
    .unwrap();

    assert_eq!(resolve_dropped(&track).unwrap(), playlist);
}

/// The playlist is a link to a file in the same folder. We treat a link
/// that resolves inside the folder as a sheet, as when collecting.
#[cfg(unix)]
#[test]
fn a_symlinked_playlist_inside_the_folder_takes_the_sheet() {
    let root = fixture("playlist-symlink-inside");
    let track = root.join("track.bin");
    fs::write(&track, b"data").unwrap();
    fs::write(
        root.join("game.cue"),
        "FILE \"track.bin\" BINARY\n  TRACK 01 MODE1/2352\n",
    )
    .unwrap();
    let body = root.join("body");
    fs::write(&body, "game.cue\n").unwrap();
    let playlist = root.join("game.m3u");
    std::os::unix::fs::symlink(&body, &playlist).unwrap();

    assert_eq!(
        resolve_dropped(&track).unwrap(),
        playlist,
        "a playlist linked inside the folder was not read"
    );
}

/// A link to a file in another folder is not a playlist in this folder.
#[cfg(unix)]
#[test]
fn a_symlinked_playlist_outside_the_folder_is_not_read() {
    let root = fixture("playlist-symlink-outside");
    let elsewhere = fixture("playlist-symlink-elsewhere");
    let track = root.join("track.bin");
    fs::write(&track, b"data").unwrap();
    let cue = root.join("game.cue");
    fs::write(&cue, "FILE \"track.bin\" BINARY\n  TRACK 01 MODE1/2352\n").unwrap();
    let body = elsewhere.join("body");
    fs::write(&body, "game.cue\n").unwrap();
    std::os::unix::fs::symlink(&body, root.join("game.m3u")).unwrap();

    assert_eq!(
        resolve_dropped(&track).unwrap(),
        cue,
        "a playlist linked outside the folder was read"
    );
}

/// A playlist in the parent folder can have the sheet's filename and
/// still be another game. We read only the folder of the dropped file.
#[test]
fn a_playlist_outside_the_dropped_files_folder_does_not_take_the_sheet() {
    let parent = fixture("playlist-parent");
    let root = parent.join("disc");
    fs::create_dir(&root).unwrap();
    let track = root.join("track.bin");
    fs::write(&track, b"data").unwrap();
    let cue = root.join("game.cue");
    fs::write(&cue, "FILE \"track.bin\" BINARY\n  TRACK 01 MODE1/2352\n").unwrap();
    fs::write(parent.join("game.m3u"), "game.cue\n").unwrap();

    assert_eq!(
        resolve_dropped(&track).unwrap(),
        cue,
        "a playlist outside the folder took the drop"
    );
}

/// A folder with more playlists than the cap. The playlists that sort
/// first do not name the sheet, and the one that does has its name. In
/// directory order we would reach the cap before it.
#[test]
fn the_playlist_that_names_a_sheet_is_found_among_more_decoys_than_the_cap() {
    let root = fixture("playlist-cap");
    let track = root.join("Sonic (Track 3).bin");
    fs::write(&track, b"track").unwrap();
    fs::write(
        root.join("Sonic.cue"),
        "FILE \"Sonic (Track 3).bin\" BINARY\n  TRACK 03 AUDIO\n",
    )
    .unwrap();
    let playlist = root.join("Sonic.m3u");
    fs::write(&playlist, "Sonic.cue\n").unwrap();
    for index in 0..SHEET_MATCH_LIMIT {
        let decoy = format!("Sonic (Track 3) extra {index:03}.m3u");
        fs::write(root.join(decoy), "nobody.cue\n").unwrap();
    }

    assert_eq!(
        resolve_dropped(&track).unwrap(),
        playlist,
        "the playlist that names the sheet was not chosen"
    );
}

/// A folder with more sheets than the cap. The sheets that sort first do
/// not name the track. The one that does has the track's name without
/// `(Track 3)`. In directory order we would reach the cap before it.
#[test]
fn the_sheet_that_names_a_track_is_found_among_more_decoys_than_the_cap() {
    let root = fixture("sheet-cap");
    let track = root.join("Sonic (Track 3).bin");
    fs::write(&track, b"track").unwrap();
    let sheet = root.join("Sonic.gdi");
    fs::write(&sheet, "1\n1 0 4 2352 \"Sonic (Track 3).bin\" 0\n").unwrap();
    for index in 0..SHEET_MATCH_LIMIT {
        let decoy = format!("Sonic (Track 3) extra {index:03}.gdi");
        fs::write(root.join(decoy), "1\n1 0 4 2352 \"nobody.bin\" 0\n").unwrap();
    }

    assert_eq!(
        resolve_dropped(&track).unwrap(),
        sheet,
        "the sheet that names the track was not chosen"
    );
}

/// A disc image renamed to `.cue` is not a sheet. We do not read a file past
/// the limit, so we never take such an image into the drop.
#[test]
fn a_sheet_larger_than_the_limit_does_not_name_the_track() {
    let root = fixture("huge-sheet");
    let track = root.join("track03.bin");
    fs::write(&track, b"track").unwrap();
    let sheet = root.join("game.cue");
    fs::write(
        &sheet,
        cue_of_length("track03.bin", sheet_bytes(SHEET_BYTE_LIMIT) + 1),
    )
    .unwrap();

    assert_eq!(
        resolve_dropped(&track).unwrap(),
        track,
        "a sheet past the size limit was read"
    );
}

/// We still read a sheet whose size is exactly the limit.
#[test]
fn a_sheet_at_the_size_limit_names_the_track() {
    let root = fixture("sheet-at-limit");
    let track = root.join("track03.bin");
    fs::write(&track, b"track").unwrap();
    let sheet = root.join("game.cue");
    fs::write(
        &sheet,
        cue_of_length("track03.bin", sheet_bytes(SHEET_BYTE_LIMIT)),
    )
    .unwrap();

    assert_eq!(resolve_dropped(&track).unwrap(), sheet);
}

/// The cue is a link to a file in the same folder. We follow such a link
/// when collecting, so we also read that sheet when someone drops a track.
#[cfg(unix)]
#[test]
fn a_symlinked_sheet_inside_the_folder_names_the_track() {
    let root = fixture("symlink-inside");
    let track = root.join("track03.bin");
    fs::write(&track, b"track").unwrap();
    let body = root.join("body");
    fs::write(&body, "FILE \"track03.bin\" BINARY\n  TRACK 03 AUDIO\n").unwrap();
    let sheet = root.join("game.cue");
    std::os::unix::fs::symlink(&body, &sheet).unwrap();

    assert_eq!(
        resolve_dropped(&track).unwrap(),
        sheet,
        "a sheet linked inside the folder was not read"
    );
}

/// A link to a file in another folder is not a sheet in this folder.
#[cfg(unix)]
#[test]
fn a_symlinked_sheet_outside_the_folder_is_not_read() {
    let root = fixture("symlink-outside");
    let elsewhere = fixture("symlink-elsewhere");
    let track = root.join("track03.bin");
    fs::write(&track, b"track").unwrap();
    let body = elsewhere.join("body");
    fs::write(&body, "FILE \"track03.bin\" BINARY\n  TRACK 03 AUDIO\n").unwrap();
    let sheet = root.join("game.cue");
    std::os::unix::fs::symlink(&body, &sheet).unwrap();

    assert_eq!(
        resolve_dropped(&track).unwrap(),
        track,
        "a sheet linked outside the folder was read"
    );
}

#[test]
fn a_clonecd_sheet_refuses_the_sibling_its_core_requires() {
    let root = fixture("pce-ccd");
    fs::write(root.join("game.ccd"), b"[CloneCD]\n").unwrap();
    fs::write(root.join("game.img"), b"data").unwrap();
    let missing = collect_for(&root.join("game.ccd"), Some("pcecd")).unwrap_err();
    assert!(missing.contains("game.sub"), "{missing}");

    fs::write(root.join("game.sub"), b"sub").unwrap();
    let names = relative_names(&collect_for(&root.join("game.ccd"), Some("pcecd")).unwrap());
    for name in ["game.ccd", "game.img", "game.sub"] {
        assert!(names.iter().any(|found| found == name), "{names:?}");
    }

    // A CloneCD image runs in PCSX without the subchannel file but not
    // in Beetle. We declare that difference in the console packages.
    let ps1 = fixture("ps1-ccd");
    fs::write(ps1.join("game.ccd"), b"[CloneCD]\n").unwrap();
    fs::write(ps1.join("game.img"), b"data").unwrap();
    let names = relative_names(&collect_for(&ps1.join("game.ccd"), Some("ps1")).unwrap());
    assert!(names.iter().any(|name| name == "game.img"), "{names:?}");
    assert!(
        !names.iter().any(|name| name.ends_with(".sub")),
        "{names:?}"
    );
}

fn sheet_bytes(limit: u64) -> usize {
    usize::try_from(limit).expect("the sheet limit fits in a file length")
}

fn cue_of_length(track: &str, bytes: usize) -> String {
    text_of_length(
        &format!("FILE \"{track}\" BINARY\n  TRACK 01 MODE1/2352\n"),
        bytes,
    )
}

fn text_of_length(head: &str, bytes: usize) -> String {
    assert!(
        head.len() <= bytes,
        "sheet text is longer than the size under test"
    );
    let mut text = head.to_owned();
    text.extend(std::iter::repeat('\n').take(bytes - text.len()));
    text
}

fn sheet_text(parser: SheetParser, name: &str) -> String {
    match parser {
        SheetParser::Cue => format!("FILE \"{name}\" BINARY\n  TRACK 01 MODE1/2352\n"),
        SheetParser::Gdi => format!("1\n1 0 4 2352 {name} 0\n"),
        SheetParser::Playlist => format!("{name}\n"),
        SheetParser::Toc => format!("CD_ROM\nTRACK MODE1_RAW\nDATAFILE \"{name}\" 0\n"),
    }
}
