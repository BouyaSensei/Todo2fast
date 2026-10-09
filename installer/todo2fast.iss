; Todo2fast Inno Setup script
; Build: ISCC.exe todo2fast.iss  (Inno Setup 6)
; Produces: Output\Todo2fast-Setup-<version>.exe

#define MyAppName "Todo2fast"
; Version comes from the T2F_VERSION env var (set by CI from the git tag, e.g.
; v0.2.0 -> 0.2.0) so the installer name always matches the release tag. Falls
; back to 0.1.0 for local builds without that variable.
#if GetEnv("T2F_VERSION") == ""
  #define MyAppVersion "0.1.0"
#else
  #define MyAppVersion GetEnv("T2F_VERSION")
#endif
#define MyAppPublisher "BouyaSensei"
#define MyAppURL "https://github.com/BouyaSensei/Todo2fast"
#define MyAppExeName "todo2fast.exe"
; Path to the release binary (relative to this .iss file)
#define MyExePath "..\backend\target\release\todo2fast.exe"
; Path to the built frontend (SPA served by the backend)
#define MyWebPath "..\frontend\dist"

[Setup]
AppId={{8F1C4A7E-3D2B-4E6A-9C5F-A1B2C3D4E5F6}
AppName={#MyAppName}
AppVersion={#MyAppVersion}
AppVerName={#MyAppName} {#MyAppVersion}
AppPublisher={#MyAppPublisher}
AppPublisherURL={#MyAppURL}
AppSupportURL={#MyAppURL}
DefaultDirName={autopf}\{#MyAppName}
DefaultGroupName={#MyAppName}
DisableProgramGroupPage=yes
OutputDir=Output
OutputBaseFilename=Todo2fast-Setup-{#MyAppVersion}
Compression=lzma2
SolidCompression=yes
WizardStyle=modern
ArchitecturesInstallIn64BitMode=x64compatible
PrivilegesRequired=admin
UninstallDisplayIcon={app}\{#MyAppExeName}

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"
Name: "french"; MessagesFile: "compiler:Languages\French.isl"

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"

[Files]
Source: "{#MyExePath}"; DestDir: "{app}"; Flags: ignoreversion
; Frontend SPA — served by the backend from {app}\web (T2F_WEB_DIR fallback)
Source: "{#MyWebPath}\*"; DestDir: "{app}\web"; Flags: ignoreversion recursesubdirs createallsubdirs
; SQLite DB is created at first run in the user profile (portable per-user data)

[Icons]
Name: "{group}\{#MyAppName}"; Filename: "{app}\{#MyAppExeName}"
Name: "{group}\Uninstall {#MyAppName}"; Filename: "{uninstallexe}"
Name: "{autodesktop}\{#MyAppName}"; Filename: "{app}\{#MyAppExeName}"; Tasks: desktopicon

[Registry]
; Store install location for the PowerShell uninstaller / service tooling
Root: HKLM; Subkey: "Software\{#MyAppName}"; ValueType: string; ValueName: "InstallDir"; ValueData: "{app}"; Flags: uninsdeletevalue

[Run]
Filename: "{app}\{#MyAppExeName}"; Description: "{cm:LaunchProgram,{#StringChange(MyAppName, '&', '&&')}}"; Flags: nowait postinstall skipifsilent

[UninstallDelete]
; Remove user data (SQLite DB) on uninstall — opt-out by editing this line
Type: filesandordirs; Name: "{localappdata}\{#MyAppName}"
