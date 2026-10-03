//! Applying a patch file to a game. We take the original and the patch, and
//! produce the patched game.
//!
//! We use RetroArch's own code. In build.rs we compile `tasks/patch_stream.c`
//! and libretro-common's VCDIFF decoder from the fork we build the player
//! from, which is the code RetroArch uses for a patch beside a game. We apply
//! the patch once, at export, because in RetroArch a patch applies only to a
//! game that RetroArch loads into memory itself, and Genesis Plus GX and the
//! emulators of disc consoles open their game files themselves.
//!
//! With the fork's `patch_stream_apply_into`, we read the original in place
//! and write the patched game into memory that we size beforehand from the
//! patch. Here both are files mapped from disk, so for a disc we keep in
//! memory only the parts of the two files that the patch touches.

use std::fs::{self, File, OpenOptions};
use std::io;
use std::os::raw::c_int;
use std::path::Path;

use memmap2::{Mmap, MmapMut};
use serde::Serialize;

/// The patch formats in RetroArch, which we tell apart by a patch's first bytes.
/// The numbers match `enum patch_stream_format` in `tasks/patch_stream.h`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
#[repr(C)]
pub enum PatchFormat {
    Ips = 0,
    Ups = 1,
    Bps = 2,
    Xdelta = 3,
}

/// The VCDIFF magic number (RFC 3284, 4.1), which xdelta patches start with.
const VCDIFF_MAGIC: [u8; 4] = [0xD6, 0xC3, 0xC4, 0x00];
/// The header indicator bit for a secondary compressor, whose id follows.
const VCD_DECOMPRESS: u8 = 0x01;

impl PatchFormat {
    pub fn of(patch: &[u8]) -> Option<Self> {
        if patch.starts_with(b"PATCH") {
            Some(Self::Ips)
        } else if patch.starts_with(b"UPS1") {
            Some(Self::Ups)
        } else if patch.starts_with(b"BPS1") {
            Some(Self::Bps)
        } else if patch.starts_with(&VCDIFF_MAGIC) {
            Some(Self::Xdelta)
        } else {
            None
        }
    }
}

/// The compression of a patch's sections, as stated in its header (the
/// xdelta3 secondary compressor ids). LZMA is the default of the xdelta
/// command.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum XdeltaCompression {
    None,
    Djw,
    Lzma,
    Fgk,
    Other(u8),
}

impl XdeltaCompression {
    pub fn of(patch: &[u8]) -> Option<Self> {
        if !patch.starts_with(&VCDIFF_MAGIC) {
            return None;
        }
        let indicator = *patch.get(4)?;
        if indicator & VCD_DECOMPRESS == 0 {
            return Some(Self::None);
        }
        Some(match *patch.get(5)? {
            1 => Self::Djw,
            2 => Self::Lzma,
            16 => Self::Fgk,
            other => Self::Other(other),
        })
    }
}

/// Why we did not apply a patch.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum PatchError {
    /// Not one of the formats in RetroArch.
    NotAPatch,
    /// The patch failed in RetroArch's code, because it is damaged, was made
    /// for another game, or (xdelta) has a compression RetroArch cannot read.
    /// BPS, UPS and xdelta contain a check of the original game, and IPS does
    /// not. For an xdelta patch, we name the compression stated in its header.
    #[serde(rename_all = "camelCase")]
    DoesNotApply { compression: Option<XdeltaCompression> },
}

impl PatchError {
    /// Why we did not apply `patch` to `game`, in words for the author.
    pub fn explain(&self, patch: &str, game: &str) -> String {
        match self {
            Self::NotAPatch => format!("{patch} is not a patch ROM-in-a-Box can read."),
            Self::DoesNotApply { compression: Some(XdeltaCompression::None | XdeltaCompression::Lzma) | None } => {
                format!("{patch} does not apply to {game}: it was made for another version of the game, or it is damaged.")
            }
            Self::DoesNotApply { compression: Some(_) } => format!(
                "{patch} is compressed in a way ROM-in-a-Box cannot read. Make it again with the xdelta command's defaults."
            ),
        }
    }
}

/// The C declarations of `tasks/patch_stream.h`.
mod ffi {
    use super::PatchFormat;

    extern "C" {
        pub fn patch_stream_target_room(
            format: PatchFormat,
            patch: *const u8,
            patch_len: usize,
            src_len: usize,
            room: *mut usize,
        ) -> bool;
        pub fn patch_stream_apply_into(
            format: PatchFormat,
            patch: *const u8,
            patch_len: usize,
            src: *const u8,
            src_len: usize,
            out: *mut u8,
            out_room: usize,
            out_len: *mut usize,
        ) -> bool;
    }
}

/// How many bytes the patched game needs room for, from the patch alone.
fn room(format: PatchFormat, patch: &[u8], game_len: usize) -> Result<usize, PatchError> {
    let mut room = 0usize;
    if unsafe { ffi::patch_stream_target_room(format, patch.as_ptr(), patch.len(), game_len, &mut room) } {
        Ok(room)
    } else {
        Err(refused(patch))
    }
}

