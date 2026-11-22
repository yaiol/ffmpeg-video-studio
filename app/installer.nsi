; The installer script `dx bundle` renders (Dioxus.toml → [bundle.windows.nsis] template).
;
; It is dx's built-in script with three changes, and nothing else:
;   - the name shown in Settings > Apps is "yaiol <name>" (like the electron installers), where dx
;     would show its PascalCased executable name ("AudioConverter");
;   - the uninstall entry carries an icon (DisplayIcon) - dx writes none, so Settings showed a blank;
;   - the Start Menu / Desktop shortcuts are named with the real display name, spaces included.
;
; ⚠ the display name is the one literal below. It is identity, so it is written from the
; birth certificate by the cockpit's app-info (and by the scaffolder) - never edit it by hand.
; Everything in double curly braces is dx's own template syntax, filled in at bundle time.
!define PRODUCT_DISPLAY "FFmpeg Video Studio"

!include "MUI2.nsh"
!include "FileFunc.nsh"
!include "x64.nsh"
!include "LogicLib.nsh"
!include "nsDialogs.nsh"

; ── Who the app is installed for ─────────────────────────────────────────────────────────────────
; With install_mode "Both" (Dioxus.toml) the installer ASKS: everyone on this computer, or only the
; current user. dx's own script never asked: it took administrator rights, put the files in Program
; Files and still wrote the shortcuts and the Settings > Apps entry for the current user only.
;   all users : Program Files\yaiol\<name>, shortcuts and uninstall entry for everyone (HKLM), needs
;               administrator - the installer starts WITHOUT it and restarts itself elevated only then
;   only me   : %LOCALAPPDATA%\Programs\yaiol\<name>, shortcuts and uninstall entry of the user (HKCU),
;               no administrator prompt at all
; A previous install decides the default, so an update keeps its mode and its folder. The command
; line decides without the page: /ALLUSERS or /CURRENTUSER (with /S for a silent install; a silent
; install with neither keeps the previous mode, else all users when already elevated, else only me).
!define UNINST_KEY "Software\Microsoft\Windows\CurrentVersion\Uninstall\{{bundle_id}}"
Var AllUsers   ; "1" = everyone on this computer, "0" = only the current user
Var ModeGiven  ; "1" = decided without the page (command line, or a mode with no choice)
Var IsAdmin    ; "1" = this process holds administrator rights
Var PrevAll    ; the mode of the install already there ("" = none)
Var PrevDir    ; its folder
Var AutoDir    ; the folder this script proposed (a folder the user typed is never replaced)
Var RadioAll

; Basic installer attributes
Name "${PRODUCT_DISPLAY}"
OutFile "{{output_path}}"
Unicode true
; No InstallDir: the default folder depends on the choice (ApplyMode below), the same
; Program Files\yaiol\<Product Name> as every yaiol electron app for all users. The user can still
; pick another one and the cockpit finds it through the uninstall entry's InstallLocation.

