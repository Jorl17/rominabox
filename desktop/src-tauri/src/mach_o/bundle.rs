//! We sign an app ad hoc, as with `codesign --force --sign - --entitlements`
//! after signing each piece of code inside it on its own.
//!
//! We sign the main program of the app last, sealed with the app's
//! Info.plist, its entitlements and its resources. The resources are in
//! `Contents/_CodeSignature/CodeResources`, which lists every other file in
//! the app with its hash, and each piece of nested code (a library beside the
//! program or in Frameworks) by its cdhash. We use the same rules in
//! CodeResources as `codesign`, and apply them to each file the same way.

use super::entitlements::Entitlements;
use super::signature::{code_directory_hashes, identifier_for, sign, Seal};
use super::Cpu;
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};

/// What we sign an app as.
pub struct AppSeal<'a> {
    /// The app itself, `Name.app`.
    pub app: &'a Path,
    /// Its main program, which the Info.plist names.
    pub executable: &'a Path,
    /// Its CFBundleIdentifier, which is also the name of its signature.
    pub identifier: &'a str,
    pub entitlements: &'a Entitlements,
    /// Every other piece of code in it, which we sign on its own first,
    /// without the entitlements.
    pub nested: &'a [PathBuf],
}

/// Sign the nested code, then seal the app's resources, then sign the main
/// program. We check `cancelled` between files.
pub fn sign_app(seal: &AppSeal<'_>, cancelled: &dyn Fn() -> bool) -> Result<(), String> {
    for path in seal.nested {
        if cancelled() {
            return Err("cancelled".into());
        }
        let bytes = read(path)?;
        let name = path.file_name().and_then(|name| name.to_str()).unwrap_or("code");
        let signed = sign(&bytes, &Seal { identifier: &identifier_for(name, &bytes), ..Seal::default() })
            .map_err(|error| format!("{}: {error}", path.display()))?;
        write(path, &signed)?;
    }
    let contents = seal.app.join("Contents");
    let resources = code_resources(&contents, seal.executable)?;
    let signature = contents.join("_CodeSignature");
    fs::create_dir_all(&signature).map_err(|error| format!("{}: {error}", signature.display()))?;
    write(&signature.join("CodeResources"), resources.as_bytes())?;
    if cancelled() {
        return Err("cancelled".into());
    }
    let info = read(&contents.join("Info.plist"))?;
    let program = read(seal.executable)?;
    let signed = sign(
        &program,
        &Seal {
            identifier: seal.identifier,
            info_plist: Some(&info),
            resources: Some(resources.as_bytes()),
            entitlements: Some(seal.entitlements),
        },
    )
    .map_err(|error| format!("{}: {error}", seal.executable.display()))?;
    write(seal.executable, &signed)
}

fn read(path: &Path) -> Result<Vec<u8>, String> {
    fs::read(path).map_err(|error| format!("{}: {error}", path.display()))
}

fn write(path: &Path, bytes: &[u8]) -> Result<(), String> {
    fs::write(path, bytes).map_err(|error| format!("{}: {error}", path.display()))
}

/// How we seal a file in `Contents`, by the rules of `codesign` for an app.
#[derive(Debug, PartialEq, Eq)]
enum Sealed {
    /// Not listed: the Info.plist and PkgInfo, which we seal in the signature
    /// itself, and Finder's .DS_Store.
    Omitted,
    /// Hashed, and `optional` for a localisation, which someone may remove.
    File { optional: bool, in_version_1: bool },
    /// Code that we sign on its own and list by its cdhash.
    Nested,
}

/// `path`, relative to `Contents` with "/" between its parts, by the rules
/// in CodeResources (`RULES`). We apply the heaviest rule that matches.
fn sealed_as(path: &str) -> Sealed {
    let name = path.rsplit('/').next().unwrap_or(path);
    let resource = path.starts_with("Resources/");
    let localised = resource && path.contains(".lproj/");
    if name == ".DS_Store" || (localised && name == "locversion.plist") {
        return Sealed::Omitted;
    }
    if localised {
        // A base localisation is always there, so it is not optional.
        return Sealed::File { optional: !path.starts_with("Resources/Base.lproj/"), in_version_1: true };
    }
    if path == "Info.plist" || path == "PkgInfo" {
        return Sealed::Omitted;
    }
    if resource || path == "version.plist" {
        return Sealed::File { optional: false, in_version_1: true };
    }
    if path == "embedded.provisionprofile" {
        return Sealed::File { optional: false, in_version_1: false };
    }
    let dsym = path.split('/').any(|part| part.ends_with(".dSYM"));
    let code_folder = [
        "Frameworks/", "SharedFrameworks/", "PlugIns/", "Plug-ins/", "XPCServices/", "Helpers/", "MacOS/",
        "Library/Automator/", "Library/Spotlight/", "Library/LoginItems/",
    ]
    .iter()
    .any(|folder| path.starts_with(folder));
    if !dsym && (code_folder || !path.contains('/')) {
        return Sealed::Nested;
    }
    Sealed::File { optional: false, in_version_1: false }
}

