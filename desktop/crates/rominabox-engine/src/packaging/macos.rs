//! A macOS app: the packager, and the work for a Mac export only. That
//! is the Info.plist and the sandbox entitlements, the launch library next to
//! the player in the kit, and relocating and signing the Mach-O files. We use
//! no Apple tool, so someone can make a Mac game on any system (`crate::mach_o`).

use super::app_files::{copy_file, make_executable, tree_size};
use super::launch_plan::{accounts_folder, game_data_folder};
use super::slices::{self, Arch};
use super::{check_cancelled, kit_file, ErrorStage, ExportError, ExportRequest, Packager};
use crate::icons;
use crate::launch_contract::{app_file, core_file, shipped};
use crate::mach_o::{self, bundle, entitlements::Entitlements, entitlements::Value};
use crate::target::Target;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet, VecDeque};
use std::ffi::OsStr;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};

/// Where the files are in a Mac kit, which is one kit for every Mac.
const KIT: Target = Target::MacosArm64;

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
    /// Every Mach-O in the app, in the order we relocate and sign them, with
    /// the player first.
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
        // We compile nothing and run no Apple tool, so someone can make a Mac
        // game on any machine with a Mac kit.
        Ok(())
    }

    fn player_in_kit(&self) -> PathBuf {
        kit_file(KIT, "player")
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

    /// The kit's launch library next to the player, at the path already
    /// linked into the kit's player, with the slices for the app's processors.
    fn install_launcher(&mut self, runtime_kit: &Path) -> Result<(), ExportError> {
        let library = runtime_kit.join(kit_file(KIT, "launcher"));
        if !library.is_file() {
            return Err(ExportError::new(
                ErrorStage::Validate,
                format!("the runtime kit has no launch library at {}", library.display()),
            )
            .about(&library));
        }
        let installed = self.macos.join(launch_library_name());
        slices::keep(&library, &self.archs, &installed, "This builder's launcher", ErrorStage::Configure)?;
        self.mach_objects.push(installed);
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
            &request.game.title,
            identity,
            request.game.icon.is_some() || icons::default_icon_path(&request.runtime_kit).is_some(),
            &super::macos_minimum::newest(&self.mach_objects)?,
        )?;
        let default_icon = icons::default_icon_path(&request.runtime_kit);
        if let Some(icon) = request.game.icon.as_deref().or(default_icon.as_deref()) {
            icons::create_macos_icon(icon, &self.resources.join("GameIcon.icns"), staging)?;
        }
        // The standard About panel contains Credits.html from Resources.
        let credits = self.resources.join("Credits.html");
        fs::write(&credits, super::legal::credits_html(&self.resources.join("Legal/Licenses"))?)
            .map_err(|error| ExportError::io(ErrorStage::Stage, &credits, error))?;
        Ok(())
    }

    fn finishing(&self) -> Option<&'static str> {
        Some("Signing the local app")
    }

    fn finish(
        &mut self,
        request: &ExportRequest,
        identity: &str,
        _staging: &Path,
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
        // We sign every Mach-O separately without the entitlements, then the
        // player last with them, which seals the rest. We seal a stand-in
        // core that is not code as data.
        let mut nested = Vec::new();
        for object in &self.mach_objects[1..] {
            if is_mach_o_file(object)? {
                nested.push(object.clone());
            }
        }
        let entitlements =
            sandbox_entitlements(identity, accounts_folder(request)?.as_deref(), request.game.show_menu);
        bundle::sign_app(
            &bundle::AppSeal {
                app: &self.app,
                executable: &self.runtime,
                identifier: &bundle_identifier(identity),
                entitlements: &entitlements,
                nested: &nested,
            },
            &|| cancelled.load(Ordering::Relaxed),
        )
        .map_err(|error| {
            if cancelled.load(Ordering::Relaxed) {
                ExportError::new(ErrorStage::Cancelled, "export cancelled while signing")
            } else {
                ExportError::new(ErrorStage::Sign, error)
            }
        })
    }

    fn runtime_bytes(&self) -> Result<u64, ExportError> {
        Ok(tree_size(&self.runtime)?
            + tree_size(&self.resources.join(core_file!(Macos)))?
            + tree_size(&self.frameworks)?
            + tree_size(&self.resources.join(app_file!(MenuAssets)))?
            + tree_size(&self.resources.join(shipped!(Autoconfig).0))?)
    }
}

/// The identifier in the bundle and the signature of a game.
fn bundle_identifier(identity: &str) -> String {
    format!("app.rominabox.game.{identity}")
}