/// Apply `patch` to `game` and write the result into `out`, which has the
/// size from [`room`]. Returns the patched game's length.
fn apply_into(format: PatchFormat, patch: &[u8], game: &[u8], out: &mut [u8]) -> Result<usize, PatchError> {
    let mut length = 0usize;
    let applied = unsafe {
        ffi::patch_stream_apply_into(
            format,
            patch.as_ptr(),
            patch.len(),
            game.as_ptr(),
            game.len(),
            out.as_mut_ptr(),
            out.len(),
            &mut length,
        )
    };
    if applied {
        Ok(length)
    } else {
        Err(refused(patch))
    }
}

fn refused(patch: &[u8]) -> PatchError {
    PatchError::DoesNotApply { compression: XdeltaCompression::of(patch) }
}

/// `game` with `patch` applied, in the same way as in RetroArch.
pub fn apply(patch: &[u8], game: &[u8]) -> Result<Vec<u8>, PatchError> {
    let format = PatchFormat::of(patch).ok_or(PatchError::NotAPatch)?;
    let room = room(format, patch, game.len())?;
    let mut out = Vec::new();
    // A patch can declare any size, so we refuse one we have no memory for.
    out.try_reserve_exact(room).map_err(|_| refused(patch))?;
    out.resize(room, 0);
    let length = apply_into(format, patch, game, &mut out)?;
    out.truncate(length);
    Ok(out)
}

/// Why we did not patch a game file.
#[derive(Debug)]
pub enum FileFailure {
    /// We refused the patch, for a reason from [`apply`].
    Patch(PatchError),
    /// We could not read, create or map a file.
    Io(io::Error),
}

impl From<io::Error> for FileFailure {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

/// Apply `patch` to the game in the file `game` and write it to the new file
/// `made`. We map both from disk instead of reading them, so only the parts
/// the patch changes are in memory. We refuse a file already at `made` and
/// leave it as it is, and remove a `made` we created when the patch fails.
pub fn apply_file(patch: &[u8], game: &Path, made: &Path) -> Result<(), FileFailure> {
    // PPF is not one of the RetroArch formats, so we write its bytes into a
    // copy of the game (crate::ppf).
    if crate::ppf::is_ppf(patch) {
        let records = crate::ppf::records(patch).map_err(|_| FileFailure::Patch(PatchError::NotAPatch))?;
        return crate::ppf::apply(&records, game, made).map_err(FileFailure::Io);
    }
    let format = PatchFormat::of(patch).ok_or(FileFailure::Patch(PatchError::NotAPatch))?;
    let source = File::open(game)?;
    // With these maps we assume that nothing else changes the files while we
    // apply the patch. The original is the author's, and `made` is new.
    let original = if source.metadata()?.len() == 0 { None } else { Some(unsafe { Mmap::map(&source)? }) };
    let original: &[u8] = original.as_deref().unwrap_or(&[]);
    let room = room(format, patch, original.len()).map_err(FileFailure::Patch)?;
    let target = OpenOptions::new().read(true).write(true).create_new(true).open(made)?;
    let written = (|| {
        target.set_len(room as u64)?;
        let length = {
            let mut map = if room == 0 { None } else { Some(unsafe { MmapMut::map_mut(&target)? }) };
            apply_into(format, patch, original, map.as_deref_mut().unwrap_or(&mut [])).map_err(FileFailure::Patch)?
        };
        target.set_len(length as u64)?;
        Ok(())
    })();
    if written.is_err() {
        drop(target);
        let _ = fs::remove_file(made);
    }
    written
}

/// Apply each of `patches` in turn to the game in `game`, each to the result
/// of the one before, and write it to the new file `made`. With no patches,
/// `made` is a copy. On a refusal we name the patch by its place in
/// `patches`. We put intermediate files beside `made` and remove them.
pub fn apply_files(game: &Path, patches: &[&Path], made: &Path) -> Result<(), (usize, FileFailure)> {
    if patches.is_empty() {
        return fs::copy(game, made).map(|_| ()).map_err(|error| (0, FileFailure::Io(error)));
    }
    let between = |index: usize| {
        let mut name = made.file_name().unwrap_or_default().to_os_string();
        name.push(format!(".patch-{index}"));
        made.with_file_name(name)
    };
    let mut from = game.to_path_buf();
    for (index, patch) in patches.iter().enumerate() {
        let to = if index + 1 == patches.len() { made.to_path_buf() } else { between(index) };
        let applied = fs::read(patch).map_err(FileFailure::Io).and_then(|bytes| apply_file(&bytes, &from, &to));
        if index > 0 {
            // The previous result, which we have now read for this patch.
            let _ = fs::remove_file(&from);
        }
        applied.map_err(|failure| (index, failure))?;
        from = to;
    }
    Ok(())
}

/// The C enum is an int.
const _: () = assert!(std::mem::size_of::<PatchFormat>() == std::mem::size_of::<c_int>());
