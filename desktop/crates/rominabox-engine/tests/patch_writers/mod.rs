//! Patch writers for the tests. We write IPS, UPS and BPS from the
//! published layouts of the formats, each turning `source` into `target`.
//! In a test file, import only the ones in use.
#![allow(dead_code)]

/// The variable-length number in UPS and BPS (byuu's encoding).
fn number(mut value: u64, out: &mut Vec<u8>) {
    loop {
        let low = (value & 0x7f) as u8;
        value >>= 7;
        if value == 0 {
            out.push(0x80 | low);
            return;
        }
        out.push(low);
        value -= 1;
    }
}

fn crc(bytes: &[u8]) -> [u8; 4] {
    crc32fast::hash(bytes).to_le_bytes()
}

/// "PATCH", then records of a 24-bit offset, a 16-bit length and the bytes, then "EOF".
pub fn ips(source: &[u8], target: &[u8]) -> Vec<u8> {
    let mut patch = b"PATCH".to_vec();
    let mut offset = 0;
    while offset < target.len() {
        if source.get(offset) == Some(&target[offset]) {
            offset += 1;
            continue;
        }
        let start = offset;
        while offset < target.len() && source.get(offset) != Some(&target[offset]) && offset - start < 0xFFFF {
            offset += 1;
        }
        patch.extend(&(start as u32).to_be_bytes()[1..]);
        patch.extend(((offset - start) as u16).to_be_bytes());
        patch.extend(&target[start..offset]);
    }
    patch.extend(b"EOF");
    patch
}

/// "UPS1", both sizes, runs of source bytes to skip each followed by the bytes
/// that differ XORed with the source's and a zero, then the three CRC-32s.
pub fn ups(source: &[u8], target: &[u8]) -> Vec<u8> {
    let mut patch = b"UPS1".to_vec();
    number(source.len() as u64, &mut patch);
    number(target.len() as u64, &mut patch);
    let byte = |bytes: &[u8], at: usize| bytes.get(at).copied().unwrap_or(0);
    let (mut offset, mut relative) = (0, 0);
    while offset < target.len() {
        if byte(source, offset) == target[offset] {
            offset += 1;
            continue;
        }
        number((offset - relative) as u64, &mut patch);
        while offset < target.len() && byte(source, offset) != target[offset] {
            patch.push(byte(source, offset) ^ target[offset]);
            offset += 1;
        }
        patch.push(0);
        offset += 1;
        relative = offset;
    }
    patch.extend(crc(source));
    patch.extend(crc(target));
    let whole = crc(&patch);
    patch.extend(whole);
    patch
}

/// "BPS1", both sizes, no metadata, one TargetRead of the whole game, then the
/// three CRC-32s.
pub fn bps(source: &[u8], target: &[u8]) -> Vec<u8> {
    let mut patch = b"BPS1".to_vec();
    number(source.len() as u64, &mut patch);
    number(target.len() as u64, &mut patch);
    number(0, &mut patch);
    number((((target.len() - 1) as u64) << 2) | 1, &mut patch);
    patch.extend(target);
    patch.extend(crc(source));
    patch.extend(crc(target));
    let whole = crc(&patch);
    patch.extend(whole);
    patch
}
