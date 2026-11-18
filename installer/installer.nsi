; PhotoSorter Windows installer (NSIS 3, cross-built on Linux with makensis).
; Modelled on the Encode FFS installer (same layout, brand folder and modes).
; All identity values come from scripts/build-release.sh (-D...).
;
; - Per-user install by default (no administrator rights): %LOCALAPPDATA%\Programs\<APP_DIRNAME>
; - Optional "for everyone" (Program Files; asks for administrator rights)
; - Start Menu shortcut under <PUBLISHER>\<APP_NAME> (default on), desktop shortcut (optional)
; - Listed in Add/Remove Programs (HKCU or HKLM ...\Uninstall\<APP_ID>)
; - A later setup finds that entry and upgrades in place (same folder and mode, who/where
;   pages skipped); old shortcuts are replaced, never duplicated. If PhotoSorter is running,
;   setup asks you to close it first.
; - Writes <MARKER> next to the exe so the app keeps PhotoSorter.ini in
;   %APPDATA%\<PUBLISHER>\<APP_NAME> (Program Files is not writable)
; - Uninstall removes only the files setup installed and the shortcuts; the brand Start Menu
;   folder only if it is empty (other PrimeCrew apps keep theirs). Settings, presets, undo logs
;   and photos are never deleted.
; Silent: setup.exe /S [/ALLUSERS] [/DESKTOP] [/D=path]   uninstall.exe /S [/ALLUSERS]

Unicode true
Target amd64-unicode
!ifndef APP_NAME
  !error "Build with scripts/build-release.sh"
!endif
!ifndef PUBLISHER
  !error "Build with scripts/build-release.sh (PUBLISHER is required)"
!endif

!include "MUI2.nsh"
!include "LogicLib.nsh"
!include "FileFunc.nsh"
!include "nsDialogs.nsh"
!include "x64.nsh"

Name "${APP_NAME}"
OutFile "${OUTFILE}"
RequestExecutionLevel user
SetCompressor /SOLID lzma
BrandingText "${APP_NAME} ${APP_VERSION} - ${PUBLISHER}"
ShowInstDetails show
ShowUninstDetails show

VIProductVersion "${VERSION4}"
VIAddVersionKey "ProductName" "${APP_NAME}"
VIAddVersionKey "CompanyName" "${PUBLISHER}"
VIAddVersionKey "FileDescription" "${APP_NAME} setup"
VIAddVersionKey "FileVersion" "${APP_VERSION}"
VIAddVersionKey "ProductVersion" "${APP_VERSION}"
VIAddVersionKey "LegalCopyright" "Copyright (C) 2026 ${PUBLISHER}. GPL v3 or later."

!define UNINST_KEY "Software\Microsoft\Windows\CurrentVersion\Uninstall\${APP_ID}"

Var AllUsers        ; 1 = for everyone (HKLM, Program Files)
Var RadioMe
Var RadioAll
Var WantDesktop     ; 1 = desktop shortcut (silent /DESKTOP)
Var Upgrade         ; 1 = existing install found
Var InstalledVersion
Var WelcomeMsg

!define MUI_ICON "${ICON}"
!define MUI_UNICON "${ICON}"
!define MUI_ABORTWARNING
!define MUI_COMPONENTSPAGE_SMALLDESC
!define MUI_WELCOMEPAGE_TEXT "This installs ${APP_NAME} ${APP_VERSION} by ${PUBLISHER}.$\r$\n$\r$\n${APP_NAME} renames and sorts photos and videos into date folders. It always shows a preview first, never overwrites or deletes files, and every run can be undone.$\r$\n$\r$\nSettings and presets are kept in %APPDATA%\${PUBLISHER}\${APP_NAME}."
!define MUI_FINISHPAGE_RUN "$INSTDIR\${APP_EXE}"
!define MUI_FINISHPAGE_RUN_TEXT "Start ${APP_NAME} now"

!define MUI_PAGE_CUSTOMFUNCTION_SHOW WelcomeShow
!insertmacro MUI_PAGE_WELCOME
Page custom InstallModePage InstallModeLeave
!define MUI_PAGE_CUSTOMFUNCTION_PRE DirectoryPagePre
!insertmacro MUI_PAGE_DIRECTORY
!insertmacro MUI_PAGE_COMPONENTS
!insertmacro MUI_PAGE_INSTFILES
!insertmacro MUI_PAGE_FINISH