/// `accounts` is the QUICK SIGN IN folder, present exactly when the game has
/// achievements. We grant the network and that folder together. With `menu`,
/// the game has a menu, where DATA may be, and we grant the one file the
/// player chooses in a system panel there, to export the game's data to or
/// import it from.
fn sandbox_entitlements(identity: &str, accounts: Option<&str>, menu: bool) -> Entitlements {
    let mut entitlements = Entitlements::default().with("com.apple.security.app-sandbox", Value::Bool(true));
    if accounts.is_some() {
        entitlements = entitlements.with("com.apple.security.network.client", Value::Bool(true));
    }
    entitlements = entitlements
        .with("com.apple.security.device.usb", Value::Bool(true))
        .with("com.apple.security.device.bluetooth", Value::Bool(true))
        .with(
            "com.apple.security.temporary-exception.files.home-relative-path.read-only",
            Value::Strings(vec![format!("/Library/Application Support/{}/", game_data_folder(identity))]),
        );
    if menu {
        entitlements = entitlements.with("com.apple.security.files.user-selected.read-write", Value::Bool(true));
    }
    if let Some(folder) = accounts {
        entitlements = entitlements.with(
            "com.apple.security.temporary-exception.files.home-relative-path.read-write",
            Value::Strings(vec![format!("/Library/Application Support/{folder}/")]),
        );
    }
    entitlements
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
<key>CFBundleIdentifier</key><string>{}</string>
<key>CFBundleInfoDictionaryVersion</key><string>6.0</string>
<key>CFBundleName</key><string>{}</string>
<key>CFBundlePackageType</key><string>APPL</string>
<key>CFBundleShortVersionString</key><string>1.0</string>
<key>NSHighResolutionCapable</key><true/>
<key>LSMinimumSystemVersion</key><string>{}</string>
{}
</dict></plist>
"#,
        mach_o::plist_text(title),
        bundle_identifier(identity),
        mach_o::plist_text(title),
        minimum_macos,
        icon
    );
    fs::write(path, plist).map_err(|error| ExportError::io(ErrorStage::Configure, path, error))
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
        for dependency in macho_dependencies(&object)? {
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

/// The name of the launch library next to the player, as linked into the
/// kit's player (`@executable_path/`), which is its name in the kit.
fn launch_library_name() -> String {
    kit_file(KIT, "launcher")
        .file_name()
        .expect("the kit's launcher is a file")
        .to_string_lossy()
        .into_owned()
}

/// Whether `path` is a Mach-O file, one slice or several.
fn is_mach_o_file(path: &Path) -> Result<bool, ExportError> {
    let mut head = Vec::with_capacity(8);
    fs::File::open(path)
        .and_then(|file| file.take(8).read_to_end(&mut head))
        .map_err(|error| ExportError::io(ErrorStage::Sign, path, error))?;
    Ok(mach_o::is_mach_o(&head))
}

/// Point every non-system library that `objects` load at its copy under
/// `framework_prefix`, and name each library in a Frameworks folder by
/// `@rpath/`. The launch library is next to the executable, not in
/// Frameworks, and we leave its install name `@executable_path` unchanged,
/// because a rewritten name would point at a missing file and the player
/// would fail to load.
fn relocate_dependencies(
    objects: &[PathBuf],
    framework_prefix: &str,
    cancelled: Option<&AtomicBool>,
) -> Result<(), ExportError> {
    let launch_library = launch_library_name();
    for object in objects {
        if let Some(cancelled) = cancelled {
            check_cancelled(cancelled)?;
        }
        let bytes = fs::read(object).map_err(|error| ExportError::io(ErrorStage::Dependencies, object, error))?;
        if !mach_o::is_mach_o(&bytes) {
            continue;
        }
        let moved = |dependency: &str| {
            let name = Path::new(dependency).file_name()?.to_str()?;
            let kept = is_system_dependency(dependency)
                || dependency.starts_with(&format!("{framework_prefix}/"))
                || name == launch_library;
            (!kept).then(|| format!("{framework_prefix}/{name}"))
        };
        let own = (object.parent().and_then(Path::file_name) == Some(OsStr::new("Frameworks")))
            .then(|| format!("@rpath/{}", object.file_name().unwrap().to_string_lossy()));
        let relocated = mach_o::rename_libraries(&bytes, moved, own.as_deref()).map_err(|error| {
            ExportError::new(ErrorStage::Dependencies, format!("{}: {error}", object.display()))
        })?;
        if relocated != bytes {
            fs::write(object, relocated).map_err(|error| ExportError::io(ErrorStage::Dependencies, object, error))?;
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

/// What `path` loads, each once, without the install name of a library
/// itself. Nothing for a file that is not a Mach-O, according to `otool`.
pub(super) fn macho_dependencies(path: &Path) -> Result<Vec<String>, ExportError> {
    let bytes = fs::read(path).map_err(|error| ExportError::io(ErrorStage::Dependencies, path, error))?;
    if !mach_o::is_mach_o(&bytes) {
        return Ok(Vec::new());
    }
    mach_o::dependencies(&bytes)
        .map_err(|error| ExportError::new(ErrorStage::Dependencies, format!("{}: {error}", path.display())))
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
