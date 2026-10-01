/* The Windows entry of a game's launcher, in the game's own .exe, the one a
 * person double-clicks. We prepare the launch (launch.c), then start the
 * player beside it with RetroArch's arguments, the game's variables, the
 * launch log as its output and the data folder as its working directory, and
 * wait for it, so the game opens and closes as one program. */
#define WIN32_LEAN_AND_MEAN
#include <windows.h>
#include <aclapi.h>
#include <knownfolders.h>
#include <sddl.h>
#include <shlobj.h>
#include <tlhelp32.h>
#include <userenv.h>

#include <ctype.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <wchar.h>

#include "../accounts_folder.h"
#include "../launch.h"
#include "../portable_fs.h"
#include "pad_relay.h"
#include "unpack.h"
#include "../../../../vendor/retroarch/rominabox_launch.h"

/* The longest path Windows takes, in UTF-16 units, with its terminator. */
#define WIDE_PATH_CAP 32768
/* FNV-1a, 64 bits, for the name of the mapping we keep for a running game. */
#define FNV_OFFSET_BASIS 1469598103934665603ULL
#define FNV_PRIME 1099511628211ULL

#define RIB_CORE_FILE(platform, file) static const char core_##platform[] = file;
#define RIB_WINDOWS_PART(name, path) static const char part_##name[] = path;
#include "../launch_contract.inc"

static wchar_t *wide(const char *text) {
    int size = MultiByteToWideChar(CP_UTF8, MB_ERR_INVALID_CHARS, text, -1, NULL, 0);
    wchar_t *result = size > 0 ? malloc((size_t)size * sizeof *result) : NULL;
    if (!result || !MultiByteToWideChar(CP_UTF8, MB_ERR_INVALID_CHARS, text, -1, result, size))
        rominabox_launch_die("a path is not text Windows can spell");
    return result;
}

static char *utf8(const wchar_t *text) {
    int size = WideCharToMultiByte(CP_UTF8, WC_ERR_INVALID_CHARS, text, -1, NULL, 0, NULL, NULL);
    char *result = size > 0 ? malloc((size_t)size) : NULL;
    if (!result || !WideCharToMultiByte(CP_UTF8, WC_ERR_INVALID_CHARS, text, -1, result, size, NULL, NULL))
        rominabox_launch_die("a path is not text the launcher can read");
    return result;
}

/* This .exe, in full. */
static void own_path(wchar_t *path, DWORD capacity) {
    DWORD length = GetModuleFileNameW(NULL, path, capacity);
    if (length == 0 || length >= capacity)
        rominabox_launch_die("could not find the launcher");
}

/* The folder this .exe is in, as UTF-8. */
static void own_folder(char *out, size_t out_cap) {
    wchar_t path[WIDE_PATH_CAP];
    wchar_t *last;
    char *folder;
    own_path(path, sizeof path / sizeof path[0]);
    last = wcsrchr(path, L'\\');
    if (!last)
        rominabox_launch_die("the launcher is not inside a folder");
    *last = L'\0';
    folder = utf8(path);
    if (strlen(folder) >= out_cap)
        rominabox_launch_die("a path does not fit");
    strcpy(out, folder);
    free(folder);
}

/* We run a game made into one program from the folder we unpacked it into:
 * that folder and the launcher in it, set once before anything else. Empty
 * for a game laid out as a folder, which we run from where this program is. */
static char unpacked_folder[LAUNCH_PATH_CAP];
static wchar_t unpacked_program[WIDE_PATH_CAP];

static void game_folder(char *out, size_t out_cap) {
    if (!unpacked_folder[0]) {
        own_folder(out, out_cap);
        return;
    }
    if (strlen(unpacked_folder) >= out_cap)
        rominabox_launch_die("a path does not fit");
    strcpy(out, unpacked_folder);
}

/* The launcher to start inside the sandbox: the one in the game's folder. */
static void game_program(wchar_t *path, DWORD capacity) {
    if (!unpacked_program[0]) {
        own_path(path, capacity);
        return;
    }
    if (wcslen(unpacked_program) >= capacity)
        rominabox_launch_die("a path does not fit");
    wcscpy(path, unpacked_program);
}

/* The per-user application data folder, %LOCALAPPDATA%, or the folder that
 * a test sets in its place. */
