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
      CreateDirectory "$APPDATA\LiveAudio\sessions"
      DetailPrint "Preserving legacy sessions in $APPDATA\LiveAudio\sessions"
      CopyFiles /SILENT "${DIR_PATH}\data\sessions\*.*" "$APPDATA\LiveAudio\sessions"
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

    ; Remove legacy InnoSetup uninstaller files if present
    Delete "${DIR_PATH}\unins000.exe"
    Delete "${DIR_PATH}\unins000.dat"

    ; If this legacy path is NOT the current target install directory ($INSTDIR)
    ${If} "${DIR_PATH}" != "$INSTDIR"
      ; Clean old uninstaller artifacts if present
      Delete "${DIR_PATH}\uninstall.exe"
      ${If} ${FileExists} "${DIR_PATH}\data"
        RMDir /r "${DIR_PATH}\data"
      ${EndIf}
      RMDir "${DIR_PATH}"
    ${EndIf}
  ${EndIf}
!macroend

; -----------------------------------------------------------------------------
; Function: CleanInstallLocationJsonFile
; Inspects install_location.json recorded by prior Python launchers (v1.1 - v1.2.7),
; purges the legacy install_root directory, migrates settings, and removes the file.
; -----------------------------------------------------------------------------
Function CleanInstallLocationJsonFile
  Exch $0 ; target file path
  Push $1
  Push $2
  Push $3
  Push $4
  Push $5
  Push $6
  Push $7
  Push $8
  Push $9

  ${If} ${FileExists} "$0"
    ClearErrors
    FileOpen $1 "$0" r
    ${IfNot} ${Errors}
      read_loc_loop:
        ClearErrors
        FileRead $1 $2
        ${If} ${Errors}
          Goto read_loc_done
        ${EndIf}
        ; Look for install_root token
        ${StrLoc} $3 $2 "install_root" ">"
        ${If} $3 != ""
          ${StrLoc} $4 $2 ":" ">"
          ${If} $4 != ""
            IntOp $4 $4 + 1
            StrCpy $5 $2 "" $4
            ${StrLoc} $6 $5 '"' ">"
            ${If} $6 != ""
              IntOp $6 $6 + 1
              StrCpy $5 $5 "" $6
              ${StrLoc} $6 $5 '"' ">"
              ${If} $6 != ""
                StrCpy $5 $5 $6
                ; Unescape double backslashes
                StrCpy $7 ""
                StrLen $8 $5
                StrCpy $9 0
                unescape_slash_loop:
                  ${If} $9 >= $8
                    Goto unescape_slash_done
                  ${EndIf}
                  StrCpy $3 $5 2 $9
                  ${If} $3 == "\\"
                    StrCpy $7 "$7\"
                    IntOp $9 $9 + 2
                  ${Else}
                    StrCpy $3 $5 1 $9
                    StrCpy $7 "$7$3"
                    IntOp $9 $9 + 1
                  ${EndIf}
                  Goto unescape_slash_loop
                unescape_slash_done:
                ${If} $7 != ""
                ${AndIf} $7 != "$APPDATA\LiveAudio"
                ${AndIf} $7 != "$APPDATA\LiveAudio\"
                ${AndIf} ${FileExists} "$7"
                  DetailPrint "Detected legacy installation root from $0: $7"
                  !insertmacro PurgeLegacyPythonDirectory "$7"
                ${EndIf}
              ${EndIf}
            ${EndIf}
          ${EndIf}
        ${EndIf}
        Goto read_loc_loop
      read_loc_done:
        FileClose $1
    ${EndIf}
    Delete "$0"
  ${EndIf}

  Pop $9
  Pop $8
  Pop $7
  Pop $6
  Pop $5
  Pop $4
  Pop $3
  Pop $2
  Pop $1
  Pop $0
FunctionEnd

