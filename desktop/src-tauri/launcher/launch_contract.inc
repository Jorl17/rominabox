/* What the exporter writes into a game's app for its launcher: the launch
 * plan's fields, the files beside it, and what the launcher does with them.
 * Declared once, here. The launcher includes it (main.c, player_settings.c);
 * the exporter reads it as the launcher's compiler does
 * (desktop/crates/rominabox-engine/src/launch_contract.rs), so a name either side spells
 * differently stops its build instead of exporting a game that cannot start.
 *
 * RIB_APP_FILE(name, "path")                in the app's own files (Resources)
 * RIB_CORE_FILE(platform, "file")           the core, named as its platform
 *                                           names a library
 * RIB_WINDOWS_PART(name, "path")            where a Windows game's parts sit
 *                                           beside its .exe
 * RIB_PLAN_FIELD(name, "field")             a plan line: field, tab, value
 * RIB_PLAN_MARK(name, "line")               a line of its own in the plan
 * RIB_TOKEN(name, "$token")                 a place, in a config line or the
 *                                           data_dir; the launcher writes the
 *                                           path
 * RIB_USER_FOLDER(name, "path")             a folder below the per-user data
 *                                           folder ($user_data)
 * RIB_GAME_DATA(name, "path")               a folder the launcher keeps in a
 *                                           game's data for itself
 * RIB_GAME_FILE(name, "path")               a file the launcher keeps in a
 *                                           game's data for itself
 * RIB_PLAYER_FOLDER(name, "path")           a folder of the player's own in
 *                                           a game's data, which a backup of
 *                                           the data contains
 * RIB_SHIPPED_SETTINGS(name, "app", "data") a folder the app ships, which the
 *                                           launcher applies to that folder in
 *                                           the game's data on every launch
 * RIB_SHIPPED_FILES(name, "app", "data")    likewise, replacing what an
 *                                           earlier export shipped there */
/* A reader defines the declarations it reads; the others read as nothing. */
#ifndef RIB_APP_FILE
#define RIB_APP_FILE(name, path)
#endif
#ifndef RIB_CORE_FILE
#define RIB_CORE_FILE(platform, file)
#endif
#ifndef RIB_WINDOWS_PART
#define RIB_WINDOWS_PART(name, path)
#endif
#ifndef RIB_PLAN_FIELD
#define RIB_PLAN_FIELD(name, field)
#endif
#ifndef RIB_PLAN_MARK
#define RIB_PLAN_MARK(name, line)
#endif
#ifndef RIB_TOKEN
#define RIB_TOKEN(name, token)
#endif
#ifndef RIB_USER_FOLDER
#define RIB_USER_FOLDER(name, path)
#endif
#ifndef RIB_GAME_DATA
#define RIB_GAME_DATA(name, path)
#endif
#ifndef RIB_GAME_FILE
#define RIB_GAME_FILE(name, path)
#endif
#ifndef RIB_PLAYER_FOLDER
#define RIB_PLAYER_FOLDER(name, path)
#endif
#ifndef RIB_SHIPPED_SETTINGS
#define RIB_SHIPPED_SETTINGS(name, app, data)
#endif
#ifndef RIB_SHIPPED_FILES
#define RIB_SHIPPED_FILES(name, app, data)
#endif

RIB_APP_FILE(Plan, "launch.plan")
/* The game's icon as a picture, which the launcher copies beside the game's
 * manifest for the builder's list of games. */
RIB_APP_FILE(Icon, "game-icon.png")
RIB_CORE_FILE(Macos, "game-core.dylib")
RIB_CORE_FILE(Windows, "game-core.dll")
/* The composed menu: menu.rml and the files declarations.inc names. */
RIB_APP_FILE(MenuAssets, "menu-assets")

/* A Windows game is a folder with one program at its top, the game's .exe,
 * which a person double-clicks: its own files and the player sit in
 * folders beside it. */
RIB_WINDOWS_PART(Resources, "Resources")
RIB_WINDOWS_PART(Player, "Runtime/retroarch.exe")

RIB_PLAN_FIELD(Identity, "identity")
RIB_PLAN_FIELD(Content, "content")
RIB_PLAN_FIELD(Title, "title")
/* The console's id, as in desktop/systems.json, and its name, for the
 * game's manifest. */
