//! We read and write Mac programs and libraries without the Apple tools, so
//! that someone can make a Mac game with a builder on any system.
//!
//! A Mach-O file is one slice of code for one processor, or a universal
//! ("fat") file with several slices, each a whole Mach-O file in itself.
//! Here we list the slices in a file, take one out and join several, and
//! read and change the libraries a file depends on, its install name and the
//! oldest macOS it runs on. In `signature` we sign a file ad hoc, and in
//! `bundle` we seal an app, in the same way as `codesign`.

pub mod bundle;
pub mod entitlements;
pub mod signature;

/// The header of a universal file, big-endian: the magic and the slice count.
const FAT_MAGIC: u32 = 0xcafe_babe;
const FAT_MAGIC_64: u32 = 0xcafe_babf;
/// A slice's header, in the processor's (little-endian) byte order.
const MH_MAGIC: u32 = 0xfeed_face;
const MH_MAGIC_64: u32 = 0xfeed_facf;

const CPU_ARCH_ABI64: u32 = 0x0100_0000;
const CPU_TYPE_X86: u32 = 7;
const CPU_TYPE_ARM: u32 = 12;
/// The bits of a subtype that are capabilities, not the processor.
const CPU_SUBTYPE_MASK: u32 = 0xff00_0000;

/// Load commands that we read or write here.
pub(crate) const LC_SEGMENT: u32 = 0x1;
pub(crate) const LC_SEGMENT_64: u32 = 0x19;
const LC_LOAD_DYLIB: u32 = 0xc;
const LC_ID_DYLIB: u32 = 0xd;
const LC_LOAD_WEAK_DYLIB: u32 = 0x8000_0018;
const LC_REEXPORT_DYLIB: u32 = 0x8000_001f;
const LC_LAZY_LOAD_DYLIB: u32 = 0x20;
const LC_LOAD_UPWARD_DYLIB: u32 = 0x8000_0023;
pub(crate) const LC_UUID: u32 = 0x1b;
pub(crate) const LC_CODE_SIGNATURE: u32 = 0x1d;
const LC_VERSION_MIN_MACOSX: u32 = 0x24;
const LC_BUILD_VERSION: u32 = 0x32;

/// `text` escaped for an element of a property list, as in the Apple writer:
/// only what XML requires.
pub(crate) fn plist_text(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// The largest alignment of a slice in a universal file, 32 KB, as in `lipo`.
const MAXIMUM_ALIGNMENT: u32 = 15;

/// The processor a slice is for, as given in its header.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Cpu {
    pub kind: u32,
    pub subtype: u32,
}

impl Cpu {
    pub const ARM64: Cpu = Cpu {
        kind: CPU_ARCH_ABI64 | CPU_TYPE_ARM,
        subtype: 0,
    };
    pub const X86_64: Cpu = Cpu {
        kind: CPU_ARCH_ABI64 | CPU_TYPE_X86,
        subtype: 3,
    };

    fn of(kind: u32, subtype: u32) -> Cpu {
        Cpu {
            kind,
            subtype: subtype & !CPU_SUBTYPE_MASK,
        }
    }

    /// Its name in the spelling of `lipo` and `cc -arch`.
    pub fn name(self) -> String {
        match (self.kind, self.subtype) {
            (kind, 2) if kind == Cpu::ARM64.kind => "arm64e".into(),
            (kind, _) if kind == Cpu::ARM64.kind => "arm64".into(),
            (kind, 8) if kind == Cpu::X86_64.kind => "x86_64h".into(),
            (kind, _) if kind == Cpu::X86_64.kind => "x86_64".into(),
            (CPU_TYPE_X86, _) => "i386".into(),
            (CPU_TYPE_ARM, _) => "arm".into(),
            (kind, subtype) => format!("cputype {kind} subtype {subtype}"),
        }
    }

