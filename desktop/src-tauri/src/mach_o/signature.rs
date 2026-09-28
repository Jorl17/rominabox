//! An ad hoc code signature, the same as from `codesign --force --sign -`.
//!
//! The signature is at the end of each slice, inside its `__LINKEDIT`
//! segment, where `LC_CODE_SIGNATURE` points. It is a SuperBlob of blobs:
//! the CodeDirectory, with the hash of every page of the slice before the
//! signature and, in its special slots, of what we seal the code with (the
//! app's Info.plist and resources, its requirements, its entitlements as XML
//! and as DER), the requirements, which are none for ad hoc code, the
//! entitlements, and an empty CMS signature, because there is no certificate.
//! The hash of the CodeDirectory (its "cdhash") makes the code trusted.

use super::entitlements::Entitlements;
use super::{
    commands, first_content, map_slices, slices, u32_le, u64_le, Command, Cpu, LC_CODE_SIGNATURE,
    LC_SEGMENT_64, LC_UUID, MH_EXECUTE,
};
use sha2::{Digest, Sha256};

const SUPERBLOB: u32 = 0xfade_0cc0;
const CODE_DIRECTORY: u32 = 0xfade_0c02;
const REQUIREMENTS: u32 = 0xfade_0c01;
const ENTITLEMENTS: u32 = 0xfade_7171;
const DER_ENTITLEMENTS: u32 = 0xfade_7172;
const SIGNATURE_WRAPPER: u32 = 0xfade_0b01;

/// The slots of a SuperBlob, in their order in it.
const SLOT_CODE_DIRECTORY: u32 = 0;
const SLOT_REQUIREMENTS: u32 = 2;
const SLOT_ENTITLEMENTS: u32 = 5;
const SLOT_DER_ENTITLEMENTS: u32 = 7;
const SLOT_SIGNATURE: u32 = 0x10000;

/// CodeDirectory version 0x20400 contains the executable segment, the
/// newest field required in an ad hoc signature, as in `codesign`.
const VERSION: u32 = 0x20400;
const HEADER: usize = 88;
const CS_ADHOC: u32 = 0x2;
const CS_EXECSEG_MAIN_BINARY: u64 = 0x1;
const HASH_SHA256: u8 = 2;
const HASH_SIZE: usize = 32;
/// We round `__LINKEDIT` up to this page size after adding the signature.
const SEGMENT_PAGE: u64 = 0x4000;

/// What we seal with the code in a signature. We seal the main program of an
/// app with its Info.plist, its resources (CodeResources) and its
/// entitlements, and any other code with none of them.
#[derive(Clone, Copy, Debug, Default)]
pub struct Seal<'a> {
    pub identifier: &'a str,
    pub info_plist: Option<&'a [u8]>,
    pub resources: Option<&'a [u8]>,
    pub entitlements: Option<&'a Entitlements>,
}

/// `file` signed ad hoc, every slice, any signature it had replaced.
pub fn sign(file: &[u8], seal: &Seal<'_>) -> Result<Vec<u8>, String> {
    map_slices(file, |cpu, slice| sign_slice(cpu, slice, seal))
}

/// The name that `codesign` uses for code signed ad hoc outside a bundle: the
/// file's name without its extension, then "-", then "UUID" and the UUID of
/// its first slice that has one, in hexadecimal.
pub fn identifier_for(name: &str, file: &[u8]) -> String {
    let stem = name.split('.').next().unwrap_or(name);
    let uuid = slices(file).ok().and_then(|slices| {
        slices.iter().find_map(|slice| {
            let (_, commands) = commands(slice.bytes).ok()?;
            let command = commands.iter().find(|command| command.cmd == LC_UUID)?;
            command.bytes.get(8..24).map(<[u8]>::to_vec)
        })
    });
    match uuid {
        Some(uuid) => format!("{stem}-{}{}", hex(b"UUID"), hex(&uuid)),
        None => stem.to_string(),
    }
}

