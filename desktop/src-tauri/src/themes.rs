//! The registry of designs, palettes and sound packs for previews and exports.
//! The code that composes a menu from a design is in `crate::menu`.
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
};

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Design {
    pub id: String,
    pub name: String,
}
#[derive(Debug, Deserialize, Serialize)]
pub struct Palette {
    pub id: String,
    pub name: String,
    pub screen: String,
    pub background: String,
    pub surface: String,
    pub picture: String,
    pub edge: String,
    pub highlight: String,
    pub muted: String,
    pub focus: String,
    /// Values for tokens that the design declares and the palette roles above
    /// do not name, such as the outer frame, the bevels and the disabled greys.
    ///
    /// Without them, the design's defaults would apply in every palette. A
    /// palette gives a value for each token the design declares, so to add a
    /// token, add a line to each palette and not a field here.
    #[serde(default)]
    pub tokens: std::collections::BTreeMap<String, String>,
}
/// One pack is one complete set of the four menu cues, `up`, `down`, `ok` and
/// `cancel`. Packs have no variants or layers. `off` is the one entry with no
/// assets, and we write it as `audio_enable_menu=false` at export.
#[derive(Debug, Deserialize, Serialize)]
pub struct SoundPack {
    pub id: String,
    pub name: String,
    pub description: String,
}

/// The basenames of menu sounds in RetroArch. A pack must have all of them.
pub const SOUND_CUES: [&str; 4] = ["up.wav", "down.wav", "ok.wav", "cancel.wav"];
#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Registry {
    pub designs: Vec<Design>,
    pub palettes: Vec<Palette>,
    pub sound_packs: Vec<SoundPack>,
}
pub fn registry() -> Result<Registry, String> {
    serde_json::from_str(include_str!("../../designs.json")).map_err(|e| e.to_string())
}

/// The directory that contains a design's documents and fonts.
///
/// A design defines its screens as well as its colours, and another design may
/// lay out the menu in a different way (three save slots instead of six), so
/// each design is a directory and not only a name.
pub fn design_root(design: &str) -> Result<PathBuf, String> {
    let declared = registry()?
        .designs
        .into_iter()
        .find(|entry| entry.id == design)
        .ok_or_else(|| format!("Unknown menu design: {design}"))?;
    // We resolve the path against this crate and not the working directory,
    // because the builder does not run from the repository root. We check the
    // path on disk, so we fail here for a design with no directory.
    let root = crate::repo::at("integrations/designs").join(&declared.id);
    if !root.is_dir() {
        return Err(format!(
            "Menu design '{design}' is declared but its package is missing at {}",
            root.display()
        ));
    }
    Ok(root)
}

/// Where a design's staged files are in a prepared kit.
///
/// Each design has a directory in the kit, so we choose the set of files to
/// stage by the design id.
pub fn staged_design(kit: &Path, design: &str) -> PathBuf {
    kit.join("designs").join(design)
}

