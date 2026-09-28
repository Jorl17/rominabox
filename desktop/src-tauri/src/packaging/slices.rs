//! The processors that the code of a Mac app is built for.
//!
//! A Mach-O file contains one slice of code for each processor it runs on,
//! and a universal file contains several. We read the slices from the file
//! (`lipo -archs`), never from a list kept next to it. An ordinary game
//! contains only the slice for this Mac, so it stays small. A game that also
//! runs on Intel Macs has slices for Apple silicon and Intel in its player,
//! its core and its launch library.

use super::app_files::copy_file;
use super::{ErrorStage, ExportError, OwnedStaging};
use crate::target::Target;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const LIPO: &str = "/usr/bin/lipo";

/// A processor a Mac runs on.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) enum Arch {
    Arm64,
    X86_64,
}

impl Arch {
    pub(crate) const ALL: [Arch; 2] = [Arch::Arm64, Arch::X86_64];

    /// The processor a Mac target runs on, or `None` for a target that is
    /// not a Mac.
    pub(crate) fn of(target: Target) -> Option<Arch> {
        match target {
            Target::MacosArm64 => Some(Arch::Arm64),
            Target::MacosX86_64 => Some(Arch::X86_64),
            Target::WindowsX86_64 => None,
        }
    }

    /// Its name for `lipo`, for `cc -arch` and in the Mach-O readers.
    pub(crate) fn name(self) -> &'static str {
        match self {
            Arch::Arm64 => "arm64",
            Arch::X86_64 => "x86_64",
        }
    }

    /// The name that an author uses for the Macs with this processor.
    fn macs(self) -> &'static str {
        match self {
            Arch::Arm64 => "Apple silicon",
            Arch::X86_64 => "Intel",
        }
    }

    fn named(name: &str) -> Option<Arch> {
        Arch::ALL.into_iter().find(|arch| arch.name() == name)
    }
}

/// The slices in a file.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Slices {
    pub archs: Vec<Arch>,
    /// Slices for processors that we do not ship a game for (arm64e, i386),
    /// with their names in `lipo`.
    pub others: Vec<String>,
}

impl Slices {
    /// The first of `wanted` this file has no slice for.
    fn missing(&self, wanted: &[Arch]) -> Option<Arch> {
        wanted.iter().copied().find(|arch| !self.archs.contains(arch))
    }

    /// It contains `wanted` and nothing else.
    fn exactly(&self, wanted: &[Arch]) -> bool {
        self.others.is_empty()
            && self.missing(wanted).is_none()
            && self.archs.iter().all(|arch| wanted.contains(arch))
    }
}

/// The slices in `path`.
pub(crate) fn read(path: &Path, stage: ErrorStage) -> Result<Slices, ExportError> {
    let output = Command::new(LIPO)
        .arg("-archs")
        .arg(path)
        .output()
        .map_err(|error| ExportError::new(stage, format!("could not run lipo: {error}")))?;
    if !output.status.success() {
        return Err(ExportError::command(stage, "lipo", &output).about(path));
    }
    let mut slices = Slices {
        archs: Vec::new(),
        others: Vec::new(),
    };
    for name in String::from_utf8_lossy(&output.stdout).split_whitespace() {
        match Arch::named(name) {
            Some(arch) => slices.archs.push(arch),
            None => slices.others.push(name.to_string()),
        }
    }
    Ok(slices)
}

/// Make `destination` a copy of `source` with exactly the slices `wanted`,
/// either `source` itself when it has no others or the result of `lipo`. We
/// use `what` as the name of the file in the error when one is missing.
pub(super) fn keep(
    source: &Path,
    wanted: &[Arch],
    destination: &Path,
    what: &str,
    stage: ErrorStage,
) -> Result<(), ExportError> {
    let slices = read(regular(source, stage)?, stage)?;
    refuse_missing(&slices, wanted, what)?;
    if slices.exactly(wanted) {
        return copy_file(source, destination);
    }
    let mut lipo = Command::new(LIPO);
    lipo.arg(source);
    match wanted {
        [only] => {
            lipo.args(["-thin", only.name()]);
        }
        several => {
            for arch in several {
                lipo.args(["-extract", arch.name()]);
            }
        }
    }
    run(lipo.arg("-output").arg(destination), stage)
}