static char *local_application_data(void) {
    static wchar_t named[WIDE_PATH_CAP];
    wchar_t *name = wide(RIB_ENV_TEST_USER_DATA);
    DWORD length = GetEnvironmentVariableW(name, named, sizeof named / sizeof named[0]);
    PWSTR found = NULL;
    char *path;
    free(name);
    if (length > 0 && length < sizeof named / sizeof named[0]) {
        path = utf8(named);
        if (!fs_is_absolute(path))
            rominabox_launch_die(RIB_ENV_TEST_USER_DATA " is not an absolute path");
        return path;
    }
    if (FAILED(SHGetKnownFolderPath(&FOLDERID_LocalAppData, 0, NULL, &found)))
        rominabox_launch_die("there is no per-user folder to keep this game's files in");
    path = utf8(found);
    CoTaskMemFree(found);
    return path;
}

/* When a person double-clicks the game or opens it from a shortcut or the
 * Start menu, Explorer is its parent. Otherwise a script or a harness
 * started it. */
static int opened_by_explorer(void) {
    DWORD own = GetCurrentProcessId();
    DWORD parent = 0;
    int found = 0;
    PROCESSENTRY32W entry;
    HANDLE snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
    if (snapshot == INVALID_HANDLE_VALUE)
        return 0;
    entry.dwSize = sizeof entry;
    for (BOOL more = Process32FirstW(snapshot, &entry); more; more = Process32NextW(snapshot, &entry))
        if (entry.th32ProcessID == own) {
            parent = entry.th32ParentProcessID;
            break;
        }
    entry.dwSize = sizeof entry;
    for (BOOL more = parent ? Process32FirstW(snapshot, &entry) : FALSE; more;
         more = Process32NextW(snapshot, &entry))
        if (entry.th32ProcessID == parent) {
            found = _wcsicmp(entry.szExeFile, L"explorer.exe") == 0;
            break;
        }
    CloseHandle(snapshot);
    return found;
}

/* When a person opens a game again while it runs, we bring it to the front
 * instead of starting it a second time, as on a Mac, because two players
 * would use one data folder. In the running game's launcher we keep a mapping
 * named after its data folder, with the player's process id, while it runs.
 * This rule does not apply to a launch from a test or a script. */
static DWORD *running_player;

static BOOL CALLBACK bring_forward(HWND window, LPARAM player) {
    DWORD owner = 0;
    GetWindowThreadProcessId(window, &owner);
    if (owner != (DWORD)player || !IsWindowVisible(window) || GetWindow(window, GW_OWNER))
        return TRUE;
    if (IsIconic(window))
        ShowWindow(window, SW_RESTORE);
    SetForegroundWindow(window);
    return FALSE;
}

static void one_game_per_data_folder(const char *data_dir) {
    unsigned long long hash = FNV_OFFSET_BASIS;
    const unsigned char *cursor;
    wchar_t name[64];
    HANDLE mapping;
    DWORD player;
    /* A folder's name, however it is spelled: case and separators aside. */
    for (cursor = (const unsigned char *)data_dir; *cursor; cursor++)
        hash = (hash ^ (unsigned char)(*cursor == '\\' ? '/' : tolower(*cursor))) * FNV_PRIME;
    swprintf(name, sizeof name / sizeof name[0], L"Local\\" ROMINABOX_NAME L" game %016llx", hash);
    mapping = CreateFileMappingW(INVALID_HANDLE_VALUE, NULL, PAGE_READWRITE, 0, sizeof *running_player, name);
    if (!mapping)
        return;
    if (GetLastError() != ERROR_ALREADY_EXISTS) {
        running_player = MapViewOfFile(mapping, FILE_MAP_ALL_ACCESS, 0, 0, sizeof *running_player);
        return;
    }
    running_player = MapViewOfFile(mapping, FILE_MAP_READ, 0, 0, sizeof *running_player);
    player = running_player ? *running_player : 0;
    if (player)
        EnumWindows(bring_forward, (LPARAM)player);
    exit(0);
}

/* One argument in the form that the C runtime parses back into the same
 * argument, quoted, with the backslashes before a quote doubled. */
