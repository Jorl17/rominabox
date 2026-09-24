//! The core step of an export. Before we build the game, we make sure that
//! every core for it is in the cache and is the newest nightly.
//!
//! We download a missing core, and replace a cached core whose nightly has
//! changed. When we cannot reach the server about a cached core, we use it as
//! it is and say nothing. We report only what we are fetching, and only when
//! we fetch something.

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::cores::{self, Need, Transport};

/// What we show in the builder's pop-up and print in the CLI.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum CoreActivity {
    /// We are fetching cores. We never report this with both counts at zero.
    Fetching { downloading: usize, updating: usize },
    /// A required core is not cached, and we could not download it.
    Failed { missing: usize },
}

impl CoreActivity {
    /// The words of the pop-up, for the progress line in the CLI.
    pub fn message(&self) -> String {
        fn cores(count: usize) -> String {
            format!("{count} core{}", if count == 1 { "" } else { "s" })
        }
        match self {
            Self::Fetching {
                downloading,
                updating,
            } => {
                let mut lines = Vec::new();
                if *downloading > 0 {
                    lines.push(format!("Downloading {}", cores(*downloading)));
                }
                if *updating > 0 {
                    lines.push(format!("Updating {}", cores(*updating)));
                }
                lines.join("\n")
            }
            Self::Failed { missing } => {
                format!(
                    "{} could not be downloaded. Try again later.",
                    cores(*missing)
                )
            }
        }
    }
}

/// One core an export needs.
pub struct Wanted<'a> {
    pub component: &'a str,
    /// The download-list platform of the export, not of this machine.
    pub platform: &'a str,
    /// The core and its licence are already present, in the cache or the kit.
    /// We read this only for a core that is not in the download list.
    pub present: bool,
}

/// Bring every wanted core into `cache`. `Err` contains the reported failure,
/// because at least one core is not cached and we could not download it.
pub fn prepare(
    cache: &Path,
    wanted: &[Wanted<'_>],
    transport: &dyn Transport,
    mut report: impl FnMut(&CoreActivity),
) -> Result<(), CoreActivity> {
    let mut downloads = Vec::new();
    let mut updates = Vec::new();
    let mut missing = 0;
    for core in wanted {
        match cores::assess(cache, core.platform, core.component, transport) {
            Some(Need::Download) => downloads.push(core),
            Some(Need::Update) => updates.push(core),
            Some(Need::UseCache) => {}
            None if core.present => {}
            None => missing += 1,
        }
    }
    if !downloads.is_empty() || !updates.is_empty() {
        report(&CoreActivity::Fetching {
            downloading: downloads.len(),
            updating: updates.len(),
        });
    }
    for core in downloads {
        let usable = cores::install_component(cache, core.platform, core.component, transport)
            .is_some_and(|install| install.usable());
        if !usable {
            missing += 1;
        }
    }
    for core in updates {
        // When an update fails, we keep the cached core, which still works.
        let _ = cores::update(cache, core.platform, core.component, transport);
    }
    if missing > 0 {
        let failed = CoreActivity::Failed { missing };
        report(&failed);
        return Err(failed);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_cli_line_uses_the_pop_ups_words() {
        let both = CoreActivity::Fetching {
            downloading: 1,
            updating: 2,
        };
        assert_eq!(both.message(), "Downloading 1 core\nUpdating 2 cores");
        let one = CoreActivity::Fetching {
            downloading: 0,
            updating: 1,
        };
        assert_eq!(one.message(), "Updating 1 core");
        assert_eq!(
            serde_json::to_value(&both).unwrap(),
            serde_json::json!({"kind": "fetching", "downloading": 1, "updating": 2})
        );
        assert_eq!(
            serde_json::to_value(CoreActivity::Failed { missing: 1 }).unwrap(),
            serde_json::json!({"kind": "failed", "missing": 1})
        );
    }
}
