//! The declarations in a console package.
//!
//! One directory under `integrations/consoles/` describes one console. It
//! contains its `console.json` and any controller profile or core component
//! that the console introduces. Another console refers to those by id. We
//! never copy them into its package or change the first package to add it.
//! We derive the reverse relationships when we load the packages, so the
//! links between consoles and profiles always match in both directions.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// We raise this when a field changes meaning. We reject a package that
/// declares a version unknown to this build, and never read it in part.
pub const SCHEMA_VERSION: u32 = 1;

/// A position of the standard pad (RetroArch's RetroPad), with the words we
/// show for it in the builder. The id is RetroArch's bind name
/// (`input_player1_<id>`, configuration.c).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct PadPosition {
    pub id: &'static str,
    pub name: &'static str,
    /// The other half of the axis of a stick direction. In RetroArch we read
    /// an axis whole. If a remap moved one half and left the other, the other
    /// half would do nothing, so we move the two together.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub opposite: Option<&'static str>,
}

const fn button(id: &'static str, name: &'static str) -> PadPosition {
    PadPosition { id, name, opposite: None }
}

const fn stick(id: &'static str, name: &'static str, opposite: &'static str) -> PadPosition {
    PadPosition { id, name, opposite: Some(opposite) }
}

/// Every position of the standard pad. We read a control from one of these,
/// and with RetroArch's controller profiles we map every player's controller
/// onto this pad, so a control bound to a position works on any pad. The
/// controls of a profile are these positions. We list a stick's directions
/// up, right, down, left, the order in which we capture them in the menu.
pub const PAD_POSITIONS: &[PadPosition] = &[
    button("up", "D-pad up"),
    button("down", "D-pad down"),
    button("left", "D-pad left"),
    button("right", "D-pad right"),
    button("b", "Bottom button"),
    button("a", "Right button"),
    button("y", "Left button"),
    button("x", "Top button"),
    button("l", "L1"),
    button("r", "R1"),
    button("l2", "L2"),
    button("r2", "R2"),
    button("select", "Select"),
    button("start", "Start"),
    button("l3", "L3"),
    button("r3", "R3"),
    stick("l_y_minus", "Left stick up", "l_y_plus"),
    stick("l_x_plus", "Left stick right", "l_x_minus"),
    stick("l_y_plus", "Left stick down", "l_y_minus"),
    stick("l_x_minus", "Left stick left", "l_x_plus"),
    stick("r_y_minus", "Right stick up", "r_y_plus"),
    stick("r_x_plus", "Right stick right", "r_x_minus"),
    stick("r_y_plus", "Right stick down", "r_y_minus"),
    stick("r_x_minus", "Right stick left", "r_x_plus"),
];

/// Whether `id` is a control a profile may declare: a position of the pad. A
/// profile may use any of them, in any order, but may not invent one.
pub fn is_control_id(id: &str) -> bool {
    PAD_POSITIONS.iter().any(|position| position.id == id)
}

/// Whether we expect a build to include this console.
///
/// This is the intent that the package declares. We keep it apart from
/// whether the console resolves against a prepared kit at the moment. A
/// missing file must never turn `Enabled` into `Planned` unnoticed, because
/// then we would claim support for a console that the build does not have.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SupportIntent {
    /// The build must include this console, or we stop the preparation.
    Enabled,
    /// We describe and recognise the console, but do not claim to support it.
    Planned,
    /// Deliberately not offered.
    Unsupported,
}

/// How we present a controller profile to the author.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Presentation {
    /// A drawn pad with per-control anchors and callouts.
    Illustrated {
        /// Illustration we rasterise for the player, relative to the package.
        image: String,
    },
    /// The control grid without artwork. It is always available and is not
    /// a fallback. A profile that *declares* an illustration but has no file
    /// is a broken package, and we do not fall back to the grid for it.
    Generic,
}

/// Firmware without which a console cannot run.
///
/// We declare it per console because the console hardware uses it, whatever the
/// file is called. We never include, search for or download proprietary
/// firmware. The author supplies a file they are entitled to use, and these
/// names only identify it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FirmwareGroup {
    pub id: String,
    #[serde(rename = "acceptedNames")]
    pub accepted_names: Vec<String>,
    /// How many of the accepted files the author must supply.
    pub minimum: u32,
    pub help: String,
}

/// Which way a stick member points, or its click. A stick declares its
/// members in this order, the order in which we capture them in the menu.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum StickDirection {
    Up,
    Right,
    Down,
    Left,
    Press,
}

