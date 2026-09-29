//! A Windows game is one program file that contains the game's launcher
//! followed by the rest of the game in packed form. The first time someone
//! starts the game, we unpack the rest into a folder under that person's
//! local application data, which the game's sandbox can read but not write,
//! and run the game from there. We unpack in `launcher/windows/unpack.c`.
//!
//! The file contains the launcher's bytes, then every other file of the
//! game as one zstd frame each, then an index, then a trailer.
//!
//! ```text
//! index    "RIBPACK1"
//!          u16 + runtime folder, under the local application data, with "/"
//!          u16 + the launcher's name in that folder
//!          [32] the launcher's SHA-256 (the bytes before the first file)
//!          u32 + the dialog's logo (PNG)
//!          u32 + the dialog's lettering (TrueType)
//!          u32 file count, then per file:
//!            u16 + path under the folder, with "/"
//!            u64 where its frame starts, u64 the frame's length, u64 its size
//!            [32] its SHA-256
//!            u8 1 when we check all of it at each launch, 0 for only its ends
//!            [32] SHA-256 of its first and last 64 KB
//! trailer  u64 where the index starts, u64 its length, "RIBTAIL1"
//! ```
//!
//! All numbers are little-endian. At each launch we check the programs whole
//! and every other file only by its size and ends, which is fast for any disc.

use crate::export_error::{ErrorStage, ExportError};
use sha2::{Digest, Sha256};
use std::fs::{self, File};
use std::io::{self, BufReader, BufWriter, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

const INDEX_MAGIC: &[u8; 8] = b"RIBPACK1";
const TRAILER_MAGIC: &[u8; 8] = b"RIBTAIL1";
/// How much of each end of a file we read at launch.
const END: u64 = 64 * 1024;
/// The dialog we show while we unpack a game on its first launch, with the
/// builder's logo and the lettering of the ROM-in-a-Box menus.
const DIALOG_LOGO: &[u8] = include_bytes!("../../icons/icon-large.png");
const DIALOG_FONT: &[u8] = include_bytes!("../../../../integrations/designs/native/ScienceGothic-Bold.ttf");

struct Entry {
    path: String,
    offset: u64,
    packed: u64,
    size: u64,
    whole: [u8; 32],
    read_whole: bool,
    ends: [u8; 32],
}

/// Pass reads through, and compute the SHA-256 of every byte read.
struct Hashing<R> {
    inner: R,
    hash: Sha256,
    read: u64,
}

impl<R: Read> Read for Hashing<R> {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        let count = self.inner.read(buffer)?;
        self.hash.update(&buffer[..count]);
        self.read += count as u64;
        Ok(count)
    }
}

/// Pass writes through, and count the bytes written.
struct Counting<W> {
    inner: W,
    written: u64,
}

impl<W: Write> Write for Counting<W> {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        let count = self.inner.write(buffer)?;
        self.written += count as u64;
        Ok(count)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}

fn ends_of(path: &Path, size: u64) -> io::Result<[u8; 32]> {
    let mut file = File::open(path)?;
    let mut hash = Sha256::new();
    let head = size.min(END);
    let mut buffer = vec![0u8; head as usize];
    file.read_exact(&mut buffer)?;
    hash.update(&buffer);
    let tail = size.min(END);
    file.seek(SeekFrom::Start(size - tail))?;
    buffer.resize(tail as usize, 0);
    file.read_exact(&mut buffer)?;
    hash.update(&buffer);
    Ok(hash.finalize().into())
}

/// Every file under `folder` except `launcher`, by its path under `folder`
/// with "/", in a fixed order.
fn files(folder: &Path, launcher: &Path) -> io::Result<Vec<(String, PathBuf)>> {
    let mut found = Vec::new();
    let mut waiting = vec![folder.to_path_buf()];
    while let Some(directory) = waiting.pop() {
        for entry in fs::read_dir(&directory)? {
            let entry = entry?;
            let path = entry.path();
            let kind = entry.file_type()?;
            if kind.is_dir() {
                waiting.push(path);
            } else if kind.is_file() && path != launcher {
                let relative = path
                    .strip_prefix(folder)
                    .expect("found under the folder")
                    .components()
                    .map(|part| part.as_os_str().to_string_lossy().into_owned())
                    .collect::<Vec<_>>()
                    .join("/");
                found.push((relative, path));
            }
        }
    }
    found.sort();
    Ok(found)
}

