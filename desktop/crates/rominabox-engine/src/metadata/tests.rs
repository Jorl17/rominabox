use super::*;
use std::io::Write;

fn fixture_directory(label: &str) -> rominabox_scratch::Scratch {
    rominabox_scratch::Scratch::dir(&format!("rominabox-metadata-{label}"))
}

#[test]
fn cue_requires_an_explicit_system_and_accepts_a_valid_override() {
    let root = fixture_directory("cue");
    let rom = root.join("game.cue");
    fs::write(&rom, b"FILE \"track.bin\" BINARY\n").unwrap();

    let unresolved = inspect_game(&rom, &root.join("cache"), false).unwrap();
    assert_eq!(unresolved.system, "");
    assert!(unresolved.warnings[0].contains("multiple systems"));

    let selected =
        inspect_game_with_system(&rom, &root.join("cache"), false, Some("Sega CD")).unwrap();
    assert_eq!(selected.system, "segacd");
}

#[test]
fn public_inspection_matches_headerless_nes_catalog_data() {
    let root = fixture_directory("ines");
    let rom = root.join("fixture.nes");
    let mut bytes = b"NES\x1a\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00".to_vec();
    bytes.extend([1, 2, 3, 4]);
    fs::write(&rom, bytes).unwrap();

    let cache = root.join("cache");
    let catalog = cache
        .join("catalogs")
        .join("Nintendo - Nintendo Entertainment System.dat");
    fs::create_dir_all(catalog.parent().unwrap()).unwrap();
    fs::write(
        catalog,
        r#"clrmamepro (
  name "fixture"
  description "fixture"
)
game (
  name "Tiny Adventure (USA)"
  description "A generated test fixture"
  rom ( name "fixture.nes" size 4 crc B63CFBCD sha1 12dada1fff4d4787ade3333147202c3b443e376f )
)
"#,
    )
    .unwrap();

    let inspection = inspect_game(&rom, &cache, false).unwrap();
    assert_eq!(inspection.system, "nes");
    assert!(inspection.matched);
    assert_eq!(inspection.title, "Tiny Adventure");
    assert_eq!(
        inspection.catalog_name.as_deref(),
        Some("Tiny Adventure (USA)")
    );
}

#[test]
fn known_headers_can_identify_generic_binary_files() {
    let root = fixture_directory("headers");
    let rom = root.join("upload.bin");
    let mut bytes = vec![0; 512];
    bytes[..4].copy_from_slice(&[0x80, 0x37, 0x12, 0x40]);
    bytes[0x20..0x2B].copy_from_slice(b"MARIO TEST ");
    fs::write(&rom, bytes).unwrap();

    let inspection = inspect_game(&rom, &root.join("cache"), false).unwrap();
    assert_eq!(inspection.system, "n64");
    assert_eq!(inspection.title, "MARIO TEST");

    let fake_game_boy = root.join("not-a-rom.gb");
    fs::write(&fake_game_boy, vec![0; 512]).unwrap();
    let inspection = inspect_game(&fake_game_boy, &root.join("cache"), false).unwrap();
    assert_eq!(inspection.system, "");
    assert!(inspection.warnings[0].contains("checksum is invalid"));
}

/// We declare in package data, for each console, where the title is in a
/// cartridge header (the title window), and do not branch here. These
/// fixtures fix the title we read from that window in a dropped ROM.
mod declared_header_titles {
    use super::*;

    /// A ROM whose header contains `title` at `offset`, padded to `size`.
    fn cartridge(root: &Path, name: &str, offset: usize, title: &str, size: usize) -> PathBuf {
        let mut bytes = vec![0u8; size];
        bytes[offset..offset + title.len()].copy_from_slice(title.as_bytes());
        let rom = root.join(name);
        fs::write(&rom, bytes).unwrap();
        rom
    }