; -----------------------------------------------------------------------------
; Macro: CheckAndCleanLegacyRegistryKey
; Inspects a specific registry root and subkey.
; If it represents a legacy Python LiveAudio installation (not v1.5.0+ Rust app),
; executes uninstaller silently and purges legacy files.
; -----------------------------------------------------------------------------
!macro CheckAndCleanLegacyRegistryKey ROOT_KEY SUBKEY_PATH
  ClearErrors
  ReadRegStr $R0 ${ROOT_KEY} "${SUBKEY_PATH}" "DisplayVersion"
  ReadRegStr $R1 ${ROOT_KEY} "${SUBKEY_PATH}" "MainBinaryName"
  ReadRegStr $R2 ${ROOT_KEY} "${SUBKEY_PATH}" "InstallLocation"
  ReadRegStr $R3 ${ROOT_KEY} "${SUBKEY_PATH}" "UninstallString"
  ReadRegStr $R4 ${ROOT_KEY} "${SUBKEY_PATH}" "QuietUninstallString"
  ReadRegStr $R8 ${ROOT_KEY} "${SUBKEY_PATH}" "DisplayName"

  ; Check if this is the modern v1.5.0+ Rust desktop app. Any of these markers is enough:
  ; the MainBinaryName value, an install directory that holds liveaudio-desktop.exe, or a
  ; major.minor version >= 1.5. Python releases are 1.2.x and older, so later Rust versions
  ; (1.6.0, ...) stay modern even if the MainBinaryName value is missing.
  StrCpy $R5 "0"
  ${If} $R1 == "liveaudio-desktop.exe"
    StrCpy $R5 "1"
  ${EndIf}

  ${If} $R5 == "0"
  ${AndIf} $R2 != ""
    IfFileExists "$R2\liveaudio-desktop.exe" 0 +2
      StrCpy $R5 "1"
  ${EndIf}

  ${If} $R5 == "0"
  ${AndIf} $R0 != ""
    ${StrLoc} $R6 $R0 "1." ">"
    ${If} $R6 == "0"
      StrCpy $R9 $R0 "" 2
      ${StrLoc} $R6 $R9 "." ">"
      ${If} $R6 != ""
        StrCpy $R9 $R9 $R6
      ${EndIf}
      ${If} $R9 >= 5
        StrCpy $R5 "1"
      ${EndIf}
    ${EndIf}
  ${EndIf}

  ; If NOT modern app and the key contains LiveAudio uninstall information
  ${If} $R5 == "0"
    ${If} $R3 != ""
    ${OrIf} $R2 != ""
    ${OrIf} $R0 != ""
    ${OrIf} $R8 != ""
      DetailPrint "Detected legacy LiveAudio registration in ${ROOT_KEY}\${SUBKEY_PATH} (v$R0)"

      ; If InstallLocation is empty, derive it from UninstallString
      ${If} $R2 == ""
      ${AndIf} $R3 != ""
        StrCpy $R9 $R3
        StrCpy $R7 $R9 1 0
        ${If} $R7 == '"'
          StrCpy $R9 $R9 "" 1
          ${StrLoc} $R6 $R9 '"' ">"
          ${If} $R6 != ""
            StrCpy $R9 $R9 $R6
          ${EndIf}
        ${Else}
          ${StrLoc} $R6 $R9 " " ">"
          ${If} $R6 != ""
            StrCpy $R9 $R9 $R6
          ${EndIf}
        ${EndIf}
        ${StrLoc} $R6 $R9 "\" "<"
        ${If} $R6 != ""
          StrCpy $R2 $R9 $R6
        ${EndIf}
      ${EndIf}

      ; Attempt silent execution of legacy uninstaller
      ${If} $R4 != ""
        DetailPrint "Running silent legacy uninstaller: $R4"
        ExecWait '$R4' $R7
      ${ElseIf} $R3 != ""
        ; Determine uninstaller command and switches without corrupting quotes
        ${StrCase} $R9 $R3 "L"
        ${StrLoc} $R6 $R9 "unins" ">"
        ${If} $R6 != ""
          ; InnoSetup uninstaller
          StrCpy $R7 $R3 1 0
          ${If} $R7 == '"'
            StrCpy $R9 '$R3 /VERYSILENT /SUPPRESSMSGBOXES /NORESTART'
          ${Else}
            StrCpy $R9 '"$R3" /VERYSILENT /SUPPRESSMSGBOXES /NORESTART'
          ${EndIf}
          DetailPrint "Running silent InnoSetup legacy uninstaller: $R9"
          ExecWait '$R9' $R7
        ${Else}
          ; NSIS uninstaller
          StrCpy $R7 $R3 1 0
          ${If} $R7 == '"'
            ${If} $R2 != ""
              StrCpy $R9 '$R3 /S _?=$R2'
            ${Else}
              StrCpy $R9 '$R3 /S'
            ${EndIf}
          ${Else}
            ${If} $R2 != ""
              StrCpy $R9 '"$R3" /S _?=$R2'
            ${Else}
              StrCpy $R9 '"$R3" /S'
            ${EndIf}
          ${EndIf}
          DetailPrint "Running silent NSIS legacy uninstaller: $R9"
          ExecWait '$R9' $R7
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

  ; 1. Stop legacy running processes if active
  nsis_tauri_utils::FindProcessCurrentUser "liveaudio.exe"
  Pop $R0
  ${If} $R0 = 0
    DetailPrint "Stopping running legacy liveaudio.exe process..."
    nsis_tauri_utils::KillProcessCurrentUser "liveaudio.exe"
    Sleep 500
  ${EndIf}

  nsis_tauri_utils::FindProcessCurrentUser "liveaudio-launcher.exe"
  Pop $R0
  ${If} $R0 = 0
    DetailPrint "Stopping running legacy liveaudio-launcher.exe process..."
    nsis_tauri_utils::KillProcessCurrentUser "liveaudio-launcher.exe"
    Sleep 500
  ${EndIf}

  nsis_tauri_utils::FindProcessCurrentUser "LiveAudio-Setup.exe"
  Pop $R0
  ${If} $R0 = 0
    DetailPrint "Stopping running legacy LiveAudio-Setup.exe process..."
    nsis_tauri_utils::KillProcessCurrentUser "LiveAudio-Setup.exe"
    Sleep 500
  ${EndIf}

  ; 2. Inspect 64-bit registry keys (HKCU & HKLM)
  ${If} ${RunningX64}
    SetRegView 64
  ${EndIf}
  !insertmacro CheckAndCleanLegacyRegistryKey HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\LiveAudio"
  !insertmacro CheckAndCleanLegacyRegistryKey HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\LiveAudio"
  !insertmacro CheckAndCleanLegacyRegistryKey HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\LiveAudio-Setup"
  !insertmacro CheckAndCleanLegacyRegistryKey HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\LiveAudio-Setup"
  !insertmacro CheckAndCleanLegacyRegistryKey HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\LiveAudio_is1"
  !insertmacro CheckAndCleanLegacyRegistryKey HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\LiveAudio_is1"

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

  ; 4. Inspect install_location.json recorded by legacy Python launchers
  Push "$APPDATA\LiveAudio\install_location.json"
  Call CleanInstallLocationJsonFile

  Push "$LOCALAPPDATA\LiveAudio\install_location.json"
  Call CleanInstallLocationJsonFile

  ; 5. Check known legacy directories and signatures
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

  ; 6. Purge old shortcuts so they do not conflict with v1.5.0
  !insertmacro PurgeLegacyShortcuts

  DetailPrint "Legacy cleanup completed. Continuing with LiveAudio v1.5.0 installation."
!macroend

!macro NSIS_HOOK_POSTINSTALL
!macroend

!macro NSIS_HOOK_PREUNINSTALL
!macroend

!macro NSIS_HOOK_POSTUNINSTALL
!macroend
