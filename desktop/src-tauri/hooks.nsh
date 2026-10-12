; LiveAudio NSIS Installer Hooks
; Migrates and cleans legacy Python-based LiveAudio installations (v1.0.x - v1.2.7)
; and protects user settings/data in %APPDATA%\LiveAudio.

; -----------------------------------------------------------------------------
; Macro: PurgeLegacyPythonDirectory
; Removes deprecated Python runtime, virtualenvs, launchers, and log files.
; Strictly avoids touching %APPDATA%\LiveAudio.
; -----------------------------------------------------------------------------
!macro PurgeLegacyPythonDirectory DIR_PATH
  ${If} "${DIR_PATH}" != ""
  ${AndIf} "${DIR_PATH}" != "$APPDATA\LiveAudio"
  ${AndIf} "${DIR_PATH}" != "$APPDATA\LiveAudio\"
    DetailPrint "Scanning for deprecated LiveAudio Python files in: ${DIR_PATH}"

    ; If legacy data/config.json exists, ensure it is safely preserved in %APPDATA%\LiveAudio
    ${If} ${FileExists} "${DIR_PATH}\data\config.json"
      CreateDirectory "$APPDATA\LiveAudio"
      ${IfNot} ${FileExists} "$APPDATA\LiveAudio\config.json"
        DetailPrint "Preserving legacy configuration in $APPDATA\LiveAudio\config.json"
        CopyFiles /SILENT "${DIR_PATH}\data\config.json" "$APPDATA\LiveAudio\config.json"
      ${EndIf}
    ${EndIf}

    ${If} ${FileExists} "${DIR_PATH}\data\sessions"
      CreateDirectory "$APPDATA\LiveAudio"
      ${IfNot} ${FileExists} "$APPDATA\LiveAudio\sessions"
        DetailPrint "Preserving legacy sessions in $APPDATA\LiveAudio\sessions"
        CreateDirectory "$APPDATA\LiveAudio\sessions"
        CopyFiles /SILENT "${DIR_PATH}\data\sessions\*.*" "$APPDATA\LiveAudio\sessions"
      ${EndIf}
    ${EndIf}

    ; Clean deprecated Python environment and application code
    ${If} ${FileExists} "${DIR_PATH}\app"
      DetailPrint "Purging deprecated Python application directory: ${DIR_PATH}\app"
      RMDir /r "${DIR_PATH}\app"
    ${EndIf}

    ${If} ${FileExists} "${DIR_PATH}\python"
      DetailPrint "Purging deprecated Python runtime directory: ${DIR_PATH}\python"
      RMDir /r "${DIR_PATH}\python"
    ${EndIf}

    ${If} ${FileExists} "${DIR_PATH}\uv"
      DetailPrint "Purging legacy uv directory: ${DIR_PATH}\uv"
      RMDir /r "${DIR_PATH}\uv"
    ${EndIf}

    ${If} ${FileExists} "${DIR_PATH}\.venv"
      DetailPrint "Purging legacy virtualenv: ${DIR_PATH}\.venv"
      RMDir /r "${DIR_PATH}\.venv"
    ${EndIf}

    ; Remove legacy marker and log files
    ${If} ${FileExists} "${DIR_PATH}\installed.json"
      DetailPrint "Removing legacy installed.json marker"
      Delete "${DIR_PATH}\installed.json"
    ${EndIf}

    ${If} ${FileExists} "${DIR_PATH}\bootstrap.log"
      Delete "${DIR_PATH}\bootstrap.log"
    ${EndIf}

    ${If} ${FileExists} "${DIR_PATH}\liveaudio_bootstrap.log"
      Delete "${DIR_PATH}\liveaudio_bootstrap.log"
    ${EndIf}

    ; Remove legacy launcher binaries
    ${If} ${FileExists} "${DIR_PATH}\liveaudio.exe"
      DetailPrint "Removing legacy liveaudio.exe launcher"
      Delete "${DIR_PATH}\liveaudio.exe"
    ${EndIf}

    ${If} ${FileExists} "${DIR_PATH}\LiveAudio.exe"
      DetailPrint "Removing legacy LiveAudio.exe launcher"
      Delete "${DIR_PATH}\LiveAudio.exe"
    ${EndIf}

    ${If} ${FileExists} "${DIR_PATH}\liveaudio-launcher.exe"
      DetailPrint "Removing legacy liveaudio-launcher.exe"
      Delete "${DIR_PATH}\liveaudio-launcher.exe"
    ${EndIf}

    ${If} ${FileExists} "${DIR_PATH}\LiveAudio-Setup.exe"
      DetailPrint "Removing legacy LiveAudio-Setup.exe"
      Delete "${DIR_PATH}\LiveAudio-Setup.exe"
    ${EndIf}

    ; If this legacy path is NOT the current target install directory ($INSTDIR)
    ${If} "${DIR_PATH}" != "$INSTDIR"
      ; Clean old uninstaller artifacts if present
      Delete "${DIR_PATH}\uninstall.exe"
      Delete "${DIR_PATH}\unins000.exe"
      Delete "${DIR_PATH}\unins000.dat"
      ${If} ${FileExists} "${DIR_PATH}\data"
        RMDir /r "${DIR_PATH}\data"
      ${EndIf}
      RMDir "${DIR_PATH}"
    ${EndIf}
  ${EndIf}