    #[test]
    fn a_mega_drive_title_is_read_from_its_declared_window() {
        let root = fixture_directory("header-md");
        // The domestic title is at 0x150 in the Mega Drive header.
        let rom = cartridge(&root, "game.md", 0x150, "SONIC THE HEDGEHOG", 0x400);
        let inspection = inspect_game(&rom, &root.join("cache"), false).unwrap();
        assert_eq!(inspection.system, "megadrive");
        assert!(
            matches!(inspection.source, MetadataSource::Header),
            "the title should come from the header, not the filename"
        );
        assert_eq!(inspection.title, "SONIC THE HEDGEHOG");
    }

    #[test]
    fn a_game_boy_title_is_read_from_its_declared_window() {
        let root = fixture_directory("header-gb");
        // A .gb file can be for Game Boy or Game Boy Color, so we check for
        // the boot logo at 0x104 before we use the header.
        let mut bytes = vec![0u8; 0x200];
        bytes[0x104..0x134].copy_from_slice(NINTENDO_LOGO);
        bytes[0x134..0x134 + 6].copy_from_slice(b"TETRIS");
        // We also verify the header checksum at 0x14D, which every
        // genuine cartridge has, so the fixture has one too.
        bytes[0x14D] = bytes[0x134..=0x14C].iter().fold(0_u8, |checksum, byte| {
            checksum.wrapping_sub(*byte).wrapping_sub(1)
        });
        let rom = root.join("game.gb");
        fs::write(&rom, bytes).unwrap();
        let inspection = inspect_game(&rom, &root.join("cache"), false).unwrap();
        assert_eq!(inspection.system, "gb");
        assert!(matches!(inspection.source, MetadataSource::Header));
        assert_eq!(inspection.title, "TETRIS");
    }

    #[test]
    fn a_console_declaring_no_header_window_falls_back_to_the_filename() {
        let root = fixture_directory("header-none");
        // The Master System header has no title field, so we declare none
        // in its package and use the filename.
        let rom = cartridge(&root, "Wonder Boy.sms", 0x10, "NOTATITLE", 0x200);
        let inspection = inspect_game(&rom, &root.join("cache"), false).unwrap();
        assert_eq!(inspection.system, "mastersystem");
        assert!(
            matches!(inspection.source, MetadataSource::Filename),
            "nothing should be read out of a console that declares no window"
        );
        assert_eq!(inspection.title, "Wonder Boy");
    }

    /// "Sonic Advance" does not fit in a twelve-character GBA title, so
    /// we keep that filename instead of the header title.
    #[test]
    fn a_good_filename_beats_a_truncated_header_title() {
        let root = fixture_directory("gba-filename");
        let mut bytes = vec![0u8; 0x100];
        bytes[4..8].copy_from_slice(&[0x24, 0xff, 0xae, 0x51]);
        bytes[0xB2] = 0x96;
        bytes[0xA0..0xAC].copy_from_slice(b"SONIC ADVANC");
        let rom = root.join("Sonic Advance (Europe).gba");
        fs::write(&rom, &bytes).unwrap();
        let inspection = inspect_game(&rom, &root.join("cache"), false).unwrap();
        assert_eq!(inspection.system, "gba");
        assert_eq!(inspection.title, "Sonic Advance (Europe)");
        assert!(
            matches!(inspection.source, MetadataSource::Filename),
            "a truncated header title replaced the filename: {}",
            inspection.title
        );
    }

    /// We declare the title window of every console in package data.
    #[test]
    fn the_consoles_that_had_hardcoded_windows_still_declare_them() {
        for (id, offset, length) in [
            ("megadrive", 0x150, 0x180 - 0x150),
            ("gb", 0x134, 0x143 - 0x134),
            ("gbc", 0x134, 0x143 - 0x134),
            ("gba", 0xA0, 0xAC - 0xA0),
            ("n64", 0x20, 0x34 - 0x20),
            ("atari7800", 17, 49 - 17),
        ] {
            let windows = &crate::systems::find(id)
                .expect("known console")
                .header_title;
            assert_eq!(windows.len(), 1, "{id}");
            let window = &windows[0];
            assert!(window.anchor.is_none(), "{id}");
            assert_eq!((window.offset, window.length), (offset, length), "{id}");
        }
    }
}

