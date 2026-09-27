//! The operating system and processor that an exported game runs on.
//!
//! We spell a target as in the core download list and the runtime kit
//! (`macos-arm64`), and nowhere else as a bare string. The author chooses a
//! platform ([`crate::packaging::ExportTarget`]), and we derive the target
//! from it. We declare here, once, what differs between targets.

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Target {
    #[serde(rename = "macos-arm64")]
    MacosArm64,
    #[serde(rename = "macos-x86_64")]
    MacosX86_64,
    #[serde(rename = "windows-x86_64")]
    WindowsX86_64,
}

impl Target {
    pub const ALL: [Target; 3] = [Target::MacosArm64, Target::MacosX86_64, Target::WindowsX86_64];

    /// The key in the download list and the runtime kit.
    pub fn key(self) -> &'static str {
        match self {
            Target::MacosArm64 => "macos-arm64",
            Target::MacosX86_64 => "macos-x86_64",
            Target::WindowsX86_64 => "windows-x86_64",
        }
    }

    pub fn parse(key: &str) -> Option<Target> {
        Target::ALL.into_iter().find(|target| target.key() == key)
    }

    /// The target this build runs on, when we build the engine for it.
    pub fn host() -> Option<Target> {
        match (std::env::consts::OS, std::env::consts::ARCH) {
            ("macos", "aarch64") => Some(Target::MacosArm64),
            ("macos", "x86_64") => Some(Target::MacosX86_64),
            ("windows", "x86_64") => Some(Target::WindowsX86_64),
            _ => None,
        }
    }
}

impl std::fmt::Display for Target {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.key())
    }
}

#[cfg(test)]
mod tests {
    use super::Target;

    #[test]
    fn every_target_reads_back_from_its_key() {
        for target in Target::ALL {
            assert_eq!(Target::parse(target.key()), Some(target));
            assert_eq!(
                serde_json::to_string(&target).unwrap(),
                format!("\"{}\"", target.key())
            );
            assert_eq!(
                serde_json::from_str::<Target>(&format!("\"{}\"", target.key())).unwrap(),
                target
            );
        }
        assert_eq!(Target::parse("linux-x86_64"), None);
    }
}