static void append_argument(wchar_t **line, size_t *length, size_t *capacity, const wchar_t *argument) {
    size_t needed = *length + wcslen(argument) * 2 + 4;
    const wchar_t *cursor;
    if (needed > *capacity) {
        *capacity = needed * 2;
        *line = realloc(*line, *capacity * sizeof **line);
        if (!*line)
            rominabox_launch_die("out of memory");
    }
    if (*length)
        (*line)[(*length)++] = L' ';
    (*line)[(*length)++] = L'"';
    for (cursor = argument; *cursor; cursor++) {
        size_t backslashes = 0;
        while (cursor[0] == L'\\') {
            backslashes++;
            cursor++;
        }
        if (*cursor == L'\0') {
            while (backslashes--) {
                (*line)[(*length)++] = L'\\';
                (*line)[(*length)++] = L'\\';
            }
            break;
        }
        if (*cursor == L'"') {
            while (backslashes--) {
                (*line)[(*length)++] = L'\\';
                (*line)[(*length)++] = L'\\';
            }
            (*line)[(*length)++] = L'\\';
        } else {
            while (backslashes--)
                (*line)[(*length)++] = L'\\';
        }
        (*line)[(*length)++] = *cursor;
    }
    (*line)[(*length)++] = L'"';
    (*line)[*length] = L'\0';
}

/* We run a game in its own sandbox, like a Mac game in its container. The
 * game can read its own folder, its data is in the sandbox's folder, and it
 * can reach the network and the QUICK SIGN IN folder only with achievements.
 * On macOS the system starts an app in its sandbox. On Windows we set up the
 * sandbox in the program that a person opens and start that program again
 * inside it. Inside, Windows reports the sandbox's own per-user folder, so we
 * pass the values known only outside in these. */
static const wchar_t outside_user_data[] = L"ROMINABOX_OUTSIDE_USER_DATA";
static const wchar_t outside_opened_by_person[] = L"ROMINABOX_OPENED_BY_PERSON";
/* The program that the person opened, also started from a taskbar pin. */
static const wchar_t outside_program[] = L"ROMINABOX_OUTSIDE_PROGRAM";
/* internetClient, the capability to open connections to the internet. */
static const wchar_t internet_client[] = L"S-1-15-3-1";

static int inside_sandbox(void) {
    HANDLE token;
    DWORD contained = 0;
    DWORD size = 0;
    BOOL asked;
    if (!OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &token))
        rominabox_launch_die("could not tell whether the game is in its sandbox");
    asked = GetTokenInformation(token, TokenIsAppContainer, &contained, sizeof contained, &size);
    CloseHandle(token);
    if (!asked)
        rominabox_launch_die("could not tell whether the game is in its sandbox");
    return contained != 0;
}

/* The game's sandbox, registered for this user at the game's first launch. */
static PSID sandbox_of(const LaunchGame *game) {
    char name[sizeof RIB_GAME_APP_ID_PREFIX + sizeof game->identity];
    wchar_t *wide_name;
    wchar_t *shown;
    PSID sid = NULL;
    HRESULT made;
    snprintf(name, sizeof name, RIB_GAME_APP_ID_PREFIX "%s", game->identity);
    wide_name = wide(name);
    shown = wide(game->title[0] ? game->title : name);
    made = CreateAppContainerProfile(wide_name, shown, shown, NULL, 0, &sid);
    if (made == HRESULT_FROM_WIN32(ERROR_ALREADY_EXISTS))
        made = DeriveAppContainerSidFromAppContainerName(wide_name, &sid);
    free(wide_name);
    free(shown);
    if (FAILED(made))
        rominabox_launch_die("could not make the game's sandbox");
    return sid;
}

/* Whether `acl` already grants `rights` to `sid` here and in everything below. */
static int allowed(PACL acl, PSID sid, DWORD rights) {
    DWORD index;
    for (index = 0; acl && index < acl->AceCount; index++) {
        ACCESS_ALLOWED_ACE *entry;
        if (!GetAce(acl, index, (void **)&entry) || entry->Header.AceType != ACCESS_ALLOWED_ACE_TYPE)
            continue;
        if ((entry->Header.AceFlags & (OBJECT_INHERIT_ACE | CONTAINER_INHERIT_ACE))
                == (OBJECT_INHERIT_ACE | CONTAINER_INHERIT_ACE)
            && (entry->Mask & rights) == rights && EqualSid((PSID)&entry->SidStart, sid))
            return 1;
    }
    return 0;
}

/* Grant the sandbox `rights` in `folder` and everything in it. Granting
 * rewrites the access list of every file below, so we do it once, not on
 * every launch. On a volume without access lists, such as a FAT memory
 * stick, everyone already has access. */