/// Consoles with the name of the game in the cartridge or disc header. A
/// file called `game` gives no name, so the name must come from the header.
mod headers_the_filename_does_not_have {
    use super::*;

    fn write_snes_header(bytes: &mut [u8], offset: usize, title: &str, valid: bool) {
        let end = offset + title.len();
        bytes[offset..end].copy_from_slice(title.as_bytes());
        // The checksum and its complement sum to 0xFFFF only in the genuine
        // header. The same area in the other mapping often has ASCII too, so
        // text that looks like a title is not enough.
        let pair = if valid {
            [0x00, 0x00, 0xFF, 0xFF]
        } else {
            [0x00, 0x00, 0x00, 0x00]
        };
        bytes[offset + 0x1C..offset + 0x20].copy_from_slice(&pair);
    }

    #[test]
    fn a_lynx_header_names_a_file_that_does_not() {
        let root = fixture_directory("lynx");
        let mut bytes = vec![0u8; 64];
        bytes[..4].copy_from_slice(b"LYNX");
        bytes[10..26].copy_from_slice(b"CALIFORNIA GAMES");
        let rom = root.join("game.lnx");
        fs::write(&rom, bytes).unwrap();
        let inspection = inspect_game(&rom, &root.join("cache"), false).unwrap();
        assert_eq!(inspection.system, "lynx");
        assert_eq!(inspection.title, "CALIFORNIA GAMES");
        assert!(matches!(inspection.source, MetadataSource::Header));
    }

    #[test]
    fn a_super_nintendo_lorom_title_is_read_from_the_cartridge() {
        let root = fixture_directory("snes-lo");
        let mut bytes = vec![0u8; 0x10000];
        write_snes_header(&mut bytes, 0x7FC0, "SUPER MARIOWORLD", true);
        let rom = root.join("game.sfc");
        fs::write(&rom, bytes).unwrap();
        let inspection = inspect_game(&rom, &root.join("cache"), false).unwrap();
        assert_eq!(inspection.system, "snes");
        assert_eq!(inspection.title, "SUPER MARIOWORLD");
        assert!(matches!(inspection.source, MetadataSource::Header));
    }

    #[test]
    fn a_super_nintendo_hirom_slot_is_not_mistaken_for_the_lorom_one() {
        let root = fixture_directory("snes-hi");
        let mut bytes = vec![0u8; 0x10000];
        write_snes_header(&mut bytes, 0x7FC0, "NOT THE TITLE HERE", false);
        write_snes_header(&mut bytes, 0xFFC0, "CHRONO TRIGGER", true);
        let rom = root.join("game.sfc");
        fs::write(&rom, bytes).unwrap();
        let inspection = inspect_game(&rom, &root.join("cache"), false).unwrap();
        assert_eq!(inspection.system, "snes");
        assert_eq!(inspection.title, "CHRONO TRIGGER");
    }

    #[test]
    fn a_super_nintendo_copier_header_does_not_shift_the_title() {
        let root = fixture_directory("snes-copier");
        let mut cartridge = vec![0u8; 0x10000];
        write_snes_header(&mut cartridge, 0x7FC0, "SUPER MARIOWORLD", true);
        let mut file = vec![0xAA; 512];
        file.extend(cartridge);
        assert_eq!(file.len() % 1024, 512);
        let rom = root.join("game.sfc");
        fs::write(&rom, file).unwrap();
        let inspection = inspect_game(&rom, &root.join("cache"), false).unwrap();
        assert_eq!(inspection.title, "SUPER MARIOWORLD");
    }

