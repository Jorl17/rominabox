//! The byte layouts in which a dumped ROM can come.
//!
//! No-Intro lists the decoded cartridge. People keep the same game as a zip,
//! a byte-shuffled Mega Drive dump, or a Super Nintendo or Nintendo 64 dump
//! with a copier chunk at the start. If we hashed the file as it is stored,
//! we would miss those. For each variant we add a checksum of the same bytes,
//! so a layout we do not recognise still matches on the file as it is.

use std::fs::File;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use crc32fast::Hasher;
use sha1::{Digest, Sha1};
use zip::ZipArchive;

pub struct Fingerprint {
    pub size: u64,
    pub crc32: u32,
    pub sha1: String,
}

/// The file to read for identification. For a zip, we read the game inside
/// it, and keep the name of the file the person dropped.
pub struct PreparedRom {
    pub path: PathBuf,
    pub extension: String,
    pub filename: String,
    pub warnings: Vec<String>,
    temporary: Option<PathBuf>,
}

impl Drop for PreparedRom {
    fn drop(&mut self) {
        if let Some(path) = self.temporary.take() {
            let _ = std::fs::remove_file(path);
        }
    }
}

pub fn prepare(path: &Path) -> Result<PreparedRom, String> {
    let filename = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| "Choose a game file with a valid filename.".to_owned())?
        .to_owned();
    let extension = extension_of(&filename);
    if extension != "zip" && !is_zip_magic(path) {
        return Ok(PreparedRom {
            path: path.to_owned(),
            extension,
            filename,
            warnings: Vec::new(),
            temporary: None,
        });
    }
    let file = File::open(path).map_err(|error| error.to_string())?;
    let mut archive = ZipArchive::new(file).map_err(|_| {
        "The zip archive could not be read. Export the game as a single ROM and drop that instead."
            .to_owned()
    })?;
    let index = choose_entry(&mut archive)?;
    let mut entry = archive.by_index(index).map_err(|error| error.to_string())?;
    let inner_name = entry.name().to_owned();
    let inner_extension = extension_of(&inner_name);
    static SEQUENCE: AtomicU64 = AtomicU64::new(0);
    let temporary = std::env::temp_dir().join(format!(
        "rominabox-rom-{}-{}-{}",
        std::process::id(),
        SEQUENCE.fetch_add(1, Ordering::Relaxed),
        inner_extension
    ));
    let mut output = File::create(&temporary).map_err(|error| error.to_string())?;
    io::copy(&mut entry, &mut output).map_err(|error| error.to_string())?;
    output.flush().map_err(|error| error.to_string())?;
    Ok(PreparedRom {
        path: temporary.clone(),
        extension: inner_extension,
        filename,
        warnings: Vec::new(),
        temporary: Some(temporary),
    })
}

fn choose_entry(archive: &mut ZipArchive<File>) -> Result<usize, String> {
    let mut best: Option<(bool, u64, usize)> = None;
    for index in 0..archive.len() {
        let entry = archive.by_index(index).map_err(|error| error.to_string())?;
        if !entry.is_file() {
            continue;
        }
        let name = entry.name();
        if name.starts_with("__MACOSX/") || name.ends_with('/') {
            continue;
        }
        let extension = extension_of(name);
        if matches!(extension.as_str(), "txt" | "nfo" | "html" | "htm" | "diz") {
            continue;
        }
        let known = !crate::systems::candidates_for_extension(&extension).is_empty();
        let size = entry.size();
        let rank = (known, size, index);
        if best.map(|current| rank > current).unwrap_or(true) {
            best = Some(rank);
        }
    }
    best.map(|(_, _, index)| index)
        .ok_or_else(|| "The archive does not contain a game file.".to_owned())
}

fn is_zip_magic(path: &Path) -> bool {
    let mut magic = [0; 4];
    let Ok(mut file) = File::open(path) else {
        return false;
    };
    file.read(&mut magic).ok() == Some(4) && magic == *b"PK\x03\x04"
}

pub fn extension_of(filename: &str) -> String {
    Path::new(filename)
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase()
}

