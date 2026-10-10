//! A Windows game made into one program, opened as a person opens it. We
//! unpack it under the local application data on its first launch and run
//! that copy on later launches. We replace an older copy with a newer
//! version, still unpack a file whose path passes Windows' 260 characters
//! during unpacking, remove everything the game stores on this computer on
//! UNINSTALL, and refuse a program whose index lists a folder outside the
//! runtimes folder. The launcher is the real one, built from this tree. In
//! the stand-in player we only write the folder it ran from into the launch
//! log, and exit.
//!
//! We ignore them in the exporter tests and run them in the wingame tests.
#![cfg(windows)]

mod export_fixture;
mod sandboxes;
mod support;

use export_fixture::{export_request_from, program_from, unpack, windows_kit, workspace};
use rominabox_engine::packaging::{ExportRequest, ExportTarget};
use sandboxes::{copies, declared, kept, local, previous_data, registered, runtimes, sandbox_folder};
use std::{
    collections::BTreeMap,
    ffi::OsStr,
    fs,
    os::windows::ffi::OsStrExt,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::atomic::AtomicBool,
    time::{Duration, Instant, SystemTime},
};

/// The exit code of the stand-in player. A launch with this code reached the
/// player, while a launcher that stops early exits with 1.
const PLAYED: i32 = 42;
const RAN_AT: &str = "stand-in player ran at ";
/// With STAND_IN_RUN_MS, the stand-in player keeps running for that many
/// milliseconds after it writes its folder.
const STAND_IN: &str = "#include <stdio.h>\n#include <stdlib.h>\n#include <windows.h>\n\
    int main(void) {\n\
        static char path[32768];\n\
        const char *run = getenv(\"STAND_IN_RUN_MS\");\n\
        GetModuleFileNameA(NULL, path, sizeof path);\n\
        printf(\"stand-in player ran at %s\\n\", path);\n\
        fflush(stdout);\n\
        if (run)\n\
            Sleep((DWORD)atoi(run));\n\
        return 42;\n\
    }\n";

/// The length of `path` as Windows counts it, in UTF-16 units.
fn length(path: &Path) -> usize {
    OsStr::encode_wide(path.as_os_str()).count()
}

/// A kit with the launcher from this tree and the stand-in player.
fn kit(root: &Path) -> PathBuf {
    let kit = windows_kit(root);
    let built = rominabox_engine::repo::python()
        .arg(rominabox_engine::repo::at("scripts/build_launcher.py"))
        .arg(root.join("launcher"))
        .output()
        .unwrap();
    assert!(built.status.success(), "{}", String::from_utf8_lossy(&built.stderr));
    // The output contains the path of the launcher from the last build.
    let launcher = String::from_utf8(built.stdout).unwrap();
    fs::copy(launcher.lines().last().unwrap().trim(), kit.join("bin/launcher.exe")).unwrap();
    program_from(&kit.join("bin/retroarch.exe"), STAND_IN, Some("1 ICON \"retroarch.ico\"\n"));
    kit
}

/// A Windows export of the game called `title` from `kit`, into `folder`.
fn request(root: &Path, kit: &Path, title: &str, folder: &str) -> ExportRequest {
    let mut request = export_request_from(root, kit.to_path_buf());
    request.game.target = ExportTarget::Windows;
    request.game.title = title.to_string();
    request.output_dir = root.join(folder);
    request
}

/// An exported game, with its one program and the contents of its pack and plan.
struct Game {
    program: PathBuf,
    identity: String,
    /// The folder in the runtimes folder into which we unpack it.
    runtime: String,
    /// Its data folder, inside its sandbox.
    data: PathBuf,
}