!define MUI_UNCONFIRMPAGE_TEXT_TOP "${APP_NAME} will be removed from the folder below. Your photos, undo logs, settings and presets (%APPDATA%\${PUBLISHER}\${APP_NAME}) are not touched."
!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES

!insertmacro MUI_LANGUAGE "English"

; ------------------------------------------------------------------ helpers

Function TrimDir
  ; In/out: $0 - drop a trailing backslash
  Push $1
  Push $2
  StrLen $1 $0
  ${If} $1 > 0
    IntOp $1 $1 - 1
    StrCpy $2 $0 1 $1
    ${If} $2 == "\"
      StrCpy $0 $0 $1
    ${EndIf}
  ${EndIf}
  Pop $2
  Pop $1
FunctionEnd

Function DetectExistingInstall
  ; HKCU first (unless /ALLUSERS), then HKLM; the folder must still contain ${APP_EXE}.
  ${If} $AllUsers != 1
    ClearErrors
    ReadRegStr $0 HKCU "${UNINST_KEY}" "InstallLocation"
    ${If} $0 != ""
      Call TrimDir
      ${If} ${FileExists} "$0\${APP_EXE}"
        ReadRegStr $1 HKCU "${UNINST_KEY}" "DisplayVersion"
        StrCpy $INSTDIR $0
        StrCpy $InstalledVersion $1
        StrCpy $AllUsers 0
        StrCpy $Upgrade 1
        Return
      ${EndIf}
    ${EndIf}
  ${EndIf}
  ClearErrors
  ReadRegStr $0 HKLM "${UNINST_KEY}" "InstallLocation"
  ${If} $0 != ""
    Call TrimDir
    ${If} ${FileExists} "$0\${APP_EXE}"
      ReadRegStr $1 HKLM "${UNINST_KEY}" "DisplayVersion"
      StrCpy $INSTDIR $0
      StrCpy $InstalledVersion $1
      StrCpy $AllUsers 1
      StrCpy $Upgrade 1
      Return
    ${EndIf}
  ${EndIf}
FunctionEnd

Function SetUpgradeWelcome
  ${If} $InstalledVersion == "${APP_VERSION}"
    StrCpy $WelcomeMsg "${APP_NAME} ${APP_VERSION} is already installed. This will reinstall it in place.$\r$\n$\r$\nYour settings, presets and undo logs are kept."
  ${ElseIf} $InstalledVersion == ""
    StrCpy $WelcomeMsg "${APP_NAME} is already installed. This will replace it in place with ${APP_VERSION}.$\r$\n$\r$\nYour settings, presets and undo logs are kept."
  ${Else}
    StrCpy $WelcomeMsg "${APP_NAME} $InstalledVersion is installed. This will replace it in place with ${APP_VERSION}.$\r$\n$\r$\nYour settings, presets and undo logs are kept."
  ${EndIf}
FunctionEnd

Function WelcomeShow
  ${If} $Upgrade == 1
  ${AndIf} $WelcomeMsg != ""
    ${NSD_SetText} $mui.WelcomePage.Text "$WelcomeMsg"
  ${EndIf}
FunctionEnd

Function DirectoryPagePre
  ${If} $Upgrade == 1
    Abort
  ${EndIf}
FunctionEnd

Function ExeIsInUse
  ; 1 if a PhotoSorter window is open (winit's window class + the app title; an Explorer
  ; window on a folder named PhotoSorter has another class), or the installed exe cannot be
  ; opened exclusively (it is running)
  FindWindow $0 "Window Class" "${APP_NAME}"
  ${If} $0 != 0
    Push 1
    Return
  ${EndIf}
  ${IfNot} ${FileExists} "$INSTDIR\${APP_EXE}"
    Push 0
    Return
  ${EndIf}
  System::Call 'kernel32::CreateFileW(w "$INSTDIR\${APP_EXE}", i 0x80000000, i 0, p 0, i 3, i 0, p 0) p .r0'
  StrCmp $0 "-1" ExeIsInUse_Yes
  StrCmp $0 "0xFFFFFFFF" ExeIsInUse_Yes
  StrCmp $0 "0xFFFFFFFFFFFFFFFF" ExeIsInUse_Yes
  System::Call 'kernel32::CloseHandle(p r0)'
  Push 0
  Return
  ExeIsInUse_Yes:
  Push 1