; Privileges: "Both" starts as the plain user and elevates only for an all-users install.
{{#if install_mode_per_machine}}
!define YAIOL_SCOPE "machine"
RequestExecutionLevel admin
{{else if install_mode_both}}
!define YAIOL_SCOPE "both"
RequestExecutionLevel user
{{else}}
!define YAIOL_SCOPE "user"
RequestExecutionLevel user
{{/if}}

; Version information
VIProductVersion "{{version}}.0"
VIAddVersionKey "ProductName" "${PRODUCT_DISPLAY}"
VIAddVersionKey "FileVersion" "{{version}}"
VIAddVersionKey "ProductVersion" "{{version}}"
VIAddVersionKey "FileDescription" "{{short_description}}"
{{#if publisher}}
VIAddVersionKey "CompanyName" "{{publisher}}"
{{/if}}
{{#if copyright}}
VIAddVersionKey "LegalCopyright" "{{copyright}}"
{{/if}}

; MUI settings
!define MUI_ABORTWARNING
{{#if installer_icon}}
!define MUI_ICON "{{installer_icon}}"
{{/if}}
{{#if header_image}}
!define MUI_HEADERIMAGE
!define MUI_HEADERIMAGE_BITMAP "{{header_image}}"
{{/if}}
{{#if sidebar_image}}
!define MUI_WELCOMEFINISHPAGE_BITMAP "{{sidebar_image}}"
{{/if}}

; Pages
{{#if license}}
!insertmacro MUI_PAGE_LICENSE "{{license}}"
{{/if}}
Page custom ModePage ModePageLeave
!insertmacro MUI_PAGE_DIRECTORY
!insertmacro MUI_PAGE_INSTFILES
!insertmacro MUI_PAGE_FINISH

; Uninstaller pages
!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES

; Language
!insertmacro MUI_LANGUAGE "English"
{{#each additional_languages}}
!insertmacro MUI_LANGUAGE "{{this}}"
{{/each}}

; The folder and the shortcut / registry scope that go with the chosen mode.
Function ApplyMode
    ${If} $AllUsers == "1"
        SetShellVarContext all
        StrCpy $0 "$PROGRAMFILES64\yaiol\${PRODUCT_DISPLAY}"
    ${Else}
        SetShellVarContext current
        StrCpy $0 "$LOCALAPPDATA\Programs\yaiol\${PRODUCT_DISPLAY}"
    ${EndIf}
    ${If} $PrevAll == $AllUsers
    ${AndIf} $PrevDir != ""
        StrCpy $0 $PrevDir
    ${EndIf}
    ${If} $INSTDIR == ""
    ${OrIf} $INSTDIR == $AutoDir
        StrCpy $INSTDIR $0
        StrCpy $AutoDir $0
    ${EndIf}
FunctionEnd

; Start this installer again with administrator rights, for all users. The error flag is set when
; the user refuses the prompt.
Function Elevate
    ${GetParameters} $R0
    ClearErrors
    ExecShell "runas" "$EXEPATH" "/ALLUSERS $R0"
FunctionEnd

Function .onInit
    SetRegView 64
    UserInfo::GetAccountType
    Pop $0
    StrCpy $IsAdmin "0"
    ${If} $0 == "Admin"
        StrCpy $IsAdmin "1"
    ${EndIf}

    ; The install already there. One made before the choice existed has its entry in the user's
    ; registry and its files in the machine's folder: it counts as all users.
    StrCpy $PrevAll ""
    ReadRegStr $PrevDir HKLM "${UNINST_KEY}" "InstallLocation"
    ${If} $PrevDir != ""
        StrCpy $PrevAll "1"
    ${Else}
        ReadRegStr $PrevDir HKCU "${UNINST_KEY}" "InstallLocation"
        ${If} $PrevDir != ""
            ReadRegStr $0 HKCU "${UNINST_KEY}" "InstallMode"
            ${If} $0 == "CurrentUser"
                StrCpy $PrevAll "0"
            ${Else}
                StrCpy $PrevAll "1"
            ${EndIf}
        ${EndIf}
    ${EndIf}

    StrCpy $ModeGiven "0"
    StrCpy $AllUsers $PrevAll
    !if "${YAIOL_SCOPE}" == "machine"
        StrCpy $AllUsers "1"
        StrCpy $ModeGiven "1"
    !else if "${YAIOL_SCOPE}" == "user"
        StrCpy $AllUsers "0"
        StrCpy $ModeGiven "1"
    !else
        ${GetParameters} $R0
        ClearErrors
        ${GetOptions} $R0 "/ALLUSERS" $R1
        ${IfNot} ${Errors}
            StrCpy $AllUsers "1"
            StrCpy $ModeGiven "1"
        ${EndIf}
        ClearErrors
        ${GetOptions} $R0 "/CURRENTUSER" $R1
        ${IfNot} ${Errors}
            StrCpy $AllUsers "0"
            StrCpy $ModeGiven "1"
        ${EndIf}
        ClearErrors
    !endif
    ${If} $AllUsers == ""
        StrCpy $AllUsers "1"
        ${If} ${Silent}
        ${AndIf} $IsAdmin != "1"
            StrCpy $AllUsers "0"
        ${EndIf}
    ${EndIf}

    ; No page will ask (silent, or decided): elevate now when all users needs it.
    ${If} ${Silent}
    ${OrIf} $ModeGiven == "1"
        ${If} $AllUsers == "1"
        ${AndIf} $IsAdmin != "1"
            Call Elevate
            ${If} ${Errors}
                Abort
            ${EndIf}
            Quit
        ${EndIf}
    ${EndIf}
    Call ApplyMode
FunctionEnd

Function ModePage
    ${If} $ModeGiven == "1"
        Abort
    ${EndIf}
    !insertmacro MUI_HEADER_TEXT "Choose who to install for" "${PRODUCT_DISPLAY} can be installed for everyone or only for you."
    nsDialogs::Create 1018
    Pop $0
    ${NSD_CreateLabel} 0 0 100% 20u "Install ${PRODUCT_DISPLAY} for:"
    Pop $0
    ${NSD_CreateRadioButton} 10u 26u 95% 12u "Anyone who uses this computer (all users)"
    Pop $RadioAll
    ${NSD_CreateRadioButton} 10u 44u 95% 12u "Only for me"
    Pop $1
    ${NSD_CreateLabel} 0 70u 100% 20u "Installing for all users asks for administrator rights."
    Pop $0
    ${If} $AllUsers == "1"
        ${NSD_Check} $RadioAll
    ${Else}
        ${NSD_Check} $1
    ${EndIf}
    nsDialogs::Show
FunctionEnd

Function ModePageLeave
    ${NSD_GetState} $RadioAll $0
    ${If} $0 == ${BST_CHECKED}
        StrCpy $AllUsers "1"
    ${Else}
        StrCpy $AllUsers "0"
    ${EndIf}
    ${If} $AllUsers == "1"
    ${AndIf} $IsAdmin != "1"
        Call Elevate
        ${If} ${Errors}
            Abort
        ${EndIf}
        Quit
    ${EndIf}
    Call ApplyMode
FunctionEnd

; Installer section
Section "Install"
    SetOutPath $INSTDIR

    ; Install main binary
    File "{{main_binary_path}}"

    ; Install resources
    {{#each staged_files}}
    SetOutPath "$INSTDIR{{#if this.target_dir}}\{{this.target_dir}}{{/if}}"
    File "{{this.source}}"
    {{/each}}

    SetOutPath $INSTDIR

    {{#if installer_icon}}
    ; The icon Settings > Apps shows for the uninstall entry
    File /oname=app.ico "{{installer_icon}}"
    {{/if}}

    ; Create uninstaller
    WriteUninstaller "$INSTDIR\uninstall.exe"

    ; Create Start Menu shortcuts
    CreateDirectory "$SMPROGRAMS\{{start_menu_folder}}"
    CreateShortcut "$SMPROGRAMS\{{start_menu_folder}}\${PRODUCT_DISPLAY}.lnk" "$INSTDIR\{{main_binary_name}}"
    CreateShortcut "$SMPROGRAMS\{{start_menu_folder}}\Uninstall ${PRODUCT_DISPLAY}.lnk" "$INSTDIR\uninstall.exe"

    ; Create Desktop shortcut
    CreateShortcut "$DESKTOP\${PRODUCT_DISPLAY}.lnk" "$INSTDIR\{{main_binary_name}}"

    ; An all-users install replaces what an install from before the choice existed (or an "only me"
    ; one in the same account) left in the user's own Start menu, desktop and registry.
    ${If} $AllUsers == "1"
        SetShellVarContext current
        Delete "$SMPROGRAMS\{{start_menu_folder}}\${PRODUCT_DISPLAY}.lnk"
        Delete "$SMPROGRAMS\{{start_menu_folder}}\Uninstall ${PRODUCT_DISPLAY}.lnk"
        RMDir "$SMPROGRAMS\{{start_menu_folder}}"
        Delete "$DESKTOP\${PRODUCT_DISPLAY}.lnk"
        DeleteRegKey HKCU "${UNINST_KEY}"
        SetShellVarContext all
    ${EndIf}

    ; Write registry keys for Add/Remove Programs
    WriteRegStr SHCTX "Software\Microsoft\Windows\CurrentVersion\Uninstall\{{bundle_id}}" \
        "DisplayName" "yaiol ${PRODUCT_DISPLAY}"
    WriteRegStr SHCTX "Software\Microsoft\Windows\CurrentVersion\Uninstall\{{bundle_id}}" \
        "UninstallString" '"$INSTDIR\uninstall.exe"'
    WriteRegStr SHCTX "Software\Microsoft\Windows\CurrentVersion\Uninstall\{{bundle_id}}" \
        "DisplayVersion" "{{version}}"
    {{#if publisher}}
    WriteRegStr SHCTX "Software\Microsoft\Windows\CurrentVersion\Uninstall\{{bundle_id}}" \
        "Publisher" "{{publisher}}"
    {{/if}}
    WriteRegStr SHCTX "Software\Microsoft\Windows\CurrentVersion\Uninstall\{{bundle_id}}" \
        "InstallLocation" "$INSTDIR"
    ${If} $AllUsers == "1"
        WriteRegStr SHCTX "Software\Microsoft\Windows\CurrentVersion\Uninstall\{{bundle_id}}" \
            "InstallMode" "AllUsers"
    ${Else}
        WriteRegStr SHCTX "Software\Microsoft\Windows\CurrentVersion\Uninstall\{{bundle_id}}" \
            "InstallMode" "CurrentUser"
    ${EndIf}
    {{#if installer_icon}}
    WriteRegStr SHCTX "Software\Microsoft\Windows\CurrentVersion\Uninstall\{{bundle_id}}" \
        "DisplayIcon" "$INSTDIR\app.ico"
    {{/if}}

    ; Get installed size
    ${GetSize} "$INSTDIR" "/S=0K" $0 $1 $2
    IntFmt $0 "0x%08X" $0
    WriteRegDWORD SHCTX "Software\Microsoft\Windows\CurrentVersion\Uninstall\{{bundle_id}}" \
        "EstimatedSize" "$0"

    {{#if install_webview}}
    ; WebView2 installation
    {{webview_install_code}}
    {{/if}}

SectionEnd

{{#if installer_hooks}}
!include "{{installer_hooks}}"
{{/if}}

; The uninstaller removes what its install wrote: the machine's entries (with administrator rights,
; asked for here) when this folder is the all-users install, the user's own otherwise.
Function un.onInit
    SetRegView 64
    ReadRegStr $0 HKLM "${UNINST_KEY}" "InstallLocation"
    ${If} $0 == $INSTDIR
        UserInfo::GetAccountType
        Pop $1
        ${If} $1 != "Admin"
            StrCpy $2 ""
            ${If} ${Silent}
                StrCpy $2 "/S"
            ${EndIf}
            ClearErrors
            ExecShell "runas" "$INSTDIR\uninstall.exe" "$2"
            ${If} ${Errors}
                Abort
            ${EndIf}
            Quit
        ${EndIf}
        SetShellVarContext all
    ${Else}
        SetShellVarContext current
    ${EndIf}
FunctionEnd

; Uninstaller section
Section "Uninstall"
    ; Remove files
    RMDir /r "$INSTDIR"

    ; Remove this app's Start Menu shortcuts. ⚠ NOT `RMDir /r` on the folder as dx's script
    ; does - the folder is "yaiol", shared by every yaiol app, so a recursive remove deleted the
    ; others' shortcuts. A plain RMDir only removes it once nothing else is left inside.
    Delete "$SMPROGRAMS\{{start_menu_folder}}\${PRODUCT_DISPLAY}.lnk"
    Delete "$SMPROGRAMS\{{start_menu_folder}}\Uninstall ${PRODUCT_DISPLAY}.lnk"
    RMDir "$SMPROGRAMS\{{start_menu_folder}}"

    ; Remove Desktop shortcut
    Delete "$DESKTOP\${PRODUCT_DISPLAY}.lnk"

    ; Remove registry keys
    DeleteRegKey SHCTX "Software\Microsoft\Windows\CurrentVersion\Uninstall\{{bundle_id}}"
SectionEnd
