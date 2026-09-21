; The installed desktop payload is separate from the private supervised host.
; Never kill that host or delete its profile, journal, or compatibility listener.
!macro NSIS_HOOK_PREUNINSTALL
  ${If} $UpdateMode <> 1
    nsExec::ExecToStack '"$INSTDIR\cxweb.exe" prepare-uninstall'
    Pop $0
    Pop $1
    ${If} $0 != 0
      MessageBox MB_OK|MB_ICONEXCLAMATION "cxweb could not safely remove its Codex connections. Finish active tasks and check the connection in cxweb, then try again. No application files were removed." /SD IDOK
      SetErrorLevel 1
      Abort
    ${EndIf}
    DetailPrint "Codex connections removed. Private session data and compatibility runtimes are retained for already open clients."
  ${EndIf}
!macroend

!macro NSIS_HOOK_PREINSTALL
  ${If} ${FileExists} "$INSTDIR\cxweb.exe"
    nsExec::ExecToStack '"$INSTDIR\cxweb.exe" prepare-uninstall --check'
    Pop $0
    Pop $1
    ${If} $0 != 0
      MessageBox MB_OK|MB_ICONEXCLAMATION "cxweb is busy or a connection needs attention. Finish active tasks and check cxweb before installing this version." /SD IDOK
      SetErrorLevel 1
      Abort
    ${EndIf}
  ${EndIf}
!macroend
