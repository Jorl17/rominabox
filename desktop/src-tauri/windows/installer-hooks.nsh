; ROM-in-a-Box's additions to the builder's Windows installer
; (tauri.conf.json, bundle.windows.nsis.installerHooks).
;
; Installing puts rominabox-cli on this person's PATH, asking nothing.
; Uninstalling removes everything the builder and the games made with it
; keep on this computer. An update, which uninstalls the old version with
; /UPDATE before installing the new one, removes nothing. The builder
; program does both, before it opens any window (src/installation.rs): it
; edits the PATH whatever its length, and names what it removes as the
; builder and the games declare it.

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
    ExecWait '"$INSTDIR\${MAINBINARYNAME}.exe" --uninstall-cleanup "$LOCALAPPDATA" "$APPDATA" "${RIB_CLI_FOLDER}"'
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