    #[test]
    fn a_gamecube_disc_header_names_the_game() {
        let root = fixture_directory("gc");
        let mut bytes = vec![0u8; 0x80];
        bytes[0x1C..0x20].copy_from_slice(&[0xC2, 0x33, 0x9F, 0x3D]);
        bytes[0x20..0x34].copy_from_slice(b"SUPER MARIO SUNSHINE");
        let rom = root.join("game.gcm");
        fs::write(&rom, bytes).unwrap();
        let inspection = inspect_game(&rom, &root.join("cache"), false).unwrap();
        assert_eq!(inspection.system, "gamecube");
        assert_eq!(inspection.title, "SUPER MARIO SUNSHINE");
        assert!(matches!(inspection.source, MetadataSource::Header));
    }

    #[test]
    fn a_dreamcast_ip_names_a_gd_rom_whose_filename_does_not() {
        let root = fixture_directory("dc");
        let mut image = vec![0u8; 0x100];
        image[..15].copy_from_slice(b"SEGA SEGAKATANA");
        image[0x40..0x4A].copy_from_slice(b"MK-5111750");
        image[0x80..0x91].copy_from_slice(b"SONIC ADVENTURE 2");
        fs::write(root.join("track.bin"), &image).unwrap();
        let gdi = root.join("game.gdi");
        fs::write(&gdi, "1\n1 0 4 2352 \"track.bin\" 0\n").unwrap();
        let inspection = inspect_game(&gdi, &root.join("cache"), false).unwrap();
        assert_eq!(inspection.system, "dreamcast");
        assert_eq!(inspection.title, "SONIC ADVENTURE 2");
        assert!(matches!(inspection.source, MetadataSource::Header));
    }

    #[test]
    fn a_dreamcast_filename_that_already_names_the_game_is_kept() {
        let root = fixture_directory("dc-named");
        let mut image = vec![0u8; 0x100];
        image[..15].copy_from_slice(b"SEGA SEGAKATANA");
        image[0x40..0x4A].copy_from_slice(b"MK-5111750");
        image[0x80..0x91].copy_from_slice(b"SONIC ADVENTURE 2");
        let track = "Sonic Adventure 2 (Europe) (Track 1).bin";
        fs::write(root.join(track), &image).unwrap();
        let gdi = root.join("Sonic Adventure 2 (Europe).gdi");
        fs::write(&gdi, format!("1\n1 0 4 2352 \"{track}\" 0\n")).unwrap();
        let inspection = inspect_game(&gdi, &root.join("cache"), false).unwrap();
        assert_eq!(inspection.system, "dreamcast");
        assert_eq!(inspection.title, "Sonic Adventure 2 (Europe)");
        assert!(matches!(inspection.source, MetadataSource::Filename));
    }

