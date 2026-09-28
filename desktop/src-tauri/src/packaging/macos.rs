//! A macOS app. This file contains the packager and the steps only for a Mac
//! export, which are the Info.plist, the sandbox entitlements, the launch
//! library in the app, and freezing, relocating and signing its Mach-O files.

use super::app_files::{copy_file, make_executable, tree_size};
use super::launch_plan::accounts_folder;
use super::slices::{self, Arch};
use super::{
    check_cancelled, player_recipe, ErrorStage, ExportError, ExportRequest, OwnedStaging, Packager,
};
use crate::icons;
use crate::launch_contract::{app_file, core_file, shipped};
use crate::target::Target;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet, VecDeque};
use std::ffi::OsStr;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::Duration;

/// A macOS app: one bundle, with the player in the launcher process, the
/// libraries relocated next to it, and a signature with the game's sandbox.
pub(super) struct MacosPackager {
    /// The processors for the code of the app, with the platform's first.
    archs: Vec<Arch>,
    app: PathBuf,
    macos: PathBuf,
    resources: PathBuf,
    frameworks: PathBuf,
    runtime: PathBuf,
    /// Every Mach-O file in the app, which we relocate and sign.
    mach_objects: Vec<PathBuf>,
}

impl MacosPackager {
    /// A packager for an app that runs on `targets`, each a Mac.
    pub(super) fn for_targets(targets: &[Target]) -> Self {
        Self {
            archs: targets.iter().filter_map(|&target| Arch::of(target)).collect(),
            app: PathBuf::new(),
            macos: PathBuf::new(),
            resources: PathBuf::new(),
            frameworks: PathBuf::new(),
            runtime: PathBuf::new(),
            mach_objects: Vec::new(),
        }
    }
}

impl Packager for MacosPackager {
    fn check_host(&self) -> Result<(), ExportError> {
        if !cfg!(target_os = "macos") {
            return Err(ExportError::new(
                ErrorStage::Refused,
                "Mac apps can only be made on a Mac.",
            ));
        }
        if !Path::new("/usr/bin/codesign").is_file() {
            return Err(ExportError::new(ErrorStage::Refused, "This version of macOS does not provide the signing service required by this build. No tools were installed and no app was exported."));
        }
        Ok(())
    }

    fn player_in_kit(&self) -> PathBuf {
        PathBuf::from("bin/retroarch")
    }

    fn lay_out(&mut self, app: &Path) -> Result<PathBuf, ExportError> {
        let contents = app.join("Contents");
        self.app = app.to_path_buf();
        self.macos = contents.join("MacOS");
        self.resources = contents.join("Resources");
        self.frameworks = contents.join("Frameworks");
        for directory in [&self.macos, &self.resources, &self.frameworks] {
            fs::create_dir_all(directory)
                .map_err(|error| ExportError::io(ErrorStage::Stage, directory, error))?;
        }
        Ok(self.resources.clone())
    }

    /// The kit's player, with the slices for the processors of the app and no
    /// others. For an ordinary game we thin a universal player.
    fn place_player(&mut self, runtime_kit: &Path) -> Result<(), ExportError> {
        self.runtime = self.macos.join("retroarch");
        slices::keep(
            &runtime_kit.join(self.player_in_kit()),
            &self.archs,
            &self.runtime,
            "This builder's runtime",
            ErrorStage::Stage,
        )?;
        make_executable(&self.runtime)
    }