static void let_sandbox(PSID sid, const char *folder, DWORD rights) {
    wchar_t *path = wide(folder);
    wchar_t root[MAX_PATH];
    DWORD features = 0;
    PACL acl = NULL;
    PACL granted = NULL;
    PSECURITY_DESCRIPTOR descriptor = NULL;
    EXPLICIT_ACCESSW entry = {0};
    if (GetVolumePathNameW(path, root, MAX_PATH)
        && GetVolumeInformationW(root, NULL, 0, NULL, NULL, &features, NULL, 0)
        && !(features & FILE_PERSISTENT_ACLS)) {
        free(path);
        return;
    }
    if (GetNamedSecurityInfoW(path, SE_FILE_OBJECT, DACL_SECURITY_INFORMATION, NULL, NULL, &acl, NULL,
                              &descriptor) != ERROR_SUCCESS)
        rominabox_launch_die("could not read who may open the game's files");
    if (!allowed(acl, sid, rights)) {
        entry.grfAccessPermissions = rights;
        entry.grfAccessMode = GRANT_ACCESS;
        entry.grfInheritance = SUB_CONTAINERS_AND_OBJECTS_INHERIT;
        entry.Trustee.TrusteeForm = TRUSTEE_IS_SID;
        entry.Trustee.TrusteeType = TRUSTEE_IS_WELL_KNOWN_GROUP;
        entry.Trustee.ptstrName = (LPWSTR)sid;
        if (SetEntriesInAclW(1, &entry, acl, &granted) != ERROR_SUCCESS
            || SetNamedSecurityInfoW(path, SE_FILE_OBJECT, DACL_SECURITY_INFORMATION, NULL, NULL, granted,
                                     NULL) != ERROR_SUCCESS)
            rominabox_launch_die("could not let the game's sandbox open its files");
        LocalFree(granted);
    }
    LocalFree(descriptor);
    free(path);
}

/* Outside the sandbox: set it up, start this program inside it, and wait
 * for it, so the game still opens and closes as one program. */
static int start_in_sandbox(const char *folder, const LaunchGame *game) {
    char accounts[LAUNCH_PATH_CAP];
    char previous[LAUNCH_PATH_CAP];
    char *user_data = local_application_data();
    static wchar_t program[WIDE_PATH_CAP];
    wchar_t *line = _wcsdup(GetCommandLineW());
    wchar_t *user_data_wide = wide(user_data);
    PSID internet = NULL;
    SID_AND_ATTRIBUTES capability = {0};
    SECURITY_CAPABILITIES capabilities = {0};
    SIZE_T size = 0;
    LPPROC_THREAD_ATTRIBUTE_LIST attributes;
    STARTUPINFOEXW startup = {0};
    PROCESS_INFORMATION process = {0};
    JOBOBJECT_EXTENDED_LIMIT_INFORMATION limits = {0};
    HANDLE job;
    PadRelay *pads;
    DWORD code = 1;

    game_program(program, sizeof program / sizeof program[0]);
    capabilities.AppContainerSid = sandbox_of(game);
    let_sandbox(capabilities.AppContainerSid, folder, FILE_GENERIC_READ | FILE_GENERIC_EXECUTE);
    /* The data folder of a game exported in the older layout, outside a
     * sandbox, read only, so that we can copy it on the first launch inside,
     * as a Mac game may read its old folder in the real home. */
    rominabox_game_data_folder(game, user_data, previous, sizeof previous);
    if (fs_is_directory(previous))
        let_sandbox(capabilities.AppContainerSid, previous, FILE_GENERIC_READ | FILE_GENERIC_EXECUTE);
    if (game->achievements) {
        if (!ConvertStringSidToSidW(internet_client, &internet))
            rominabox_launch_die("could not name the network for the game's sandbox");
        capability.Sid = internet;
        capability.Attributes = SE_GROUP_ENABLED;
        capabilities.Capabilities = &capability;
        capabilities.CapabilityCount = 1;
        /* Without the folder, the player plays on without QUICK SIGN IN, as
         * we report in the launch inside. */
        if (game->accounts_name[0]
            && rominabox_accounts_folder(user_data, game->accounts_name, accounts, sizeof accounts) == 0)
            let_sandbox(capabilities.AppContainerSid, accounts, FILE_ALL_ACCESS);
    }

    SetEnvironmentVariableW(outside_user_data, user_data_wide);
    SetEnvironmentVariableW(outside_opened_by_person, opened_by_explorer() ? L"" LAUNCH_SWITCH_ON : NULL);
    {
        static wchar_t opened[WIDE_PATH_CAP];
        own_path(opened, sizeof opened / sizeof opened[0]);
        SetEnvironmentVariableW(outside_program, opened);
    }
    pads = pad_relay_start();
    InitializeProcThreadAttributeList(NULL, 1, 0, &size);
    attributes = HeapAlloc(GetProcessHeap(), 0, size);
    if (!line || !attributes || !InitializeProcThreadAttributeList(attributes, 1, 0, &size)
        || !UpdateProcThreadAttribute(attributes, 0, PROC_THREAD_ATTRIBUTE_SECURITY_CAPABILITIES, &capabilities,
                                      sizeof capabilities, NULL, NULL))
        rominabox_launch_die("could not start the game in its sandbox");
    startup.StartupInfo.cb = sizeof startup;
    startup.lpAttributeList = attributes;
    /* We give the program inside the same outputs as this one, because in a
     * harness we read why a launch stopped from its error output. */
    startup.StartupInfo.dwFlags = STARTF_USESTDHANDLES;
    startup.StartupInfo.hStdInput = GetStdHandle(STD_INPUT_HANDLE);
    startup.StartupInfo.hStdOutput = GetStdHandle(STD_OUTPUT_HANDLE);
    startup.StartupInfo.hStdError = GetStdHandle(STD_ERROR_HANDLE);
    job = CreateJobObjectW(NULL, NULL);
    limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
    if (job)
        SetInformationJobObject(job, JobObjectExtendedLimitInformation, &limits, sizeof limits);
    if (!CreateProcessW(program, line, NULL, NULL, TRUE, EXTENDED_STARTUPINFO_PRESENT | CREATE_SUSPENDED, NULL,
                        NULL, &startup.StartupInfo, &process))
        rominabox_launch_die("could not start the game in its sandbox");
    if (job)
        AssignProcessToJobObject(job, process.hProcess);
    ResumeThread(process.hThread);
    CloseHandle(process.hThread);
    WaitForSingleObject(process.hProcess, INFINITE);
    GetExitCodeProcess(process.hProcess, &code);
    CloseHandle(process.hProcess);
    pad_relay_stop(pads);
    if (job)
        CloseHandle(job);
    DeleteProcThreadAttributeList(attributes);
    HeapFree(GetProcessHeap(), 0, attributes);
    FreeSid(capabilities.AppContainerSid);
    LocalFree(internet);
    free(line);
    free(user_data_wide);
    free(user_data);
    return (int)code;
}

