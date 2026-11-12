; Portlight installer (Inno Setup 6).
;
; Built by .github/workflows/release.yml, or locally with installer\build.ps1:
;   iscc /DAppVersion=0.1.0 installer\portlight.iss
;
; Installs per user into %LOCALAPPDATA%\Programs\Portlight, so no admin prompt.
; The optional Portlight Collector service is installed later from inside the
; app, with one admin prompt (portlight-service.exe install), and removed by
; the uninstaller.
; A silent update can be run with:
;   Portlight-Setup-x.y.z.exe /VERYSILENT /SUPPRESSMSGBOXES /NORESTART /launch=--background

#ifndef AppVersion
  #define AppVersion "0.0.0"
#endif

; Must match "identifier" in src-tauri\tauri.conf.json. Windows uses it to
; show notifications under Portlight's name and icon.
#define AppUserModelId "com.portlight.app"

[Setup]
AppId={{B7C2F3A4-9D51-4E86-A3B7-2F6C1D8E4A90}
AppName=Portlight
AppVersion={#AppVersion}
AppVerName=Portlight {#AppVersion}
AppPublisher=PercyGFX
AppCopyright=Copyright (c) 2026 PercyGFX. MIT License.
AppPublisherURL=https://github.com/PercyGFX/Portlight
AppSupportURL=https://github.com/PercyGFX/Portlight/issues
AppUpdatesURL=https://github.com/PercyGFX/Portlight/releases
VersionInfoVersion={#AppVersion}
DefaultDirName={localappdata}\Programs\Portlight
DisableDirPage=yes
DisableProgramGroupPage=yes
DisableReadyPage=yes
PrivilegesRequired=lowest
OutputDir=Output
OutputBaseFilename=Portlight-Setup-{#AppVersion}
SetupIconFile=..\src-tauri\icons\icon.ico
UninstallDisplayIcon={app}\portlight.exe
UninstallDisplayName=Portlight
Compression=lzma2/max
SolidCompression=yes
WizardStyle=modern
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
; Windows 10 1809 or newer.
MinVersion=10.0.17763
CloseApplications=yes
RestartApplications=no

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"

[Tasks]
Name: "autostart"; Description: "Start Portlight when I sign in, so it can tell me about new apps"

[Files]
Source: "..\target\release\portlight.exe"; DestDir: "{app}"; Flags: ignoreversion
; The background service for detailed monitoring. Copied to Program Files when
; the person turns it on.
Source: "..\target\release\portlight-service.exe"; DestDir: "{app}"; Flags: ignoreversion
; Who owns each server's network, from iptoasn.com (public domain). Looked up on
; the PC; built by scripts\update-owners.ps1.
Source: "..\target\release\owners.bin"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\LICENSE"; DestDir: "{app}"; DestName: "LICENSE.txt"; Flags: ignoreversion

[Icons]
Name: "{autoprograms}\Portlight"; Filename: "{app}\portlight.exe"; AppUserModelID: "{#AppUserModelId}"

[Registry]
; Start with Windows, hidden in the tray. Removed again on uninstall.
Root: HKCU; Subkey: "Software\Microsoft\Windows\CurrentVersion\Run"; ValueType: string; ValueName: "Portlight"; ValueData: """{app}\portlight.exe"" --background"; Flags: uninsdeletevalue; Tasks: autostart

[Run]
; Normal install: offer to start Portlight (opens the window).
Filename: "{app}\portlight.exe"; Description: "Start Portlight"; Flags: nowait postinstall skipifsilent
; Silent update: start it again the way the updater asked.
Filename: "{app}\portlight.exe"; Parameters: "{param:launch|--background}"; Flags: nowait; Check: WizardSilent

[UninstallRun]
Filename: "{app}\portlight.exe"; Parameters: "--quit"; Flags: runhidden waituntilterminated; RunOnceId: "QuitPortlight"
; Removing the service needs administrator rights, so this asks once, and only
; if the service was turned on.
Filename: "{app}\portlight-service.exe"; Parameters: "uninstall"; Verb: "runas"; Flags: shellexec runhidden waituntilterminated; RunOnceId: "RemoveCollector"; Check: CollectorInstalled

[Code]
// Portlight draws its window with Microsoft's WebView2, which ships with
// Windows 11 and with Microsoft Edge on Windows 10.
function WebView2Installed: Boolean;
var
  Version: String;
begin
  Result :=
    (RegQueryStringValue(HKLM, 'SOFTWARE\WOW6432Node\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}', 'pv', Version)
      or RegQueryStringValue(HKCU, 'Software\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}', 'pv', Version))
    and (Version <> '') and (Version <> '0.0.0.0');
end;

function InitializeSetup: Boolean;
begin
  Result := True;
  if not WebView2Installed and not WizardSilent then
    Result := MsgBox(
      'Portlight needs the Microsoft Edge WebView2 Runtime, which was not found on this PC.' + #13#10 + #13#10 +
      'You can get it free from Microsoft:' + #13#10 +
      'https://developer.microsoft.com/microsoft-edge/webview2/' + #13#10 + #13#10 +
      'Install Portlight anyway?', mbConfirmation, MB_YESNO or MB_DEFBUTTON2) = IDYES;
end;

// True when the Portlight Collector service has been turned on.
function CollectorInstalled: Boolean;
begin
  Result := RegKeyExists(HKLM, 'SYSTEM\CurrentControlSet\Services\PortlightCollector');
end;

// Ask a running Portlight to close before its files are replaced.
function PrepareToInstall(var NeedsRestart: Boolean): String;
var
  ResultCode: Integer;
  Exe: String;
begin
  Exe := ExpandConstant('{app}\portlight.exe');
  if FileExists(Exe) then
    Exec(Exe, '--quit', '', SW_HIDE, ewWaitUntilTerminated, ResultCode);
  Result := '';
end;

procedure CurUninstallStepChanged(CurUninstallStep: TUninstallStep);
begin
  if (CurUninstallStep = usPostUninstall) and not UninstallSilent then
    if MsgBox('Also delete the list of apps Portlight has seen?', mbConfirmation, MB_YESNO or MB_DEFBUTTON2) = IDYES then
      DelTree(ExpandConstant('{userappdata}\com.portlight.app'), True, True, True);
end;