    fn core_file(&self) -> &'static str {
        core_file!(Macos)
    }

    /// We copy one core as it is, and join the cores for several processors
    /// into one file with the slice of each.
    fn place_core(
        &mut self,
        builds: &[(Target, PathBuf)],
        destination: &Path,
        system_name: &str,
    ) -> Result<(), ExportError> {
        match builds {
            [(_, only)] => copy_file(only, destination),
            several => {
                let parts = several
                    .iter()
                    .map(|(target, file)| {
                        Arch::of(*target).map(|arch| (arch, file.clone())).ok_or_else(|| {
                            ExportError::new(ErrorStage::Stage, format!("a Mac game has no {target} core"))
                        })
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                slices::join(
                    &parts,
                    destination,
                    &format!("The {system_name} core"),
                    ErrorStage::Stage,
                )
            }
        }
    }

    fn stage_dependencies(
        &mut self,
        runtime_kit: &Path,
        core: &Path,
        cancelled: &AtomicBool,
    ) -> Result<(), ExportError> {
        self.mach_objects = vec![self.runtime.clone(), core.to_path_buf()];
        stage_frozen_dependencies(
            runtime_kit,
            &self.frameworks,
            &mut self.mach_objects,
            cancelled,
        )
    }

    fn install_launcher(&mut self, _runtime_kit: &Path) -> Result<(), ExportError> {
        install_launch_library(&self.macos, &self.runtime, &self.archs)?;
        self.mach_objects.push(self.macos.join(LAUNCH_LIBRARY));
        Ok(())
    }

    fn describe(
        &mut self,
        request: &ExportRequest,
        identity: &str,
        staging: &Path,
    ) -> Result<(), ExportError> {
        write_plist(
            &self.app.join("Contents/Info.plist"),
            &request.title,
            identity,
            request.icon.is_some() || icons::default_icon_path(&request.runtime_kit).is_some(),
            &super::macos_minimum::newest(&self.mach_objects)?,
        )?;
        let default_icon = icons::default_icon_path(&request.runtime_kit);
        if let Some(icon) = request.icon.as_deref().or(default_icon.as_deref()) {
            icons::create_macos_icon(icon, &self.resources.join("GameIcon.icns"), staging)?;
        }
        Ok(())
    }

    fn finishing(&self) -> Option<&'static str> {
        Some("Signing the local app")
    }

    fn finish(
        &mut self,
        request: &ExportRequest,
        identity: &str,
        staging: &Path,
        cancelled: &AtomicBool,
    ) -> Result<(), ExportError> {
        // A freshly built player still contains the names of the libraries it
        // was linked against, and in the frozen kit we already rewrote them.
        // Either way, the game must load the copies we just staged next to it.
        relocate_dependencies(
            &self.mach_objects,
            "@executable_path/../Frameworks",
            Some(cancelled),
        )?;
        for object in self.mach_objects.iter().rev() {
            run_command_cancellable(
                ErrorStage::Sign,
                Command::new("/usr/bin/codesign")
                    .args(["--force", "--sign", "-"])
                    .arg(object),
                cancelled,
            )?;
        }
        let entitlements = staging.join("entitlements.plist");
        fs::write(
            &entitlements,
            sandbox_entitlements(identity, accounts_folder(request)?.as_deref()),
        )
        .map_err(|error| ExportError::io(ErrorStage::Sign, &entitlements, error))?;
        run_command_cancellable(
            ErrorStage::Sign,
            Command::new("/usr/bin/codesign")
                .args(["--force", "--sign", "-", "--entitlements"])
                .arg(&entitlements)
                .arg(&self.app),
            cancelled,
        )
    }

    fn runtime_bytes(&self) -> Result<u64, ExportError> {
        Ok(tree_size(&self.runtime)?
            + tree_size(&self.resources.join(core_file!(Macos)))?
            + tree_size(&self.frameworks)?
            + tree_size(&self.resources.join(app_file!(MenuAssets)))?
            + tree_size(&self.resources.join(shipped!(Autoconfig).0))?)
    }
}

