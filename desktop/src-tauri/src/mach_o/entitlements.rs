//! Entitlements: permissions for a signed program beyond the defaults, such
//! as running in the App Sandbox. A signature contains them twice, as a
//! property list in XML and as DER, which is the form for macOS. We make
//! both from one declaration.

/// An entitlement's value: a switch, or a list of strings (paths).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Value {
    Bool(bool),
    Strings(Vec<String>),
}

/// Entitlements, in the order of their declaration.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Entitlements {
    entries: Vec<(String, Value)>,
}

impl Entitlements {
    /// `self` with `key` set to `value`.
    pub fn with(mut self, key: &str, value: Value) -> Self {
        self.entries.retain(|(existing, _)| existing != key);
        self.entries.push((key.to_string(), value));
        self
    }

    /// The property list, one entitlement to a line.
    pub fn xml(&self) -> String {
        let mut text = String::from(concat!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n",
            "<!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n",
            "<plist version=\"1.0\"><dict>\n",
        ));
        for (key, value) in &self.entries {
            text += &format!("<key>{}</key>", escape(key));
            match value {
                Value::Bool(true) => text += "<true/>\n",
                Value::Bool(false) => text += "<false/>\n",
                Value::Strings(strings) => {
                    text += "\n<array>";
                    for string in strings {
                        text += &format!("<string>{}</string>", escape(string));
                    }
                    text += "</array>\n";
                }
            }
        }
        text + "</dict></plist>\n"
    }

    /// The DER form, as in `codesign`: version 1 and a dictionary with its
    /// entries sorted by key, each a SEQUENCE of the key and its value.
    pub fn der(&self) -> Vec<u8> {
        let mut entries: Vec<&(String, Value)> = self.entries.iter().collect();
        entries.sort_by(|(left, _), (right, _)| left.as_bytes().cmp(right.as_bytes()));
        let dictionary: Vec<u8> = entries
            .iter()
            .flat_map(|(key, value)| {
                let value = match value {
                    Value::Bool(on) => vec![0x01, 0x01, if *on { 0xff } else { 0x00 }],
                    Value::Strings(strings) => {
                        tagged(0x30, &strings.iter().flat_map(|string| utf8(string)).collect::<Vec<_>>())
                    }
                };
                tagged(0x30, &[utf8(key), value].concat())
            })
            .collect();
        // [APPLICATION 16] { INTEGER 1, [CONTEXT 16] { entries } }
        tagged(0x70, &[vec![0x02, 0x01, 0x01], tagged(0xb0, &dictionary)].concat())
    }
}

fn utf8(text: &str) -> Vec<u8> {
    tagged(0x0c, text.as_bytes())
}

/// A DER value: its tag, its length (short or long form) and `content`.
fn tagged(tag: u8, content: &[u8]) -> Vec<u8> {
    let length = content.len();
    let mut bytes = vec![tag];
    if length < 0x80 {
        bytes.push(length as u8);
    } else {
        let digits: Vec<u8> = length.to_be_bytes().into_iter().skip_while(|&byte| byte == 0).collect();
        bytes.push(0x80 | digits.len() as u8);
        bytes.extend(digits);
    }
    bytes.extend_from_slice(content);
    bytes
}

fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}
