/* The Windows entry of a game's launcher, in the game's own .exe, the one a
 * person double-clicks. We prepare the launch (launch.c), then start the
 * player beside it with RetroArch's arguments, the game's variables, the
 * launch log as its output and the data folder as its working directory, and
 * wait for it, so the game opens and closes as one program. */
#define WIN32_LEAN_AND_MEAN
#include <windows.h>
#include <knownfolders.h>
#include <shlobj.h>
#include <tlhelp32.h>

#include <ctype.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <wchar.h>

#include "../launch.h"
#include "../../../../vendor/retroarch/rominabox_launch.h"

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
    wchar_t path[32768];
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

/* The per-user application data folder, %LOCALAPPDATA%. */
static char *local_application_data(void) {
    PWSTR found = NULL;
    char *path;
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
    unsigned long long hash = 1469598103934665603ULL;
    const unsigned char *cursor;
    wchar_t name[64];
    HANDLE mapping;
    DWORD player;
    /* A folder's name, however it is spelled: case and separators aside. */
    for (cursor = (const unsigned char *)data_dir; *cursor; cursor++)
        hash = (hash ^ (unsigned char)(*cursor == '\\' ? '/' : tolower(*cursor))) * 1099511628211ULL;
    swprintf(name, sizeof name / sizeof name[0], L"Local\\ROM-in-a-Box game %016llx", hash);
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

static int run(void) {
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

    own_folder(folder, sizeof folder);
    rominabox_launch_join(resources, sizeof resources, folder, part_Resources);
    rominabox_launch_join(player, sizeof player, folder, part_Player);
    places.resources = resources;
    places.user_data = user_data;
    places.accounts_root = user_data;
    places.opened_by_person = opened_by_explorer();
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
        static wchar_t path[32768];
        wchar_t *name = wide(RIB_ENV_RELAUNCH);
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
    if (!CreateProcessW(player_wide, line, NULL, NULL, TRUE, CREATE_SUSPENDED | CREATE_UNICODE_ENVIRONMENT,
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

int WINAPI wWinMain(HINSTANCE instance, HINSTANCE previous, PWSTR arguments, int show) {
    (void)instance;
    (void)previous;
    (void)arguments;
    (void)show;
    return run();
}