FunctionEnd

Function CheckAppNotRunning
  CheckAppNotRunning_Retry:
  Call ExeIsInUse
  Pop $0
  ${If} $0 == 0
    Return
  ${EndIf}
  ${If} ${Silent}
    SetErrorLevel 1618
    Abort "${APP_NAME} is running. Close it and run setup again."
  ${EndIf}
  MessageBox MB_RETRYCANCEL|MB_ICONEXCLAMATION "${APP_NAME} is running.$\r$\n$\r$\nPlease close it and click Retry to continue, or Cancel to stop setup." IDRETRY CheckAppNotRunning_Retry
  Abort
FunctionEnd

!macro SetMode
  ${If} $AllUsers == 1
    SetShellVarContext all
  ${Else}
    SetShellVarContext current
  ${EndIf}
!macroend

Function IsAdmin
  UserInfo::GetAccountType
  Pop $0
  ${If} $0 == "Admin"
    Push 1
  ${Else}
    Push 0
  ${EndIf}
FunctionEnd

Function DefaultDir
  ${If} $AllUsers == 1
    ${If} ${RunningX64}
      StrCpy $INSTDIR "$PROGRAMFILES64\${APP_DIRNAME}"
    ${Else}
      StrCpy $INSTDIR "$PROGRAMFILES\${APP_DIRNAME}"
    ${EndIf}
  ${Else}
    StrCpy $INSTDIR "$LOCALAPPDATA\Programs\${APP_DIRNAME}"
  ${EndIf}
FunctionEnd

Function InstallModePage
  ${If} $Upgrade == 1
  ${OrIf} $AllUsers == 1
    Abort
  ${EndIf}
  !insertmacro MUI_HEADER_TEXT "Who should get ${APP_NAME}?" "Choose whether to install it just for you or for everyone on this PC."
  nsDialogs::Create 1018
  Pop $0
  ${NSD_CreateRadioButton} 0 10u 100% 12u "&Just for me (recommended – no administrator rights needed)"
  Pop $RadioMe
  ${NSD_CreateLabel} 12u 24u 95% 20u "Installs to $LOCALAPPDATA\Programs\${APP_DIRNAME}."
  Pop $0
  ${NSD_CreateRadioButton} 0 50u 100% 12u "For &everyone on this PC (needs administrator rights)"
  Pop $RadioAll
  ${NSD_CreateLabel} 12u 64u 95% 20u "Installs to Program Files. Windows will ask for permission."
  Pop $0
  ${NSD_Check} $RadioMe
  nsDialogs::Show
FunctionEnd

Function InstallModeLeave
  ${NSD_GetState} $RadioAll $0
  ${If} $0 == ${BST_CHECKED}
    Call IsAdmin
    Pop $1
    ${If} $1 != 1
      ExecShell "runas" "$EXEPATH" "/ALLUSERS"
      Quit
    ${EndIf}
    StrCpy $AllUsers 1
  ${Else}
    StrCpy $AllUsers 0
  ${EndIf}
  Call DefaultDir
FunctionEnd

