#define AppName "ss-bridge"
#ifndef AppVersion
#define AppVersion "0.0.0"
#endif
#define AppExe "ss-bridge.exe"

[Setup]
AppId={{7C0B2E6A-4C4B-4D5F-9C3E-5A1B9F2D0E11}
AppName={#AppName}
AppVersion={#AppVersion}
AppPublisher=giuli
AppPublisherURL=https://ss.giuli.dev
DefaultDirName={autopf}\{#AppName}
DefaultGroupName={#AppName}
PrivilegesRequired=lowest
PrivilegesRequiredOverridesAllowed=dialog
OutputDir=..\dist
OutputBaseFilename=ss-bridge-windows-setup
SetupIconFile=app.ico
UninstallDisplayIcon={app}\{#AppExe}
Compression=lzma2
SolidCompression=yes
WizardStyle=modern
CloseApplications=yes
RestartApplications=no

[Languages]
Name: "brazilianportuguese"; MessagesFile: "compiler:Languages\BrazilianPortuguese.isl"
Name: "english"; MessagesFile: "compiler:Default.isl"

[Tasks]
Name: "autostart"; Description: "Abrir o ss-bridge ao iniciar o Windows"; GroupDescription: "Opções:"

[Files]
Source: "..\target\release\{#AppExe}"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{group}\{#AppName}"; Filename: "{app}\{#AppExe}"
Name: "{autodesktop}\{#AppName}"; Filename: "{app}\{#AppExe}"

[Registry]
Root: HKCU; Subkey: "Software\Microsoft\Windows\CurrentVersion\Run"; ValueType: string; ValueName: "{#AppName}"; ValueData: """{app}\{#AppExe}"""; Flags: uninsdeletevalue; Tasks: autostart

[Run]
Filename: "{app}\{#AppExe}"; Description: "Abrir o ss-bridge agora"; Flags: nowait postinstall skipifsilent
