#define MyAppName "Voice Watch"
#define MyAppExeName "voice-watch.exe"
#ifndef MyAppVersion
#define MyAppVersion "0.1.0"
#endif
#ifndef ExtensionId
#define ExtensionId ""
#endif

[Setup]
AppId={{3F8743C8-3C3F-48F5-80F4-8C2DFEE4D91F}
AppName={#MyAppName}
AppVersion={#MyAppVersion}
AppVerName={#MyAppName} {#MyAppVersion}
AppPublisher=Voice Watch contributors
AppPublisherURL=https://github.com/Qxshio/VoiceWatch
AppSupportURL=https://github.com/Qxshio/VoiceWatch/issues
AppUpdatesURL=https://github.com/Qxshio/VoiceWatch/releases
DefaultDirName={localappdata}\Programs\Voice Watch
DefaultGroupName=Voice Watch
AllowNoIcons=yes
LicenseFile=..\LICENSE
OutputDir=..\dist
OutputBaseFilename=VoiceWatch-{#MyAppVersion}-Setup
Compression=lzma2
SolidCompression=yes
WizardStyle=modern dynamic
UninstallDisplayIcon={app}\{#MyAppExeName}
PrivilegesRequired=lowest
DisableWelcomePage=no

[Tasks]
Name: "desktopicon"; Description: "Create a &desktop shortcut"; GroupDescription: "Additional shortcuts:"; Flags: unchecked
#if ExtensionId != ""
Name: "nativehost"; Description: "Register the browser connector for supported Chromium browsers"; GroupDescription: "Browser integration:"; Flags: checkedonce
#endif

[Files]
Source: "..\target\release\voice-watch.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\README.md"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\LICENSE"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\extension\*"; DestDir: "{app}\extension"; Flags: ignoreversion recursesubdirs createallsubdirs
Source: "..\scripts\register-native-host.ps1"; DestDir: "{app}\scripts"; Flags: ignoreversion
Source: "..\scripts\unregister-native-host.ps1"; DestDir: "{app}\scripts"; Flags: ignoreversion

[Icons]
Name: "{group}\{#MyAppName}"; Filename: "{app}\{#MyAppExeName}"; WorkingDir: "{app}"
Name: "{group}\Uninstall {#MyAppName}"; Filename: "{uninstallexe}"
Name: "{userdesktop}\{#MyAppName}"; Filename: "{app}\{#MyAppExeName}"; WorkingDir: "{app}"; Tasks: desktopicon

[Run]
#if ExtensionId != ""
Filename: "powershell.exe"; Parameters: "-NoProfile -ExecutionPolicy Bypass -File ""{app}\scripts\register-native-host.ps1"" -ExtensionId ""{#ExtensionId}"" -ExePath ""{app}\{#MyAppExeName}"" -Browser All"; Flags: runhidden; Tasks: nativehost
#endif
Filename: "{app}\{#MyAppExeName}"; Description: "Launch {#MyAppName}"; Flags: nowait postinstall skipifsilent unchecked

[Registry]
Root: HKCU; Subkey: "Software\Classes\voice-watch"; ValueType: string; ValueData: "URL:Voice Watch"; Flags: uninsdeletekey
Root: HKCU; Subkey: "Software\Classes\voice-watch"; ValueType: string; ValueName: "URL Protocol"; ValueData: ""
Root: HKCU; Subkey: "Software\Classes\voice-watch\shell\open\command"; ValueType: string; ValueData: """{app}\{#MyAppExeName}"" ""%1"""
Root: HKCU; Subkey: "Software\Microsoft\Windows\CurrentVersion\Run"; ValueType: string; ValueName: "{#MyAppName}"; ValueData: """{app}\{#MyAppExeName}"""; Flags: uninsdeletevalue

[Code]
procedure StopRunningVoiceWatch();
var
  ResultCode: Integer;
begin
  Exec(
    ExpandConstant('{cmd}'),
    '/C taskkill /IM voice-watch.exe /F /T >NUL 2>NUL',
    '',
    SW_HIDE,
    ewWaitUntilTerminated,
    ResultCode
  );
end;

function PrepareToInstall(var NeedsRestart: Boolean): String;
begin
  StopRunningVoiceWatch();
  Result := '';
end;

#if ExtensionId != ""
[UninstallRun]
Filename: "powershell.exe"; Parameters: "-NoProfile -ExecutionPolicy Bypass -File ""{app}\scripts\unregister-native-host.ps1"" -Browser All -RemoveManifest"; Flags: runhidden
#endif