/* When the player chooses UNINSTALL in the menu, we leave RIB_FORGET_MARKER
 * in the game's data folder. Once the game has ended, we remove its sandbox,
 * with the saves and settings in the sandbox's folder, its data folder from
 * the older layout outside a sandbox, and every unpacked copy of it. The
 * program that the person opened stays. We run this outside the sandbox,
 * with all of the person's rights, so we only remove the folder of the
 * sandbox, a folder directly inside the games folder, or a copy unpacked
 * beside the one that ran. */
static void forget_if_asked(const LaunchGame *game) {
    char name[sizeof RIB_GAME_APP_ID_PREFIX + sizeof game->identity];
    char data[LAUNCH_PATH_CAP];
    char marker[LAUNCH_PATH_CAP];
    char *user_data = local_application_data();
    char *container_utf8 = NULL;
    wchar_t *wide_name;
    PWSTR container = NULL;
    PSID sid = NULL;
    snprintf(name, sizeof name, RIB_GAME_APP_ID_PREFIX "%s", game->identity);
    wide_name = wide(name);
    /* Inside its sandbox the game's per-user folder is the sandbox's own. */
    if (game->sandbox && SUCCEEDED(DeriveAppContainerSidFromAppContainerName(wide_name, &sid))) {
        LPWSTR sid_text = NULL;
        if (ConvertSidToStringSidW(sid, &sid_text)) {
            if (SUCCEEDED(GetAppContainerFolderPath(sid_text, &container)))
                container_utf8 = utf8(container);
            LocalFree(sid_text);
        }
        FreeSid(sid);
    }
    rominabox_game_data_folder(game, container_utf8 ? container_utf8 : user_data, data, sizeof data);
    rominabox_launch_join(marker, sizeof marker, data, RIB_FORGET_MARKER);
    if (fs_exists(marker)) {
        if (game->sandbox) {
            DeleteAppContainerProfile(wide_name);
            /* The sandbox's folder, if anything is left of it. It is the folder
             * above its AC, with the name of the sandbox. */
            if (container) {
                wchar_t *last = wcsrchr(container, L'\\');
                if (last) {
                    wchar_t *above;
                    *last = L'\0';
                    above = wcsrchr(container, L'\\');
                    if (above && _wcsicmp(above + 1, wide_name) == 0)
                        unpack_remove_tree(container);
                }
            }
        }
        if (rominabox_game_folder_to_forget(game, user_data, data, sizeof data) == 0) {
            wchar_t *before = wide(data);
            unpack_remove_tree(before);
            free(before);
        }
        if (unpacked_folder[0])
            unpack_forget_all(unpacked_folder);
    }
    if (container)
        CoTaskMemFree(container);
    free(container_utf8);
    free(wide_name);
    free(user_data);
}