/// `accounts` is the QUICK SIGN IN folder, present exactly when the game has
/// achievements. We grant the network and that folder together.
fn sandbox_entitlements(identity: &str, accounts: Option<&str>) -> String {
    let (network, shared) = match accounts {
        Some(folder) => (
            "<key>com.apple.security.network.client</key><true/>".to_string(),
            format!(
                "<key>com.apple.security.temporary-exception.files.home-relative-path.read-write</key>\n\
                 <array><string>/Library/Application Support/{folder}/</string></array>"
            ),
        ),
        None => (String::new(), String::new()),
    };
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>com.apple.security.app-sandbox</key><true/>
{network}
<key>com.apple.security.device.usb</key><true/>
<key>com.apple.security.device.bluetooth</key><true/>
<key>com.apple.security.temporary-exception.files.home-relative-path.read-only</key>
<array><string>/Library/Application Support/ROM-in-a-Box/Games/{identity}/</string></array>
{shared}
</dict></plist>
"#
    )
}

fn write_plist(
    path: &Path,
    title: &str,
    identity: &str,
    has_icon: bool,
    minimum_macos: &str,
) -> Result<(), ExportError> {
    let icon = if has_icon {
        "<key>CFBundleIconFile</key><string>GameIcon</string>"
    } else {
        ""
    };
    let plist = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>CFBundleDevelopmentRegion</key><string>en</string>
<key>CFBundleDisplayName</key><string>{}</string>
<key>CFBundleExecutable</key><string>retroarch</string>
<key>CFBundleIdentifier</key><string>app.rominabox.game.{}</string>
<key>CFBundleInfoDictionaryVersion</key><string>6.0</string>
<key>CFBundleName</key><string>{}</string>
<key>CFBundlePackageType</key><string>APPL</string>
<key>CFBundleShortVersionString</key><string>1.0</string>
<key>NSHighResolutionCapable</key><true/>
<key>LSMinimumSystemVersion</key><string>{}</string>
{}
</dict></plist>
"#,
        xml_escape(title),
        identity,
        xml_escape(title),
        minimum_macos,
        icon
    );
    fs::write(path, plist).map_err(|error| ExportError::io(ErrorStage::Configure, path, error))
}

fn xml_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

/// Every file that the `.c` files among `inputs` include, with the paths from
/// the compiler, or `None` when that list is not available. The launcher
/// includes declarations from the player tree, and we must rebuild it after a
/// change there as after a change next to it.
fn included_files(inputs: &[PathBuf]) -> Option<Vec<PathBuf>> {
    let sources = inputs
        .iter()
        .filter(|input| input.extension() == Some(OsStr::new("c")));
    let listed = Command::new("cc").arg("-MM").args(sources).output().ok()?;
    if !listed.status.success() {
        return None;
    }
    // Make rules, `object: source header ...`. A line that ends in a backslash
    // continues on the next, and a space in a path has a backslash before it.
    let text = String::from_utf8_lossy(&listed.stdout)
        .replace("\\\n", " ")
        .replace("\\ ", "\0");
    Some(
        text.lines()
            .filter_map(|rule| rule.split_once(": "))
            .flat_map(|(_, files)| files.split_whitespace())
            .map(|file| PathBuf::from(file.replace('\0', " ")))
            .collect(),
    )
}

/// Build `destination` from `inputs`, every file it is made from. We compile
/// the `.c` files among them, and we rebuild it after a change to any header
/// it includes, wherever that header is.
pub(super) fn compile_c(inputs: &[PathBuf], destination: &Path, extra: &[&str]) -> Result<(), ExportError> {
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| ExportError::io(ErrorStage::Configure, parent, error))?;
    }
    let built = fs::metadata(destination)
        .and_then(|meta| meta.modified())
        .ok();
    let unchanged = |input: &PathBuf| {
        fs::metadata(input)
            .and_then(|meta| meta.modified())
            .is_ok_and(|changed| Some(changed) <= built)
    };
    let current = built.is_some()
        && inputs.iter().all(unchanged)
        && included_files(inputs).is_some_and(|included| included.iter().all(unchanged));
    if current {
        return Ok(());
    }
    let parent = destination.parent().ok_or_else(|| {
        ExportError::new(
            ErrorStage::Configure,
            "compiled output needs a parent directory",
        )
    })?;
    let staging = OwnedStaging::create(parent)?;
    let temporary = staging.path().join("compiled");
    let status = Command::new("cc")
        .args(extra)
        .arg("-o")
        .arg(&temporary)
        .args(
            inputs
                .iter()
                .filter(|input| input.extension() == Some(OsStr::new("c"))),
        )
        .status()
        .map_err(|error| {
            ExportError::new(
                ErrorStage::Configure,
                format!("could not compile the launcher: {error}"),
            )
        })?;
    if !status.success() {
        return Err(ExportError::new(
            ErrorStage::Configure,
            "the launcher failed to compile",
        ));
    }
    fs::rename(&temporary, destination)
        .map_err(|error| ExportError::io(ErrorStage::Configure, destination, error))?;
    Ok(())
}

