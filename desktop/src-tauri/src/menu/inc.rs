//! The declaration files of the player, the `*.inc` next to the menu C++,
//! read with one `MACRO(field, ...)` per line, as the compiler reads them.
//!
//! We use the same code for every lookup. It is made of `const fn`s, so we
//! read a name in a `const` block (`key!`, `file_name!`, `contract!`) when
//! the exporter compiles, and the build fails on a name that the file does
//! not declare, as the build of the player does. The same functions go
//! through the declarations while the exporter runs.

/// One `MACRO(field, ...)` line.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Declaration {
    macro_name: &'static str,
    /// The text between the parentheses.
    fields: &'static str,
}

impl Declaration {
    pub(crate) const fn macro_name(&self) -> &'static str {
        self.macro_name
    }

    /// Field `index`, trimmed and without its quotes, or `None` past the last.
    /// A comma inside quotes is part of its field, as in the compiler.
    pub(crate) const fn field(&self, index: usize) -> Option<&'static str> {
        let mut rest = self.fields.as_bytes();
        let mut at = 0;
        loop {
            let mut end = 0;
            let mut quoted = false;
            while end < rest.len() && (quoted || rest[end] != b',') {
                if rest[end] == b'"' {
                    quoted = !quoted;
                }
                end += 1;
            }
            let (field, tail) = rest.split_at(end);
            if at == index {
                return Some(text(unquoted(field)));
            }
            if tail.is_empty() {
                return None;
            }
            rest = tail.split_at(1).1;
            at += 1;
        }
    }

    /// Every field, in order.
    pub(crate) fn fields(&self) -> Vec<&'static str> {
        (0..).map_while(|index| self.field(index)).collect()
    }

    const fn is_any_of(&self, macros: &[&str]) -> bool {
        let mut which = 0;
        while which < macros.len() {
            if same(self.macro_name.as_bytes(), macros[which].as_bytes()) {
                return true;
            }
            which += 1;
        }
        false
    }
}

/// Every declaration in a file, in order.
#[derive(Clone, Debug)]
pub(crate) struct Declarations {
    rest: &'static [u8],
}

/// The declarations of `source`.
pub(crate) const fn declarations(source: &'static str) -> Declarations {
    Declarations {
        rest: source.as_bytes(),
    }
}

impl Declarations {
    const fn next_declaration(&mut self) -> Option<Declaration> {
        while !self.rest.is_empty() {
            let mut end = 0;
            while end < self.rest.len() && self.rest[end] != b'\n' {
                end += 1;
            }
            let (line, rest) = self.rest.split_at(end);
            self.rest = if rest.is_empty() {
                rest
            } else {
                rest.split_at(1).1
            };
            if let Some(declaration) = parse(line) {
                return Some(declaration);
            }
        }
        None
    }
}

impl Iterator for Declarations {
    type Item = Declaration;

    fn next(&mut self) -> Option<Declaration> {
        self.next_declaration()
    }
}

/// The declaration `MACRO(name, ...)` in `source`, for any of `macros`.
pub(crate) const fn find(source: &'static str, macros: &[&str], name: &str) -> Option<Declaration> {
    let mut all = declarations(source);
    while let Some(declaration) = all.next_declaration() {
        if declaration.is_any_of(macros) {
            if let Some(declared) = declaration.field(0) {
                if same(declared.as_bytes(), name.as_bytes()) {
                    return Some(declaration);
                }
            }
        }
    }
    None
}

/// Field `field` of the declaration from `find`. In a `const` block, the
/// build fails on a name that `source` does not declare.
pub(crate) const fn declared(
    source: &'static str,
    macros: &[&str],
    name: &str,
    field: usize,
) -> &'static str {
    match find(source, macros, name) {
        Some(declaration) => match declaration.field(field) {
            Some(value) => value,
            None => panic!("the declaration has fewer fields than asked for"),
        },
        None => panic!("the player's declarations do not declare this name"),
    }
}