    /// We keep a longer filename instead of a twelve- or fifteen-character
    /// header title. Game Boy Advance and Dreamcast have separate fixtures
    /// for this, and here we test the other title windows.
    #[test]
    fn a_longer_filename_is_not_replaced_by_a_shorter_header() {
        let root = fixture_directory("named-headers");
        let cache = root.join("cache");
        let mut cases: Vec<(PathBuf, &str, &str)> = Vec::new();

        cases.push((
            game_boy(&root, "Palette Demo (USA, Europe).gbc", "PALETTE DEMO", true),
            "gbc",
            "Palette Demo (USA, Europe)",
        ));
        cases.push((
            game_boy(&root, "Palette Demo (USA).gb", "PALETTE DEMO", false),
            "gb",
            "Palette Demo (USA)",
        ));

        let mut megadrive = vec![0u8; 0x200];
        megadrive[0x100..0x104].copy_from_slice(b"SEGA");
        megadrive[0x150..0x15B].copy_from_slice(b"VECTOR DEMO");
        let megadrive_path = root.join("Vector Demo (World).md");
        fs::write(&megadrive_path, megadrive).unwrap();
        cases.push((megadrive_path, "megadrive", "Vector Demo (World)"));

        let mut snes = vec![0u8; 0x10000];
        write_snes_header(&mut snes, 0x7FC0, "SUPER DEMOWORLD", true);
        let snes_path = root.join("Super Demo World (USA).sfc");
        fs::write(&snes_path, snes).unwrap();
        cases.push((snes_path, "snes", "Super Demo World (USA)"));

        // The NES has no title window, so we use the filename as the name.
        let mut nes = b"NES\x1a\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00".to_vec();
        nes.extend([1, 2, 3, 4]);
        let nes_path = root.join("Nest Demo (USA).nes");
        fs::write(&nes_path, nes).unwrap();
        cases.push((nes_path, "nes", "Nest Demo (USA)"));

        for (path, system, title) in cases {
            let inspection = inspect_game(&path, &cache, false).unwrap();
            assert_eq!(inspection.system, system, "{}", path.display());
            assert_eq!(inspection.title, title, "{}", path.display());
            assert!(
                matches!(inspection.source, MetadataSource::Filename),
                "{} took {} from {:?}",
                path.display(),
                inspection.title,
                inspection.source
            );
        }
    }

    fn game_boy(root: &Path, name: &str, title: &str, color: bool) -> PathBuf {
        let mut bytes = vec![0u8; 0x200];
        bytes[0x104..0x134].copy_from_slice(NINTENDO_LOGO);
        bytes[0x134..0x134 + title.len()].copy_from_slice(title.as_bytes());
        if color {
            bytes[0x143] = 0x80;
        }
        bytes[0x14D] = bytes[0x134..=0x14C]
            .iter()
            .fold(0_u8, |checksum, byte| checksum.wrapping_sub(*byte).wrapping_sub(1));
        let rom = root.join(name);
        fs::write(&rom, &bytes).unwrap();
        rom
    }

    #[test]
    fn a_sega_cd_header_names_a_disc_whose_filename_does_not() {
        let root = fixture_directory("mcd");
        let mut image = vec![0u8; 0x200];
        image[..14].copy_from_slice(b"SEGADISCSYSTEM");
        image[0x150..0x158].copy_from_slice(b"SONIC CD");
        image[0x180..0x187].copy_from_slice(b"T-93175");
        fs::write(root.join("track.bin"), &image).unwrap();
        let cue = root.join("game.cue");
        fs::write(&cue, "FILE \"track.bin\" BINARY\n  TRACK 01 MODE1/2352\n").unwrap();
        let inspection = inspect_game(&cue, &root.join("cache"), false).unwrap();
        assert_eq!(inspection.system, "segacd");
        assert_eq!(inspection.title, "SONIC CD");
        assert!(matches!(inspection.source, MetadataSource::Header));
    }
}

#[test]
fn a_zipped_rom_is_identified_from_the_game_inside() {
    let root = fixture_directory("zip-nes");
    let archive_path = root.join("game.zip");
    let file = std::fs::File::create(&archive_path).unwrap();
    let mut writer = zip::ZipWriter::new(file);
    writer
        .start_file("readme.txt", zip::write::SimpleFileOptions::default())
        .unwrap();
    writer.write_all(b"notes").unwrap();
    let mut bytes = b"NES\x1a\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00".to_vec();
    bytes.extend([1, 2, 3, 4]);
    writer
        .start_file("fixture.nes", zip::write::SimpleFileOptions::default())
        .unwrap();
    writer.write_all(&bytes).unwrap();
    writer.finish().unwrap();

    let cache = root.join("cache");
    let catalog = cache
        .join("catalogs")
        .join("Nintendo - Nintendo Entertainment System.dat");
    std::fs::create_dir_all(catalog.parent().unwrap()).unwrap();
    std::fs::write(
        &catalog,
        r#"clrmamepro (
  name "fixture"
)
game (
  name "Tiny Adventure (USA)"
  rom ( name "fixture.nes" size 4 crc B63CFBCD sha1 12dada1fff4d4787ade3333147202c3b443e376f )
)
"#,
    )
    .unwrap();

    let inspection = inspect_game(&archive_path, &cache, false).unwrap();
    assert!(inspection.matched);
    assert_eq!(inspection.filename, "game.zip");
    assert_eq!(inspection.title, "Tiny Adventure");
}