/// Every file under `folder`, as paths relative to `contents`, sorted.
fn files_in(contents: &Path, folder: &Path, found: &mut Vec<(String, PathBuf)>) -> Result<(), String> {
    let entries = fs::read_dir(folder).map_err(|error| format!("{}: {error}", folder.display()))?;
    for entry in entries {
        let entry = entry.map_err(|error| format!("{}: {error}", folder.display()))?;
        let path = entry.path();
        let kind = entry.file_type().map_err(|error| format!("{}: {error}", path.display()))?;
        let relative = path
            .strip_prefix(contents)
            .ok()
            .and_then(Path::to_str)
            .ok_or_else(|| format!("{} is not a UTF-8 path", path.display()))?
            .replace('\\', "/");
        if kind.is_symlink() {
            return Err(format!("{relative} is a link, which an app made here does not carry"));
        } else if kind.is_dir() {
            if relative != "_CodeSignature" {
                files_in(contents, &path, found)?;
            }
        } else {
            found.push((relative, path));
        }
    }
    Ok(())
}

/// The CodeResources of the app whose `Contents` folder is `contents` and
/// whose main program is `executable`, which we seal in the signature itself.
fn code_resources(contents: &Path, executable: &Path) -> Result<String, String> {
    let mut found = Vec::new();
    files_in(contents, contents, &mut found)?;
    found.sort_by(|(left, _), (right, _)| left.cmp(right));
    let mut version_1 = String::new();
    let mut version_2 = String::new();
    for (relative, path) in &found {
        if path == executable {
            continue;
        }
        let key = format!("\t\t<key>{}</key>\n", escape(relative));
        match sealed_as(relative) {
            Sealed::Omitted => {}
            Sealed::File { optional, in_version_1 } => {
                let bytes = read(path)?;
                let optional_entry = if optional { "\t\t\t<key>optional</key>\n\t\t\t<true/>\n" } else { "" };
                if in_version_1 {
                    let sha1 = base64(&<sha1::Sha1 as sha1::Digest>::digest(&bytes));
                    version_1 += &key;
                    version_1 += &if optional {
                        format!("\t\t<dict>\n\t\t\t<key>hash</key>\n\t\t\t<data>\n\t\t\t{sha1}\n\t\t\t</data>\n{optional_entry}\t\t</dict>\n")
                    } else {
                        format!("\t\t<data>\n\t\t{sha1}\n\t\t</data>\n")
                    };
                }
                version_2 += &key;
                version_2 += &format!(
                    "\t\t<dict>\n\t\t\t<key>hash2</key>\n\t\t\t<data>\n\t\t\t{}\n\t\t\t</data>\n{optional_entry}\t\t</dict>\n",
                    base64(&Sha256::digest(&bytes))
                );
            }
            Sealed::Nested => {
                let hashes = code_directory_hashes(&read(path)?)
                    .map_err(|error| format!("{relative} is not signed code: {error}"))?;
                // Apple silicon first, as in `codesign` on an Apple silicon Mac.
                let mut ordered: Vec<&(Cpu, [u8; 20])> = hashes.iter().filter(|(cpu, _)| *cpu == Cpu::ARM64).collect();
                ordered.extend(hashes.iter().filter(|(cpu, _)| *cpu != Cpu::ARM64));
                let requirement = ordered
                    .iter()
                    .map(|(_, cdhash)| format!("cdhash H\"{}\"", hex(cdhash)))
                    .collect::<Vec<_>>()
                    .join(" or ");
                version_2 += &key;
                version_2 += &format!(
                    "\t\t<dict>\n\t\t\t<key>cdhash</key>\n\t\t\t<data>\n\t\t\t{}\n\t\t\t</data>\n\t\t\t<key>requirement</key>\n\t\t\t<string>{}</string>\n\t\t</dict>\n",
                    base64(&ordered[0].1),
                    escape(&requirement)
                );
            }
        }
    }
    Ok(format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n<plist version=\"1.0\">\n<dict>\n\t<key>files</key>\n\t<dict>\n{version_1}\t</dict>\n\t<key>files2</key>\n\t<dict>\n{version_2}\t</dict>\n{RULES}</dict>\n</plist>\n"
    ))
}

