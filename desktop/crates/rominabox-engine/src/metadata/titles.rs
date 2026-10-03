//! Game titles from file names, cartridge headers and catalogue names.

use super::*;

pub(super) const WEAK_FILENAMES: &[&str] = &[
    "GAME",
    "ROM",
    "CART",
    "UPLOAD",
    "UNTITLED",
    "CARTRIDGE",
    "FILE",
    "IMAGE",
    "DUMP",
    "DISC",
    "TRACK",
];

/// We use the header text instead of the filename only when the filename is
/// not more complete. "SONIC ADVANC" must not replace "Sonic Advance (Europe)".
pub(super) fn header_is_better(filename_title: &str, header_title: &str) -> bool {
    let file_key = alnum_upper(filename_title);
    let head_key = alnum_upper(header_title);
    if head_key
        .chars()
        .filter(|character| character.is_ascii_alphabetic())
        .count()
        < 3
    {
        return false;
    }
    if file_key.len() < 4 || WEAK_FILENAMES.contains(&file_key.as_str()) {
        return true;
    }
    let common = file_key
        .bytes()
        .zip(head_key.bytes())
        .take_while(|(left, right)| left == right)
        .count();
    if common >= 8 && file_key.len() > head_key.len() {
        return false;
    }
    if head_key.starts_with(&file_key) && head_key.len() > file_key.len() {
        return true;
    }
    common < 4 && head_key.len() > file_key.len()
}

pub(super) fn alnum_upper(value: &str) -> String {
    value
        .chars()
        .filter(|character| character.is_ascii_alphanumeric())
        .map(|character| character.to_ascii_uppercase())
        .collect()
}

pub(super) fn ascii_title(header: &[u8], start: usize, end: usize) -> Option<String> {
    let bytes = header.get(start..header.len().min(end))?;
    let title: String = bytes
        .iter()
        .take_while(|&&byte| byte != 0)
        .map(|&byte| {
            if byte.is_ascii_graphic() || byte == b' ' {
                byte as char
            } else {
                ' '
            }
        })
        .collect();
    let clean = clean_title(&title);
    (!clean.is_empty()).then_some(clean)
}

pub(super) fn filename_title(filename: &str) -> String {
    let stem = Path::new(filename)
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or(filename);
    let title = clean_title(stem);
    if title.is_empty() {
        "Untitled game".into()
    } else {
        title
    }
}

pub(super) fn display_title(catalog_name: &str) -> String {
    let mut groups = Vec::new();
    let mut base_end = catalog_name.len();
    while catalog_name[..base_end].ends_with(')') {
        let Some(open) = catalog_name[..base_end].rfind(" (") else {
            break;
        };
        groups.push(&catalog_name[open + 2..base_end - 1]);
        base_end = open;
    }
    groups.reverse();
    let mut title = catalog_name[..base_end].to_owned();
    for group in groups {
        if !is_catalog_tag(group) {
            title.push_str(" (");
            title.push_str(group);
            title.push(')');
        }
    }
    clean_title(&title)
}

pub(super) fn is_catalog_tag(group: &str) -> bool {
    const REGIONS: &[&str] = &[
        "Australia",
        "Brazil",
        "Canada",
        "China",
        "Europe",
        "France",
        "Germany",
        "Italy",
        "Japan",
        "Korea",
        "Netherlands",
        "Russia",
        "Spain",
        "Sweden",
        "Taiwan",
        "USA",
        "World",
    ];
    let parts: Vec<_> = group.split(',').map(str::trim).collect();
    if !parts.is_empty() && parts.iter().all(|part| REGIONS.contains(part)) {
        return true;
    }
    if artwork::is_language_tag(group) {
        return true;
    }
    let lower = group.to_ascii_lowercase();
    lower.starts_with("rev ")
        || lower.starts_with("revision ")
        || lower.starts_with("beta")
        || lower.starts_with("proto")
        || lower.starts_with("demo")
        || lower.ends_with(" enhanced")
        || matches!(
            lower.as_str(),
            "gb compatible" | "virtual console" | "aftermarket" | "unl"
        )
}

pub(super) fn clean_title(value: &str) -> String {
    value
        .chars()
        .map(|character| {
            if character.is_control() || "<>:\"/\\|?*".contains(character) {
                ' '
            } else {
                character
            }
        })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .trim_matches([' ', '.'])
        .chars()
        .take(100)
        .collect()
}