/// The cdhash of each slice's signature, in the file's order: the first 20
/// bytes of the SHA-256 of its CodeDirectory.
pub fn code_directory_hashes(file: &[u8]) -> Result<Vec<(Cpu, [u8; 20])>, String> {
    slices(file)?
        .iter()
        .map(|slice| {
            let (_, commands) = commands(slice.bytes)?;
            let signature = commands
                .iter()
                .find(|command| command.cmd == LC_CODE_SIGNATURE)
                .ok_or_else(|| format!("its {} slice is not signed", slice.cpu.name()))?;
            let at = u32_le(signature.bytes, 8)? as usize;
            let size = u32_le(signature.bytes, 12)? as usize;
            let blob = slice.bytes.get(at..at + size).ok_or("its signature is past the end of the file")?;
            let directory = blob_in(blob, SLOT_CODE_DIRECTORY)?;
            let mut cdhash = [0u8; 20];
            cdhash.copy_from_slice(&Sha256::digest(directory)[..20]);
            Ok((slice.cpu, cdhash))
        })
        .collect()
}

/// The blob in `slot` of the SuperBlob `superblob`.
fn blob_in(superblob: &[u8], slot: u32) -> Result<&[u8], String> {
    let word = |at: usize| {
        superblob
            .get(at..at + 4)
            .map(|word| u32::from_be_bytes(word.try_into().unwrap()) as usize)
            .ok_or_else(|| "the signature ends inside a blob's header".to_string())
    };
    if word(0)? != SUPERBLOB as usize {
        return Err("the signature is not a SuperBlob".into());
    }
    for index in 0..word(8)? {
        if word(12 + index * 8)? == slot as usize {
            let at = word(16 + index * 8)?;
            let length = word(at + 4)?;
            return superblob.get(at..at + length).ok_or_else(|| "a blob runs past the signature".into());
        }
    }
    Err(format!("the signature has no blob in slot {slot}"))
}