fn export(request: &ExportRequest, root: &Path) -> Game {
    let cancelled = AtomicBool::new(false);
    let program = rominabox_engine::packaging::export_game(request, &cancelled, |_| {})
        .unwrap_or_else(|error| panic!("export failed: {}", error.message))
        .app_path;
    let look = root.join(format!("look-{}", request.output_dir.file_name().unwrap().to_string_lossy()));
    let runtime = unpack(&program, &look);
    let plan = fs::read_to_string(look.join("Resources/launch.plan")).unwrap();
    let field = |name: &str| {
        plan.lines()
            .find_map(|line| line.strip_prefix(&format!("{name}\t")).map(str::to_owned))
            .unwrap_or_else(|| panic!("the plan has no {name}\n{plan}"))
    };
    let identity = field("identity");
    // Inside the sandbox the per-user folder is the sandbox's AC folder.
    let data_dir = field("data_dir");
    let below = data_dir.strip_prefix("$user_data").unwrap_or(&data_dir);
    let data = sandbox_folder(&identity)
        .join("AC")
        .join(below.trim_start_matches(['/', '\\']).replace('/', "\\"));
    Game {
        program,
        identity,
        runtime: runtime.rsplit('/').next().unwrap().to_string(),
        data,
    }
}

struct Launched {
    code: Option<i32>,
    errors: String,
}

/// Open the game quietly, as in a harness, so that there is no dialog or
/// sound, and without anything that a test may have set for another launch.
fn launch(program: &Path, root: &Path) -> Launched {
    launch_in(program, root, None)
}

/// Open the game as in `launch`, with `user_data`, when there is one, in
/// place of the person's per-user folder.
fn launch_in(program: &Path, root: &Path, user_data: Option<&Path>) -> Launched {
    launch_with(program, root, user_data, &[])
}

/// Open the game as in `launch_in`, with the variables `extra` set.
fn launch_with(program: &Path, root: &Path, user_data: Option<&Path>, extra: &[(&str, &str)]) -> Launched {
    static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let errors = root.join(format!(
        "launch-{}.err",
        NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    let mut command = Command::new(program);
    let test_user_data = declared("RIB_ENV_TEST_USER_DATA");
    match user_data {
        Some(folder) => command.env(&test_user_data, folder),
        None => command.env_remove(&test_user_data),
    };
    command.envs(extra.iter().copied());
    let mut child = command
        .env("ROMINABOX_QUIET", "1")
        .env_remove("ROMINABOX_PLAN_ONLY")
        .env_remove("ROMINABOX_MENU_SCRIPT")
        .env_remove("ROMINABOX_MENU_SHOT")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(fs::File::create(&errors).unwrap())
        .spawn()
        .unwrap();
    let started = Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break status;
        }
        assert!(
            started.elapsed() < Duration::from_secs(60),
            "the game did not end within a minute (pid {}); left running for inspection",
            child.id()
        );
        std::thread::sleep(Duration::from_millis(50));
    };
    Launched { code: status.code(), errors: fs::read_to_string(&errors).unwrap_or_default() }
}

/// The folder of the stand-in player at each launch, from the game's log.
fn ran_at(game: &Game) -> Vec<String> {
    fs::read_to_string(game.data.join("logs/launch.log"))
        .unwrap_or_default()
        .lines()
        .filter_map(|line| line.strip_prefix(RAN_AT).map(str::to_lowercase))
        .collect()
}

fn player_in(copy: &Path) -> String {
    copy.join("Runtime").join("retroarch.exe").to_string_lossy().to_lowercase()
}

/// Every file below `folder`, by its path there, with its bytes and when it
/// was written.
fn files(folder: &Path) -> BTreeMap<PathBuf, (Vec<u8>, SystemTime)> {
    let mut found = BTreeMap::new();
    let mut pending = vec![folder.to_path_buf()];
    while let Some(next) = pending.pop() {
        for entry in fs::read_dir(&next).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                pending.push(path);
            } else {
                let written = fs::metadata(&path).unwrap().modified().unwrap();
                found.insert(path.strip_prefix(folder).unwrap().to_path_buf(), (fs::read(&path).unwrap(), written));
            }
        }
    }
    found
}

fn contents(folder: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    files(folder).into_iter().map(|(path, (bytes, _))| (path, bytes)).collect()
}

