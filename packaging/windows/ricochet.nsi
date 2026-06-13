!include "MUI2.nsh"

!ifndef VERSION
  !error "VERSION must be defined"
!endif

!ifndef INPUT_DIR
  !error "INPUT_DIR must be defined"
!endif

!ifndef OUT_FILE
  !error "OUT_FILE must be defined"
!endif

!ifndef LICENSE_FILE
  !error "LICENSE_FILE must be defined"
!endif

Name "Ricochet ${VERSION}"
OutFile "${OUT_FILE}"
InstallDir "$LOCALAPPDATA\Programs\Ricochet"
RequestExecutionLevel user
SetCompressor /SOLID lzma

!define MUI_ABORTWARNING
!define MUI_ICON "${NSISDIR}\Contrib\Graphics\Icons\modern-install.ico"
!define MUI_UNICON "${NSISDIR}\Contrib\Graphics\Icons\modern-uninstall.ico"

!insertmacro MUI_PAGE_WELCOME
!insertmacro MUI_PAGE_LICENSE "${LICENSE_FILE}"
!insertmacro MUI_PAGE_DIRECTORY
!insertmacro MUI_PAGE_INSTFILES

!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES

!insertmacro MUI_LANGUAGE "English"

Section "Ricochet CLI" SEC_MAIN
  SectionIn RO

  SetOutPath "$INSTDIR"
  File /r "${INPUT_DIR}\*.*"

  WriteUninstaller "$INSTDIR\Uninstall.exe"

  CreateDirectory "$SMPROGRAMS\Ricochet"
  CreateShortCut "$SMPROGRAMS\Ricochet\Ricochet Shell.lnk" "$INSTDIR\Ricochet Shell.cmd"
  CreateShortCut "$SMPROGRAMS\Ricochet\Reference Docs.lnk" "$INSTDIR\docs\reference\index.html"
  CreateShortCut "$SMPROGRAMS\Ricochet\Uninstall Ricochet.lnk" "$INSTDIR\Uninstall.exe"

  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\Ricochet" "DisplayName" "Ricochet"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\Ricochet" "DisplayVersion" "${VERSION}"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\Ricochet" "Publisher" "Ricochet"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\Ricochet" "InstallLocation" "$INSTDIR"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\Ricochet" "UninstallString" "$\"$INSTDIR\Uninstall.exe$\""
  WriteRegDWORD HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\Ricochet" "NoModify" 1
  WriteRegDWORD HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\Ricochet" "NoRepair" 1
SectionEnd

Section "Uninstall"
  RMDir /r "$SMPROGRAMS\Ricochet"
  DeleteRegKey HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\Ricochet"
  RMDir /r "$INSTDIR"
SectionEnd