/// Make `destination` one file with the slice of each part. `(arch, file)`
/// gives the slice for `arch`, which `file` contains alone or among others.
/// We use `what` to name the files to the author when one lacks its slice.
pub(super) fn join(
    parts: &[(Arch, PathBuf)],
    destination: &Path,
    what: &str,
    stage: ErrorStage,
) -> Result<(), ExportError> {
    let parent = destination
        .parent()
        .ok_or_else(|| ExportError::new(stage, "a joined file needs a folder"))?;
    let staging = OwnedStaging::create(parent)?;
    let mut thin = Vec::new();
    for (arch, file) in parts {
        let slices = read(regular(file, stage)?, stage)?;
        refuse_missing(&slices, &[*arch], what)?;
        if slices.exactly(&[*arch]) {
            thin.push(file.clone());
        } else {
            let taken = staging.path().join(arch.name());
            run(
                Command::new(LIPO)
                    .arg(file)
                    .args(["-thin", arch.name(), "-output"])
                    .arg(&taken),
                stage,
            )?;
            thin.push(taken);
        }
    }
    run(
        Command::new(LIPO)
            .arg("-create")
            .args(&thin)
            .arg("-output")
            .arg(destination),
        stage,
    )?;
    staging.cleanup()
}

/// Run `edit` on each slice of `path` as a separate file, then join the
/// slices again in `path`. We edit a file with one slice in place.
pub(super) fn each_slice(
    path: &Path,
    stage: ErrorStage,
    mut edit: impl FnMut(&Path, Arch) -> Result<(), ExportError>,
) -> Result<(), ExportError> {
    let slices = read(path, stage)?;
    if let Some(other) = slices.others.first() {
        return Err(ExportError::new(
            stage,
            format!("{} has a {other} slice, which a game does not ship", path.display()),
        ));
    }
    if let [only] = slices.archs.as_slice() {
        return edit(path, *only);
    }
    let parent = path
        .parent()
        .ok_or_else(|| ExportError::new(stage, "a sliced file needs a folder"))?;
    let staging = OwnedStaging::create(parent)?;
    let mut parts = Vec::new();
    for arch in &slices.archs {
        let slice = staging.path().join(arch.name());
        run(
            Command::new(LIPO)
                .arg(path)
                .args(["-thin", arch.name(), "-output"])
                .arg(&slice),
            stage,
        )?;
        edit(&slice, *arch)?;
        parts.push(slice);
    }
    run(
        Command::new(LIPO)
            .arg("-create")
            .args(&parts)
            .arg("-output")
            .arg(path),
        stage,
    )?;
    staging.cleanup()
}

fn refuse_missing(slices: &Slices, wanted: &[Arch], what: &str) -> Result<(), ExportError> {
    match slices.missing(wanted) {
        Some(arch) => Err(ExportError::new(
            ErrorStage::Refused,
            format!(
                "{what} has no {macs} version, so the game cannot run on {macs} Macs.",
                macs = arch.macs()
            ),
        )),
        None => Ok(()),
    }
}

/// `path`, when it is a file and not a link to one, as required in `copy_file`
/// for everything that we copy in an export.
fn regular(path: &Path, stage: ErrorStage) -> Result<&Path, ExportError> {
    let metadata =
        fs::symlink_metadata(path).map_err(|error| ExportError::io(stage, path, error))?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(ExportError::new(
            stage,
            format!("refusing to stage non-regular file: {}", path.display()),
        ));
    }
    Ok(path)
}

fn run(command: &mut Command, stage: ErrorStage) -> Result<(), ExportError> {
    let output = command
        .output()
        .map_err(|error| ExportError::new(stage, format!("could not run lipo: {error}")))?;
    if output.status.success() {
        Ok(())
    } else {
        Err(ExportError::command(stage, "lipo", &output))
    }
}