#[test]
#[ignore = "launches a stand-in game in its sandbox; the wingame scope runs it"]
fn the_first_launch_unpacks_the_game_and_the_next_runs_that_copy() {
    let root = workspace();
    let kit = kit(&root);
    let game = export(&request(&root, &kit, "Unpacked Once", "out"), &root);
    let _kept = kept(&game.identity);

    let first = launch(&game.program, &root);
    assert_eq!(first.code, Some(PLAYED), "{}", first.errors);
    assert_eq!(copies(&game.identity), [game.runtime.clone()], "one copy, and nothing an unpacking left");
    let copy = runtimes().join(&game.runtime);
    // Everything in the program, as we packed it in the export.
    let packed = root.join("packed");
    unpack(&game.program, &packed);
    assert_eq!(contents(&copy), contents(&packed));
    let unpacked = files(&copy);

    let second = launch(&game.program, &root);
    assert_eq!(second.code, Some(PLAYED), "{}", second.errors);
    assert_eq!(copies(&game.identity), [game.runtime.clone()]);
    assert!(files(&copy) == unpacked, "the second launch wrote the game's files again");
    assert_eq!(ran_at(&game), [player_in(&copy), player_in(&copy)]);
}

/// When the game is opened again while it runs, we start no second copy,
/// whatever started it: the second launch quits at once, and only the first
/// reaches the player, because two players would use one data folder.
#[test]
#[ignore = "launches a stand-in game in its sandbox; the wingame scope runs it"]
fn a_game_opened_again_while_it_runs_is_not_started_twice() {
    let root = workspace();
    let kit = kit(&root);
    let game = export(&request(&root, &kit, "Opened Twice", "out"), &root);
    let _kept = kept(&game.identity);

    let first = std::thread::spawn({
        let program = game.program.clone();
        let root = root.to_path_buf();
        move || launch_with(&program, &root, None, &[("STAND_IN_RUN_MS", "8000")])
    });
    let started = Instant::now();
    while ran_at(&game).is_empty() {
        assert!(started.elapsed() < Duration::from_secs(60), "the first launch never reached the player");
        std::thread::sleep(Duration::from_millis(50));
    }
    let second = launch(&game.program, &root);
    assert_eq!(second.code, Some(0), "{}", second.errors);
    assert_eq!(ran_at(&game).len(), 1, "the second launch started a second player");
    let first = first.join().unwrap();
    assert_eq!(first.code, Some(PLAYED), "{}", first.errors);
    assert_eq!(ran_at(&game).len(), 1);
}

/// We unpack a game into a folder with a longer name than the game's, then
/// rename it into place, and Windows cannot open an ordinary path of 260
/// characters or more. So a file with a 259-character path at the place the
/// player reads it is past the limit while we unpack it.
#[test]
#[ignore = "launches a stand-in game in its sandbox; the wingame scope runs it"]
fn a_file_past_windows_path_limit_while_it_is_unpacked_still_unpacks_and_the_game_starts() {
    let root = workspace();
    let kit = kit(&root);
    let mut long = request(&root, &kit, "Long Paths", "out");
    // The game's file keeps its name in Resources/content. We name the folder
    // we unpack into after the game's identity and pack, in 33 characters.
    let folder = local().join("ROM-in-a-Box").join("Runtimes").join("x".repeat(33));
    let content = folder.join("Resources").join("content");
    let name_length = 259 - length(&content) - 1 - ".bin".len();
    long.game.rom = root.join(format!("{}.bin", "l".repeat(name_length)));
    fs::write(&long.game.rom, b"RIBlong").unwrap();
    let game = export(&long, &root);
    let _kept = kept(&game.identity);
    let file = runtimes().join(&game.runtime).join("Resources/content").join(long.game.rom.file_name().unwrap());
    assert_eq!(length(&file), 259, "{}", file.display());

    let launched = launch(&game.program, &root);
    assert_eq!(launched.code, Some(PLAYED), "{}", launched.errors);
    assert_eq!(copies(&game.identity), [game.runtime.clone()]);
    assert_eq!(fs::read(&file).unwrap(), b"RIBlong");
    assert_eq!(ran_at(&game), [player_in(&runtimes().join(&game.runtime))]);
}

