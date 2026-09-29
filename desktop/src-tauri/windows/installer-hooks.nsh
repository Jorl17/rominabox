; ROM-in-a-Box's additions to the builder's Windows installer
; (tauri.conf.json, bundle.windows.nsis.installerHooks).
;
; Installing puts rominabox-cli on this person's PATH, asking nothing.
; Uninstalling removes everything the builder and the games made with it
; keep on this computer. An update, which uninstalls the old version with
; /UPDATE before installing the new one, removes nothing.

; The command line's own folder: rominabox-cli and nothing else, so no other
; program of the builder answers by name in a terminal. The command line
; finds the builder's runtime kit from here, one folder up.
!define RIB_CLI_FOLDER "$INSTDIR\bin"

${UnStrRep}

!macro NSIS_HOOK_POSTINSTALL
  ; Tauri installs every program the builder's package makes beside the
  ; builder; the command line goes into its folder.
  CreateDirectory "${RIB_CLI_FOLDER}"
  Delete "${RIB_CLI_FOLDER}\rominabox-cli.exe"
  Rename "$INSTDIR\rominabox-cli.exe" "${RIB_CLI_FOLDER}\rominabox-cli.exe"
  ReadRegStr $0 HKCU "Environment" "Path"
  ${StrLoc} $1 ";$0;" ";${RIB_CLI_FOLDER};" ">"
  ${If} $1 == ""
    ${If} $0 == ""
      WriteRegExpandStr HKCU "Environment" "Path" "${RIB_CLI_FOLDER}"
    ${Else}
      WriteRegExpandStr HKCU "Environment" "Path" "$0;${RIB_CLI_FOLDER}"
    ${EndIf}
    SendMessage ${HWND_BROADCAST} ${WM_SETTINGCHANGE} 0 "STR:Environment" /TIMEOUT=5000
  ${EndIf}
!macroend

!macro NSIS_HOOK_POSTUNINSTALL
  Delete "${RIB_CLI_FOLDER}\rominabox-cli.exe"
  RMDir "${RIB_CLI_FOLDER}"
  ${If} $UpdateMode <> 1
    ReadRegStr $0 HKCU "Environment" "Path"
    ${UnStrRep} $1 ";$0;" ";${RIB_CLI_FOLDER};" ";"
    ; Without the semicolons that the line above adds around it.
    StrCpy $1 $1 "" 1
    StrCpy $1 $1 -1
    ${If} $1 != $0
      WriteRegExpandStr HKCU "Environment" "Path" "$1"
      SendMessage ${HWND_BROADCAST} ${WM_SETTINGCHANGE} 0 "STR:Environment" /TIMEOUT=5000
    ${EndIf}

    ; Every game's sandbox, as its launcher registered it
    ; (RIB_GAME_APP_ID_PREFIX in vendor/retroarch/rominabox_launch.h), with
    ; the saves and settings in the sandbox's folder, which Windows names
    ; for it in lower case.
    FindFirst $2 $3 "$LOCALAPPDATA\Packages\rominabox.game.*"
    ${DoWhile} $3 != ""
      StrCpy $4 $3 "" 15
      System::Call 'userenv::DeleteAppContainerProfile(w "ROMinaBox.Game.$4") i .r5'
      RMDir /r "$LOCALAPPDATA\Packages\$3"
      FindNext $2 $3
    ${Loop}
    FindClose $2

    ; Data that games stored outside a sandbox, and their unpacked
    ; copies, then the folder, if nothing else is in it.
    RMDir /r "$LOCALAPPDATA\ROM-in-a-Box\Games"
    RMDir /r "$LOCALAPPDATA\ROM-in-a-Box\Runtimes"
    RMDir "$LOCALAPPDATA\ROM-in-a-Box"
    ; QUICK SIGN IN's saved accounts, which the games share.
    RMDir /r "$LOCALAPPDATA\ROM-in-a-Box Accounts"
    ; The builder's own data: its core and lookup caches and kits.
    RMDir /r "$LOCALAPPDATA\${BUNDLEID}"
    RMDir /r "$APPDATA\${BUNDLEID}"
  ${EndIf}
!macroend
