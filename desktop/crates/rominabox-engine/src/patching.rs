//! Applying a patch file to a game. We take the bytes of the original and of
//! the patch, and produce the bytes of the patched game.
//!
//! We use RetroArch's own code. In build.rs we compile `tasks/patch_stream.c`
//! and libretro-common's VCDIFF decoder from the fork we build the player
//! from, which is the code RetroArch uses for a patch beside a game. We apply
//! the patch once, at export, because in RetroArch a patch applies only to a
//! game that RetroArch loads into memory itself, and Genesis Plus GX and the
//! emulators of disc consoles open their game files themselves.

use std::os::raw::c_void;
use std::ptr;

use serde::Serialize;

/// The patch formats in RetroArch, which we tell apart by a patch's first bytes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum PatchFormat {
    Ips,
    Ups,
    Bps,
    Xdelta,
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

/// The C declarations of `tasks/patch_stream.h`.
mod ffi {
    use std::os::raw::c_void;

    pub type Open = unsafe extern "C" fn(*const u8, usize, usize) -> *mut c_void;

    extern "C" {
        pub fn patch_stream_ips_open(patch: *const u8, patch_len: usize, src_len: usize) -> *mut c_void;
        pub fn patch_stream_ups_open(patch: *const u8, patch_len: usize, src_len: usize) -> *mut c_void;
        pub fn patch_stream_bps_open(patch: *const u8, patch_len: usize, src_len: usize) -> *mut c_void;
        pub fn patch_stream_xdelta_open(patch: *const u8, patch_len: usize, src_len: usize) -> *mut c_void;
        pub fn patch_stream_feed(stream: *mut c_void, chunk: *const u8, len: usize) -> usize;
        pub fn patch_stream_finish(stream: *mut c_void, out: *mut *mut u8, out_len: *mut usize) -> bool;
        pub fn patch_stream_free(stream: *mut c_void);
        // The C library's, because we allocated the result in C.
        pub fn free(pointer: *mut c_void);
    }
}

/// `game` with `patch` applied, in the same way as in RetroArch.
pub fn apply(patch: &[u8], game: &[u8]) -> Result<Vec<u8>, PatchError> {
    let format = PatchFormat::of(patch).ok_or(PatchError::NotAPatch)?;
    let open: ffi::Open = match format {
        PatchFormat::Ips => ffi::patch_stream_ips_open,
        PatchFormat::Ups => ffi::patch_stream_ups_open,
        PatchFormat::Bps => ffi::patch_stream_bps_open,
        PatchFormat::Xdelta => ffi::patch_stream_xdelta_open,
    };
    let refused = || PatchError::DoesNotApply { compression: XdeltaCompression::of(patch) };
    // Under patch_stream.h, the patch must outlive the stream. We pass the
    // whole game as one chunk, and a chunk can be any size.
    let stream = unsafe { open(patch.as_ptr(), patch.len(), game.len()) };
    if stream.is_null() {
        return Err(refused());
    }
    let mut out: *mut u8 = ptr::null_mut();
    let mut out_len: usize = 0;
    let finished = unsafe {
        ffi::patch_stream_feed(stream, game.as_ptr(), game.len());
        let finished = ffi::patch_stream_finish(stream, &mut out, &mut out_len);
        ffi::patch_stream_free(stream);
        finished
    };
    let result = if !finished {
        Err(refused())
    } else if out.is_null() {
        Ok(Vec::new())
    } else {
        Ok(unsafe { std::slice::from_raw_parts(out, out_len) }.to_vec())
    };
    if !out.is_null() {
        unsafe { ffi::free(out as *mut c_void) };
    }
    result
}