/// The same game exported again with another setting is a newer version. We
/// unpack it beside the previous one and then remove that one, and the
/// player's saves stay.
#[test]
#[ignore = "launches a stand-in game in its sandbox; the wingame scope runs it"]
fn a_newer_version_unpacks_into_a_folder_of_its_own_and_the_older_copy_goes() {
    let root = workspace();
    let kit = kit(&root);
    let older = export(&request(&root, &kit, "Newer Version", "older"), &root);
    let mut changed = request(&root, &kit, "Newer Version", "newer");
    changed.game.keep_playing_in_background = true;
    let newer = export(&changed, &root);
    assert_eq!(newer.identity, older.identity, "the same game");
    assert_ne!(newer.runtime, older.runtime, "another version of it");
    let _kept = kept(&older.identity);

    let first = launch(&older.program, &root);
    assert_eq!(first.code, Some(PLAYED), "{}", first.errors);
    assert_eq!(copies(&older.identity), [older.runtime.clone()]);
    let save = older.data.join("saves/kept.srm");
    fs::create_dir_all(save.parent().unwrap()).unwrap();
    fs::write(&save, b"the player's save").unwrap();

    let second = launch(&newer.program, &root);
    assert_eq!(second.code, Some(PLAYED), "{}", second.errors);
    assert_eq!(copies(&newer.identity), [newer.runtime.clone()], "the older copy is gone");
    assert_eq!(
        ran_at(&newer),
        [player_in(&runtimes().join(&older.runtime)), player_in(&runtimes().join(&newer.runtime))]
    );
    assert_eq!(fs::read(&save).unwrap(), b"the player's save");
}

/// In a test we give the launcher a per-user folder of the test's own in place
/// of the person's. We unpack the game there, outside its sandbox, and the
/// game's data is in the sandbox's folder, as Windows reports it inside,
/// because nothing inside the sandbox can open the test's folder.
#[test]
#[ignore = "launches a stand-in game in its sandbox; the wingame scope runs it"]
fn a_game_given_a_tests_per_user_folder_unpacks_there_and_plays_in_its_sandbox() {
    let root = workspace();
    let kit = kit(&root);
    let game = export(&request(&root, &kit, "Own Folder", "out"), &root);
    let _kept = kept(&game.identity);
    // It exists, as a person's per-user folder always does.
    let user_data = root.join("user data");
    fs::create_dir_all(&user_data).unwrap();

    let launched = launch_in(&game.program, &root, Some(&user_data));
    assert_eq!(launched.code, Some(PLAYED), "{}", launched.errors);
    let copy = user_data.join("ROM-in-a-Box").join("Runtimes").join(&game.runtime);
    assert!(copy.is_dir(), "it did not unpack into the test's folder");
    assert_eq!(copies(&game.identity), Vec::<String>::new(), "it unpacked into the person's folder");
    assert!(game.data.join("retroarch.cfg").is_file(), "its data is not in its sandbox's folder");
    assert_eq!(ran_at(&game), [player_in(&copy)]);
}