/// The launcher sources for one platform, the shared ones at the top of
/// `launcher/` and those in the folders listed for the platform in the
/// player recipe, with its entry and its file layer.
fn launcher_sources(platform: &str) -> Result<Vec<PathBuf>, ExportError> {
    let launcher = crate::repo::at("desktop/src-tauri/launcher");
    let declared: Vec<String> =
        serde_json::from_value(player_recipe()["launcher"]["folders"][platform].clone())
            .unwrap_or_else(|error| {
                panic!("the player recipe names no launcher folders for {platform}: {error}")
            });
    let folders =
        std::iter::once(launcher.clone()).chain(declared.iter().map(|name| launcher.join(name)));
    let mut sources = Vec::new();
    for folder in folders {
        let entries = fs::read_dir(&folder)
            .and_then(|entries| {
                entries
                    .map(|entry| entry.map(|entry| entry.path()))
                    .collect::<Result<Vec<_>, _>>()
            })
            .map_err(|error| ExportError::io(ErrorStage::Configure, &folder, error))?;
        sources.extend(
            entries
                .into_iter()
                .filter(|path| matches!(path.extension().and_then(OsStr::to_str), Some("c" | "h"))),
        );
    }
    sources.sort();
    Ok(sources)
}

/// The launch library, built for `archs` and for the macOS version in the
/// player recipe, next to the player and loaded by each of its slices. We keep
/// a build for other processors or systems apart, because in `compile_c` we
/// rebuild only on changed sources, not on changed flags.
fn install_launch_library(macos: &Path, retroarch: &Path, archs: &[Arch]) -> Result<(), ExportError> {
    let library_sources = launcher_sources("macos")?;
    let system = super::deployment_target(super::ExportTarget::Macos);
    let processors = archs
        .iter()
        .map(|arch| arch.name())
        .collect::<Vec<_>>()
        .join("-");
    let built_for = format!("{processors}-macos{system}");
    let library = crate::repo::at("work/launch")
        .join(built_for)
        .join(LAUNCH_LIBRARY);
    let install_name = format!("-Wl,-install_name,@executable_path/{LAUNCH_LIBRARY}");
    let minimum = format!("-mmacosx-version-min={system}");
    let mut flags = vec![
        "-Oz",
        "-dynamiclib",
        "-Wl,-dead_strip",
        install_name.as_str(),
        minimum.as_str(),
    ];
    for arch in archs {
        flags.extend(["-arch", arch.name()]);
    }
    compile_c(&library_sources, &library, &flags)?;
    let injector = launch_injector()?;
    slices::each_slice(retroarch, ErrorStage::Configure, |slice, _| {
        attach_launch_library(&injector, slice)
    })?;
    copy_file(&library, &macos.join(LAUNCH_LIBRARY))
}

/// The program with which we attach the launch library to a slice of the
/// player. We run it here at export, so we build it only for this Mac,
/// whichever processors the game is for.
pub(super) fn launch_injector() -> Result<PathBuf, ExportError> {
    let injector = crate::repo::at("work/inject-dylib");
    compile_c(
        &[crate::repo::at("scripts/native_runtime/inject_dylib.c")],
        &injector,
        &["-Oz"],
    )?;
    Ok(injector)
}

