Unicode true
!include "LogicLib.nsh"
!include "x64.nsh"
!include "WinVer.nsh"
!include "nsDialogs.nsh"
!include "FileFunc.nsh"
!ifndef APP_VERSION
  !error "Build the installer with scripts/build.ps1."
!endif
Name "RAM Cleanup"
VIProductVersion "${APP_VERSION}.0"
VIAddVersionKey "ProductName" "RAM Cleanup"
VIAddVersionKey "ProductVersion" "${APP_VERSION}"
VIAddVersionKey "FileDescription" "RAM Cleanup installer"
VIAddVersionKey "FileVersion" "${APP_VERSION}"
VIAddVersionKey "LegalCopyright" "Unlicense"
OutFile "..\dist\RAMCleanup-Setup.exe"
InstallDir "$PROGRAMFILES64\RAM Cleanup"
RequestExecutionLevel admin
SetCompressor /SOLID lzma
SetCompressorDictSize 1
Icon "..\assets\icon.ico"
UninstallIcon "..\assets\icon.ico"
ManifestDPIAware true
BrandingText "RAM Cleanup"
Var Autostart
Var AutostartCheckbox
Page custom OptionsPage OptionsLeave
Page instfiles
UninstPage instfiles
LoadLanguageFile "${NSISDIR}\Contrib\Language files\English.nlf"

Function .onInit
  ${IfNot} ${RunningX64}
    MessageBox MB_OK|MB_ICONSTOP "RAM Cleanup requires x64 Windows."
    Abort
  ${EndIf}
  ${IfNot} ${AtLeastWin10}
    MessageBox MB_OK|MB_ICONSTOP "RAM Cleanup requires Windows 10 or newer."
    Abort
  ${EndIf}
  SetRegView 64
  SetShellVarContext all
  ${DisableX64FSRedirection}
  StrCpy $Autostart ${BST_CHECKED}
  ReadRegStr $0 HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\RAMCleanup" "DisplayName"
  ${If} $0 != ""
    ReadRegStr $0 HKLM "Software\Microsoft\Windows\CurrentVersion\Run" "RAM Cleanup"
    ${If} $0 == ""
      StrCpy $Autostart ${BST_UNCHECKED}
    ${EndIf}
  ${EndIf}
  ${GetParameters} $0
  ClearErrors
  ${GetOptions} $0 "/AUTOSTART=" $1
  ${IfNot} ${Errors}
    ${If} $1 == "0"
      StrCpy $Autostart ${BST_UNCHECKED}
    ${ElseIf} $1 == "1"
      StrCpy $Autostart ${BST_CHECKED}
    ${Else}
      SetErrorLevel 1
      Abort "Use /AUTOSTART=0 or /AUTOSTART=1."
    ${EndIf}
  ${EndIf}
FunctionEnd

Function OptionsPage
  nsDialogs::Create 1018
  Pop $0
  ${If} $0 == error
    Abort
  ${EndIf}
  ${NSD_CreateCheckbox} 12u 20u 95% 12u "Start RAM Cleanup with Windows"
  Pop $AutostartCheckbox
  ${NSD_SetState} $AutostartCheckbox $Autostart
  nsDialogs::Show
FunctionEnd

Function OptionsLeave
  ${NSD_GetState} $AutostartCheckbox $Autostart
FunctionEnd

Function .onInstSuccess
  # Always launch now; the checkbox controls only subsequent Windows logons.
  # Explorer supplies the normal user token, not the elevated setup token.
  SetOutPath "$PLUGINSDIR"
  File "..\target\installer-launch.exe"
  ExecWait '"$PLUGINSDIR\installer-launch.exe" "$INSTDIR\ramcleanup.exe"' $0
  ${If} $0 != 0
    MessageBox MB_OK|MB_ICONEXCLAMATION "Installation completed, but RAM Cleanup could not start. Open RAM Cleanup from the Start menu."
  ${EndIf}
FunctionEnd

Section
  StrCpy $INSTDIR "$PROGRAMFILES64\RAM Cleanup"
  InitPluginsDir
  SetOutPath "$PLUGINSDIR"
  File "..\dist\ramcleanup.exe"
  File "..\dist\ramcleanup-helper.exe"
  File "provision.ps1"
  ExecWait '"$PLUGINSDIR\ramcleanup-helper.exe" --stop'
  Sleep 500
  nsExec::ExecToLog '"$SYSDIR\WindowsPowerShell\v1.0\powershell.exe" -NoLogo -NoProfile -NonInteractive -ExecutionPolicy Bypass -File "$PLUGINSDIR\provision.ps1" -Payload "$PLUGINSDIR\ramcleanup.exe"'
  Pop $0
  ${If} $0 != 0
    SetErrorLevel 1
    Abort "Could not install the protected executable or cleanup task."
  ${EndIf}
  SetOutPath "$INSTDIR"
  File "..\THIRD-PARTY-NOTICES.txt"
  File "..\LICENSE"
  WriteUninstaller "$INSTDIR\Uninstall.exe"
  ${If} $Autostart == ${BST_CHECKED}
    WriteRegStr HKLM "Software\Microsoft\Windows\CurrentVersion\Run" "RAM Cleanup" '$\"$INSTDIR\ramcleanup.exe$\"'
  ${Else}
    DeleteRegValue HKLM "Software\Microsoft\Windows\CurrentVersion\Run" "RAM Cleanup"
  ${EndIf}
  WriteRegStr HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\RAMCleanup" "DisplayName" "RAM Cleanup"
  WriteRegStr HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\RAMCleanup" "DisplayVersion" "${APP_VERSION}"
  WriteRegStr HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\RAMCleanup" "UninstallString" '$\"$INSTDIR\Uninstall.exe$\"'
  WriteRegStr HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\RAMCleanup" "DisplayIcon" "$INSTDIR\ramcleanup.exe"
  WriteRegDWORD HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\RAMCleanup" "NoModify" 1
  WriteRegDWORD HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\RAMCleanup" "NoRepair" 1
  CreateShortcut "$SMPROGRAMS\RAM Cleanup.lnk" "$INSTDIR\ramcleanup.exe"
SectionEnd

Section "Uninstall"
  SetRegView 64
  SetShellVarContext all
  ${DisableX64FSRedirection}
  StrCpy $INSTDIR "$PROGRAMFILES64\RAM Cleanup"
  InitPluginsDir
  SetOutPath "$PLUGINSDIR"
  File "provision.ps1"
  ExecWait '"$INSTDIR\ramcleanup-helper.exe" --stop'
  Sleep 500
  nsExec::ExecToLog '"$SYSDIR\WindowsPowerShell\v1.0\powershell.exe" -NoLogo -NoProfile -NonInteractive -ExecutionPolicy Bypass -File "$PLUGINSDIR\provision.ps1" -Remove'
  Pop $0
  ${If} $0 != 0
    SetErrorLevel 1
    Abort "Could not remove the cleanup task."
  ${EndIf}
  DeleteRegValue HKLM "Software\Microsoft\Windows\CurrentVersion\Run" "RAM Cleanup"
  DeleteRegKey HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\RAMCleanup"
  Delete "$SMPROGRAMS\RAM Cleanup.lnk"
  Delete "$INSTDIR\ramcleanup.exe"
  Delete "$INSTDIR\ramcleanup-helper.exe"
  Delete "$INSTDIR\Uninstall.exe"
  Delete "$INSTDIR\THIRD-PARTY-NOTICES.txt"
  Delete "$INSTDIR\LICENSE"
  Delete "$INSTDIR\cleanup-last.txt"
  RMDir "$INSTDIR"
SectionEnd
