; Stage the complete desktop image before publishing it. Never continue with
; an old executable after a skipped or failed file write.
!macro CXWEB_INSTALL_DESKTOP
  SetOutPath "$INSTDIR"
  SetOverwrite on
  ClearErrors
  File "/oname=${MAINBINARYNAME}.pending.exe" "${MAINBINARYSRCPATH}"
  IfErrors desktop_payload_failed
  GetDLLVersion "$INSTDIR\${MAINBINARYNAME}.pending.exe" $R8 $R9
  IfErrors desktop_payload_failed
  IfFileExists "$INSTDIR\${MAINBINARYNAME}.exe" desktop_payload_replace desktop_payload_first
  desktop_payload_replace:
    GetTempFileName $R5 "$INSTDIR"
    IfErrors desktop_payload_failed
    Delete "$R5"
    System::Call 'kernel32::ReplaceFileW(w "$INSTDIR\${MAINBINARYNAME}.exe", w "$INSTDIR\${MAINBINARYNAME}.pending.exe", w "$R5", i 0, p 0, p 0) i.r0'
    StrCmp $0 0 desktop_payload_failed
    Delete /REBOOTOK "$R5"
    Goto desktop_payload_verify
  desktop_payload_first:
    Rename "$INSTDIR\${MAINBINARYNAME}.pending.exe" "$INSTDIR\${MAINBINARYNAME}.exe"
    IfErrors desktop_payload_failed
  desktop_payload_verify:
    ClearErrors
    GetDLLVersion "$INSTDIR\${MAINBINARYNAME}.exe" $R6 $R7
    IfErrors desktop_payload_failed
    StrCmp $R6 $R8 0 desktop_payload_failed
    StrCmp $R7 $R9 0 desktop_payload_failed
    Goto desktop_payload_done
  desktop_payload_failed:
    SetErrorLevel 1
    Abort "The desktop update could not be verified. Close cxweb and run this installer again. Installation has not completed."
  desktop_payload_done:
!macroend
