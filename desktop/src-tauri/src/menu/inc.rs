//! The declaration files of the player, the `*.inc` next to the menu C++,
//! read with one `MACRO(field, ...)` per line, as the compiler reads them.

/// Whether `bytes` contains `prefix` at `at`.
const fn holds(bytes: &[u8], at: usize, prefix: &[u8]) -> bool {
    if at + prefix.len() > bytes.len() {
        return false;
    }
    let mut index = 0;
    while index < prefix.len() {
        if bytes[at + index] != prefix[index] {
            return false;
        }
        index += 1;
    }
    true
}

/// The quoted field number `field` of the line `MACRO(name, ...)` in
/// `source`, for any of `macros`, as the compiler of the player reads it.
///
/// This is a `const fn`, so we read a name in a `const` block when the
/// exporter compiles, and the build fails on a name that `source` does not
/// declare, as the build of the player does.
pub(crate) const fn quoted(
    source: &'static str,
    macros: &[&str],
    name: &str,
    field: usize,
) -> &'static str {
    let bytes = source.as_bytes();
    let mut line = 0;
    while line < bytes.len() {
        let mut at = line;
        while at < bytes.len() && (bytes[at] == b' ' || bytes[at] == b'\t') {
            at += 1;
        }
        let mut which = 0;
        while which < macros.len() {
            let opening = macros[which].as_bytes();
            let named = at + opening.len() + 1;
            if holds(bytes, at, opening)
                && holds(bytes, at + opening.len(), b"(")
                && holds(bytes, named, name.as_bytes())
                && holds(bytes, named + name.len(), b",")
            {
                let mut cursor = named + name.len();
                let mut skipped = 0;
                loop {
                    while cursor < bytes.len() && bytes[cursor] != b'"' {
                        if bytes[cursor] == b')' || bytes[cursor] == b'\n' {
                            panic!("the declaration has fewer quoted fields than asked for");
                        }
                        cursor += 1;
                    }
                    let start = cursor + 1;
                    let mut end = start;
                    while end < bytes.len() && bytes[end] != b'"' {
                        if bytes[end] == b'\n' {
                            panic!("a quoted field is never closed");
                        }
                        end += 1;
                    }
                    if skipped == field {
                        let (_, rest) = bytes.split_at(start);
                        let (value, _) = rest.split_at(end - start);
                        return match core::str::from_utf8(value) {
                            Ok(value) => value,
                            Err(_) => panic!("a quoted field is not UTF-8"),
                        };
                    }
                    skipped += 1;
                    cursor = end + 1;
                }
            }
            which += 1;
        }
        while line < bytes.len() && bytes[line] != b'\n' {
            line += 1;
        }
        line += 1;
    }
    panic!("the player's declarations do not declare this name")
}

/// The fields of every `macro_name(...)` line in `source`, in order, each
/// trimmed and without its quotes. A field cannot contain a comma.
pub(crate) fn declarations<'a>(
    source: &'a str,
    macro_name: &str,
) -> impl Iterator<Item = Vec<String>> + 'a {
    let opening = format!("{macro_name}(");
    source.lines().filter_map(move |line| {
        let fields = line.trim().strip_prefix(&opening)?.strip_suffix(')')?;
        Some(
            fields
                .split(',')
                .map(|field| field.trim().trim_matches('"').to_string())
                .collect(),
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const SOURCE: &str = "/* RIB_KEY(Old, \"commented\") */\n\
                          #define RIB_KEY(name, key)\n\
                          RIB_KEY(Screens, \"screens\")\n\
                          RIB_KEYS(Screen, \"screen_\")\n  \
                          RIB_FILES(Scene, \"scene-\", \".rml\")\n";

    #[test]
    fn a_name_reads_its_own_declaration_and_field() {
        assert_eq!(quoted(SOURCE, &["RIB_KEY"], "Screens", 0), "screens");
        // RIB_KEY is not RIB_KEYS, and Screen is not Screens.
        assert_eq!(quoted(SOURCE, &["RIB_KEYS"], "Screen", 0), "screen_");
        assert_eq!(
            quoted(SOURCE, &["RIB_KEY", "RIB_FILES"], "Scene", 1),
            ".rml"
        );
    }

    #[test]
    #[should_panic(expected = "do not declare this name")]
    fn a_name_only_a_comment_mentions_is_not_declared() {
        quoted(SOURCE, &["RIB_KEY"], "Old", 0);
    }
}
