; Hooks into the installer that Tauri's bundler builds from
; tauri.installer.conf.json. The runtime, appytappy-runtime.exe, runs from
; sign-in, so these hooks stop it before its file is replaced or removed, and
; turn its autostart on. wiki/developer/Installer.md describes the behavior.

; Asks the runtime to close: its tray window gets WM_CLOSE, which the runtime
; treats like the end of the session, so it undoes what its automations
; changed and exits. The class and title are TRAY_CLASS and the window title in
; crates/platform/src/win/tray.rs. The request goes again each second, since one
; sent while the tray menu is open is lost. A runtime that is still running
; after about 10 seconds, with or without its tray window, is killed, because a
; running runtime keeps its file locked.
!macro APPYTAPPY_STOP_RUNTIME
  StrCpy $1 0
  ${Do}
    FindWindow $0 "AppyTappy.Tray" "AppyTappy"
    ${If} $0 = 0
      ${ExitDo}
    ${EndIf}
    IntOp $2 $1 % 10
    ${If} $2 = 0
      SendMessage $0 ${WM_CLOSE} 0 0 /TIMEOUT=100
    ${EndIf}
    ${If} $1 >= 100
      ${ExitDo}
    ${EndIf}
    Sleep 100
    IntOp $1 $1 + 1
  ${Loop}
  ; The tray window goes before the program exits, so wait for the program.
  ${Do}
    nsis_tauri_utils::FindProcessCurrentUser "appytappy-runtime.exe"
    Pop $0
    ${If} $0 <> 0
    ${OrIf} $1 >= 100
      ${ExitDo}
    ${EndIf}
    Sleep 100
    IntOp $1 $1 + 1
  ${Loop}
  ${If} $0 = 0
    nsis_tauri_utils::KillProcessCurrentUser "appytappy-runtime.exe"
    Pop $0
    ${If} $0 <> 0
      DetailPrint "AppyTappy could not stop its runtime; close it from the tray and retry."
    ${EndIf}
    Sleep 500
  ${EndIf}
!macroend

!macro NSIS_HOOK_PREINSTALL
  !insertmacro APPYTAPPY_STOP_RUNTIME
!macroend

; The runtime writes its own autostart entry, then starts as the signed-in
; user, not as an administrator who ran the installer.
!macro NSIS_HOOK_POSTINSTALL
  ExecWait '"$INSTDIR\appytappy-runtime.exe" --autostart on' $0
  ${If} $0 <> 0
    DetailPrint "AppyTappy could not turn on starting at sign-in."
  ${EndIf}
  nsis_tauri_utils::RunAsUser "$INSTDIR\appytappy-runtime.exe" ""
  Pop $0
  ${If} $0 <> 0
    DetailPrint "AppyTappy could not start its runtime; it starts at the next sign-in."
  ${EndIf}
!macroend

; Tauri's uninstaller removes the Run value named for the product after this,
; once it knows the uninstall goes ahead, so the product name stays
; AUTOSTART_VALUE from crates/platform/src/autostart.rs.
!macro NSIS_HOOK_PREUNINSTALL
  !insertmacro APPYTAPPY_STOP_RUNTIME
!macroend

; Tauri's "Delete the application data" box removes only the setup app's web
; view data, so it also removes AppyTappy's saved values, automations, and
; logs. The install folder is the logs' parent, so it goes once it is empty.
!macro NSIS_HOOK_POSTUNINSTALL
  ${If} $DeleteAppDataCheckboxState = 1
  ${AndIf} $UpdateMode <> 1
    RMDir /r "$APPDATA\AppyTappy"
    RMDir /r "$LOCALAPPDATA\AppyTappy\logs"
    RMDir "$LOCALAPPDATA\AppyTappy"
  ${EndIf}
!macroend
