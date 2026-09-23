; The installed desktop payload is separate from the private supervised host.
; Keep its journal and compatibility listener. Session removal is a separate opt-in.
!include "FileFunc.nsh"
!include "${__FILEDIR__}\desktop-payload.nsh"
!macro NSIS_HOOK_POSTINSTALL
  nsExec::ExecToStack '"$INSTDIR\cxweb.exe" stage-runtime-update'
  Pop $0
  Pop $1
  ${If} $0 == 3010
    SetRebootFlag true
    DetailPrint "Runtime update installed. Restart Windows to use the new runtime; existing tasks continue until restart."
    SetErrorLevel 3010
  ${ElseIf} $0 != 0
    MessageBox MB_OK|MB_ICONEXCLAMATION "The desktop files were installed, but the private runtime update could not be completed. Existing connections were preserved. Run this installer again to retry; do not assume the running runtime has been updated." /SD IDOK
    SetErrorLevel 1
    Abort
  ${EndIf}
!macroend
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
    StrCpy $2 0
    ${GetParameters} $3
    ClearErrors
    ${GetOptions} $3 "/CLEARSESSION" $3
    ${IfNot} ${Errors}
      StrCpy $2 1
    ${Else}
      ${IfNot} ${Silent}
        MessageBox MB_YESNO|MB_DEFBUTTON2|MB_ICONQUESTION "Also delete the saved ChatGPT session in cxweb? Codex sign-in and your personal browser profiles are preserved. Remote ChatGPT data is not deleted." /SD IDNO IDNO keep_local_session
        StrCpy $2 1
        keep_local_session:
      ${EndIf}
    ${EndIf}
    ${If} $2 = 1
      nsExec::ExecToStack '"$INSTDIR\cxweb.exe" clear-local-session'
      Pop $0
      Pop $1
      ${If} $0 != 0
        MessageBox MB_OK|MB_ICONEXCLAMATION "The local ChatGPT session could not be fully cleared. Close cxweb sign-in windows and check the connection in cxweb before trying again. Application files were kept so you can resolve this." /SD IDOK
        SetErrorLevel 1
        Abort
      ${EndIf}
      DetailPrint "The dedicated local ChatGPT session was cleared."
    ${EndIf}
    DetailPrint "Codex connections removed. Compatibility runtimes are retained for already open clients."
  ${EndIf}
!macroend

!macro NSIS_HOOK_PREINSTALL
  ; No running host is stopped or disconnected by an update. POSTINSTALL
  ; validates journal/scheduler ownership and preserves the mapped old image,
  ; so an unavailable or busy old host must not block delivering its repair.
!macroend
