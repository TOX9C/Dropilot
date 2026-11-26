!macro NSIS_HOOK_POSTINSTALL
  DetailPrint "Registering Dropilot Windows Package Identity (if supported)..."
  nsExec::ExecToLog `powershell.exe -NoProfile -NonInteractive -ExecutionPolicy Bypass -WindowStyle Hidden -Command "try { $$msix = Join-Path '$INSTDIR' 'packaging\DropilotIdentity.msix'; $$manifest = Join-Path '$INSTDIR' 'packaging\msix\AppxManifest.xml'; $$dev = (Get-ItemProperty -Path 'HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\AppModelUnlock' -ErrorAction SilentlyContinue).AllowDevelopmentWithoutDevLicense -eq 1; if (Test-Path $$msix) { try { Add-AppxPackage -Path $$msix -ExternalLocation '$INSTDIR' -ForceUpdateFromAnyVersion -ErrorAction Stop } catch { if ($$dev -and (Test-Path $$manifest)) { Add-AppxPackage -Register $$manifest -ExternalLocation '$INSTDIR' -ForceUpdateFromAnyVersion -ErrorAction SilentlyContinue } } } elseif ($$dev -and (Test-Path $$manifest)) { Add-AppxPackage -Register $$manifest -ExternalLocation '$INSTDIR' -ForceUpdateFromAnyVersion -ErrorAction SilentlyContinue } } catch { }"`
  Pop $0
!macroend

!macro NSIS_HOOK_POSTUNINSTALL
  DeleteRegKey HKCU "Software\Classes\AppUserModelId\io.dropilot.app"
  DetailPrint "Removing Dropilot Windows Package Identity..."
  nsExec::ExecToLog `powershell.exe -NoProfile -NonInteractive -ExecutionPolicy Bypass -WindowStyle Hidden -Command "try { Get-AppxPackage -Name 'io.dropilot.app' -ErrorAction SilentlyContinue | Remove-AppxPackage -ErrorAction SilentlyContinue } catch { }"`
  Pop $0
!macroend