!macroend

; -----------------------------------------------------------------------------
; Macro: CheckAndCleanLegacyRegistryKey
; Inspects a specific registry root and subkey.
; If it represents a legacy Python LiveAudio installation (not v1.5.0 Rust app),
; executes uninstaller silently and purges legacy files.
; -----------------------------------------------------------------------------
!macro CheckAndCleanLegacyRegistryKey ROOT_KEY SUBKEY_PATH
  ClearErrors
  ReadRegStr $R0 ${ROOT_KEY} "${SUBKEY_PATH}" "DisplayVersion"
  ReadRegStr $R1 ${ROOT_KEY} "${SUBKEY_PATH}" "MainBinaryName"
  ReadRegStr $R2 ${ROOT_KEY} "${SUBKEY_PATH}" "InstallLocation"
  ReadRegStr $R3 ${ROOT_KEY} "${SUBKEY_PATH}" "UninstallString"
  ReadRegStr $R4 ${ROOT_KEY} "${SUBKEY_PATH}" "QuietUninstallString"

  ; Check if this is the modern v1.5.0 Rust desktop app
  StrCpy $R5 "0"
  ${If} $R1 == "liveaudio-desktop.exe"
    ${StrLoc} $R6 $R0 "1.5." ">"
    ${If} $R6 == "0"
      StrCpy $R5 "1"
    ${EndIf}
    ${If} $R0 == "1.5.0"
      StrCpy $R5 "1"
    ${EndIf}
  ${EndIf}

  ; If NOT v1.5.0 and the key contains LiveAudio uninstall information
  ${If} $R5 == "0"
    ${If} $R3 != ""
    ${OrIf} $R2 != ""
    ${OrIf} $R0 != ""
      DetailPrint "Detected legacy LiveAudio registration in ${ROOT_KEY}\${SUBKEY_PATH} (v$R0)"

      ; Attempt silent execution of legacy uninstaller
      ${If} $R4 != ""
        DetailPrint "Running silent legacy uninstaller: $R4"
        ExecWait '$R4' $R7
      ${ElseIf} $R3 != ""
        DetailPrint "Running legacy uninstaller: $R3 /S"
        ${If} $R2 != ""
          ExecWait '"$R3" /S _?=$R2' $R7
        ${Else}
          ExecWait '$R3 /S' $R7
        ${EndIf}
      ${EndIf}

      ; Purge files from InstallLocation if provided
      ${If} $R2 != ""
        !insertmacro PurgeLegacyPythonDirectory "$R2"
      ${EndIf}

      ; Delete the deprecated uninstall registry entry
      DeleteRegKey ${ROOT_KEY} "${SUBKEY_PATH}"
    ${EndIf}
  ${EndIf}
