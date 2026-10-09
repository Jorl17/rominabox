; ROM-in-a-Box's additions to the builder's Windows installer
; (tauri.conf.json, bundle.windows.nsis.installerHooks).
;
; Installing puts rominabox-cli on this person's PATH, asking nothing.
; Uninstalling removes everything the builder and the games made with it
; keep on this computer, saves included, only when the person ticks "Delete
; the application data" in the uninstaller. The box starts unticked, and it
; is not shown in a passive uninstall. Our installer template (installer.nsi)
; installs a new version over the same or an older one without uninstalling
; it. The builder program does each step, before it opens any window
; (src/installation.rs): it edits the PATH whatever its length, and names
; what it removes as the builder and the games declare it.

; The command line's own folder: rominabox-cli and nothing else, so no other
; program of the builder answers by name in a terminal. The command line
; finds the builder's runtime kit from here, one folder up.
!define RIB_CLI_FOLDER "$INSTDIR\bin"

!macro NSIS_HOOK_POSTINSTALL
  ; Tauri installs every program the builder's package makes beside the
  ; builder; the command line goes into its folder.
  CreateDirectory "${RIB_CLI_FOLDER}"
  Delete "${RIB_CLI_FOLDER}\rominabox-cli.exe"
  Rename "$INSTDIR\rominabox-cli.exe" "${RIB_CLI_FOLDER}\rominabox-cli.exe"
  ExecWait '"$INSTDIR\${MAINBINARYNAME}.exe" --add-to-path "${RIB_CLI_FOLDER}"'
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  ; While the builder program is still there to ask.
  ${If} $UpdateMode <> 1
    ExecWait '"$INSTDIR\${MAINBINARYNAME}.exe" --remove-from-path "${RIB_CLI_FOLDER}"'
    ${If} $DeleteAppDataCheckboxState = 1
      ExecWait '"$INSTDIR\${MAINBINARYNAME}.exe" --remove-data "$LOCALAPPDATA" "$APPDATA"'
    ${EndIf}
  ${EndIf}
!macroend

!macro NSIS_HOOK_POSTUNINSTALL
  Delete "${RIB_CLI_FOLDER}\rominabox-cli.exe"
  RMDir "${RIB_CLI_FOLDER}"
  ${If} $UpdateMode <> 1
    ; The builder's folder, once nothing else is in it: the games keep
    ; their data in it unless the person chose another, and that went
    ; before the builder's files did.
    RMDir "$INSTDIR"
  ${EndIf}
!macroend
