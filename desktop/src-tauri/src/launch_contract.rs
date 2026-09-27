//! What we write in an export for the game's launcher, under the names
//! declared in `launcher/launch_contract.inc`. We read that file when the
//! exporter compiles, so the build fails on a name it does not declare.

const SOURCE: &str = include_str!("../launcher/launch_contract.inc");

/// Field `field` of the `MACRO(name, ...)` in `launch_contract.inc`, for any
/// of `macros`. We read it through the macros below.
pub(crate) const fn declared(macros: &[&str], name: &str, field: usize) -> &'static str {
    crate::menu::inc::declared(SOURCE, macros, name, field)
}

/// `app_file!(Name)`: a file in the app's resources (`Contents/Resources`).
macro_rules! app_file {
    ($name:ident) => {
        const { $crate::launch_contract::declared(&["RIB_APP_FILE"], stringify!($name), 1) }
    };
}
pub(crate) use app_file;

/// `core_file!(Platform)`: the file name of the core in the app, in the form
/// of a library name on that platform.
macro_rules! core_file {
    ($platform:ident) => {
        const { $crate::launch_contract::declared(&["RIB_CORE_FILE"], stringify!($platform), 1) }
    };
}
pub(crate) use core_file;

/// `plan_field!(Name)`: a field of `launch.plan`, followed on its line by a
/// tab and its value.
macro_rules! plan_field {
    ($name:ident) => {
        const { $crate::launch_contract::declared(&["RIB_PLAN_FIELD"], stringify!($name), 1) }
    };
}
pub(crate) use plan_field;

/// `plan_mark!(Name)`: a line of its own in `launch.plan`.
macro_rules! plan_mark {
    ($name:ident) => {
        const { $crate::launch_contract::declared(&["RIB_PLAN_MARK"], stringify!($name), 1) }
    };
}
pub(crate) use plan_mark;

/// `token!(Name)`: a place named in a config line, which we replace with its
/// path in the launcher.
macro_rules! token {
    ($name:ident) => {
        const { $crate::launch_contract::declared(&["RIB_TOKEN"], stringify!($name), 1) }
    };
}
pub(crate) use token;

/// `shipped!(Name)`: a folder we ship in the app, and the folder in the
/// game's data to which we apply it in the launcher.
macro_rules! shipped {
    ($name:ident) => {
        const {
            (
                $crate::launch_contract::declared(
                    &["RIB_SHIPPED_SETTINGS", "RIB_SHIPPED_FILES"],
                    stringify!($name),
                    1,
                ),
                $crate::launch_contract::declared(
                    &["RIB_SHIPPED_SETTINGS", "RIB_SHIPPED_FILES"],
                    stringify!($name),
                    2,
                ),
            )
        }
    };
}
pub(crate) use shipped;