/// Change the one-processor Mach-O `slice` so that it loads the launch
/// library before its main and passes main the arguments from the library.
pub(super) fn attach_launch_library(injector: &Path, slice: &Path) -> Result<(), ExportError> {
    let output = Command::new(injector)
        .arg(slice)
        .arg(format!("@executable_path/{LAUNCH_LIBRARY}"))
        .output()
        .map_err(|error| {
            ExportError::new(
                ErrorStage::Configure,
                format!("could not attach the launcher: {error}"),
            )
        })?;
    if !output.status.success() {
        return Err(ExportError::command(ErrorStage::Configure, "inject-dylib", &output));
    }
    Ok(())
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct NativeDependencyInventory {
    format_version: u32,
    files: Vec<NativeDependencyFile>,
}

#[derive(Deserialize)]
struct NativeDependencyFile {
    name: String,
    sha256: String,
}

/// The libraries in `frameworks` that `roots` load, following the links of
/// each dylib. `None` means that none of the roots was a Mach-O we could read,
/// for example a fixture shell script, so we ship the whole inventory, as
/// those tests expect. For a compiled player we return the closure, and we
/// leave anything outside it in the kit and do not copy it into the game.
fn framework_closure(
    roots: &[PathBuf],
    frameworks: &Path,
) -> Result<Option<HashSet<String>>, ExportError> {
    fn leaf(dependency: &str, frameworks: &Path) -> Option<String> {
        let name = Path::new(dependency).file_name()?.to_str()?.to_string();
        frameworks.join(&name).is_file().then_some(name)
    }

    let mut needed = HashSet::new();
    let mut readable = false;
    let mut queue = VecDeque::new();
    for root in roots {
        let dependencies = match macho_dependencies(root) {
            Ok(dependencies) => dependencies,
            Err(_) => continue,
        };
        readable = true;
        for dependency in dependencies {
            if let Some(name) = leaf(&dependency, frameworks) {
                queue.push_back(name);
            }
        }
    }
    if !readable {
        return Ok(None);
    }
    while let Some(name) = queue.pop_front() {
        if !needed.insert(name.clone()) {
            continue;
        }
        for dependency in macho_dependencies(&frameworks.join(&name))? {
            if let Some(next) = leaf(&dependency, frameworks) {
                queue.push_back(next);
            }
        }
    }
    Ok(Some(needed))
}

fn stage_frozen_dependencies(
    runtime_kit: &Path,
    destination: &Path,
    signed_objects: &mut Vec<PathBuf>,
    cancelled: &AtomicBool,
) -> Result<(), ExportError> {
    let inventory_path = runtime_kit.join("runtime-dependencies.json");
    let inventory_bytes = fs::read(&inventory_path)
        .map_err(|error| ExportError::io(ErrorStage::Dependencies, &inventory_path, error))?;
    let inventory: NativeDependencyInventory =
        serde_json::from_slice(&inventory_bytes).map_err(|error| {
            ExportError::new(
                ErrorStage::Dependencies,
                format!("invalid {}: {error}", inventory_path.display()),
            )
        })?;
    if inventory.format_version != 1 {
        return Err(ExportError::new(
            ErrorStage::Dependencies,
            format!(
                "unsupported runtime dependency inventory version: {}",
                inventory.format_version
            ),
        ));
    }
    let source_directory = runtime_kit.join("Frameworks");
    let closure = framework_closure(signed_objects, &source_directory)?;
    let mut declared = HashSet::new();
    for dependency in inventory.files {
        check_cancelled(cancelled)?;
        let dependency_path = Path::new(&dependency.name);
        if dependency_path.components().count() != 1 || dependency.name.starts_with('.') {
            return Err(ExportError::new(
                ErrorStage::Dependencies,
                format!("invalid dependency filename: {}", dependency.name),
            ));
        }
        if !declared.insert(dependency.name.clone()) {
            return Err(ExportError::new(
                ErrorStage::Dependencies,
                format!("duplicate dependency filename: {}", dependency.name),
            ));
        }
        if closure
            .as_ref()
            .is_some_and(|needed| !needed.contains(&dependency.name))
        {
            continue;
        }
        let source = source_directory.join(&dependency.name);
        let actual = sha256_file(&source)?;
        if !actual.eq_ignore_ascii_case(&dependency.sha256) {
            return Err(ExportError::new(
                ErrorStage::Dependencies,
                format!(
                    "checksum mismatch for {}: expected {}, got {actual}",
                    dependency.name, dependency.sha256
                ),
            ));
        }
        let staged = destination.join(&dependency.name);
        copy_file(&source, &staged)?;
        signed_objects.push(staged);
    }
    // A kit whose player links no library has none, and we leave the empty
    // folder out of a builder bundle, so a missing folder means no libraries.
    let entries = match fs::read_dir(&source_directory) {
        Ok(entries) => entries.collect::<Vec<_>>(),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Vec::new(),
        Err(error) => {
            return Err(ExportError::io(ErrorStage::Dependencies, &source_directory, error))
        }
    };
    for entry in entries {
        let entry = entry
            .map_err(|error| ExportError::io(ErrorStage::Dependencies, &source_directory, error))?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if entry
            .file_type()
            .map_err(|error| ExportError::io(ErrorStage::Dependencies, &entry.path(), error))?
            .is_file()
            && !declared.contains(&name)
        {
            return Err(ExportError::new(
                ErrorStage::Dependencies,
                format!("undeclared file in frozen Frameworks: {name}"),
            ));
        }
    }
    if let Some(needed) = &closure {
        let mut missing: Vec<_> = needed.difference(&declared).cloned().collect();
        missing.sort();
        if !missing.is_empty() {
            return Err(ExportError::new(
                ErrorStage::Dependencies,
                format!(
                    "player links {} which the runtime dependency inventory does not list",
                    missing.join(", ")
                ),
            ));
        }
    }
    Ok(())
}

fn sha256_file(path: &Path) -> Result<String, ExportError> {
    let mut file = fs::File::open(path)
        .map_err(|error| ExportError::io(ErrorStage::Dependencies, path, error))?;
    let mut hash = Sha256::new();
    let mut buffer = [0u8; 1024 * 128];
    loop {
        let count = file
            .read(&mut buffer)
            .map_err(|error| ExportError::io(ErrorStage::Dependencies, path, error))?;
        if count == 0 {
            break;
        }
        hash.update(&buffer[..count]);
    }
    Ok(format!("{:x}", hash.finalize()))
}

fn bundle_dependencies(
    mach_objects: &mut Vec<PathBuf>,
    frameworks: &Path,
    search_dirs: &[PathBuf],
    cancelled: Option<&AtomicBool>,
) -> Result<(), ExportError> {
    let mut queue: VecDeque<(PathBuf, PathBuf)> = mach_objects
        .iter()
        .cloned()
        .map(|path| (path.clone(), path))
        .collect();
    let mut copied: HashMap<String, PathBuf> = HashMap::new();
    let mut inspected = HashSet::new();
    while let Some((object, origin)) = queue.pop_front() {
        if let Some(cancelled) = cancelled {
            check_cancelled(cancelled)?;
        }
        if !inspected.insert(object.clone()) {
            continue;
        }
        for (index, dependency) in macho_dependencies(&object)?.into_iter().enumerate() {
            if index == 0 && object.extension() == Some(OsStr::new("dylib")) {
                continue;
            }
            if is_system_dependency(&dependency) {
                continue;
            }
            let Some(source) = resolve_dependency_source(&dependency, &origin, search_dirs) else {
                return Err(ExportError::new(
                    ErrorStage::Dependencies,
                    format!(
                        "could not resolve native dependency {dependency} required by {}",
                        origin.display()
                    ),
                ));
            };
            if !source.is_file() {
                return Err(ExportError::new(
                    ErrorStage::Dependencies,
                    format!("missing native dependency {dependency}"),
                ));
            }
            let name = source.file_name().unwrap().to_string_lossy().into_owned();
            let canonical = dunce::canonicalize(&source)
                .map_err(|error| ExportError::io(ErrorStage::Dependencies, &source, error))?;
            if let Some(previous) = copied.get(&name) {
                if dunce::canonicalize(previous).ok().as_ref() != Some(&canonical) {
                    return Err(ExportError::new(
                        ErrorStage::Dependencies,
                        format!("dependency filename collision: {name}"),
                    ));
                }
                continue;
            }
            let destination = frameworks.join(&name);
            copy_file(&canonical, &destination)?;
            copied.insert(name, canonical.clone());
            queue.push_back((destination.clone(), canonical));
            mach_objects.push(destination);
        }
    }
    Ok(())
}

/// The library that we build and sign next to the player at export. Its
/// install name is `@executable_path/` on purpose.
const LAUNCH_LIBRARY: &str = "librominabox-launch.dylib";

fn relocate_dependencies(
    objects: &[PathBuf],
    framework_prefix: &str,
    cancelled: Option<&AtomicBool>,
) -> Result<(), ExportError> {
    for object in objects {
        if let Some(cancelled) = cancelled {
            check_cancelled(cancelled)?;
        }
        for (index, dependency) in macho_dependencies(object)?.into_iter().enumerate() {
            if index == 0 && object.extension() == Some(OsStr::new("dylib")) {
                continue;
            }
            if is_system_dependency(&dependency)
                || dependency.starts_with(&format!("{framework_prefix}/"))
                // The launch library is next to the executable, not in
                // Frameworks, because we sign it separately, without the
                // entitlements of the bundle, and its install name is
                // @executable_path. We do not rewrite its path to ../Frameworks,
                // because the file is not there and dyld would fail before main.
                || Path::new(&dependency).file_name()
                    == Some(OsStr::new(LAUNCH_LIBRARY))
            {
                continue;
            }
            let name = Path::new(&dependency).file_name().ok_or_else(|| {
                ExportError::new(
                    ErrorStage::Dependencies,
                    format!("invalid dependency: {dependency}"),
                )
            })?;
            let replacement = format!("{framework_prefix}/{}", name.to_string_lossy());
            let mut command = Command::new("/usr/bin/install_name_tool");
            command
                .args(["-change", &dependency, &replacement])
                .arg(object);
            if let Some(cancelled) = cancelled {
                run_command_cancellable(ErrorStage::Dependencies, &mut command, cancelled)?;
            } else {
                run_command(ErrorStage::Dependencies, &mut command)?;
            }
        }
        if object.parent().and_then(Path::file_name) == Some(OsStr::new("Frameworks")) {
            let name = object.file_name().unwrap().to_string_lossy();
            let mut command = Command::new("/usr/bin/install_name_tool");
            command.args(["-id", &format!("@rpath/{name}")]).arg(object);
            if let Some(cancelled) = cancelled {
                run_command_cancellable(ErrorStage::Dependencies, &mut command, cancelled)?;
            } else {
                run_command(ErrorStage::Dependencies, &mut command)?;
            }
        }
    }
    Ok(())
}

/// Freeze one helper executable next to its recursive native dependencies.
///
/// We create the destination exactly as requested and put the dependencies in
/// a `Frameworks` directory next to it. We refuse a destination that exists.
pub fn freeze_macos_executable(source: &Path, destination: &Path) -> Result<u64, ExportError> {
    if !cfg!(target_os = "macos") {
        return Err(ExportError::new(
            ErrorStage::Freeze,
            "macOS helper freezing requires a macOS host",
        ));
    }
    crate::publish::refuse_existing(destination)?;
    let parent = destination
        .parent()
        .ok_or_else(|| ExportError::new(ErrorStage::Freeze, "helper destination has no parent"))?;
    fs::create_dir_all(parent)
        .map_err(|error| ExportError::io(ErrorStage::Freeze, parent, error))?;
    copy_file(source, destination)?;
    make_executable(destination)?;
    let frameworks = parent.join("Frameworks");
    fs::create_dir_all(&frameworks)
        .map_err(|error| ExportError::io(ErrorStage::Freeze, &frameworks, error))?;
    let mut objects = vec![destination.to_path_buf()];
    let source_directory = source
        .parent()
        .map(Path::to_path_buf)
        .into_iter()
        .collect::<Vec<_>>();
    bundle_dependencies(&mut objects, &frameworks, &source_directory, None)?;
    relocate_dependencies(&objects, "@executable_path/Frameworks", None)?;
    for object in objects.iter().rev() {
        run_command(
            ErrorStage::Freeze,
            Command::new("/usr/bin/codesign")
                .args(["--force", "--sign", "-"])
                .arg(object),
        )?;
    }
    tree_size(parent)
}

fn resolve_dependency_source(
    dependency: &str,
    origin: &Path,
    search_dirs: &[PathBuf],
) -> Option<PathBuf> {
    if dependency.starts_with('/') {
        return Some(PathBuf::from(dependency));
    }
    let name = Path::new(dependency).file_name()?;
    let sibling = origin.parent()?.join(name);
    if sibling.is_file() {
        return Some(sibling);
    }
    search_dirs
        .iter()
        .map(|directory| directory.join(name))
        .find(|path| path.is_file())
}

fn macho_dependencies(path: &Path) -> Result<Vec<String>, ExportError> {
    let output = Command::new("/usr/bin/otool")
        .arg("-L")
        .arg(path)
        .output()
        .map_err(|error| {
            ExportError::new(
                ErrorStage::Dependencies,
                format!("could not run otool: {error}"),
            )
        })?;
    if !output.status.success() {
        return Err(ExportError::command(
            ErrorStage::Dependencies,
            "otool",
            &output,
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout)
        .lines()
        .skip(1)
        .filter_map(|line| line.trim().split(" (compatibility").next())
        .map(str::to_owned)
        .collect())
}

fn is_system_dependency(path: &str) -> bool {
    path.starts_with("/System/Library/") || path.starts_with("/usr/lib/")
}

fn run_command(stage: ErrorStage, command: &mut Command) -> Result<(), ExportError> {
    let program = command.get_program().to_string_lossy().into_owned();
    let output = command
        .output()
        .map_err(|error| ExportError::new(stage, format!("could not run {program}: {error}")))?;
    if output.status.success() {
        Ok(())
    } else {
        Err(ExportError::command(stage, &program, &output))
    }
}

fn run_command_cancellable(
    stage: ErrorStage,
    command: &mut Command,
    cancelled: &AtomicBool,
) -> Result<(), ExportError> {
    let program = command.get_program().to_string_lossy().into_owned();
    let mut child = command
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| ExportError::new(stage, format!("could not run {program}: {error}")))?;
    loop {
        if cancelled.load(Ordering::Relaxed) {
            let _ = child.kill();
            let _ = child.wait();
            return Err(ExportError::new(
                ErrorStage::Cancelled,
                format!("export cancelled while running {program}"),
            ));
        }
        if let Some(status) = child.try_wait().map_err(|error| {
            ExportError::new(stage, format!("could not monitor {program}: {error}"))
        })? {
            let mut stdout = Vec::new();
            let mut stderr = Vec::new();
            if let Some(mut pipe) = child.stdout.take() {
                let _ = pipe.read_to_end(&mut stdout);
            }
            if let Some(mut pipe) = child.stderr.take() {
                let _ = pipe.read_to_end(&mut stderr);
            }
            let output = Output {
                status,
                stdout,
                stderr,
            };
            return if output.status.success() {
                Ok(())
            } else {
                Err(ExportError::command(stage, &program, &output))
            };
        }
        thread::sleep(Duration::from_millis(20));
    }
}