fn is_program(path: &str) -> bool {
    let lower = path.to_ascii_lowercase();
    lower.ends_with(".exe") || lower.ends_with(".dll")
}

/// The error for a failure to read or write `path` while we pack.
fn failed(path: &Path) -> impl Fn(io::Error) -> ExportError + '_ {
    move |error| ExportError::io(ErrorStage::Sign, path, error)
}

fn short(text: &str, stage: ErrorStage) -> Result<u16, ExportError> {
    u16::try_from(text.len()).map_err(|_| ExportError::new(stage, format!("a name is too long to pack: {text}")))
}

/// Pack the Windows game laid out in `folder`, with `launcher` as its
/// program, into the single program `destination`. We unpack the game into
/// the folder `runtime` under the local application data, with the pack's
/// id added, so we never unpack a new export over the copy that is running.
pub(super) fn pack(
    folder: &Path,
    launcher: &Path,
    runtime: &str,
    destination: &Path,
    cancelled: &AtomicBool,
) -> Result<(), ExportError> {
    let stage = ErrorStage::Sign;
    let head = fs::read(launcher).map_err(failed(launcher))?;
    let head_hash: [u8; 32] = Sha256::digest(&head).into();
    let mut output = Counting {
        inner: BufWriter::new(File::create(destination).map_err(failed(destination))?),
        written: 0,
    };
    output.write_all(&head).map_err(failed(destination))?;

    let mut entries = Vec::new();
    for (relative, path) in files(folder, launcher).map_err(failed(folder))? {
        if cancelled.load(Ordering::SeqCst) {
            return Err(ExportError::new(ErrorStage::Cancelled, "Export cancelled"));
        }
        let size = fs::metadata(&path).map_err(failed(&path))?.len();
        let offset = output.written;
        let mut reading = Hashing {
            inner: BufReader::new(File::open(&path).map_err(failed(&path))?),
            hash: Sha256::new(),
            read: 0,
        };
        ruzstd::encoding::compress(&mut reading, &mut output, ruzstd::encoding::CompressionLevel::Fastest);
        if reading.read != size {
            return Err(ExportError::new(stage, format!("{} changed while it was packed", path.display())));
        }
        entries.push(Entry {
            ends: ends_of(&path, size).map_err(failed(&path))?,
            read_whole: is_program(&relative),
            path: relative,
            offset,
            packed: output.written - offset,
            size,
            whole: reading.hash.finalize().into(),
        });
    }

    // The pack's id is a hash of the name and content of every file.
    let mut id = Sha256::new();
    id.update(head_hash);
    for entry in &entries {
        id.update(entry.path.as_bytes());
        id.update([0]);
        id.update(entry.whole);
    }
    let id = format!("{:x}", id.finalize());
    let runtime = format!("{runtime}-{}", &id[..8]);
    let program = launcher
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();

    let mut index = Vec::new();
    index.extend_from_slice(INDEX_MAGIC);
    for text in [&runtime, &program] {
        index.extend_from_slice(&short(text, stage)?.to_le_bytes());
        index.extend_from_slice(text.as_bytes());
    }
    index.extend_from_slice(&head_hash);
    for blob in [DIALOG_LOGO, DIALOG_FONT] {
        index.extend_from_slice(&(blob.len() as u32).to_le_bytes());
        index.extend_from_slice(blob);
    }
    index.extend_from_slice(&(entries.len() as u32).to_le_bytes());
    for entry in &entries {
        index.extend_from_slice(&short(&entry.path, stage)?.to_le_bytes());
        index.extend_from_slice(entry.path.as_bytes());
        for number in [entry.offset, entry.packed, entry.size] {
            index.extend_from_slice(&number.to_le_bytes());
        }
        index.extend_from_slice(&entry.whole);
        index.push(u8::from(entry.read_whole));
        index.extend_from_slice(&entry.ends);
    }
    let index_offset = output.written;
    output.write_all(&index).map_err(failed(destination))?;
    output.write_all(&index_offset.to_le_bytes()).map_err(failed(destination))?;
    output.write_all(&(index.len() as u64).to_le_bytes()).map_err(failed(destination))?;
    output.write_all(TRAILER_MAGIC).map_err(failed(destination))?;
    output.flush().map_err(failed(destination))?;
    Ok(())
}