/// `line` as a declaration, when it is one: a macro's name and its
/// parenthesised fields, alone on the line. Comments, `#` directives and
/// blank lines are not declarations.
const fn parse(line: &'static [u8]) -> Option<Declaration> {
    let line = line.trim_ascii();
    let mut open = 0;
    while open < line.len()
        && (line[open] == b'_'
            || line[open].is_ascii_alphabetic()
            || (open > 0 && line[open].is_ascii_digit()))
    {
        open += 1;
    }
    if open == 0 || open == line.len() || line[open] != b'(' {
        return None;
    }
    if line[line.len() - 1] != b')' {
        panic!("a declaration is not alone on its line");
    }
    let (macro_name, call) = line.split_at(open);
    let (call, _) = call.split_at(call.len() - 1);
    let (_, fields) = call.split_at(1);
    Some(Declaration {
        macro_name: text(macro_name),
        fields: text(fields),
    })
}

/// `field` trimmed, without the quotes around it.
const fn unquoted(field: &'static [u8]) -> &'static [u8] {
    let field = field.trim_ascii();
    if field.len() >= 2 && field[0] == b'"' && field[field.len() - 1] == b'"' {
        let (inner, _) = field.split_at(field.len() - 1);
        inner.split_at(1).1
    } else {
        field
    }
}

const fn same(left: &[u8], right: &[u8]) -> bool {
    if left.len() != right.len() {
        return false;
    }
    let mut index = 0;
    while index < left.len() {
        if left[index] != right[index] {
            return false;
        }
        index += 1;
    }
    true
}

/// Bytes cut from a `str` at ASCII bytes, which are still a valid `str`.
const fn text(bytes: &'static [u8]) -> &'static str {
    match core::str::from_utf8(bytes) {
        Ok(text) => text,
        Err(_) => panic!("a declaration is not UTF-8"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SOURCE: &str = "/* RIB_KEY(Old, \"commented\") */\n\
                          #define RIB_KEY(name, key)\n\
                          RIB_KEY(Screens, \"screens\")\n\
                          RIB_KEYS(Screen, \"screen_\")\n  \
                          RIB_FILES(Scene, \"scene-\", \".rml\")\n\
                          RIB_WORD(Pair, \"pair\", \"A, B\")\n\
                          RIB_COUNT(6)";

    #[test]
    fn every_declaration_is_read_in_order_with_its_fields() {
        let read: Vec<(&str, Vec<&str>)> = declarations(SOURCE)
            .map(|declaration| (declaration.macro_name(), declaration.fields()))
            .collect();
        assert_eq!(
            read,
            [
                ("RIB_KEY", vec!["Screens", "screens"]),
                ("RIB_KEYS", vec!["Screen", "screen_"]),
                ("RIB_FILES", vec!["Scene", "scene-", ".rml"]),
                ("RIB_WORD", vec!["Pair", "pair", "A, B"]),
                ("RIB_COUNT", vec!["6"]),
            ]
        );
    }

    #[test]
    fn a_name_reads_its_own_declaration_and_field() {
        assert_eq!(declared(SOURCE, &["RIB_KEY"], "Screens", 1), "screens");
        // RIB_KEY is not RIB_KEYS, and Screen is not Screens.
        assert_eq!(declared(SOURCE, &["RIB_KEYS"], "Screen", 1), "screen_");
        assert!(find(SOURCE, &["RIB_KEY"], "Screen").is_none());
        assert_eq!(
            declared(SOURCE, &["RIB_KEY", "RIB_FILES"], "Scene", 2),
            ".rml"
        );
        const READ_WHEN_COMPILED: &str = declared(SOURCE, &["RIB_WORD"], "Pair", 2);
        assert_eq!(READ_WHEN_COMPILED, "A, B");
    }

    #[test]
    #[should_panic(expected = "do not declare this name")]
    fn a_name_only_a_comment_mentions_is_not_declared() {
        declared(SOURCE, &["RIB_KEY"], "Old", 1);
    }

    #[test]
    #[should_panic(expected = "fewer fields")]
    fn a_field_past_the_last_is_not_declared() {
        declared(SOURCE, &["RIB_KEYS"], "Screen", 2);
    }

    #[test]
    #[should_panic(expected = "not alone on its line")]
    fn a_declaration_sharing_its_line_is_refused() {
        declarations("RIB_KEY(Screens, \"screens\") /* the list */\n").count();
    }
}