/// Copy only the selected menu cue pack. With Off, we add no audio.
pub fn prepare_sound_assets(source: &Path, destination: &Path, pack: &str) -> Result<(), String> {
    if !registry()?.sound_packs.iter().any(|sound| sound.id == pack) {
        return Err("Choose an available menu sound pack.".into());
    }
    if pack == "off" {
        return Ok(());
    }
    fs::create_dir_all(destination).map_err(|e| e.to_string())?;
    for name in SOUND_CUES {
        fs::copy(source.join(pack).join(name), destination.join(name))
            .map_err(|e| e.to_string())?;
    }
    fs::copy(
        source.join("PROVENANCE.txt"),
        destination.join("PROVENANCE.txt"),
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    fn sound_source() -> std::path::PathBuf {
        crate::repo::at("desktop/assets/menu-sounds")
    }

    /// A pack is one complete set that we can play. We declare no partial
    /// pack, and ship no assets that the author cannot pick.
    #[test]
    fn every_declared_sound_pack_is_one_complete_cue_set() {
        let declared: BTreeSet<String> = registry()
            .unwrap()
            .sound_packs
            .into_iter()
            .map(|pack| pack.id)
            .filter(|id| id != "off")
            .collect();
        assert!(!declared.is_empty(), "no menu sound packs are declared");

        let source = sound_source();
        let mut present = BTreeSet::new();
        for entry in fs::read_dir(&source).expect("menu sound assets") {
            let entry = entry.expect("menu sound entry");
            if entry.file_type().expect("file type").is_dir() {
                present.insert(entry.file_name().to_string_lossy().into_owned());
            }
        }
        assert_eq!(
            declared, present,
            "declared packs and shipped pack directories must match exactly"
        );

        for id in &declared {
            for cue in SOUND_CUES {
                let path = source.join(id).join(cue);
                let bytes = fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
                assert!(bytes.len() > 44, "{} is not a usable WAV", path.display());
                assert_eq!(&bytes[0..4], b"RIFF", "{} is not RIFF", path.display());
                assert_eq!(&bytes[8..12], b"WAVE", "{} is not WAVE", path.display());
                // 44100 Hz, 16-bit, mono, the format we give the RetroArch mixer.
                assert_eq!(
                    u16::from_le_bytes([bytes[22], bytes[23]]),
                    1,
                    "{} is not mono",
                    path.display()
                );
                assert_eq!(
                    u32::from_le_bytes([bytes[24], bytes[25], bytes[26], bytes[27]]),
                    44_100,
                    "{} is not 44100 Hz",
                    path.display()
                );
                assert_eq!(
                    u16::from_le_bytes([bytes[34], bytes[35]]),
                    16,
                    "{} is not 16-bit",
                    path.display()
                );
            }
        }
    }

    /// The loudest 30 ms of a 16-bit mono WAV, as RMS in dBFS. This is how loud
    /// a short cue seems, and we level every generated cue by it.
    fn loudest_30ms_db(bytes: &[u8]) -> f64 {
        let data = bytes
            .windows(4)
            .position(|chunk| chunk == b"data")
            .expect("a data chunk")
            + 8;
        let samples: Vec<f64> = bytes[data..]
            .chunks_exact(2)
            .map(|pair| f64::from(i16::from_le_bytes([pair[0], pair[1]])) / 32768.0)
            .collect();
        let window = (44_100 * 30 / 1000).min(samples.len());
        let mut sum: f64 = samples[..window].iter().map(|v| v * v).sum();
        let mut loudest = sum;
        for i in window..samples.len() {
            sum += samples[i] * samples[i] - samples[i - window] * samples[i - window];
            loudest = loudest.max(sum);
        }
        20.0 * (loudest / window as f64).sqrt().log10()
    }

    /// The sounds for moving, confirming and going back are equally loud in
    /// every pack, so a click is as loud as a move and never louder.
    #[test]
    fn every_cue_in_a_pack_is_equally_loud() {
        let source = sound_source();
        for pack in registry().unwrap().sound_packs {
            if pack.id == "off" {
                continue;
            }
            let levels: Vec<(String, f64)> = SOUND_CUES
                .iter()
                .map(|cue| {
                    let path = source.join(&pack.id).join(cue);
                    let bytes = fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
                    (cue.to_string(), loudest_30ms_db(&bytes))
                })
                .collect();
            let loudest = levels.iter().map(|(_, db)| *db).fold(f64::MIN, f64::max);
            let quietest = levels.iter().map(|(_, db)| *db).fold(f64::MAX, f64::min);
            assert!(
                loudest - quietest <= 1.5,
                "{}: its cues are {:.1} dB apart: {:?}",
                pack.id,
                loudest - quietest,
                levels
                    .iter()
                    .map(|(cue, db)| format!("{cue} {db:.1} dBFS"))
                    .collect::<Vec<_>>()
            );
        }
    }

    /// The Off choice is already labelled Off, so we add no sentence under it.
    #[test]
    fn off_pack_does_not_restate_that_audio_is_off() {
        let off = registry()
            .unwrap()
            .sound_packs
            .into_iter()
            .find(|pack| pack.id == "off")
            .expect("off pack");
        assert!(
            !off.description
                .to_ascii_lowercase()
                .contains("no menu audio"),
            "the Off option already says it is off, and this is still under it: {}",
            off.description
        );
    }

    /// A pack that the author can hear has a line that describes its
    /// character. Off has none, because the control label is already Off.
    #[test]
    fn every_sound_pack_is_described_for_the_picker() {
        for pack in registry().unwrap().sound_packs {
            assert!(!pack.name.trim().is_empty(), "{} has no name", pack.id);
            if pack.id == "off" {
                continue;
            }
            assert!(
                !pack.description.trim().is_empty(),
                "{} has no description",
                pack.id
            );
            assert!(
                !pack.id.contains("--"),
                "{} keeps an authoring variant separator",
                pack.id
            );
        }
    }

    #[test]
    fn unknown_sound_packs_are_rejected_before_staging() {
        let temporary = rominabox_scratch::Scratch::reserve("rominabox-sound-pack");
        let error = prepare_sound_assets(&sound_source(), &temporary, "pulse")
            .expect_err("retired pack must not stage");
        assert!(error.contains("available menu sound pack"), "{error}");
        assert!(!temporary.exists(), "rejection must not create output");
    }

    /// The person who bundles the game picks the BIOS. The player never does.
    ///
    /// We put no BIOS picker and no BIOS uploader in the exported game. A
    /// player who wants a different BIOS goes through Advanced, which unlocks
    /// the whole emulator.
    ///
    /// Everything about the BIOS is in the builder (`assess_firmware`, the
    /// details step, the export refusal), and the player sees none of it. A
    /// design may not contain a screen, a button or a declaration that offers a
    /// BIOS choice. We still bundle a BIOS, with no way to change it in the menu.
    #[test]
    fn no_design_offers_the_player_a_bios() {
        let designs = crate::repo::at("integrations/designs");
        let mut looked = 0;
        for entry in fs::read_dir(&designs).expect("designs directory") {
            let design = entry.expect("design entry").path();
            if !design.is_dir() {
                continue;
            }
            for file in fs::read_dir(&design).expect("design files") {
                let file = file.expect("design file").path();
                let Some(name) = file.file_name().and_then(|n| n.to_str()) else {
                    continue;
                };
                if !name.ends_with(".rml") && !name.ends_with(".rcss") && !name.ends_with(".json") {
                    continue;
                }
                let body = fs::read_to_string(&file).unwrap_or_default();
                looked += 1;
                for (number, line) in body.lines().enumerate() {
                    assert!(
                        !line.to_ascii_lowercase().contains("bios"),
                        "{}:{} offers the player a BIOS: {}\n\
                         The BIOS is chosen by whoever bundles the game. A player \
                         who wants another one uses Advanced.",
                        file.display(),
                        number + 1,
                        line.trim()
                    );
                }
            }
        }
        assert!(
            looked > 0,
            "no design files were read, so this proved nothing"
        );
    }
}
