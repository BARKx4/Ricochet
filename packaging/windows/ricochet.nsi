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

!ifndef INSTALL_MANIFEST
  !error "INSTALL_MANIFEST must be defined"
!endif

!ifndef LEGACY_CLEANUP_MANIFEST
  !error "LEGACY_CLEANUP_MANIFEST must be defined"
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

  ; A non-empty destination is accepted only when the current user registry
  ; binds it to Ricochet and it contains either the current marker or legacy
  ; rco.exe. This permits a safe rc.4 upgrade without claiming foreign files.
  IfFileExists "$INSTDIR\*.*" 0 destination_ready
  ReadRegStr $0 HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\Ricochet" "InstallLocation"
  StrCmp $0 "$INSTDIR" 0 destination_foreign
  ReadRegStr $1 HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\Ricochet" "DisplayName"
  StrCmp $1 "Ricochet" 0 destination_foreign
  IfFileExists "$INSTDIR\.ricochet-install-owner" destination_owned
  IfFileExists "$INSTDIR\rco.exe" destination_owned destination_foreign

destination_foreign:
  MessageBox MB_ICONSTOP|MB_OK "The selected directory is not an owned Ricochet installation. Choose an empty directory or the registered Ricochet install location." /SD IDOK
  SetErrorLevel 2
  Abort

destination_owned:
  !include "${LEGACY_CLEANUP_MANIFEST}"

destination_ready:
  SetOutPath "$INSTDIR"
  File /r "${INPUT_DIR}\*.*"

  FileOpen $0 "$INSTDIR\.ricochet-install-owner" w
  FileWrite $0 "Ricochet ${VERSION}$\r$\n"
  FileClose $0

  WriteUninstaller "$INSTDIR\Uninstall.exe"

  CreateDirectory "$SMPROGRAMS\Ricochet"
  CreateShortCut "$SMPROGRAMS\Ricochet\Ricochet Shell.lnk" "$INSTDIR\Ricochet Shell.cmd"
  CreateShortCut "$SMPROGRAMS\Ricochet\Reference Docs.lnk" "$INSTDIR\docs\reference\index.html"
  CreateShortCut "$SMPROGRAMS\Ricochet\Third-Party Licenses.lnk" "$INSTDIR\THIRD_PARTY_LICENSES.html"
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
  IfFileExists "$INSTDIR\.ricochet-install-owner" uninstall_owned
  MessageBox MB_ICONSTOP|MB_OK "Ricochet ownership marker is missing; uninstall stopped without removing files." /SD IDOK
  SetErrorLevel 2
  Abort

uninstall_owned:
  Delete "$SMPROGRAMS\Ricochet\Ricochet Shell.lnk"
  Delete "$SMPROGRAMS\Ricochet\Reference Docs.lnk"
  Delete "$SMPROGRAMS\Ricochet\Third-Party Licenses.lnk"
  Delete "$SMPROGRAMS\Ricochet\Uninstall Ricochet.lnk"
  RMDir "$SMPROGRAMS\Ricochet"
  DeleteRegKey HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\Ricochet"
  !include "${INSTALL_MANIFEST}"
SectionEnd
