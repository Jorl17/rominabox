//! The words of a binding that the builder's preview shows wherever a design
//! names a hotkey: in a hint, as "{back}", and in an element marked
//! data-binding. In the game the menu writes the words of a binding of the
//! input in use (document_contract.inc, BindingAttribute), and the preview
//! shows the words of the first key, as a player at a computer sees them.

use super::{button_positions, chord, Binding, Hotkey, Hotkeys, PadInput};
use crate::menu::contract::contract;

impl Hotkeys {
    /// The words of the first key bound to `hotkey`, or else of its first
    /// binding, as the menu writes them. None for a hotkey with no binding.
    pub fn words(&self, hotkey: Hotkey) -> Result<Option<String>, String> {
        let list = self.of(hotkey);
        let Some(binding) = list.iter().find(|binding| binding.is_key()).or(list.first()) else {
            return Ok(None);
        };
        Ok(Some(match binding {
            Binding::Key(name) => crate::menu::words::key_words()
                .iter()
                .find(|(key, _)| key == name)
                .map_or_else(|| name.clone(), |(_, word)| word.clone()),
            Binding::Pad(inputs) => {
                let positions = button_positions()?;
                inputs
                    .iter()
                    .map(|input| match input {
                        PadInput::Home => PadInput::home_field(2).to_string(),
                        PadInput::Position(id) => positions
                            .iter()
                            .find(|position| &position.id == id)
                            .map_or_else(|| id.clone(), |position| position.name.clone()),
                    })
                    .collect::<Vec<_>>()
                    .join(chord())
            }
        }))
    }

    /// `hint` with the words of its binding after each hotkey it names, so
    /// that "{back}" is "{back:Escape}".
    pub fn hint(&self, hint: &str) -> Result<String, String> {
        let mut written = String::new();
        let mut rest = hint;
        while let Some(open) = rest.find('{') {
            let close = rest[open..]
                .find('}')
                .map(|at| open + at)
                .ok_or_else(|| format!("the hint '{hint}' opens a brace and never closes it"))?;
            let id = &rest[open + 1..close];
            let hotkey = Hotkey::named(id).ok_or_else(|| format!("the hint '{hint}' names no hotkey '{id}'"))?;
            written.push_str(&rest[..open]);
            written.push_str(&format!("{{{id}:{}}}", self.words(hotkey)?.unwrap_or_default()));
            rest = &rest[close + 1..];
        }
        written.push_str(rest);
        Ok(written)
    }

    /// `markup` with the words of its binding in each element marked
    /// data-binding, which hold nothing else.
    pub fn bound_words(&self, markup: &str) -> Result<String, String> {
        let marker = format!("{}=\"", contract!(BindingAttribute));
        let mut written = String::new();
        let mut rest = markup;
        while let Some(at) = rest.find(&marker) {
            let id_start = at + marker.len();
            let id_end = rest[id_start..]
                .find('"')
                .map(|end| id_start + end)
                .ok_or_else(|| "an element marked data-binding has no closing quote".to_string())?;
            let id = &rest[id_start..id_end];
            let hotkey = Hotkey::named(id).ok_or_else(|| format!("an element marked data-binding names no hotkey '{id}'"))?;
            let opened = rest[id_end..]
                .find('>')
                .map(|end| id_end + end + 1)
                .ok_or_else(|| format!("the element marked data-binding=\"{id}\" never ends its tag"))?;
            let closed = rest[opened..]
                .find('<')
                .map(|end| opened + end)
                .ok_or_else(|| format!("the element marked data-binding=\"{id}\" is never closed"))?;
            written.push_str(&rest[..opened]);
            written.push_str(&crate::lists::rml_text(&self.words(hotkey)?.unwrap_or_default()));
            rest = &rest[closed..];
        }
        written.push_str(rest);
        Ok(written)
    }
}

#[cfg(test)]
mod tests;
