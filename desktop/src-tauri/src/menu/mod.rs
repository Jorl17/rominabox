//! The in-game menu we ship in an export, composed from a design in memory
//! and written once.
//!
//! We build it with one function, `compose_menu`, for the export, the
//! builder's preview and the CLI staging commands, so their menus are always
//! the same. In `manifest` we read the design, in `document` we build the
//! page, in `scene` the controller scene, in `tokens` we fill in values, in
//! `declarations` we write `design.cfg`, and in `stage` we join the pieces.

pub mod contract;
pub(crate) mod declarations;
mod document;
pub(crate) mod inc;
mod manifest;
mod scene;
pub mod script;
mod stage;
mod tokens;
pub mod words;

use std::path::PathBuf;

pub(crate) use contract::contract;
pub(crate) use declarations::{file_name, key};
pub use document::{
    install_settings, level_markup, setting_slot, volume_control_markup, SettingsPlace, STYLESHEET,
};
pub use manifest::{
    base_design, declared_overlays, declared_screens, scene_metrics, Binds, Documents, Font,
    Manifest, Overlay, SceneMetrics, Screen, ScreenPlace, ScreenRole,
};
pub use scene::scene_titles;
pub use stage::{compose_menu, render_preview, Composition, MenuRequest, PreviewRequest, DOCUMENT};

/// One file of a composition.
#[derive(Clone, Debug)]
pub enum Content {
    Text(String),
    Bytes(Vec<u8>),
    /// A file copied as it is, from here.
    Copy(PathBuf),
}
