; CoSync needs one fixed QUIC pairing port. The per-machine NSIS installer
; runs elevated, so it can add a rule restricted to the installed executable,
; UDP 48215, private network profiles, and local-subnet peers only.

!macro NSIS_HOOK_POSTINSTALL
  DetailPrint "Allowing CoSync pairing on private local networks..."
  ExecWait '"$SYSDIR\netsh.exe" advfirewall firewall delete rule name="CoSync Pairing (UDP 48215)"' $0
  ExecWait '"$SYSDIR\netsh.exe" advfirewall firewall add rule name="CoSync Pairing (UDP 48215)" dir=in action=allow program="$INSTDIR\CoSync.exe" protocol=UDP localport=48215 profile=private remoteip=localsubnet enable=yes' $0
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  DetailPrint "Removing the CoSync pairing firewall rule..."
  ExecWait '"$SYSDIR\netsh.exe" advfirewall firewall delete rule name="CoSync Pairing (UDP 48215)"' $0
!macroend
