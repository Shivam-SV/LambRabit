; Hooks for Tauri's NSIS installer (bundle.windows.nsis.installerHooks in tauri.conf.json).

; Uninstalling removes lampRabit's runtimes, configs, logs, certificates, hosts entries,
; PATH entry and settings, and keeps the projects (www) and databases (data); the app does
; the work (src-tauri/src/uninstall.rs). Updates run this uninstaller with /UPDATE and
; must keep everything, so they skip it.
!macro NSIS_HOOK_PREUNINSTALL
  ${If} $UpdateMode <> 1
    ; The cleanup deletes runtimes in use while lampRabit runs; ask to close it first.
    !insertmacro CheckIfAppIsRunning "$INSTDIR\${MAINBINARYNAME}.exe" "${PRODUCTNAME}"
    ExecWait '"$INSTDIR\${MAINBINARYNAME}.exe" --uninstall-cleanup'
  ${EndIf}
!macroend
