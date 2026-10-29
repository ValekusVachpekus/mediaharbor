!macro NSIS_HOOK_POSTINSTALL
  ExecWait '"$INSTDIR\media-core.exe" --register bkikhcebeennfdababfkhnmmojmafefp'
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  ExecWait '"$INSTDIR\media-core.exe" --unregister'
!macroend