/// One bindable control on a pad.
///
/// `x`/`y` are the button centre and `calloutX`/`calloutY` the label box, both
/// in the 960x380 dp menu scene. They are required for an illustrated
/// presentation and unused in a generic one.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Control {
    pub id: String,
    pub label: String,
    pub key: String,
    /// We present the controls in one group as one thing.
    ///
    /// A stick is four binds (`l_x_plus` and the others) but one object on the
    /// pad. Without groups the scene would need twenty-four callouts, and eight
    /// per gutter do not fit a 380 dp frame.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group: Option<String>,
    /// A stick member's direction, and only a stick member's.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub direction: Option<StickDirection>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub x: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub y: Option<i32>,
    #[serde(default, rename = "calloutX", skip_serializing_if = "Option::is_none")]
    pub callout_x: Option<i32>,
    #[serde(default, rename = "calloutY", skip_serializing_if = "Option::is_none")]
    pub callout_y: Option<i32>,
}

/// What a profile declares about one group of its controls: a stick.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ControlGroup {
    /// The stick's name on the pad, as we show it in its box and the builder's
    /// table, for example "C-stick" for the second GameCube stick.
    pub title: String,
}

/// A pad layout. Exactly one package contains it, and any console that uses
/// it refers to it by id.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ControllerProfile {
    #[serde(rename = "schemaVersion")]
    pub schema_version: u32,
    pub id: String,
    pub name: String,
    pub presentation: Presentation,
    /// Position in the author's profile list, as in Console::presentation_order.
    #[serde(
        default,
        rename = "presentationOrder",
        skip_serializing_if = "Option::is_none"
    )]
    pub presentation_order: Option<u32>,
    /// The emulated device we report for this pad. An illustration with six
    /// buttons alone does not make us report a six-button pad.
    #[serde(
        default,
        rename = "coreDevice",
        skip_serializing_if = "Option::is_none"
    )]
    pub core_device: Option<u32>,
    pub controls: Vec<Control>,
    /// Every group a control names, by that name, and nothing else.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub groups: BTreeMap<String, ControlGroup>,
}

/// One core option that we set so the picture shows the core's own pixels.
///
/// `key` and `value` are tokens from the core. They are data, like a binding
/// name. In an export we write them into the options file and compare them
/// to nothing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PixelOption {
    pub key: String,
    pub value: String,
}

/// A built emulator core, which we identify by the artifact itself and not by
/// the features the upstream project advertises. A project that supports CHD
/// does not mean that we compiled this artifact with CHD support.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CoreComponent {
    #[serde(rename = "schemaVersion")]
    pub schema_version: u32,
    pub id: String,
    pub name: String,
    /// Target triple -> artifact filename inside a prepared kit.
    pub artifacts: std::collections::BTreeMap<String, String>,
    pub license: ComponentLicense,
    /// Capabilities that this build has, for example `chd`. In an export we
    /// check the selected artifact, never the console name.
    #[serde(default)]
    pub capabilities: Vec<String>,
    /// The name in `retro_get_system_info().library_name` for this build.
    ///
    /// In RetroArch the paths of per-core state contain this name, so it is
    /// more than a label. We apply the emulated controller only from a remap
    /// file at `<remap dir>/<library name>/<library name>.rmp`, and the core's
    /// picture options are at `<config dir>/<library name>/<library name>.opt`.
    /// The directory name must match the string in the artifact. Read it from
    /// the artifact with `frame_harness --frames 1`, never from the id.
    #[serde(
        default,
        rename = "libraryName",
        skip_serializing_if = "Option::is_none"
    )]
    pub library_name: Option<String>,
    /// Core options we set so that this build does not replace its pixel
    /// buffer with a blended reconstruction.
    ///
    /// In an export we set every other option to the default from the core.
    /// These options replace that default. Their keys and values come from
    /// the core, and we never branch on them. An empty list means the
    /// declared defaults already leave the pixels intact.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub pixels: Vec<PixelOption>,
    /// Where the shipped artifact came from and what source we retain for it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provenance: Option<ComponentProvenance>,
}