/// Header bytes with any copier chunk removed and a shuffled dump restored,
/// so that we detect the console and read the title from the cartridge.
pub fn identification_header(path: &Path) -> io::Result<Vec<u8>> {
    let mut probe = Vec::new();
    File::open(path)?.take(32 * 1024).read_to_end(&mut probe)?;
    if let Some(restored) = deinterleaved_prefix(&probe) {
        return Ok(trim_header(restored));
    }
    if let Some(restored) = normalized_n64_prefix(&probe) {
        return Ok(trim_header(restored));
    }
    Ok(trim_header(probe))
}

/// How many leading bytes are a copier header and not the cartridge.
///
/// We declare the size on the console. A dump has a header when the file is
/// that many bytes past a kilobyte boundary, as with Super Nintendo copiers,
/// where the cartridge is a whole number of kilobytes and the header adds
/// 512.
pub fn copier_prefix(extension: &str, length: u64) -> u64 {
    let candidates = crate::systems::candidates_for_extension(extension);
    if candidates.len() != 1 {
        return 0;
    }
    let Some(size) = candidates[0].copier_header else {
        return 0;
    };
    if size > 0 && length > size && length % 1024 == size {
        size
    } else {
        0
    }
}

/// Cartridge bytes long enough for a declared title, with a copier chunk
/// removed and a shuffled dump restored.
///
/// `identification_header` ends at 512 because the signatures are before
/// that. A Super Nintendo title is at 32 KiB or 64 KiB.
pub fn image_prefix(path: &Path, extension: &str, bytes_needed: usize) -> io::Result<Vec<u8>> {
    let length = std::fs::metadata(path)?.len();
    let skip = copier_prefix(extension, length);
    let mut file = File::open(path)?;
    if skip > 0 {
        let mut discarded = vec![0u8; skip as usize];
        file.read_exact(&mut discarded)?;
    }
    let mut probe = Vec::new();
    file.take(bytes_needed as u64).read_to_end(&mut probe)?;
    if let Some(restored) = deinterleaved_prefix(&probe) {
        return Ok(restored);
    }
    if let Some(restored) = normalized_n64_prefix(&probe) {
        return Ok(restored);
    }
    Ok(probe)
}

fn trim_header(bytes: Vec<u8>) -> Vec<u8> {
    let end = bytes.len().min(512);
    bytes[..end].to_vec()
}