    /// The page size of the code signature: 16 KB for Apple silicon and 4 KB
    /// for Intel, as in `codesign`.
    pub(crate) fn signature_page(self) -> usize {
        if self.kind == Cpu::ARM64.kind {
            16384
        } else {
            4096
        }
    }
}

/// One slice of a file.
#[derive(Clone, Copy, Debug)]
pub struct Slice<'a> {
    pub cpu: Cpu,
    pub bytes: &'a [u8],
    /// The power of two its offset in a universal file is a multiple of.
    align: u32,
}

pub(crate) fn u32_le(bytes: &[u8], at: usize) -> Result<u32, String> {
    bytes
        .get(at..at + 4)
        .map(|word| u32::from_le_bytes(word.try_into().unwrap()))
        .ok_or_else(|| "the file ends inside its header".to_string())
}

pub(crate) fn u64_le(bytes: &[u8], at: usize) -> Result<u64, String> {
    bytes
        .get(at..at + 8)
        .map(|word| u64::from_le_bytes(word.try_into().unwrap()))
        .ok_or_else(|| "the file ends inside its header".to_string())
}

fn u32_be(bytes: &[u8], at: usize) -> Result<u32, String> {
    bytes
        .get(at..at + 4)
        .map(|word| u32::from_be_bytes(word.try_into().unwrap()))
        .ok_or_else(|| "the file ends inside its universal header".to_string())
}

fn u64_be(bytes: &[u8], at: usize) -> Result<u64, String> {
    bytes
        .get(at..at + 8)
        .map(|word| u64::from_be_bytes(word.try_into().unwrap()))
        .ok_or_else(|| "the file ends inside its universal header".to_string())
}

/// Whether `bytes` start like a Mach-O file, with one slice or several. A Java
/// class file has the same magic as a universal file. Like `file`, we tell
/// them apart by the field with the class version or the slice count.
pub fn is_mach_o(bytes: &[u8]) -> bool {
    match (u32_be(bytes, 0), u32_le(bytes, 0)) {
        (Ok(FAT_MAGIC | FAT_MAGIC_64), _) => {
            u32_be(bytes, 4).is_ok_and(|count| (1..20).contains(&count))
        }
        (_, Ok(MH_MAGIC | MH_MAGIC_64)) => true,
        _ => false,
    }
}

/// The slices in `file`, in their order in the file.
pub fn slices(file: &[u8]) -> Result<Vec<Slice<'_>>, String> {
    let magic = u32_be(file, 0).map_err(|_| "not a Mach-O file".to_string())?;
    let wide = match magic {
        FAT_MAGIC => false,
        FAT_MAGIC_64 => true,
        _ => {
            let header = Header::read(file)?;
            return Ok(vec![Slice {
                cpu: header.cpu,
                bytes: file,
                align: alignment(file)?,
            }]);
        }
    };
    let count = u32_be(file, 4)? as usize;
    let entry = if wide { 32 } else { 20 };
    (0..count)
        .map(|index| {
            let at = 8 + index * entry;
            let cpu = Cpu::of(u32_be(file, at)?, u32_be(file, at + 4)?);
            let (offset, size, align) = if wide {
                (
                    u64_be(file, at + 8)?,
                    u64_be(file, at + 16)?,
                    u32_be(file, at + 24)?,
                )
            } else {
                (
                    u32_be(file, at + 8)? as u64,
                    u32_be(file, at + 12)? as u64,
                    u32_be(file, at + 16)?,
                )
            };
            let bytes = usize::try_from(offset)
                .ok()
                .zip(usize::try_from(size).ok())
                .and_then(|(offset, size)| file.get(offset..offset.checked_add(size)?))
                .ok_or_else(|| format!("its {} slice is past the end of the file", cpu.name()))?;
            let header = Header::read(bytes)?;
            if header.cpu != cpu {
                return Err(format!(
                    "its {} slice says it is for {}",
                    cpu.name(),
                    header.cpu.name()
                ));
            }
            Ok(Slice { cpu, bytes, align })
        })
        .collect()
}