/// How we obtained a core artifact, and what source we include with it.
///
/// These are two different facts, and mixing them up is a licensing hazard.
/// With a core we build ourselves, we include the exact source we built it
/// from. With a core we download from a nightly buildbot, we include a source
/// snapshot kept for its licence text, and we do NOT know which revision the
/// binary came from. `corresponds_to_artifact` states which case this is, so
/// we never present the second case as the first by mistake.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ComponentProvenance {
    /// `built` when we compile it, `libretro-buildbot` when we download it.
    pub origin: String,
    pub repository: String,
    pub revision: String,
    /// Paths to try, in order, when extracting the licence from that snapshot.
    #[serde(rename = "licenseCandidates")]
    pub license_candidates: Vec<String>,
    #[serde(rename = "correspondsToArtifact")]
    pub corresponds_to_artifact: bool,
    /// The branch from whose tip we take the licence text for a downloaded core.
    ///
    /// `revision` is the commit of the source snapshot we archive in the
    /// runtime kit. The nightly does not come from that commit, so in the
    /// builder we read the licence from this branch instead.
    #[serde(default, rename = "branch", skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
    /// SHA-256 of the licence text in the first candidate path that exists.
    ///
    /// We compare it in `scripts/prepare_runtime.py` when we stage a kit. We
    /// leave it out of the builder's download list, because libretro replaces
    /// the buildbot files in place and we would reject the new file.
    #[serde(
        default,
        rename = "licenseSha256",
        skip_serializing_if = "Option::is_none"
    )]
    pub license_sha256: Option<String>,
    /// Measurements we compare in `scripts/prepare_runtime.py` to stage a kit.
    ///
    /// We do not read them in the builder. The buildbot directory is `latest`,
    /// which libretro replaces in place, and with a hash recorded here we would
    /// refuse the new file.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub downloads: BTreeMap<String, PinnedDownload>,
    /// How we build it, for components we compile ourselves.
    ///
    /// What the artifact supports depends on these flags, so we keep them with
    /// the component and not in a build script. For example, with
    /// `HAVE_CHD=0` there is no CHD support for any Sega console.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub build: Option<BuildRecipe>,
}

/// One measured buildbot artifact.
///
/// We compare these hashes in `scripts/prepare_runtime.py` when we stage a
/// kit, and leave them out of the builder's download list.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PinnedDownload {
    #[serde(rename = "archiveSha256")]
    pub archive_sha256: String,
    #[serde(rename = "binarySha256")]
    pub binary_sha256: String,
    #[serde(rename = "archiveBytes")]
    pub archive_bytes: u64,
    #[serde(rename = "binaryBytes")]
    pub binary_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BuildRecipe {
    pub makefile: String,
    pub platform: String,
    #[serde(default)]
    pub flags: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ComponentLicense {
    /// SPDX id where one applies, or the exact stated terms when it is not an
    /// SPDX licence. Several libretro cores are non-commercial, which is a
    /// field-of-use restriction with no SPDX id.
    pub spdx: String,
    /// Filename of the licence text we stage into an export.
    pub file: String,
}

/// Bytes that must be present before we trust a title window.
///
/// A Lynx name is at offset 10 of a 64-byte header. The same offsets in an
/// image without a header are code, and if we read them we would get a title
/// made of opcodes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HeaderMagic {
    pub offset: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hex: Option<String>,
}

/// A bounded read of an ASCII title. One window is an offset and a length.
/// For a disc title we measure the same window from an `anchor` signature,
/// because a CHD does not start at the IP.BIN. We use two windows for Super
/// Nintendo, where the title can be at two addresses.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HeaderTitle {
    pub offset: u64,
    pub length: u64,
    /// The ASCII signature we measure `offset` from. Without it, we measure
    /// from the start of the cartridge image, after we remove a copier header.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub anchor: Option<String>,
    /// Little-endian checksum and its complement, relative to this window.
    /// We reject the window when they do not sum to 0xFFFF. Super Nintendo
    /// has two candidate addresses and only one of them is the header.
    #[serde(
        default,
        rename = "complementAt",
        skip_serializing_if = "Option::is_none"
    )]
    pub complement_at: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub magic: Option<HeaderMagic>,
}

fn one_or_many_header_titles<'de, D>(deserializer: D) -> Result<Vec<HeaderTitle>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum OneOrMany {
        One(HeaderTitle),
        Many(Vec<HeaderTitle>),
    }
    match OneOrMany::deserialize(deserializer)? {
        OneOrMany::One(window) => Ok(vec![window]),
        OneOrMany::Many(windows) => Ok(windows),
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct Recognition {
    /// One object, or a list when the title is not at a single address.
    #[serde(
        default,
        rename = "headerTitle",
        skip_serializing_if = "Vec::is_empty",
        deserialize_with = "one_or_many_header_titles"
    )]
    pub header_titles: Vec<HeaderTitle>,
    /// A copier header of this many bytes is at the front of a dump whose
    /// size is that far past a kilobyte boundary. For Super Nintendo it is
    /// 512. The offsets below are into the cartridge, not the file.
    #[serde(
        default,
        rename = "copierHeader",
        skip_serializing_if = "Option::is_none"
    )]
    pub copier_header: Option<u64>,
    /// A named Rust routine for anything an offset cannot express, such as
    /// stripping an iNES header. A declaration contains the name of a handler,
    /// never the algorithm.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub handlers: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct Metadata {
    /// The checksum catalogue in which we look up this console's games.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub catalog: Option<CatalogRef>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CatalogRef {
    /// `no-intro` for cartridges, `redump` for discs.
    pub provider: String,
    pub name: String,
}

