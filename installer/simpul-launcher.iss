; Simpul Creative Launcher — Windows Installer (Inno Setup 6)
; Single-file installer, tanpa admin (per-user), pasang ke %LOCALAPPDATA%\Programs\Simpul Creative Launcher.

#ifndef AppVersion
  #define AppVersion "1.6.3"
#endif
#ifndef Root
  #define Root ".."
#endif
#ifndef Exe
  #define Exe Root + "\target\release\simpul-creative-launcher.exe"
#endif
#ifndef OutDir
  #define OutDir Root + "\dist"
#endif

[Setup]
AppId={{9A4D2F81-6E3B-4C7A-B102-8F4E9D2C1A55}
AppName=Simpul Creative Launcher
AppVersion={#AppVersion}
AppVerName=Simpul Creative Launcher {#AppVersion}
AppPublisher=Simpul Project (PT Sahabat Berkah Bahagia)
DefaultDirName={localappdata}\Programs\Simpul Creative Launcher
DefaultGroupName=Simpul Creative Launcher
DisableProgramGroupPage=yes
PrivilegesRequired=lowest
OutputDir={#OutDir}
OutputBaseFilename=Simpul-Creative-Launcher-Setup-{#AppVersion}
SetupIconFile={#Root}\assets\simpul-launcher.ico
UninstallDisplayIcon={app}\assets\simpul-launcher.ico
Compression=lzma2/ultra64
SolidCompression=yes
WizardStyle=modern
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
CloseApplications=yes
RestartApplications=no
ChangesEnvironment=yes

[Languages]
#if FileExists(CompilerPath + "\Languages\Indonesian.isl")
Name: "id"; MessagesFile: "compiler:Languages\Indonesian.isl"
#endif
Name: "en"; MessagesFile: "compiler:Default.isl"

[Tasks]
Name: "desktopicon"; Description: "Buat ikon di Desktop"; GroupDescription: "Pintasan:"
Name: "addpath"; Description: "Tambahkan CLI ke User PATH (untuk Claude, Antigravity agy & terminal)"; GroupDescription: "Integrasi CLI:"

[Files]
Source: "{#Exe}"; DestDir: "{app}"; DestName: "Simpul Creative Launcher.exe"; Flags: ignoreversion
Source: "{#Root}\target\release\simpul-creative-launcher-cli.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#Root}\installer\simpul-creative-launcher.cmd"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#Root}\assets\simpul-launcher.ico"; DestDir: "{app}\assets"; Flags: ignoreversion
Source: "{#Root}\assets\simpul-icon.png"; DestDir: "{app}\assets"; Flags: ignoreversion
Source: "{#Root}\assets\simpul-splash.png"; DestDir: "{app}\assets"; Flags: ignoreversion
Source: "{#Root}\assets\simpul-hero-banner.png"; DestDir: "{app}\assets"; Flags: ignoreversion
Source: "{#Root}\README.md"; DestDir: "{app}\docs"; Flags: ignoreversion
Source: "{#Root}\CHANGELOG.md"; DestDir: "{app}\docs"; Flags: ignoreversion
Source: "{#Root}\NOTICE"; DestDir: "{app}\docs"; Flags: ignoreversion
Source: "{#Root}\licenses\*"; DestDir: "{app}\docs\licenses"; Flags: ignoreversion recursesubdirs
Source: "{#Root}\CLAUDE.md"; DestDir: "{app}\docs"; Flags: ignoreversion
Source: "{#Root}\examples\*"; DestDir: "{app}\examples"; Flags: ignoreversion recursesubdirs

[Icons]
Name: "{group}\Simpul Creative Launcher"; Filename: "{app}\Simpul Creative Launcher.exe"; WorkingDir: "{app}"; IconFilename: "{app}\assets\simpul-launcher.ico"
Name: "{group}\Panduan Claude & CLI"; Filename: "{app}\docs\CLAUDE.md"
Name: "{group}\Uninstall Simpul Creative Launcher"; Filename: "{uninstallexe}"
Name: "{autodesktop}\Simpul Creative Launcher"; Filename: "{app}\Simpul Creative Launcher.exe"; WorkingDir: "{app}"; IconFilename: "{app}\assets\simpul-launcher.ico"; Tasks: desktopicon

[Registry]
Root: HKCU; Subkey: "Environment"; ValueType: expandsz; ValueName: "Path"; ValueData: "{olddata};{app}"; Tasks: addpath; Check: NeedsAddPath(ExpandConstant('{app}'))

[Run]
Filename: "{app}\Simpul Creative Launcher.exe"; Description: "Jalankan Simpul Creative Launcher sekarang"; Flags: nowait postinstall skipifsilent

[Code]
function NeedsAddPath(Param: string): boolean;
var
  OrigPath: string;
begin
  if not RegQueryStringValue(HKEY_CURRENT_USER, 'Environment', 'Path', OrigPath) then
  begin
    Result := True;
    exit;
  end;
  Result := Pos(';' + Uppercase(Param) + ';', ';' + Uppercase(OrigPath) + ';') = 0;
end;

{ Saat dicopot, folder aplikasi dikeluarkan lagi dari PATH pengguna. }
procedure RemoveFromUserPath(Dir: string);
var
  OrigPath, Padded, Needle: string;
  P: Integer;
begin
  if not RegQueryStringValue(HKEY_CURRENT_USER, 'Environment', 'Path', OrigPath) then
    exit;
  Padded := ';' + OrigPath + ';';
  Needle := ';' + Uppercase(Dir) + ';';
  P := Pos(Needle, Uppercase(Padded));
  if P = 0 then
    exit;
  Delete(Padded, P, Length(Needle) - 1);
  if (Length(Padded) > 0) and (Padded[1] = ';') then
    Delete(Padded, 1, 1);
  if (Length(Padded) > 0) and (Padded[Length(Padded)] = ';') then
    Delete(Padded, Length(Padded), 1);
  RegWriteExpandStringValue(HKEY_CURRENT_USER, 'Environment', 'Path', Padded);
end;

procedure CurUninstallStepChanged(CurUninstallStep: TUninstallStep);
begin
  if CurUninstallStep = usPostUninstall then
    RemoveFromUserPath(ExpandConstant('{app}'));
end;