pub fn fingerprints(
    path: &Path,
    extension: &str,
    normalize_ines: bool,
) -> io::Result<Vec<Fingerprint>> {
    let mut found = vec![hash_file(path, 0, N64Order::Big)?];
    if normalize_ines {
        found.push(hash_file(path, 16, N64Order::Big)?);
    }
    let length = std::fs::metadata(path)?.len();
    // A Super Nintendo copier header is 512 bytes, and the cartridge itself is
    // a whole number of kilobytes, so the file is 512 bytes past a boundary.
    let skip = copier_prefix(extension, length);
    if skip > 0 {
        found.push(hash_file(path, skip, N64Order::Big)?);
    }
    if let Some((skip, order)) = n64_variant(path)? {
        found.push(hash_file(path, skip, order)?);
    }
    if smd_shape(length) && deinterleaved_prefix(&read_probe(path)?).is_some() {
        found.push(hash_smd(path)?);
    }
    Ok(found)
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum N64Order {
    Big,
    ByteSwap,
    WordSwap,
}

fn n64_variant(path: &Path) -> io::Result<Option<(u64, N64Order)>> {
    let probe = read_probe(path)?;
    if let Some(order) = n64_order(probe.get(..4)) {
        if order == N64Order::Big {
            return Ok(None);
        }
        return Ok(Some((0, order)));
    }
    if let Some(order) = n64_order(probe.get(512..516)) {
        return Ok(Some((512, order)));
    }
    Ok(None)
}

fn n64_order(bytes: Option<&[u8]>) -> Option<N64Order> {
    match bytes {
        Some([0x80, 0x37, 0x12, 0x40]) => Some(N64Order::Big),
        Some([0x37, 0x80, 0x40, 0x12]) => Some(N64Order::ByteSwap),
        Some([0x40, 0x12, 0x37, 0x80]) => Some(N64Order::WordSwap),
        _ => None,
    }
}

fn normalized_n64_prefix(probe: &[u8]) -> Option<Vec<u8>> {
    let (skip, order) = if n64_order(probe.get(..4)).is_some_and(|order| order != N64Order::Big) {
        (0, n64_order(probe.get(..4))?)
    } else if n64_order(probe.get(512..516)).is_some() {
        (512, n64_order(probe.get(512..516))?)
    } else {
        return None;
    };
    let body = probe.get(skip..)?;
    let mut restored = body.to_vec();
    reorder(&mut restored, order);
    Some(restored)
}

fn reorder(bytes: &mut [u8], order: N64Order) {
    match order {
        N64Order::Big => {}
        N64Order::ByteSwap => {
            for pair in bytes.chunks_exact_mut(2) {
                pair.swap(0, 1);
            }
        }
        N64Order::WordSwap => {
            for word in bytes.chunks_exact_mut(4) {
                word.reverse();
            }
        }
    }
}

fn smd_shape(length: u64) -> bool {
    length > 512 && (length - 512) % 16384 == 0
}

/// In a Mega Drive dump from a Super Magic Drive copier, each 16 KiB block
/// is 8 KiB of even bytes followed by 8 KiB of odd bytes, after a 512-byte header.
fn deinterleaved_prefix(probe: &[u8]) -> Option<Vec<u8>> {
    if probe.len() < 512 + 16384 {
        return None;
    }
    let block = &probe[512..512 + 16384];
    let mut restored = Vec::with_capacity(16384);
    for index in 0..8192 {
        restored.push(block[index]);
        restored.push(block[8192 + index]);
    }
    if restored.get(0x100..0x104) != Some(b"SEGA") {
        return None;
    }
    Some(restored)
}

fn hash_smd(path: &Path) -> io::Result<Fingerprint> {
    let mut file = File::open(path)?;
    let mut skipped = [0; 512];
    file.read_exact(&mut skipped)?;
    let mut crc32 = Hasher::new();
    let mut sha1 = Sha1::new();
    let mut size = 0_u64;
    let mut block = [0; 16384];
    loop {
        match file.read_exact(&mut block) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::UnexpectedEof => break,
            Err(error) => return Err(error),
        }
        for index in 0..8192 {
            let pair = [block[index], block[8192 + index]];
            crc32.update(&pair);
            sha1.update(pair);
            size += 2;
        }
    }
    Ok(finish(size, crc32, sha1))
}

fn hash_file(path: &Path, skip: u64, order: N64Order) -> io::Result<Fingerprint> {
    let mut file = File::open(path)?;
    if skip > 0 {
        io::copy(&mut (&mut file).take(skip), &mut io::sink())?;
    }
    let mut crc32 = Hasher::new();
    let mut sha1 = Sha1::new();
    let mut size = 0_u64;
    let mut buffer = [0; 1024 * 1024];
    let mut pending = Vec::new();
    loop {
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        if order == N64Order::Big {
            crc32.update(&buffer[..count]);
            sha1.update(&buffer[..count]);
            size += count as u64;
            continue;
        }
        pending.extend_from_slice(&buffer[..count]);
        let width = if order == N64Order::ByteSwap { 2 } else { 4 };
        let usable = pending.len() - pending.len() % width;
        reorder(&mut pending[..usable], order);
        crc32.update(&pending[..usable]);
        sha1.update(&pending[..usable]);
        size += usable as u64;
        pending.drain(..usable);
    }
    Ok(finish(size, crc32, sha1))
}

fn finish(size: u64, crc32: Hasher, sha1: Sha1) -> Fingerprint {
    let digest = sha1.finalize();
    Fingerprint {
        size,
        crc32: crc32.finalize(),
        sha1: digest.iter().map(|byte| format!("{byte:02x}")).collect(),
    }
}

