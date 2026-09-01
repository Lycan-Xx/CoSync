; CoSync needs fixed QUIC ports for pairing and pinned trusted sessions. The per-machine NSIS installer
; runs elevated, so it can add a rule restricted to the installed executable,
; UDP 48215-48216, private network profiles, and local-subnet peers only.

!include "LogicLib.nsh"

!macro NSIS_HOOK_PREINSTALL
  DetailPrint "Allowing CoSync connections on private local networks..."
  ; Tauri keeps the Cargo binary name (`desktop.exe`) even though the
  ; bundle product name is CoSync. Keep this path aligned with Cargo.toml.
  ; Update in place first so a failed upgrade never deletes a working rule.
  StrCpy $0 1
  ; Keep the original rule name so upgrades update existing installations.
  ExecWait '"$SYSDIR\netsh.exe" advfirewall firewall set rule name="CoSync Pairing (UDP 48215)" new dir=in action=allow program="$INSTDIR\desktop.exe" protocol=UDP localport=48215-48216 profile=private remoteip=localsubnet enable=yes' $0
  ${If} $0 != 0
    ; A fresh install has no rule to update, so create it instead.
    StrCpy $0 1
    ExecWait '"$SYSDIR\netsh.exe" advfirewall firewall add rule name="CoSync Pairing (UDP 48215)" dir=in action=allow program="$INSTDIR\desktop.exe" protocol=UDP localport=48215-48216 profile=private remoteip=localsubnet enable=yes' $0
  ${EndIf}
  ${If} $0 != 0
    DetailPrint "CoSync firewall rule creation failed with exit code $0."
    MessageBox MB_ICONSTOP|MB_OK "CoSync could not configure Windows Firewall for local connections.$\r$\n$\r$\nSetup cannot continue because pairing or reconnection would be blocked. Please run the installer as an administrator." /SD IDOK
    Abort
  ${EndIf}
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  DetailPrint "Removing the CoSync pairing firewall rule..."
  ExecWait '"$SYSDIR\netsh.exe" advfirewall firewall delete rule name="CoSync Pairing (UDP 48215)"' $0
!macroend