/// When the player chooses UNINSTALL in the game's menu, we leave a marker in
/// the game's data folder and close the game. In the launcher we then remove
/// the game's sandbox and its data, the data from before the sandbox, and
/// every unpacked copy, and keep the program that the person opened.
#[test]
#[ignore = "launches a stand-in game in its sandbox; the wingame scope runs it"]
fn uninstall_removes_the_games_data_its_sandbox_and_every_unpacked_copy() {
    let root = workspace();
    let kit = kit(&root);
    let game = export(&request(&root, &kit, "Forgotten", "out"), &root);
    let _kept = kept(&game.identity);
    let first = launch(&game.program, &root);
    assert_eq!(first.code, Some(PLAYED), "{}", first.errors);

    // An older version that was still running when we unpacked this one, and
    // the data folder of a game in the older layout, outside a sandbox.
    let older = runtimes().join(format!("{}-00000000", game.identity));
    support::copy_tree(&runtimes().join(&game.runtime), &older);
    fs::create_dir_all(previous_data(&game.identity).join("saves")).unwrap();
    fs::write(previous_data(&game.identity).join("saves/old.srm"), b"old").unwrap();
    assert!(game.data.join("retroarch.cfg").is_file(), "the first launch wrote the game's data");
    assert!(registered(&game.identity), "the first launch registered the game's sandbox");
    fs::write(game.data.join(declared("RIB_FORGET_MARKER")), b"").unwrap();

    let last = launch(&game.program, &root);
    assert_eq!(last.code, Some(PLAYED), "{}", last.errors);
    assert_eq!(copies(&game.identity), Vec::<String>::new());
    assert!(!sandbox_folder(&game.identity).exists(), "the sandbox's folder, with the game's data, is still there");
    assert!(!previous_data(&game.identity).exists(), "what the game kept before its sandbox is still there");
    assert!(game.program.is_file(), "the program the person opened is gone");
    assert!(!registered(&game.identity), "the game's sandbox is still registered");
}

/// A copy of the program `program` whose index lists `runtime` as the folder
/// to unpack the game into, written to `copy`.
fn naming(program: &Path, copy: &Path, runtime: &str) {
    let mut bytes = fs::read(program).unwrap();
    let trailer = bytes.len() - 24;
    let start = u64::from_le_bytes(bytes[trailer..trailer + 8].try_into().unwrap()) as usize;
    let index = bytes[start..trailer].to_vec();
    // "RIBPACK1", then the runtime folder's length and its name.
    let named = u16::from_le_bytes([index[8], index[9]]) as usize;
    let mut rewritten = index[..8].to_vec();
    rewritten.extend((runtime.len() as u16).to_le_bytes());
    rewritten.extend(runtime.as_bytes());
    rewritten.extend(&index[10 + named..]);
    bytes.truncate(start);
    bytes.extend(&rewritten);
    bytes.extend((start as u64).to_le_bytes());
    bytes.extend((rewritten.len() as u64).to_le_bytes());
    bytes.extend(b"RIBTAIL1");
    fs::write(copy, bytes).unwrap();
}

/// We treat a program as damaged when its index lists an unpacking folder
/// that is not directly inside the runtimes folder, as when its bytes are
/// damaged, and refuse it before we write or remove anything. When we unpack,
/// we remove the older versions beside the new folder that are named for the
/// same game, so we would otherwise unpack and remove anywhere the index
/// lists. We run this in a per-user folder of the test's own.
#[test]
#[ignore = "launches a stand-in game; the wingame scope runs it"]
fn a_program_naming_a_folder_outside_the_runtimes_folder_is_refused_and_removes_nothing() {
    let root = workspace();
    let kit = kit(&root);
    let game = export(&request(&root, &kit, "Elsewhere", "out"), &root);
    let _kept = kept(&game.identity);
    let user_data = root.join("user data");
    for (index, elsewhere) in [
        format!("Elsewhere/{}", game.runtime),
        format!("ROM-in-a-Box/Games/{}", game.runtime),
        format!("ROM-in-a-Box/Runtimes/deeper/{}", game.runtime),
    ]
    .into_iter()
    .enumerate()
    {
        let program = root.join(format!("elsewhere-{index}.exe"));
        naming(&game.program, &program, &elsewhere);
        let target = user_data.join(&elsewhere);
        // Another version of the same game beside the folder in the index.
        let beside = target.with_file_name(format!("{}-beside", game.identity));
        fs::create_dir_all(&beside).unwrap();
        fs::write(beside.join("kept"), b"kept").unwrap();

        let launched = launch_in(&program, &root, Some(&user_data));
        assert_eq!(fs::read(beside.join("kept")).ok().as_deref(), Some(&b"kept"[..]), "it removed {}", beside.display());
        assert!(!target.exists(), "it unpacked into {elsewhere}");
        assert_eq!(launched.code, Some(1), "{elsewhere}: {}", launched.errors);
        assert!(launched.errors.contains("damaged"), "{elsewhere}: {}", launched.errors);
    }
}