#[test]
fn an_extra_tag_on_the_cover_is_used_when_the_picture_list_has_it() {
    let root = fixture_directory("cover-tag");
    let rom = root.join("fixture.nes");
    let mut bytes = b"NES\x1a\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00".to_vec();
    bytes.extend([1, 2, 3, 4]);
    std::fs::write(&rom, bytes).unwrap();
    let cache = root.join("cache");
    let catalog = cache
        .join("catalogs")
        .join("Nintendo - Nintendo Entertainment System.dat");
    std::fs::create_dir_all(catalog.parent().unwrap()).unwrap();
    std::fs::write(
        &catalog,
        r#"clrmamepro (
  name "fixture"
)
game (
  name "Tiny Adventure (USA)"
  rom ( name "fixture.nes" size 4 crc B63CFBCD sha1 12dada1fff4d4787ade3333147202c3b443e376f )
)
"#,
    )
    .unwrap();
    let index = cache
        .join("artwork-index")
        .join("Nintendo - Nintendo Entertainment System.txt");
    std::fs::create_dir_all(index.parent().unwrap()).unwrap();
    std::fs::write(&index, "Tiny Adventure (USA) (Unl)\n").unwrap();
    let picture = cache
        .join("artwork")
        .join("Nintendo - Nintendo Entertainment System")
        .join("Named_Boxarts")
        .join("Tiny Adventure (USA) (Unl).png");
    std::fs::create_dir_all(picture.parent().unwrap()).unwrap();
    std::fs::write(&picture, b"\x89PNG\r\n\x1a\n").unwrap();

    let inspection = inspect_game(&rom, &cache, false).unwrap();
    assert_eq!(inspection.icon_path.as_deref(), Some(picture.as_path()));
    assert!(inspection
        .warnings
        .iter()
        .all(|warning| warning != "No cover is published for this game."));
}

#[test]
fn a_playstation_disc_is_identified_from_its_serial() {
    let root = fixture_directory("ps1-serial");
    let bin = root.join("track.bin");
    let mut image = vec![0; 64];
    image[..11].copy_from_slice(b"SLUS_012.34");
    std::fs::write(&bin, &image).unwrap();
    let cue = root.join("game.cue");
    std::fs::write(&cue, "FILE \"track.bin\" BINARY\n  TRACK 01 MODE2/2352\n").unwrap();
    let cache = root.join("cache");
    let catalog = cache.join("catalogs").join("Sony - PlayStation.dat");
    std::fs::create_dir_all(catalog.parent().unwrap()).unwrap();
    std::fs::write(
        &catalog,
        r#"clrmamepro (
  name "fixture"
)
game (
  name "Tiny Adventure (USA)"
  serial "SLUS-99999"
  rom ( name "other.bin" size 1 crc 00000000 serial "SLUS-99999" )
)
game (
  name "Crash Sample (USA)"
  serial "SLUS-01234"
  rom ( name "track.bin" size 999999 crc FFFFFFFF serial "SLUS-01234" )
)
"#,
    )
    .unwrap();

    let inspection =
        inspect_game_with_system(&cue, &cache, false, Some("PlayStation")).unwrap();
    assert_eq!(inspection.system, "ps1");
    assert!(inspection.matched, "{:?}", inspection.warnings);
    assert_eq!(inspection.title, "Crash Sample");
}