; ------------------------------------------------------------------ sections
Section "${APP_NAME} (required)" SecApp
  SectionIn RO
  !insertmacro SetMode
  Call CheckAppNotRunning
  SetOutPath "$INSTDIR"
  File "/oname=${APP_EXE}" "${EXE_PATH}"
  File "/oname=README.txt" "${DOC_DIR}\README.txt"
  File "/oname=HELP.md" "${DOC_DIR}\HELP.md"
  File "/oname=LICENSE" "${SRC_DIR}\LICENSE"
  File "/oname=THIRD-PARTY-NOTICES.txt" "${DOC_DIR}\THIRD-PARTY-NOTICES.txt"
  ; tells the app it is installed: settings go to %APPDATA%, not next to the exe
  FileOpen $0 "$INSTDIR\${MARKER}" w
  FileWrite $0 "Installed by setup. ${APP_NAME} keeps PhotoSorter.ini (settings, presets, window layout) in %APPDATA%\${PUBLISHER}\${APP_NAME}.$\r$\n"
  FileClose $0
  WriteUninstaller "$INSTDIR\uninstall.exe"

  ${If} $AllUsers == 1
    StrCpy $2 "/ALLUSERS"
  ${Else}
    StrCpy $2 "/CURRENTUSER"
  ${EndIf}
  ${GetSize} "$INSTDIR" "/S=0K" $3 $4 $5
  IntFmt $3 "0x%08X" $3
  ${If} $AllUsers == 1
    WriteRegStr HKLM "${UNINST_KEY}" "DisplayName" "${APP_NAME}"
    WriteRegStr HKLM "${UNINST_KEY}" "DisplayVersion" "${APP_VERSION}"
    WriteRegStr HKLM "${UNINST_KEY}" "Publisher" "${PUBLISHER}"
    WriteRegStr HKLM "${UNINST_KEY}" "DisplayIcon" "$INSTDIR\${APP_EXE},0"
    WriteRegStr HKLM "${UNINST_KEY}" "InstallLocation" "$INSTDIR"
    WriteRegStr HKLM "${UNINST_KEY}" "UninstallString" '"$INSTDIR\uninstall.exe" $2'
    WriteRegStr HKLM "${UNINST_KEY}" "QuietUninstallString" '"$INSTDIR\uninstall.exe" $2 /S'
    WriteRegDWORD HKLM "${UNINST_KEY}" "NoModify" 1
    WriteRegDWORD HKLM "${UNINST_KEY}" "NoRepair" 1
    WriteRegDWORD HKLM "${UNINST_KEY}" "EstimatedSize" $3
  ${Else}
    WriteRegStr HKCU "${UNINST_KEY}" "DisplayName" "${APP_NAME}"
    WriteRegStr HKCU "${UNINST_KEY}" "DisplayVersion" "${APP_VERSION}"
    WriteRegStr HKCU "${UNINST_KEY}" "Publisher" "${PUBLISHER}"
    WriteRegStr HKCU "${UNINST_KEY}" "DisplayIcon" "$INSTDIR\${APP_EXE},0"
    WriteRegStr HKCU "${UNINST_KEY}" "InstallLocation" "$INSTDIR"
    WriteRegStr HKCU "${UNINST_KEY}" "UninstallString" '"$INSTDIR\uninstall.exe" $2'
    WriteRegStr HKCU "${UNINST_KEY}" "QuietUninstallString" '"$INSTDIR\uninstall.exe" $2 /S'
    WriteRegDWORD HKCU "${UNINST_KEY}" "NoModify" 1
    WriteRegDWORD HKCU "${UNINST_KEY}" "NoRepair" 1
    WriteRegDWORD HKCU "${UNINST_KEY}" "EstimatedSize" $3
  ${EndIf}
SectionEnd

Section "Start Menu shortcut" SecStart
  !insertmacro SetMode
  ; Grouped with the other PrimeCrew Software titles (same brand folder as Encode FFS).
  ; Replaced on every install, so an upgrade never leaves a duplicate.
  CreateDirectory "$SMPROGRAMS\${PUBLISHER}"
  Delete "$SMPROGRAMS\${PUBLISHER}\${APP_NAME}.lnk"
  CreateShortcut "$SMPROGRAMS\${PUBLISHER}\${APP_NAME}.lnk" "$INSTDIR\${APP_EXE}" "" "$INSTDIR\${APP_EXE}" 0
  ; a root-level shortcut (none so far) would be a duplicate
  Delete "$SMPROGRAMS\${APP_NAME}.lnk"
SectionEnd

Section /o "Desktop shortcut" SecDesktop
  !insertmacro SetMode
  Delete "$DESKTOP\${APP_NAME}.lnk"
  CreateShortcut "$DESKTOP\${APP_NAME}.lnk" "$INSTDIR\${APP_EXE}" "" "$INSTDIR\${APP_EXE}" 0
SectionEnd

