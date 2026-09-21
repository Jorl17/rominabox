//! The declarations in a console package.
//!
//! One directory under `integrations/consoles/` describes one console. It
//! contains its `console.json` and any controller profile or core component
//! that the console introduces. Another console refers to those by id. We
//! never copy them into its package or change the first package to add it.
//! We derive the reverse relationships when we load the packages, so the
//! links between consoles and profiles always match in both directions.

use serde::{Deserialize, Serialize};

/// We raise this when a field changes meaning. We reject a package that
/// declares a version unknown to this build, and never read it in part.
pub const SCHEMA_VERSION: u32 = 1;

/// The logical controls we can bind a runtime input to. This is the fixed
/// RetroPad vocabulary. A profile may use any subset, in any order, but may
/// not invent an id, because we map these to emulator inputs in an export.
pub const CONTROL_IDS: &[&str] = &[
    "up", "down", "left", "right", "b", "a", "y", "x", "l", "r", "l2", "r2", "select", "start",
    "l3", "r3",
];

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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub x: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub y: Option<i32>,
    #[serde(default, rename = "calloutX", skip_serializing_if = "Option::is_none")]
    pub callout_x: Option<i32>,
    #[serde(default, rename = "calloutY", skip_serializing_if = "Option::is_none")]
    pub callout_y: Option<i32>,
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
    #[serde(default, rename = "presentationOrder", skip_serializing_if = "Option::is_none")]
    pub presentation_order: Option<u32>,
    /// The emulated device we report for this pad. An illustration with six
    /// buttons alone does not make us report a six-button pad.
    #[serde(default, rename = "coreDevice", skip_serializing_if = "Option::is_none")]
    pub core_device: Option<u32>,
    pub controls: Vec<Control>,
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

/// A bounded read of an ASCII title from a cartridge header, which is data
/// because it is an offset and a length. Every branch is a named handler.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HeaderTitle {
    pub offset: u64,
    pub length: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct Recognition {
    #[serde(default, rename = "headerTitle", skip_serializing_if = "Option::is_none")]
    pub header_title: Option<HeaderTitle>,
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Content {
    /// Extensions from which we can recognise this console. Recognisable is
    /// not the same as exportable, and we report that difference when we
    /// resolve the console, and do not hide it.
    pub extensions: Vec<String>,
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
    #[serde(default, rename = "presentationOrder", skip_serializing_if = "Option::is_none")]
    pub presentation_order: Option<u32>,
    /// Firmware the author must supply before we can export this console.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub firmware: Vec<FirmwareGroup>,
}