/* The value passed in from outside, as UTF-8, which we remove from the
 * environment of the player. NULL when it was not set. */
static char *from_outside(const wchar_t *name) {
    static wchar_t value[WIDE_PATH_CAP];
    DWORD length = GetEnvironmentVariableW(name, value, sizeof value / sizeof value[0]);
    SetEnvironmentVariableW(name, NULL);
    if (length == 0 || length >= sizeof value / sizeof value[0])
        return NULL;
    return utf8(value);
}

/* The launch itself. `accounts_root` is the real per-user folder, with the
 * accounts for QUICK SIGN IN. Inside the sandbox the per-user folder is the
 * sandbox's own, and `previous_user_data` is the real one too. */
static int run(char *accounts_root, char *previous_user_data, int opened_by_person) {
    char folder[LAUNCH_PATH_CAP];
    char resources[LAUNCH_PATH_CAP];
    char player[LAUNCH_PATH_CAP];
    char *user_data = local_application_data();
    LaunchPlaces places = {0};
    Launch launch;
    SECURITY_ATTRIBUTES inherit = {sizeof inherit, NULL, TRUE};
    STARTUPINFOW startup = {0};
    PROCESS_INFORMATION process = {0};
    JOBOBJECT_EXTENDED_LIMIT_INFORMATION limits = {0};
    HANDLE log;
    HANDLE job;
    wchar_t *line = NULL;
    size_t length = 0;
    size_t capacity = 0;
    wchar_t *player_wide;
    wchar_t *data_wide;
    DWORD code = 1;
    size_t index;

    game_folder(folder, sizeof folder);
    rominabox_launch_join(resources, sizeof resources, folder, part_Resources);
    rominabox_launch_join(player, sizeof player, folder, part_Player);
    places.resources = resources;
    places.core = core_Windows;
    places.user_data = user_data;
    places.accounts_root = accounts_root;
    places.previous_user_data = previous_user_data;
    places.opened_by_person = opened_by_person;
    places.before_data_folder = places.opened_by_person ? one_game_per_data_folder : NULL;
    rominabox_prepare_launch(&places, &launch);

    for (index = 0; index < launch.variable_count; index++) {
        wchar_t *name = wide(launch.variables[index].name);
        wchar_t *value = launch.variables[index].value ? wide(launch.variables[index].value) : NULL;
        SetEnvironmentVariableW(name, value);
        free(name);
        free(value);
    }
    /* The game's window is in the player process, so a pin from it would
     * start the player alone. We give the window this program instead. */
    {
        static wchar_t path[WIDE_PATH_CAP];
        wchar_t *name = wide(RIB_ENV_RELAUNCH);
        DWORD length = GetEnvironmentVariableW(outside_program, path, sizeof path / sizeof path[0]);
        SetEnvironmentVariableW(outside_program, NULL);
        if (length == 0 || length >= sizeof path / sizeof path[0])
            own_path(path, sizeof path / sizeof path[0]);
        SetEnvironmentVariableW(name, path);
        free(name);
    }

    {
        wchar_t *log_wide = wide(launch.log_path);
        log = CreateFileW(log_wide, FILE_APPEND_DATA, FILE_SHARE_READ | FILE_SHARE_WRITE, &inherit,
                          OPEN_ALWAYS, FILE_ATTRIBUTE_NORMAL, NULL);
        free(log_wide);
    }
    if (log != INVALID_HANDLE_VALUE && launch.quiet) {
        static const char quiet[] = "[RIB] quiet: audio driver null, output disabled\n";
        DWORD wrote;
        WriteFile(log, quiet, (DWORD)(sizeof quiet - 1), &wrote, NULL);
    }

    player_wide = wide(player);
    append_argument(&line, &length, &capacity, player_wide);
    for (index = 0; index < (size_t)launch.argument_count; index++) {
        wchar_t *argument = wide(launch.arguments[index]);
        append_argument(&line, &length, &capacity, argument);
        free(argument);
    }
    data_wide = wide(launch.data_dir);

    startup.cb = sizeof startup;
    if (log != INVALID_HANDLE_VALUE) {
        startup.dwFlags = STARTF_USESTDHANDLES;
        startup.hStdInput = GetStdHandle(STD_INPUT_HANDLE);
        startup.hStdOutput = log;
        startup.hStdError = log;
    }
    /* The player and the launcher end together, whichever ends first. */
    job = CreateJobObjectW(NULL, NULL);
    limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
    if (job)
        SetInformationJobObject(job, JobObjectExtendedLimitInformation, &limits, sizeof limits);
    /* No console window. The player is a windowed program with a log, and a
     * console program in its place, such as the probe in the isolation
     * tests, would otherwise open a terminal on the screen. */
    if (!CreateProcessW(player_wide, line, NULL, NULL, TRUE,
                        CREATE_SUSPENDED | CREATE_UNICODE_ENVIRONMENT | CREATE_NO_WINDOW,
                        NULL, data_wide, &startup, &process))
        rominabox_launch_die("could not start the player");
    if (job)
        AssignProcessToJobObject(job, process.hProcess);
    if (running_player)
        *running_player = process.dwProcessId;
    ResumeThread(process.hThread);
    CloseHandle(process.hThread);
    if (log != INVALID_HANDLE_VALUE)
        CloseHandle(log);
    WaitForSingleObject(process.hProcess, INFINITE);
    GetExitCodeProcess(process.hProcess, &code);
    CloseHandle(process.hProcess);
    if (job)
        CloseHandle(job);
    return (int)code;
}

