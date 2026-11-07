; Hooks into the installer that Tauri's bundler builds from
; tauri.installer.conf.json. The runtime, appytappy-runtime.exe, runs from
; sign-in, so these hooks stop it before its file is replaced or removed, and
; turn its autostart on and off. docs/installer.md describes the behavior.

; Asks the runtime to quit the way Windows does when the session ends: its
; tray window gets WM_CLOSE, so the runtime undoes what its automations
; changed and exits. It gets 10 seconds, and is then killed, because a
; running runtime keeps its file locked.
!macro APPYTAPPY_STOP_RUNTIME
  FindWindow $0 "AppyTappy.Tray" "AppyTappy"
  ${If} $0 <> 0
    SendMessage $0 ${WM_CLOSE} 0 0 /TIMEOUT=5000
  ${EndIf}
  StrCpy $1 0
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
!macroend

; An update installs again straight after, so it keeps the autostart entry.
!macro NSIS_HOOK_PREUNINSTALL
  !insertmacro APPYTAPPY_STOP_RUNTIME
  ${If} $UpdateMode <> 1
    ExecWait '"$INSTDIR\appytappy-runtime.exe" --autostart off'
  ${EndIf}
!macroend

; Tauri's "Delete the application data" box removes only the setup app's web
; view data, so it also removes AppyTappy's saved values, automations, and
; logs. The install folder is the logs' parent, so it goes once it is empty.
!macro NSIS_HOOK_POSTUNINSTALL
  ${If} $DeleteAppDataCheckboxState = 1
  ${AndIf} $UpdateMode <> 1
    SetShellVarContext current
    RMDir /r "$APPDATA\AppyTappy"
    RMDir /r "$LOCALAPPDATA\AppyTappy\logs"
    RMDir "$LOCALAPPDATA\AppyTappy"
  ${EndIf}
!macroend