/// Standard Base64, padded, as for data in a property list.
fn base64(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut text = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for group in bytes.chunks(3) {
        let word = group.iter().enumerate().fold(0u32, |word, (index, &byte)| word | (byte as u32) << (16 - 8 * index));
        for index in 0..4 {
            text.push(if index <= group.len() {
                ALPHABET[(word >> (18 - 6 * index) & 63) as usize] as char
            } else {
                '='
            });
        }
    }
    text
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// Escape text as in a property list, only where XML requires it.
fn escape(text: &str) -> String {
    text.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

/// The rules of `codesign` for an app, as they appear in CodeResources:
/// `rules` for the first version of the seal, `rules2` for the second.
const RULES: &str = "\t<key>rules</key>
\t<dict>
\t\t<key>^Resources/</key>
\t\t<true/>
\t\t<key>^Resources/.*\\.lproj/</key>
\t\t<dict>
\t\t\t<key>optional</key>
\t\t\t<true/>
\t\t\t<key>weight</key>
\t\t\t<real>1000</real>
\t\t</dict>
\t\t<key>^Resources/.*\\.lproj/locversion.plist$</key>
\t\t<dict>
\t\t\t<key>omit</key>
\t\t\t<true/>
\t\t\t<key>weight</key>
\t\t\t<real>1100</real>
\t\t</dict>
\t\t<key>^Resources/Base\\.lproj/</key>
\t\t<dict>
\t\t\t<key>weight</key>
\t\t\t<real>1010</real>
\t\t</dict>
\t\t<key>^version.plist$</key>
\t\t<true/>
\t</dict>
\t<key>rules2</key>
\t<dict>
\t\t<key>.*\\.dSYM($|/)</key>
\t\t<dict>
\t\t\t<key>weight</key>
\t\t\t<real>11</real>
\t\t</dict>
\t\t<key>^(.*/)?\\.DS_Store$</key>
\t\t<dict>
\t\t\t<key>omit</key>
\t\t\t<true/>
\t\t\t<key>weight</key>
\t\t\t<real>2000</real>
\t\t</dict>
\t\t<key>^(Frameworks|SharedFrameworks|PlugIns|Plug-ins|XPCServices|Helpers|MacOS|Library/(Automator|Spotlight|LoginItems))/</key>
\t\t<dict>
\t\t\t<key>nested</key>
\t\t\t<true/>
\t\t\t<key>weight</key>
\t\t\t<real>10</real>
\t\t</dict>
\t\t<key>^.*</key>
\t\t<true/>
\t\t<key>^Info\\.plist$</key>
\t\t<dict>
\t\t\t<key>omit</key>
\t\t\t<true/>
\t\t\t<key>weight</key>
\t\t\t<real>20</real>
\t\t</dict>
\t\t<key>^PkgInfo$</key>
\t\t<dict>
\t\t\t<key>omit</key>
\t\t\t<true/>
\t\t\t<key>weight</key>
\t\t\t<real>20</real>
\t\t</dict>
\t\t<key>^Resources/</key>
\t\t<dict>
\t\t\t<key>weight</key>
\t\t\t<real>20</real>
\t\t</dict>
\t\t<key>^Resources/.*\\.lproj/</key>
\t\t<dict>
\t\t\t<key>optional</key>
\t\t\t<true/>
\t\t\t<key>weight</key>
\t\t\t<real>1000</real>
\t\t</dict>
\t\t<key>^Resources/.*\\.lproj/locversion.plist$</key>
\t\t<dict>
\t\t\t<key>omit</key>
\t\t\t<true/>
\t\t\t<key>weight</key>
\t\t\t<real>1100</real>
\t\t</dict>
\t\t<key>^Resources/Base\\.lproj/</key>
\t\t<dict>
\t\t\t<key>weight</key>
\t\t\t<real>1010</real>
\t\t</dict>
\t\t<key>^[^/]+$</key>
\t\t<dict>
\t\t\t<key>nested</key>
\t\t\t<true/>
\t\t\t<key>weight</key>
\t\t\t<real>10</real>
\t\t</dict>
\t\t<key>^embedded\\.provisionprofile$</key>
\t\t<dict>
\t\t\t<key>weight</key>
\t\t\t<real>20</real>
\t\t</dict>
\t\t<key>^version\\.plist$</key>
\t\t<dict>
\t\t\t<key>weight</key>
\t\t\t<real>20</real>
\t\t</dict>
\t</dict>
";
