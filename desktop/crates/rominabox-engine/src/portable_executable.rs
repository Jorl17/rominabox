//! The libraries that a Windows program or library imports, from its file.
//!
//! At export we check the core against the libraries that every Windows
//! machine has, so a game never depends on a DLL that is not in its folder. We
//! read only the import table: the headers, the section map and the names.

/// The DLL names `image` imports, as written in its import table.
pub fn imports(image: &[u8]) -> Result<Vec<String>, String> {
    let pe = u32_at(image, 0x3c)? as usize;
    if image.get(0..2) != Some(b"MZ") || image.get(pe..pe + 4) != Some(b"PE\0\0") {
        return Err("not a Windows program or library".into());
    }
    let coff = pe + 4;
    let sections = u16_at(image, coff + 2)? as usize;
    let optional_size = u16_at(image, coff + 16)? as usize;
    let optional = coff + 20;
    // The data directories follow the optional header's fixed part, which is
    // longer for a 64-bit image. The import table is directory 1.
    let directories = match u16_at(image, optional)? {
        0x10b => optional + 96,
        0x20b => optional + 112,
        magic => return Err(format!("unknown optional header {magic:#x}")),
    };
    let count = u32_at(image, directories - 4)? as usize;
    if count < 2 {
        return Ok(Vec::new());
    }
    let table = u32_at(image, directories + 8)?;
    if table == 0 {
        return Ok(Vec::new());
    }
    let section_table = optional + optional_size;
    let offset = |rva: u32| -> Result<usize, String> {
        for index in 0..sections {
            let header = section_table + index * 40;
            let size = u32_at(image, header + 8)?.max(u32_at(image, header + 16)?);
            let address = u32_at(image, header + 12)?;
            let raw = u32_at(image, header + 20)?;
            if rva >= address && rva - address < size {
                return Ok((rva - address + raw) as usize);
            }
        }
        Err(format!("address {rva:#x} is in no section"))
    };
    let mut names = Vec::new();
    let mut descriptor = offset(table)?;
    loop {
        let name = u32_at(image, descriptor + 12)?;
        if name == 0 {
            return Ok(names);
        }
        let start = offset(name)?;
        let end = image
            .get(start..)
            .ok_or("an import name is past the end of the file")?
            .iter()
            .position(|&byte| byte == 0)
            .ok_or("an import name does not end")?;
        names.push(String::from_utf8_lossy(&image[start..start + end]).into_owned());
        descriptor += 20;
    }
}

fn u16_at(image: &[u8], at: usize) -> Result<u16, String> {
    image
        .get(at..at + 2)
        .map(|bytes| u16::from_le_bytes([bytes[0], bytes[1]]))
        .ok_or_else(|| "the file ends inside its headers".into())
}

fn u32_at(image: &[u8], at: usize) -> Result<u32, String> {
    image
        .get(at..at + 4)
        .map(|bytes| u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
        .ok_or_else(|| "the file ends inside its headers".into())
}

#[cfg(test)]
mod tests {
    use super::imports;

    /// A 64-bit image with one section that contains an import table naming
    /// `dlls`, laid out the way a linker writes it.
    fn image(dlls: &[&str]) -> Vec<u8> {
        let mut image = vec![0u8; 0x400];
        image[0..2].copy_from_slice(b"MZ");
        image[0x3c..0x40].copy_from_slice(&0x80u32.to_le_bytes());
        image[0x80..0x84].copy_from_slice(b"PE\0\0");
        let coff = 0x84;
        image[coff + 2..coff + 4].copy_from_slice(&1u16.to_le_bytes());
        image[coff + 16..coff + 18].copy_from_slice(&240u16.to_le_bytes());
        let optional = coff + 20;
        image[optional..optional + 2].copy_from_slice(&0x20bu16.to_le_bytes());
        let directories = optional + 112;
        image[directories - 4..directories].copy_from_slice(&16u32.to_le_bytes());
        // The section: virtual 0x1000, raw 0x200, 0x200 bytes.
        let section = optional + 240;
        image[section..section + 5].copy_from_slice(b".idat");
        image[section + 8..section + 12].copy_from_slice(&0x200u32.to_le_bytes());
        image[section + 12..section + 16].copy_from_slice(&0x1000u32.to_le_bytes());
        image[section + 16..section + 20].copy_from_slice(&0x200u32.to_le_bytes());
        image[section + 20..section + 24].copy_from_slice(&0x200u32.to_le_bytes());
        image[directories + 8..directories + 12].copy_from_slice(&0x1000u32.to_le_bytes());
        let mut name_at = 0x100;
        for (index, dll) in dlls.iter().enumerate() {
            let descriptor = 0x200 + index * 20;
            image[descriptor + 12..descriptor + 16]
                .copy_from_slice(&(0x1000 + name_at as u32).to_le_bytes());
            image[0x200 + name_at..0x200 + name_at + dll.len()].copy_from_slice(dll.as_bytes());
            name_at += dll.len() + 1;
        }
        image
    }

    #[test]
    fn reads_every_imported_library_by_name() {
        assert_eq!(
            imports(&image(&[
                "KERNEL32.dll",
                "msvcrt.dll",
                "libwinpthread-1.dll"
            ]))
            .unwrap(),
            ["KERNEL32.dll", "msvcrt.dll", "libwinpthread-1.dll"]
        );
        assert_eq!(imports(&image(&[])).unwrap(), Vec::<String>::new());
    }

    #[test]
    fn refuses_what_is_not_a_windows_image() {
        assert!(imports(
            b"\x7fELF and more bytes than a header needs, and then some more padding here"
        )
        .is_err());
        let mut truncated = image(&["KERNEL32.dll"]);
        truncated.truncate(0x90);
        assert!(imports(&truncated).is_err());
    }
}