#[test]
fn a_compressed_disc_is_not_checksummed() {
    let root = fixture_directory("chd");
    // We do not open or hash a .cdi image, and we report it with a warning.
    let cdi = root.join("game.cdi");
    std::fs::write(&cdi, b"not a real compressed disc").unwrap();
    let inspection =
        inspect_game_with_system(&cdi, &root.join("cache"), false, Some("dreamcast")).unwrap();
    assert!(!inspection.matched);
    assert!(
        inspection
            .warnings
            .iter()
            .any(|warning| warning.contains("compressed")),
        "{:?}",
        inspection.warnings
    );

    // We open a CHD. Bytes that are not a CHD stay unmatched, and we do
    // not claim that we checksummed the file.
    let rom = root.join("game.chd");
    std::fs::write(&rom, b"not a real compressed disc").unwrap();
    let inspection =
        inspect_game_with_system(&rom, &root.join("cache"), false, Some("PlayStation"))
            .unwrap();
    assert!(!inspection.matched);
    assert!(
        inspection
            .warnings
            .iter()
            .any(|warning| warning.contains("CHD")),
        "{:?}",
        inspection.warnings
    );
    assert!(
        inspection
            .warnings
            .iter()
            .all(|warning| !warning.to_ascii_lowercase().contains("checksum")),
        "{:?}",
        inspection.warnings
    );
}

/// We can only open a CHD with the vendored `chd` crate, and have no code
/// to write one. So we put the serial bytes in a cue, which is what we give
/// the same matcher in `read_chd` after decompressing.
fn stage_disc_cover(cache: &Path, system_id: &str, picture: &str, serial: &str) -> PathBuf {
    let catalog = systems::find(system_id)
        .unwrap()
        .catalog
        .clone()
        .unwrap();
    let dat = cache.join("catalogs").join(format!("{catalog}.dat"));
    fs::create_dir_all(dat.parent().unwrap()).unwrap();
    fs::write(
        &dat,
        format!(
            r#"clrmamepro (
  name "fixture"
)
game (
  name "{picture}"
  rom ( name "track.bin" size 1 crc 00000000 serial "{serial}" )
)
"#
        ),
    )
    .unwrap();
    let index = cache.join("artwork-index").join(format!("{catalog}.txt"));
    fs::create_dir_all(index.parent().unwrap()).unwrap();
    fs::write(&index, format!("{picture}\n")).unwrap();
    let png = cache
        .join("artwork")
        .join(&catalog)
        .join("Named_Boxarts")
        .join(format!("{picture}.png"));
    fs::create_dir_all(png.parent().unwrap()).unwrap();
    fs::write(&png, b"\x89PNG\r\n\x1a\n").unwrap();
    png
}

#[test]
fn a_generated_disc_is_named_from_its_serial_and_given_a_cover() {
    let root = fixture_directory("disc-cover");
    let mut image = vec![0; 64];
    image[..11].copy_from_slice(b"SLUS_012.34");
    fs::write(root.join("Tiny Disc.bin"), &image).unwrap();
    let cue = root.join("Tiny Disc.cue");
    fs::write(
        &cue,
        "FILE \"Tiny Disc.bin\" BINARY\n  TRACK 01 MODE2/2352\n",
    )
    .unwrap();
    let cache = root.join("cache");
    let picture = stage_disc_cover(&cache, "ps1", "Tiny Disc (Europe)", "SLUS-01234");

    let inspection = inspect_game(&cue, &cache, false).unwrap();
    assert_eq!(inspection.system, "ps1", "{:?}", inspection.warnings);
    assert!(inspection.matched, "{:?}", inspection.warnings);
    assert_eq!(inspection.catalog_name.as_deref(), Some("Tiny Disc (Europe)"));
    assert_eq!(inspection.title, "Tiny Disc");
    assert_eq!(inspection.icon_path.as_deref(), Some(picture.as_path()));
}

