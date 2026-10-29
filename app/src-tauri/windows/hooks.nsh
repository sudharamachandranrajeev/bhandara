; The health helper runs as SYSTEM from a scheduled task. It lives in the
; per-machine install folder, which only admins can write.

!macro NSIS_HOOK_POSTINSTALL
  nsExec::Exec '"$INSTDIR\bhandara-health.exe" --install'
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  nsExec::Exec '"$INSTDIR\bhandara-health.exe" --uninstall'
  Delete "$INSTDIR\health.json"
  Delete "$INSTDIR\health.json.tmp"
!macroend