/* Whether a person opened this game, once we have found out. Until then we
 * do not show a failure. */
static int person_opened;

/* A Windows game has no console, so we show why it cannot start in a message
 * box to a person who opened it. We never show one in a quiet run or a dry
 * run. */
void rominabox_launch_tell(const char *message) {
    wchar_t text[1024];
    if (!rominabox_launch_tells_person(person_opened))
        return;
    if (MultiByteToWideChar(CP_UTF8, 0, message, -1, text, (int)(sizeof text / sizeof text[0])))
        MessageBoxW(NULL, text, L"" ROMINABOX_NAME, MB_OK | MB_ICONERROR);
}

int WINAPI wWinMain(HINSTANCE instance, HINSTANCE previous, PWSTR arguments, int show) {
    (void)instance;
    (void)previous;
    (void)arguments;
    (void)show;
    if (inside_sandbox()) {
        char *person = from_outside(outside_opened_by_person);
        char *outside = from_outside(outside_user_data);
        person_opened = person && strcmp(person, LAUNCH_SWITCH_ON) == 0;
        return run(outside, outside, person_opened);
    }
    person_opened = opened_by_explorer();
    /* We started this from outside and it is still not in a sandbox, so
     * going on would start it again and again. */
    if (GetEnvironmentVariableW(outside_user_data, NULL, 0))
        rominabox_launch_die("the game's sandbox did not take");
    {
        char folder[LAUNCH_PATH_CAP];
        char resources[LAUNCH_PATH_CAP];
        LaunchGame game;
        wchar_t self[WIDE_PATH_CAP];
        char *user_data = local_application_data();
        own_path(self, sizeof self / sizeof self[0]);
        /* Shown only when the game would play sound, so not in a quiet launch. */
        unpack_game(self, user_data,
                    !rominabox_launch_is_quiet(opened_by_explorer(), getenv(RIB_ENV_QUIET), getenv(ROMINABOX_SOUND_ENV)),
                    unpacked_folder, sizeof unpacked_folder, unpacked_program,
                    sizeof unpacked_program / sizeof unpacked_program[0]);
        free(user_data);
        game_folder(folder, sizeof folder);
        rominabox_launch_join(resources, sizeof resources, folder, part_Resources);
        rominabox_read_game(resources, &game);
        {
            int code = game.sandbox ? start_in_sandbox(folder, &game)
                                    : run(local_application_data(), NULL, opened_by_explorer());
            forget_if_asked(&game);
            return code;
        }
    }
}