/// One universal file with each of `parts`, which are one-slice files, laid
/// out as with `lipo -create`: each slice aligned as its segments are, with
/// Apple silicon last and the others by alignment.
pub fn join(parts: &[&[u8]]) -> Result<Vec<u8>, String> {
    let mut slices = parts
        .iter()
        .map(|part| {
            let header = Header::read(part)?;
            Ok(Slice {
                cpu: header.cpu,
                bytes: part,
                align: alignment(part)?,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    slices.sort_by(|left, right| {
        let apple = |slice: &Slice<'_>| slice.cpu.kind == Cpu::ARM64.kind;
        if left.cpu.kind == right.cpu.kind {
            left.cpu.subtype.cmp(&right.cpu.subtype)
        } else {
            apple(left)
                .cmp(&apple(right))
                .then(left.align.cmp(&right.align))
        }
    });
    if let Some(pair) = slices.windows(2).find(|pair| pair[0].cpu == pair[1].cpu) {
        return Err(format!("two slices are for {}", pair[0].cpu.name()));
    }
    Ok(universal(&slices))
}

/// The alignment of a slice in a universal file, as in `lipo`: the smallest
/// power of two any of its segments' addresses is a multiple of, from 4
/// bytes to 32 KB.
fn alignment(slice: &[u8]) -> Result<u32, String> {
    let (_, commands) = commands(slice)?;
    let mut align = MAXIMUM_ALIGNMENT;
    for command in commands {
        let address = match command.cmd {
            LC_SEGMENT_64 => u64_le(command.bytes, 24)?,
            LC_SEGMENT => u32_le(command.bytes, 24)? as u64,
            _ => continue,
        };
        if address != 0 {
            align = align.min(address.trailing_zeros().clamp(2, MAXIMUM_ALIGNMENT));
        }
    }
    Ok(align)
}

/// A universal file of `slices`, in their order, each at its alignment.
fn universal(slices: &[Slice<'_>]) -> Vec<u8> {
    let mut file = Vec::new();
    file.extend(FAT_MAGIC.to_be_bytes());
    file.extend((slices.len() as u32).to_be_bytes());
    let mut offset = 8 + 20 * slices.len();
    let mut placed = Vec::new();
    for slice in slices {
        let alignment = 1usize << slice.align;
        offset = offset.div_ceil(alignment) * alignment;
        for word in [
            slice.cpu.kind,
            slice.cpu.subtype,
            offset as u32,
            slice.bytes.len() as u32,
            slice.align,
        ] {
            file.extend(word.to_be_bytes());
        }
        placed.push(offset);
        offset += slice.bytes.len();
    }
    for (slice, at) in slices.iter().zip(placed) {
        file.resize(at, 0);
        file.extend_from_slice(slice.bytes);
    }
    file
}

/// `file` with each of its slices replaced by the result of `edit`. A
/// one-slice file stays one, and the slices of a universal file keep their
/// order and alignment.
pub fn map_slices(
    file: &[u8],
    mut edit: impl FnMut(Cpu, &[u8]) -> Result<Vec<u8>, String>,
) -> Result<Vec<u8>, String> {
    let slices = slices(file)?;
    if u32_le(file, 0).is_ok_and(|magic| matches!(magic, MH_MAGIC | MH_MAGIC_64)) {
        return edit(slices[0].cpu, file);
    }
    let edited = slices
        .iter()
        .map(|slice| edit(slice.cpu, slice.bytes))
        .collect::<Result<Vec<_>, _>>()?;
    let rebuilt: Vec<Slice<'_>> = slices
        .iter()
        .zip(&edited)
        .map(|(slice, bytes)| Slice {
            cpu: slice.cpu,
            bytes,
            align: slice.align,
        })
        .collect();
    Ok(universal(&rebuilt))
}

/// A slice's header.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Header {
    pub cpu: Cpu,
    pub wide: bool,
    pub filetype: u32,
    pub ncmds: u32,
    pub sizeofcmds: u32,
}

/// A main program, as opposed to a library.
pub(crate) const MH_EXECUTE: u32 = 2;

impl Header {
    pub fn read(slice: &[u8]) -> Result<Header, String> {
        let wide = match u32_le(slice, 0) {
            Ok(MH_MAGIC_64) => true,
            Ok(MH_MAGIC) => false,
            _ => return Err("not a Mach-O file".into()),
        };
        Ok(Header {
            cpu: Cpu::of(u32_le(slice, 4)?, u32_le(slice, 8)?),
            wide,
            filetype: u32_le(slice, 12)?,
            ncmds: u32_le(slice, 16)?,
            sizeofcmds: u32_le(slice, 20)?,
        })
    }

    /// Where the load commands start.
    pub fn size(&self) -> usize {
        if self.wide {
            32
        } else {
            28
        }
    }
}

/// One load command of a slice: its kind, where it starts and its bytes.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Command<'a> {
    pub cmd: u32,
    pub offset: usize,
    pub bytes: &'a [u8],
}

/// Every load command of `slice`, in order.
pub(crate) fn commands(slice: &[u8]) -> Result<(Header, Vec<Command<'_>>), String> {
    let header = Header::read(slice)?;
    let end = header.size() + header.sizeofcmds as usize;
    if end > slice.len() {
        return Err("its load commands run past the end of the file".into());
    }
    let mut offset = header.size();
    let mut found = Vec::with_capacity(header.ncmds as usize);
    for _ in 0..header.ncmds {
        let cmd = u32_le(slice, offset)?;
        let size = u32_le(slice, offset + 4)? as usize;
        if size < 8 || offset + size > end {
            return Err(format!("a load command at {offset} has a size of {size}"));
        }
        found.push(Command {
            cmd,
            offset,
            bytes: &slice[offset..offset + size],
        });
        offset += size;
    }
    Ok((header, found))
}

fn loads_a_library(cmd: u32) -> bool {
    matches!(
        cmd,
        LC_LOAD_DYLIB
            | LC_LOAD_WEAK_DYLIB
            | LC_REEXPORT_DYLIB
            | LC_LAZY_LOAD_DYLIB
            | LC_LOAD_UPWARD_DYLIB
    )
}

/// The library a dylib command names.
fn library_name(command: &Command<'_>) -> Result<String, String> {
    let at = u32_le(command.bytes, 8)? as usize;
    let text = command
        .bytes
        .get(at..)
        .ok_or("a library's name is outside its load command")?;
    let end = text
        .iter()
        .position(|&byte| byte == 0)
        .unwrap_or(text.len());
    Ok(String::from_utf8_lossy(&text[..end]).into_owned())
}

/// The libraries `file` depends on, each once, in the order of its slices.
/// This does not include the install name of a library itself.
pub fn dependencies(file: &[u8]) -> Result<Vec<String>, String> {
    let mut own = Vec::new();
    let mut loaded: Vec<String> = Vec::new();
    for slice in slices(file)? {
        for command in commands(slice.bytes)?.1 {
            if command.cmd == LC_ID_DYLIB {
                own.push(library_name(&command)?);
            } else if loads_a_library(command.cmd) {
                let name = library_name(&command)?;
                if !loaded.contains(&name) {
                    loaded.push(name);
                }
            }
        }
    }
    loaded.retain(|name| !own.contains(name));
    Ok(loaded)
}

/// The oldest macOS for each slice of `file`, as a list of numbers
/// (`[11, 0]`, `[10, 13, 6]`): the minos of LC_BUILD_VERSION, or the version
/// of the older LC_VERSION_MIN_MACOSX, which Intel cores built for old
/// systems still contain.
pub fn minimum_systems(file: &[u8]) -> Result<Vec<Vec<u32>>, String> {
    let mut found = Vec::new();
    for slice in slices(file)? {
        for command in commands(slice.bytes)?.1 {
            let version = match command.cmd {
                LC_BUILD_VERSION => u32_le(command.bytes, 12)?,
                LC_VERSION_MIN_MACOSX => u32_le(command.bytes, 8)?,
                _ => continue,
            };
            let (major, minor, patch) = (version >> 16, (version >> 8) & 0xff, version & 0xff);
            found.push(if patch == 0 {
                vec![major, minor]
            } else {
                vec![major, minor, patch]
            });
        }
    }
    Ok(found)
}

/// `file` with each dependency renamed as in `rename`, and with `own` as
/// its install name when given, as with `install_name_tool -change` and
/// `-id`. The signature is then no longer valid, so we have to sign the
/// file again.
pub fn rename_libraries(
    file: &[u8],
    rename: impl Fn(&str) -> Option<String>,
    own: Option<&str>,
) -> Result<Vec<u8>, String> {
    map_slices(file, |cpu, slice| {
        let (header, commands) = commands(slice)?;
        let mut changed = false;
        let mut rebuilt = Vec::with_capacity(header.sizeofcmds as usize);
        for command in &commands {
            let new_name = if command.cmd == LC_ID_DYLIB {
                own.map(str::to_owned)
            } else if loads_a_library(command.cmd) {
                rename(&library_name(command)?)
            } else {
                None
            };
            match new_name {
                Some(name) if name != library_name(command)? => {
                    changed = true;
                    rebuilt.extend(library_command(command, &name, header.wide));
                }
                _ => rebuilt.extend_from_slice(command.bytes),
            }
        }
        if !changed {
            return Ok(slice.to_vec());
        }
        let room = first_content(slice, &header, &commands)?;
        if header.size() + rebuilt.len() > room {
            return Err(format!(
                "its {} slice has no room for the longer library names",
                cpu.name()
            ));
        }
        let mut edited = slice.to_vec();
        let old_end = header.size() + header.sizeofcmds as usize;
        let end = old_end.max(header.size() + rebuilt.len());
        edited[header.size()..end].fill(0);
        edited[header.size()..header.size() + rebuilt.len()].copy_from_slice(&rebuilt);
        edited[20..24].copy_from_slice(&(rebuilt.len() as u32).to_le_bytes());
        Ok(edited)
    })
}

/// A dylib command like `command`, naming `name`.
fn library_command(command: &Command<'_>, name: &str, wide: bool) -> Vec<u8> {
    let align = if wide { 8 } else { 4 };
    let size = (24 + name.len() + 1).div_ceil(align) * align;
    let mut bytes = Vec::with_capacity(size);
    bytes.extend(command.cmd.to_le_bytes());
    bytes.extend((size as u32).to_le_bytes());
    bytes.extend(24u32.to_le_bytes());
    // Its timestamp and its current and compatibility versions stay.
    bytes.extend_from_slice(&command.bytes[12..24]);
    bytes.extend(name.as_bytes());
    bytes.resize(size, 0);
    bytes
}

/// Where the first section's contents start: load commands may grow up to
/// there and no further.
pub(crate) fn first_content(
    slice: &[u8],
    header: &Header,
    commands: &[Command<'_>],
) -> Result<usize, String> {
    let mut first = slice.len();
    for command in commands {
        let (sections, count, section_size, offset_at) = match command.cmd {
            LC_SEGMENT_64 => (72, u32_le(command.bytes, 64)?, 80, 48),
            LC_SEGMENT => (56, u32_le(command.bytes, 48)?, 68, 40),
            _ => continue,
        };
        for index in 0..count as usize {
            let offset =
                u32_le(command.bytes, sections + index * section_size + offset_at)? as usize;
            if offset != 0 {
                first = first.min(offset);
            }
        }
    }
    if first < header.size() {
        return Err("a section starts inside the header".into());
    }
    Ok(first)
}

#[cfg(test)]
mod tests;
