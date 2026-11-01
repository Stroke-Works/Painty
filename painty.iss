; Inno Setup script for Painty.
; Build with: ISCC painty.iss   (or open it in Inno Setup and press F9)

; The version is passed by release.ps1 and the release workflow (/DMyAppVersion=...),
; read from Cargo.toml. This default is only used when building by hand.
#ifndef MyAppVersion
  #define MyAppVersion "0.2.4"
#endif

[Setup]
AppName=Painty
AppVersion={#MyAppVersion}
AppPublisher=Gtisseran
LicenseFile=LICENSE
DefaultDirName={autopf}\Painty
DefaultGroupName=Painty
OutputDir=dist
OutputBaseFilename=PaintySetup-{#MyAppVersion}
SetupIconFile=assets\painty.ico
UninstallDisplayIcon={app}\painty.exe
Compression=lzma2
SolidCompression=yes
ArchitecturesInstallIn64BitMode=x64compatible
; Installs for the current user only, no administrator prompt.
PrivilegesRequired=lowest

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"
Name: "french"; MessagesFile: "compiler:Languages\French.isl"

[Tasks]
Name: "desktopicon"; Description: "Create a desktop shortcut"; GroupDescription: "Additional icons:"

[Files]
Source: "target\release\painty.exe"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{group}\Painty"; Filename: "{app}\painty.exe"
Name: "{group}\Uninstall Painty"; Filename: "{uninstallexe}"
Name: "{autodesktop}\Painty"; Filename: "{app}\painty.exe"; Tasks: desktopicon

[Run]
Filename: "{app}\painty.exe"; Description: "Launch Painty"; Flags: nowait postinstall skipifsilent