fn sha256(bytes: &[u8]) -> [u8; HASH_SIZE] {
    Sha256::digest(bytes).into()
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// A blob: its magic, its length and `content`, big-endian.
fn blob(magic: u32, content: &[u8]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(8 + content.len());
    bytes.extend(magic.to_be_bytes());
    bytes.extend(((8 + content.len()) as u32).to_be_bytes());
    bytes.extend_from_slice(content);
    bytes
}

fn segment<'a>(commands: &'a [Command<'a>], name: &str) -> Option<&'a Command<'a>> {
    commands.iter().find(|command| {
        command.cmd == LC_SEGMENT_64
            && command.bytes.get(8..24).is_some_and(|field| {
                field.split(|&byte| byte == 0).next() == Some(name.as_bytes())
            })
    })
}

fn put_u32(bytes: &mut [u8], at: usize, value: u32) {
    bytes[at..at + 4].copy_from_slice(&value.to_le_bytes());
}

fn put_u64(bytes: &mut [u8], at: usize, value: u64) {
    bytes[at..at + 8].copy_from_slice(&value.to_le_bytes());
}

fn sign_slice(cpu: Cpu, slice: &[u8], seal: &Seal<'_>) -> Result<Vec<u8>, String> {
    let (header, commands) = commands(slice)?;
    if !header.wide {
        return Err(format!("its {} slice is 32-bit code, which is not signed here", cpu.name()));
    }
    let linkedit = segment(&commands, "__LINKEDIT").ok_or("it has no __LINKEDIT segment")?;
    let text = segment(&commands, "__TEXT").ok_or("it has no __TEXT segment")?;
    let (linkedit_at, linkedit_start) = (linkedit.offset, u64_le(linkedit.bytes, 40)?);
    let (text_start, text_size) = (u64_le(text.bytes, 40)?, u64_le(text.bytes, 48)?);

    // The code the signature covers ends where the signature starts: where
    // the old one started, or, for unsigned code, after everything else,
    // with a new load command pointing there.
    let (mut code, signature_at) = match commands.iter().find(|command| command.cmd == LC_CODE_SIGNATURE) {
        Some(existing) => {
            let start = u32_le(existing.bytes, 8)? as usize;
            let code = slice.get(..start).ok_or("its signature starts past the end of the file")?;
            (code.to_vec(), existing.offset)
        }
        None => {
            let end = header.size() + header.sizeofcmds as usize;
            if end + 16 > first_content(slice, &header, &commands)? {
                return Err(format!("its {} slice has no room for a signature", cpu.name()));
            }
            let linkedit_end = (linkedit_start + u64_le(linkedit.bytes, 48)?) as usize;
            let mut code = slice.to_vec();
            code.resize(linkedit_end.max(slice.len()).div_ceil(16) * 16, 0);
            code[end..end + 8].copy_from_slice(&[LC_CODE_SIGNATURE, 16].map(u32::to_le_bytes).concat());
            put_u32(&mut code, 16, header.ncmds + 1);
            put_u32(&mut code, 20, header.sizeofcmds + 16);
            (code, end)
        }
    };
    let code_limit = code.len();

    let requirements = blob(REQUIREMENTS, &0u32.to_be_bytes());
    let entitlements = seal.entitlements.map(|declared| blob(ENTITLEMENTS, declared.xml().as_bytes()));
    let der = seal.entitlements.map(|declared| blob(DER_ENTITLEMENTS, &declared.der()));
    // Special slot n is at index n - 1: Info.plist, requirements,
    // resources, (application), entitlements, (representation), DER.
    let special = [
        seal.info_plist.map(sha256),
        Some(sha256(&requirements)),
        seal.resources.map(sha256),
        None,
        entitlements.as_deref().map(sha256),
        None,
        der.as_deref().map(sha256),
    ];
    let special_slots = special.iter().rposition(Option::is_some).map_or(0, |last| last + 1);
    let page = cpu.signature_page();
    let code_slots = code_limit.div_ceil(page);
    let identifier = [seal.identifier.as_bytes(), &[0]].concat();
    let directory_length = HEADER + identifier.len() + (special_slots + code_slots) * HASH_SIZE;

    let mut blobs: Vec<(u32, Option<Vec<u8>>)> = vec![(SLOT_CODE_DIRECTORY, None), (SLOT_REQUIREMENTS, Some(requirements))];
    blobs.extend(entitlements.map(|blob| (SLOT_ENTITLEMENTS, Some(blob))));
    blobs.extend(der.map(|blob| (SLOT_DER_ENTITLEMENTS, Some(blob))));
    blobs.push((SLOT_SIGNATURE, Some(blob(SIGNATURE_WRAPPER, &[]))));
    let index_length = 12 + 8 * blobs.len();
    let superblob_length = index_length
        + directory_length
        + blobs.iter().filter_map(|(_, blob)| blob.as_ref()).map(Vec::len).sum::<usize>();
    let reserved = superblob_length.div_ceil(16) * 16;

    // We write the place and length of the signature into the header and
    // grow __LINKEDIT to fit it before we hash the pages of the header.
    put_u32(&mut code, signature_at + 8, code_limit as u32);
    put_u32(&mut code, signature_at + 12, reserved as u32);
    let linkedit_size = (code_limit + reserved) as u64 - linkedit_start;
    put_u64(&mut code, linkedit_at + 48, linkedit_size);
    put_u64(&mut code, linkedit_at + 32, linkedit_size.div_ceil(SEGMENT_PAGE) * SEGMENT_PAGE);

    let mut directory = Vec::with_capacity(directory_length);
    for word in [
        CODE_DIRECTORY,
        directory_length as u32,
        VERSION,
        CS_ADHOC,
        (HEADER + identifier.len() + special_slots * HASH_SIZE) as u32,
        HEADER as u32,
        special_slots as u32,
        code_slots as u32,
        code_limit as u32,
    ] {
        directory.extend(word.to_be_bytes());
    }
    directory.extend([HASH_SIZE as u8, HASH_SHA256, 0, page.trailing_zeros() as u8]);
    // spare2, scatterOffset, teamOffset, spare3: none.
    directory.extend([0u8; 16]);
    let main_binary = if header.filetype == MH_EXECUTE { CS_EXECSEG_MAIN_BINARY } else { 0 };
    for word in [0, text_start, text_size, main_binary] {
        directory.extend(word.to_be_bytes());
    }
    directory.extend(&identifier);
    for slot in special[..special_slots].iter().rev() {
        directory.extend(slot.unwrap_or([0; HASH_SIZE]));
    }
    for page_bytes in code.chunks(page) {
        directory.extend(sha256(page_bytes));
    }
    debug_assert_eq!(directory.len(), directory_length);

    let mut superblob = Vec::with_capacity(reserved);
    for word in [SUPERBLOB, superblob_length as u32, blobs.len() as u32] {
        superblob.extend(word.to_be_bytes());
    }
    let mut at = index_length;
    for (slot, blob) in &blobs {
        superblob.extend(slot.to_be_bytes());
        superblob.extend((at as u32).to_be_bytes());
        at += blob.as_ref().map_or(directory_length, Vec::len);
    }
    for (_, blob) in &blobs {
        superblob.extend(blob.as_deref().unwrap_or(&directory));
    }
    code.extend(superblob);
    code.resize(code_limit + reserved, 0);
    Ok(code)
}
