//! The declaration files of the player, the `*.inc` next to the menu C++,
//! read with one `MACRO(field, ...)` per line, as the compiler reads them.

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