!macroend

; -----------------------------------------------------------------------------
; Macro: PurgeLegacyShortcuts
; Removes legacy desktop and Start menu shortcuts from prior versions.
; -----------------------------------------------------------------------------
!macro PurgeLegacyShortcuts
  DetailPrint "Cleaning up legacy LiveAudio shortcuts..."
  ; Desktop shortcuts
  Delete "$DESKTOP\LiveAudio.lnk"
  Delete "$DESKTOP\Plynte LiveAudio.lnk"
  Delete "$COMMONDESKTOP\LiveAudio.lnk"
  Delete "$COMMONDESKTOP\Plynte LiveAudio.lnk"

  ; Start Menu shortcuts (per-user and machine-wide)
  Delete "$SMPROGRAMS\LiveAudio.lnk"
  Delete "$SMPROGRAMS\Plynte LiveAudio.lnk"
  Delete "$SMPROGRAMS\LiveAudio\LiveAudio.lnk"
  Delete "$SMPROGRAMS\LiveAudio\Plynte LiveAudio.lnk"
  Delete "$SMPROGRAMS\LiveAudio\Uninstall LiveAudio.lnk"
  Delete "$SMPROGRAMS\LiveAudio\Desinstalar LiveAudio.lnk"
  RMDir "$SMPROGRAMS\LiveAudio"
  Delete "$SMPROGRAMS\Plynte\LiveAudio.lnk"
  RMDir "$SMPROGRAMS\Plynte"

  Delete "$COMMONSTARTMENU\Programs\LiveAudio.lnk"
  Delete "$COMMONSTARTMENU\Programs\Plynte LiveAudio.lnk"
  Delete "$COMMONSTARTMENU\Programs\LiveAudio\LiveAudio.lnk"
  Delete "$COMMONSTARTMENU\Programs\LiveAudio\Plynte LiveAudio.lnk"
  RMDir /r "$COMMONSTARTMENU\Programs\LiveAudio"
!macroend