Function .onInit
  StrCpy $AllUsers 0
  StrCpy $WantDesktop 0
  StrCpy $Upgrade 0
  StrCpy $InstalledVersion ""
  StrCpy $WelcomeMsg ""
  ${GetParameters} $R0
  ClearErrors
  ${GetOptions} $R0 "/ALLUSERS" $R1
  ${IfNot} ${Errors}
    StrCpy $AllUsers 1
  ${EndIf}
  ClearErrors
  ${GetOptions} $R0 "/DESKTOP" $R1
  ${IfNot} ${Errors}
    StrCpy $WantDesktop 1
  ${EndIf}
  ${If} $INSTDIR == ""
    Call DetectExistingInstall
  ${EndIf}
  ${If} $AllUsers == 1
    Call IsAdmin
    Pop $0
    ${If} $0 != 1
      ${If} ${Silent}
        SetErrorLevel 740
        Quit
      ${EndIf}
      ExecShell "runas" "$EXEPATH" "/ALLUSERS"
      Quit
    ${EndIf}
  ${EndIf}
  ${If} $INSTDIR == ""
    Call DefaultDir
  ${EndIf}
  ${If} $Upgrade == 1
    Call SetUpgradeWelcome
    ; keep the desktop shortcut on upgrade if there is one
    !insertmacro SetMode
    ${If} ${FileExists} "$DESKTOP\${APP_NAME}.lnk"
      StrCpy $WantDesktop 1
    ${EndIf}
  ${EndIf}
  ${If} $WantDesktop == 1
    SectionGetFlags ${SecDesktop} $0
    IntOp $0 $0 | ${SF_SELECTED}
    SectionSetFlags ${SecDesktop} $0
  ${EndIf}
FunctionEnd

!insertmacro MUI_FUNCTION_DESCRIPTION_BEGIN
  !insertmacro MUI_DESCRIPTION_TEXT ${SecApp} "The program, README, HELP, LICENSE (GPL v3) and third-party notices."
  !insertmacro MUI_DESCRIPTION_TEXT ${SecStart} "A shortcut in the Start Menu, under ${PUBLISHER}."
  !insertmacro MUI_DESCRIPTION_TEXT ${SecDesktop} "A shortcut on the desktop."
!insertmacro MUI_FUNCTION_DESCRIPTION_END

; ------------------------------------------------------------------ uninstaller
Function un.IsAdmin
  UserInfo::GetAccountType
  Pop $0
  ${If} $0 == "Admin"
    Push 1
  ${Else}
    Push 0
  ${EndIf}
FunctionEnd

Section "un.${APP_NAME}" UnSecApp
  SectionIn RO
  !insertmacro SetMode
  Delete "$SMPROGRAMS\${PUBLISHER}\${APP_NAME}.lnk"
  RMDir "$SMPROGRAMS\${PUBLISHER}" ; only if empty: other PrimeCrew apps keep the folder
  Delete "$SMPROGRAMS\${APP_NAME}.lnk"
  Delete "$DESKTOP\${APP_NAME}.lnk"
  ; only the files setup installed (never settings, presets, undo logs or photos)
  Delete "$INSTDIR\${APP_EXE}"
  Delete "$INSTDIR\README.txt"
  Delete "$INSTDIR\HELP.md"
  Delete "$INSTDIR\LICENSE"
  Delete "$INSTDIR\THIRD-PARTY-NOTICES.txt"
  Delete "$INSTDIR\${MARKER}"
  Delete "$INSTDIR\uninstall.exe"
  RMDir "$INSTDIR" ; only if empty: anything else you put there stays
  ${If} $AllUsers == 1
    DeleteRegKey HKLM "${UNINST_KEY}"
  ${Else}
    DeleteRegKey HKCU "${UNINST_KEY}"
  ${EndIf}
SectionEnd

Function un.onInit
  StrCpy $AllUsers 0
  ${GetParameters} $R0
  ClearErrors
  ${GetOptions} $R0 "/ALLUSERS" $R1
  ${IfNot} ${Errors}
    StrCpy $AllUsers 1
    Call un.IsAdmin
    Pop $0
    ${If} $0 != 1
      ${If} ${Silent}
        SetErrorLevel 740
        Quit
      ${EndIf}
      ExecShell "runas" "$EXEPATH" "/ALLUSERS"
      Quit
    ${EndIf}
  ${EndIf}
FunctionEnd