RIB_PLAN_FIELD(System, "system")
RIB_PLAN_FIELD(Console, "console")
/* The version of ROM-in-a-Box that made the game. */
RIB_PLAN_FIELD(MadeWith, "made_with")
RIB_PLAN_FIELD(StartAtMenu, "start_at_menu")
RIB_PLAN_FIELD(Advanced, "advanced")
RIB_PLAN_FIELD(Achievements, "achievements")
/* "1": the game runs in its own sandbox. Every export says so. macOS's
 * sandbox comes from the app's signature; the Windows launcher sets one up
 * when its plan says this, so a launcher built without an export's plan, as
 * the tests build it, runs as a Mac launcher that was never signed does. */
RIB_PLAN_FIELD(Sandbox, "sandbox")
/* One per player setting: file, key and default, tab-separated. */
RIB_PLAN_FIELD(PlayerSetting, "player_setting")
/* The filter a game starts with, in the menu's folder. */
RIB_PLAN_FIELD(ShaderInitial, "shader_initial")
RIB_PLAN_FIELD(DataDir, "data_dir")
RIB_PLAN_FIELD(AccountsDir, "accounts_dir")
/* One per folder the launcher makes in the game's data. */
RIB_PLAN_FIELD(Managed, "managed")
/* After the fields: the rest of the plan is RetroArch's config. */
RIB_PLAN_MARK(Config, "---config---")

RIB_TOKEN(DataDir, "$data_dir")
RIB_TOKEN(ResourcesDir, "$resources_dir")
/* The player's per-user application data folder, at the start of the
 * plan's data_dir. Each platform's launcher says where it is. */
RIB_TOKEN(UserData, "$user_data")

/* Each game's data folder is one of Games' own folders, named for the game:
 * the plan's data_dir names it below $user_data. It is the only folder the
 * launcher removes when the player asks to forget the game. */
RIB_USER_FOLDER(Games, "ROM-in-a-Box/Games")
/* A Windows game made into one program unpacks into one of Runtimes' own
 * folders. */
RIB_USER_FOLDER(Runtimes, "ROM-in-a-Box/Runtimes")

/* What each launch applied from the export, at the path it applied it to:
 * a later launch tells the player's changes from the export's by it. */
RIB_GAME_DATA(Applied, "applied")
/* What the game is, which we write on every launch for a backup of its data
 * and for the builder's list of games (desktop/src-tauri/gamedata), and the
 * game's icon beside it. */
RIB_GAME_FILE(Manifest, "game.manifest")
RIB_GAME_FILE(Icon, "game-icon.png")
/* A backup the player chose to import in the game, which we import before
 * the next launch. */
RIB_GAME_FILE(PendingImport, "import.zip")

/* The player's saves (cartridge saves and memory cards), states and
 * screenshots. */
RIB_PLAYER_FOLDER(Saves, "saves")
RIB_PLAYER_FOLDER(States, "states")
RIB_PLAYER_FOLDER(Screenshots, "screenshots")

/* The core's options, where RetroArch looks for them per game. */
RIB_SHIPPED_SETTINGS(CoreOptions, "core-options", "config")
RIB_SHIPPED_SETTINGS(Remaps, "remaps", "remaps")
RIB_SHIPPED_SETTINGS(Autoconfig, "autoconfig", "autoconfig")
RIB_SHIPPED_FILES(Firmware, "firmware", "system")
/* A PlayStation disc's PPF patch: PCSX ReARMed reads it, named after the
 * disc's serial, from the folder the player starts in, the game's data. */
RIB_SHIPPED_FILES(Patches, "patches", ".")

#undef RIB_APP_FILE
#undef RIB_CORE_FILE
#undef RIB_WINDOWS_PART
#undef RIB_PLAN_FIELD
#undef RIB_PLAN_MARK
#undef RIB_TOKEN
#undef RIB_USER_FOLDER
#undef RIB_GAME_DATA
#undef RIB_GAME_FILE
#undef RIB_PLAYER_FOLDER
#undef RIB_SHIPPED_SETTINGS
#undef RIB_SHIPPED_FILES