; -----------------------------------------------------------------------------
; NSIS_HOOK_PREINSTALL
; Executed at the beginning of Section Install before copying files.
; -----------------------------------------------------------------------------
!macro NSIS_HOOK_PREINSTALL
  DetailPrint "Checking for legacy LiveAudio Python installations..."

  ; 1. Stop legacy liveaudio.exe process if running
  nsis_tauri_utils::FindProcessCurrentUser "liveaudio.exe"
  Pop $R0
  ${If} $R0 = 0
    DetailPrint "Stopping running legacy liveaudio.exe process..."
    nsis_tauri_utils::KillProcessCurrentUser "liveaudio.exe"
    Sleep 500
  ${EndIf}

  ; 2. Inspect 64-bit registry keys (HKCU & HKLM)
  ${If} ${RunningX64}
    SetRegView 64
  ${EndIf}
  !insertmacro CheckAndCleanLegacyRegistryKey HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\LiveAudio-Setup"
  !insertmacro CheckAndCleanLegacyRegistryKey HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\LiveAudio-Setup"
  !insertmacro CheckAndCleanLegacyRegistryKey HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\LiveAudio_is1"
  !insertmacro CheckAndCleanLegacyRegistryKey HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\LiveAudio_is1"
  !insertmacro CheckAndCleanLegacyRegistryKey HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\LiveAudio"

  ; Clean legacy product registration keys if not needed
  DeleteRegKey HKCU "Software\LiveAudio"
  DeleteRegKey HKCU "Software\Plynte\LiveAudio"

  ; 3. Inspect 32-bit registry keys (WOW6432Node on 64-bit systems)
  ${If} ${RunningX64}
    SetRegView 32
    !insertmacro CheckAndCleanLegacyRegistryKey HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\LiveAudio"
    !insertmacro CheckAndCleanLegacyRegistryKey HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\LiveAudio"
    !insertmacro CheckAndCleanLegacyRegistryKey HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\LiveAudio-Setup"
    !insertmacro CheckAndCleanLegacyRegistryKey HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\LiveAudio-Setup"
    !insertmacro CheckAndCleanLegacyRegistryKey HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\LiveAudio_is1"
    !insertmacro CheckAndCleanLegacyRegistryKey HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\LiveAudio_is1"
    ; Restore 64-bit view
    SetRegView 64
  ${EndIf}

  ; 4. Check known legacy directories and signatures
  ; (a) Current target directory ($INSTDIR)
  ${If} ${FileExists} "$INSTDIR\installed.json"
  ${OrIf} ${FileExists} "$INSTDIR\app\.venv"
  ${OrIf} ${FileExists} "$INSTDIR\python\python.exe"
  ${OrIf} ${FileExists} "$INSTDIR\liveaudio.exe"
    DetailPrint "Detected legacy Python signature in install destination ($INSTDIR)"
    !insertmacro PurgeLegacyPythonDirectory "$INSTDIR"
  ${EndIf}

  ; (b) %LOCALAPPDATA%\LiveAudio
  ${If} "$INSTDIR" != "$LOCALAPPDATA\LiveAudio"
    ${If} ${FileExists} "$LOCALAPPDATA\LiveAudio\installed.json"
    ${OrIf} ${FileExists} "$LOCALAPPDATA\LiveAudio\app\.venv"
    ${OrIf} ${FileExists} "$LOCALAPPDATA\LiveAudio\python\python.exe"
    ${OrIf} ${FileExists} "$LOCALAPPDATA\LiveAudio\liveaudio.exe"
      DetailPrint "Detected legacy Python signature in $LOCALAPPDATA\LiveAudio"
      !insertmacro PurgeLegacyPythonDirectory "$LOCALAPPDATA\LiveAudio"
    ${EndIf}
  ${EndIf}

  ; (c) %LOCALAPPDATA%\Programs\LiveAudio
  ${If} "$INSTDIR" != "$LOCALAPPDATA\Programs\LiveAudio"
    ${If} ${FileExists} "$LOCALAPPDATA\Programs\LiveAudio\installed.json"
    ${OrIf} ${FileExists} "$LOCALAPPDATA\Programs\LiveAudio\app\.venv"
    ${OrIf} ${FileExists} "$LOCALAPPDATA\Programs\LiveAudio\python\python.exe"
    ${OrIf} ${FileExists} "$LOCALAPPDATA\Programs\LiveAudio\liveaudio.exe"
      DetailPrint "Detected legacy Python signature in $LOCALAPPDATA\Programs\LiveAudio"
      !insertmacro PurgeLegacyPythonDirectory "$LOCALAPPDATA\Programs\LiveAudio"
    ${EndIf}
  ${EndIf}

  ; (d) %PROGRAMFILES%\LiveAudio
  ${If} "$INSTDIR" != "$PROGRAMFILES\LiveAudio"
    ${If} ${FileExists} "$PROGRAMFILES\LiveAudio\installed.json"
    ${OrIf} ${FileExists} "$PROGRAMFILES\LiveAudio\app\.venv"
    ${OrIf} ${FileExists} "$PROGRAMFILES\LiveAudio\python\python.exe"
      DetailPrint "Detected legacy Python signature in $PROGRAMFILES\LiveAudio"
      !insertmacro PurgeLegacyPythonDirectory "$PROGRAMFILES\LiveAudio"
    ${EndIf}
  ${EndIf}

  ; (e) Clean legacy install location marker if present, keeping config and sessions intact
  Delete "$APPDATA\LiveAudio\install_location.json"
  Delete "$LOCALAPPDATA\LiveAudio\install_location.json"

  ; 5. Purge old shortcuts so they do not conflict with v1.5.0
  !insertmacro PurgeLegacyShortcuts

  DetailPrint "Legacy cleanup completed. Continuing with LiveAudio v1.5.0 installation."
!macroend

!macro NSIS_HOOK_POSTINSTALL
!macroend

!macro NSIS_HOOK_PREUNINSTALL
!macroend

!macro NSIS_HOOK_POSTUNINSTALL
!macroend