fn read_probe(path: &Path) -> io::Result<Vec<u8>> {
    let mut probe = Vec::new();
    File::open(path)?.take(32 * 1024).read_to_end(&mut probe)?;
    Ok(probe)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "rominabox-dumps-{}-{}-{name}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&path).unwrap();
        path
    }

    #[test]
    fn a_zip_is_read_as_the_game_inside_it() {
        let root = temp("zip");
        let rom = root.join("game.nes");
        std::fs::write(&rom, b"NES\x1a cartridge").unwrap();
        let archive_path = root.join("game.zip");
        let file = File::create(&archive_path).unwrap();
        let mut writer = zip::ZipWriter::new(file);
        writer
            .start_file("notes.txt", zip::write::SimpleFileOptions::default())
            .unwrap();
        writer.write_all(b"not a rom").unwrap();
        writer
            .start_file("game.nes", zip::write::SimpleFileOptions::default())
            .unwrap();
        writer.write_all(b"NES\x1a cartridge").unwrap();
        writer.finish().unwrap();

        let prepared = prepare(&archive_path).unwrap();
        assert_eq!(prepared.extension, "nes");
        assert_eq!(prepared.filename, "game.zip");
        assert_eq!(std::fs::read(&prepared.path).unwrap(), b"NES\x1a cartridge");
    }

    #[test]
    fn a_byte_shuffled_mega_drive_dump_hashes_as_the_cartridge() {
        let mut cartridge = vec![0; 16384];
        cartridge[0x100..0x104].copy_from_slice(b"SEGA");
        cartridge[0x150..0x155].copy_from_slice(b"SONIC");
        let mut shuffled = vec![0; 512];
        let mut even = Vec::new();
        let mut odd = Vec::new();
        for (index, byte) in cartridge.iter().enumerate() {
            if index % 2 == 0 {
                even.push(*byte);
            } else {
                odd.push(*byte);
            }
        }
        shuffled.extend(even);
        shuffled.extend(odd);

        let root = temp("smd");
        let path = root.join("game.smd");
        std::fs::write(&path, &shuffled).unwrap();
        let header = identification_header(&path).unwrap();
        assert_eq!(&header[0x100..0x104], b"SEGA");
        assert_eq!(&header[0x150..0x155], b"SONIC");

        let plain = root.join("game.md");
        std::fs::write(&plain, &cartridge).unwrap();
        let restored = fingerprints(&path, "smd", false).unwrap();
        let original = fingerprints(&plain, "md", false).unwrap();
        assert!(restored
            .iter()
            .any(|found| { found.crc32 == original[0].crc32 && found.sha1 == original[0].sha1 }));
    }

    #[test]
    fn a_leading_copier_chunk_is_omitted_from_one_checksum() {
        let root = temp("snes");
        let path = root.join("game.sfc");
        let mut bytes = vec![0xAA; 512];
        bytes.extend([1, 2, 3, 4]);
        // 516 bytes is 512 + 4, and 516 % 1024 == 516, not 512.
        // Pad the payload so the whole file is 512 bytes past a kilobyte boundary.
        bytes.extend(std::iter::repeat(0).take(1024 - 4));
        assert_eq!(bytes.len() % 1024, 512);
        std::fs::write(&path, &bytes).unwrap();
        let found = fingerprints(&path, "sfc", false).unwrap();
        let payload = root.join("payload.bin");
        std::fs::write(&payload, &bytes[512..]).unwrap();
        let payload_hash = fingerprints(&payload, "bin", false).unwrap();
        assert!(found
            .iter()
            .any(|item| item.sha1 == payload_hash[0].sha1 && item.size == payload_hash[0].size));
    }

    #[test]
    fn a_byte_swapped_nintendo_64_dump_hashes_as_big_endian() {
        let root = temp("n64");
        let mut big = vec![0; 16];
        big[..4].copy_from_slice(&[0x80, 0x37, 0x12, 0x40]);
        big[4..8].copy_from_slice(b"GAME");
        let mut swapped = big.clone();
        for pair in swapped.chunks_exact_mut(2) {
            pair.swap(0, 1);
        }
        let path = root.join("game.v64");
        std::fs::write(&path, &swapped).unwrap();
        let plain = root.join("game.z64");
        std::fs::write(&plain, &big).unwrap();
        let found = fingerprints(&path, "v64", false).unwrap();
        let original = fingerprints(&plain, "z64", false).unwrap();
        assert!(found.iter().any(|item| item.sha1 == original[0].sha1));
        let header = identification_header(&path).unwrap();
        assert_eq!(&header[..4], &[0x80, 0x37, 0x12, 0x40]);
        assert_eq!(&header[4..8], b"GAME");
    }
}
