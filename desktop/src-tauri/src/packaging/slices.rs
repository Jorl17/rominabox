//! The processors for the code in a Mac app.
//!
//! A Mach-O file contains a slice of code for each processor it runs on, and
//! a universal file contains several. We read the slices from the file
//! (`crate::mach_o`), never from a list next to it. An ordinary game contains
//! only the Apple silicon slice, so it does not grow. A game that also runs
//! on Intel Macs contains the Apple silicon and Intel slices in its player,
//! its core and its launch library.

use super::app_files::copy_file;
use super::{ErrorStage, ExportError};
use crate::mach_o::{self, Cpu};
use crate::target::Target;
use std::fs;
use std::path::{Path, PathBuf};

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

    fn cpu(self) -> Cpu {
        match self {
            Arch::Arm64 => Cpu::ARM64,
            Arch::X86_64 => Cpu::X86_64,
        }
    }

    fn of_cpu(cpu: Cpu) -> Option<Arch> {
        Arch::ALL.into_iter().find(|arch| arch.cpu() == cpu)
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

fn slices_of(bytes: &[u8], path: &Path, stage: ErrorStage) -> Result<Slices, ExportError> {
    let found = mach_o::slices(bytes)
        .map_err(|error| ExportError::new(stage, format!("{}: {error}", path.display())).about(path))?;
    let mut slices = Slices { archs: Vec::new(), others: Vec::new() };
    for slice in found {
        match Arch::of_cpu(slice.cpu) {
            Some(arch) => slices.archs.push(arch),
            None => slices.others.push(slice.cpu.name()),
        }
    }
    Ok(slices)
}

/// The slice for `arch` in `bytes`, as a separate file.
fn slice_for<'a>(bytes: &'a [u8], arch: Arch, path: &Path, stage: ErrorStage) -> Result<&'a [u8], ExportError> {
    mach_o::slices(bytes)
        .map_err(|error| ExportError::new(stage, format!("{}: {error}", path.display())))?
        .into_iter()
        .find(|slice| slice.cpu == arch.cpu())
        .map(|slice| slice.bytes)
        .ok_or_else(|| ExportError::new(stage, format!("{} has no {} slice", path.display(), arch.name())))
}

/// Make `destination` `source` with exactly the slices `wanted`. It is
/// `source` itself when it has no others, or else those slices taken out of
/// it. We use `what` to name the file to the author when it lacks one.
pub(super) fn keep(
    source: &Path,
    wanted: &[Arch],
    destination: &Path,
    what: &str,
    stage: ErrorStage,
) -> Result<(), ExportError> {
    let bytes = read_regular(source, stage)?;
    let slices = slices_of(&bytes, source, stage)?;
    refuse_missing(&slices, wanted, what)?;
    if slices.exactly(wanted) {
        return copy_file(source, destination);
    }
    let kept = wanted
        .iter()
        .map(|arch| slice_for(&bytes, *arch, source, stage))
        .collect::<Result<Vec<_>, _>>()?;
    let file = match kept.as_slice() {
        [only] => only.to_vec(),
        several => mach_o::join(several).map_err(|error| ExportError::new(stage, error))?,
    };
    write(destination, &file, stage)
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
    let files = parts
        .iter()
        .map(|(_, file)| read_regular(file, stage))
        .collect::<Result<Vec<_>, _>>()?;
    let mut thin = Vec::new();
    for ((arch, file), bytes) in parts.iter().zip(&files) {
        refuse_missing(&slices_of(bytes, file, stage)?, &[*arch], what)?;
        thin.push(slice_for(bytes, *arch, file, stage)?);
    }
    let joined = mach_o::join(&thin).map_err(|error| ExportError::new(stage, error))?;
    write(destination, &joined, stage)
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

/// The bytes of `path`, when it is a file and not a link to one, as we
/// require in `copy_file` for everything we copy in an export.
fn read_regular(path: &Path, stage: ErrorStage) -> Result<Vec<u8>, ExportError> {
    let metadata =
        fs::symlink_metadata(path).map_err(|error| ExportError::io(stage, path, error))?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(ExportError::new(
            stage,
            format!("refusing to stage non-regular file: {}", path.display()),
        ));
    }
    fs::read(path).map_err(|error| ExportError::io(stage, path, error))
}

fn write(destination: &Path, bytes: &[u8], stage: ErrorStage) -> Result<(), ExportError> {
    fs::write(destination, bytes).map_err(|error| ExportError::io(stage, destination, error))
}