/// A core that a console uses, in the declared order of preference.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CoreBinding {
    /// Component id, declared in the package that introduced the component.
    pub component: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Controllers {
    /// The profile we offer first. The generic presentation is a good default,
    /// and a console can be enabled without artwork.
    pub default: String,
    /// Every profile an author may pick for this console, including the
    /// default. We state this in one direction only.
    #[serde(default)]
    pub variants: Vec<String>,
}

/// How a text file lists the other files that we must export with it.
///
/// The set is closed. To add a format, add a variant here and a reader in
/// `discs::sheet_references`. To add a console that uses an existing
/// format, add only its package.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SheetParser {
    Cue,
    Gdi,
    Playlist,
    Toc,
}

impl SheetParser {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Cue => "cue",
            Self::Gdi => "gdi",
            Self::Playlist => "playlist",
            Self::Toc => "toc",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Sheet {
    pub extension: String,
    pub parser: SheetParser,
}

/// A sibling file that the sheet does not name.
///
/// With LibCrypt the subchannel data is in an `.sbi` beside the disc. With
/// CloneCD the image and the subchannel are beside the `.ccd`, and the sheet
/// does not open in Beetle PCE Fast when either is missing. `required` marks
/// that case. We take an optional sibling when it is there and go on without
/// it when it is not, because most PlayStation discs have no `.sbi`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Companion {
    /// Only beside a file of this extension. When absent, beside any file we
    /// collected, which is how we find an `.sbi` beside a cue or a chd.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub when: Option<String>,
    pub extension: String,
    #[serde(default, skip_serializing_if = "is_false")]
    pub required: bool,
}

fn is_false(value: &bool) -> bool {
    !*value
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Content {
    /// Extensions from which we can recognise this console. Recognisable is
    /// not the same as exportable, and we report that difference when we
    /// resolve the console, and do not hide it.
    pub extensions: Vec<String>,
    /// Extensions we can recognise a game from, but cannot yet export.
    ///
    /// Recognisable and exportable are different facts. If we merged them, we
    /// would accept a file at the drop step and refuse it at the export step.
    /// A sheet whose files we collect does not belong here. This list is for
    /// the rest, formats that point at other files and have no parser yet.
    #[serde(default, rename = "recognizeOnly", skip_serializing_if = "Vec::is_empty")]
    pub recognize_only: Vec<String>,
    /// Sheets whose text lists other files, read with the `parser` reader.
    ///
    /// We declare a file that no sheet lists, such as the `.sbi` of a LibCrypt
    /// PlayStation game, separately as a `companion`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sheets: Vec<Sheet>,
    /// Siblings the sheet does not name.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub companions: Vec<Companion>,
    /// `cartridge` or `disc`.
    pub category: String,
}

/// One console, and the only file required for a simple new console.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Console {
    #[serde(rename = "schemaVersion")]
    pub schema_version: u32,
    /// Stable id. It is part of the exported save directory name, through
    /// `stable_identity`, so a new id leaves existing player data behind.
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub aliases: Vec<String>,
    pub content: Content,
    #[serde(default)]
    pub recognition: Recognition,
    #[serde(default)]
    pub metadata: Metadata,
    #[serde(default)]
    pub cores: Vec<CoreBinding>,
    pub controllers: Controllers,
    /// Target triple -> declared intent.
    #[serde(default)]
    pub support: std::collections::BTreeMap<String, SupportIntent>,
    /// The position of this console in the author's console list.
    ///
    /// We keep it in the package so that a new console may leave it out. We
    /// then sort it by name after the ordered ones, so we add a console
    /// without a central list and without new numbers for the others.
    #[serde(
        default,
        rename = "presentationOrder",
        skip_serializing_if = "Option::is_none"
    )]
    pub presentation_order: Option<u32>,
    /// Firmware the author must supply before we can export this console.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub firmware: Vec<FirmwareGroup>,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Each stick direction lists the other half of its axis, and that half
    /// lists it back. A button has no other half.
    #[test]
    fn a_stick_direction_and_its_opposite_name_each_other() {
        for position in PAD_POSITIONS {
            let Some(opposite) = position.opposite else { continue };
            let other = PAD_POSITIONS
                .iter()
                .find(|candidate| candidate.id == opposite)
                .unwrap_or_else(|| panic!("{} names {opposite}, which is no position", position.id));
            assert_eq!(other.opposite, Some(position.id), "{} and {opposite}", position.id);
        }
        let ids: std::collections::BTreeSet<&str> = PAD_POSITIONS.iter().map(|position| position.id).collect();
        assert_eq!(ids.len(), PAD_POSITIONS.len(), "a position is listed twice");
    }
}