/// When someone drops the subchannel file, we use the disc with the same
/// name next to it as the game.
#[test]
fn a_subchannel_file_is_identified_as_the_disc_beside_it() {
    let root = fixture_directory("sbi-identify");
    let mut image = vec![0; 64];
    image[..11].copy_from_slice(b"SLUS_012.34");
    fs::write(root.join("Tiny Disc.bin"), &image).unwrap();
    fs::write(
        root.join("Tiny Disc.cue"),
        "FILE \"Tiny Disc.bin\" BINARY\n  TRACK 01 MODE2/2352\n",
    )
    .unwrap();
    let subchannel = root.join("Tiny Disc.sbi");
    fs::write(&subchannel, b"subchannel").unwrap();
    let cache = root.join("cache");
    let picture = stage_disc_cover(&cache, "ps1", "Tiny Disc (Europe)", "SLUS-01234");

    let inspection = inspect_game(&subchannel, &cache, false).unwrap();
    assert!(
        inspection.matched,
        "the subchannel file was not identified as the disc: {:?}",
        inspection.warnings
    );
    assert_eq!(inspection.system, "ps1");
    assert_eq!(inspection.catalog_name.as_deref(), Some("Tiny Disc (Europe)"));
    assert_eq!(inspection.icon_path.as_deref(), Some(picture.as_path()));

    let traveling = crate::traveling::files_for(&subchannel, Some("ps1")).unwrap();
    assert!(
        traveling.names().iter().any(|name| name == "Tiny Disc.sbi"),
        "the details step would not name the subchannel file: {:?}",
        traveling.files
    );
}

#[test]
fn a_generated_gd_rom_folder_is_named_and_given_a_cover() {
    let root = fixture_directory("gd-cover");
    let folder = root.join("Tiny Disc");
    fs::create_dir(&folder).unwrap();
    let mut image = vec![0u8; 0x100];
    image[..15].copy_from_slice(b"SEGA SEGAKATANA");
    image[0x40..0x4A].copy_from_slice(b"T-00001   ");
    image[0x80..0x89].copy_from_slice(b"TINY DISC");
    let tracks = [
        "Tiny Disc (Track 1).bin",
        "Tiny Disc (Track 2).bin",
        "Tiny Disc (Track 3).bin",
    ];
    fs::write(folder.join(tracks[0]), &image).unwrap();
    fs::write(folder.join(tracks[1]), b"audio").unwrap();
    fs::write(folder.join(tracks[2]), b"data").unwrap();
    let layout = folder.join("Tiny Disc.gdi");
    fs::write(
        &layout,
        "3\n\
         1 0 4 2352 \"Tiny Disc (Track 1).bin\" 0\n\
         2 450 0 2352 \"Tiny Disc (Track 2).bin\" 0\n\
         3 2250 4 2352 \"Tiny Disc (Track 3).bin\" 0\n",
    )
    .unwrap();
    let cache = root.join("cache");
    let picture = stage_disc_cover(&cache, "dreamcast", "Tiny Disc (Europe)", "T-00001");

    let from_folder = inspect_game(&folder, &cache, false).unwrap();
    let from_layout = inspect_game(&layout, &cache, false).unwrap();
    let from_track = inspect_game(&folder.join(tracks[2]), &cache, false).unwrap();
    assert_eq!(from_folder.system, "dreamcast", "{:?}", from_folder.warnings);
    assert!(from_folder.matched, "{:?}", from_folder.warnings);
    assert_eq!(from_folder.title, "Tiny Disc");
    assert_eq!(
        from_folder.catalog_name.as_deref(),
        Some("Tiny Disc (Europe)")
    );
    assert_eq!(from_folder.icon_path.as_deref(), Some(picture.as_path()));
    assert_eq!(from_track.system, from_layout.system);
    assert_eq!(from_track.title, from_layout.title);
    assert_eq!(from_track.catalog_name, from_layout.catalog_name);
    assert_eq!(from_folder.catalog_name, from_layout.catalog_name);
}
