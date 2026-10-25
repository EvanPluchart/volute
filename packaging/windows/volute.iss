; Volute installer (decision 0005): per user, no administrator rights, into
; %LOCALAPPDATA%\Programs\Volute. Built by `cargo xtask dist` with Inno Setup 6:
;   ISCC.exe /DAppVersion=0.1.0 /DSourceExe=<path to volute.exe> /DOutputDir=<dist> volute.iss

#ifndef AppVersion
  #define AppVersion "0.0.0"
#endif
#ifndef SourceExe
  #define SourceExe "..\..\target\release\volute.exe"
#endif
#ifndef OutputDir
  #define OutputDir "..\..\dist"
#endif

[Setup]
; The updater finds this install by this id (volute-shell, update_windows.rs): never change it.
AppId={{F3BC62B8-CB24-48C6-9729-63959EDB49D4}
AppName=Volute
AppVersion={#AppVersion}
AppVerName=Volute {#AppVersion}
AppPublisher=Evan Pluchart
AppPublisherURL=https://evanpluchart.github.io/volute/
AppSupportURL=https://github.com/EvanPluchart/volute/issues
AppUpdatesURL=https://github.com/EvanPluchart/volute/releases
PrivilegesRequired=lowest
DefaultDirName={userpf}\Volute
DisableDirPage=yes
DefaultGroupName=Volute
DisableProgramGroupPage=yes
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
SetupIconFile=..\..\assets\brand\icons\volute.ico
UninstallDisplayIcon={app}\volute.exe
UninstallDisplayName=Volute
OutputDir={#OutputDir}
OutputBaseFilename=Volute-{#AppVersion}-setup
Compression=lzma2/max
SolidCompression=yes
WizardStyle=modern
; Waits for a running Volute (its single instance) and never closes one itself: Volute quits on
; its own before an update from the app, and a manual install asks the user to quit it.
AppMutex=Local\Volute.SingleInstance
CloseApplications=no
RestartApplications=no
LicenseFile=..\..\LICENSE

[Languages]
Name: "en"; MessagesFile: "compiler:Default.isl"
Name: "fr"; MessagesFile: "compiler:Languages\French.isl"

[CustomMessages]
en.StartWithWindows=Start Volute when I sign in
fr.StartWithWindows=Démarrer Volute à l'ouverture de session
en.LaunchVolute=Launch Volute
fr.LaunchVolute=Lancer Volute
en.SettingsKept=Your settings and remembered volumes are kept in %APPDATA%\Volute. Delete that folder to remove them too.
fr.SettingsKept=Vos réglages et les volumes mémorisés sont conservés dans %APPDATA%\Volute. Supprimez ce dossier pour les effacer aussi.

[Tasks]
Name: "startup"; Description: "{cm:StartWithWindows}"
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked

[Files]
Source: "{#SourceExe}"; DestDir: "{app}"; DestName: "volute.exe"; Flags: ignoreversion
Source: "..\..\LICENSE"; DestDir: "{app}"; DestName: "LICENSE.txt"; Flags: ignoreversion

[Icons]
Name: "{userprograms}\Volute"; Filename: "{app}\volute.exe"
Name: "{userdesktop}\Volute"; Filename: "{app}\volute.exe"; Tasks: desktopicon

[Registry]
; Only the value this installer creates (Settings in Volute changes the same one); removed on
; uninstall. Never written by an update from the app: Settings decides from then on.
Root: HKCU; Subkey: "Software\Microsoft\Windows\CurrentVersion\Run"; ValueType: string; ValueName: "Volute"; ValueData: """{app}\volute.exe"""; Flags: uninsdeletevalue; Tasks: startup; Check: not RelaunchAsked

[Run]
Filename: "{app}\volute.exe"; Parameters: "--open"; Description: "{cm:LaunchVolute}"; Flags: nowait postinstall skipifsilent
; An update from the app runs silently and asks for Volute to start again.
Filename: "{app}\volute.exe"; Parameters: "--updated"; Flags: nowait; Check: RelaunchAsked

[UninstallDelete]
Type: dirifempty; Name: "{app}"

[Code]
const
  VoluteMutex = 'Local\Volute.SingleInstance';

{ Started by Volute's "Update" button (decision 0005). }
function RelaunchAsked(): Boolean;
begin
  Result := ExpandConstant('{param:relaunch|0}') = '1';
end;

{ An update from the app: Volute quits right after starting this installer, so give it a few
  seconds. Another Volute still running cannot be closed from here: stop, and start the old
  Volute again rather than leave the user with none. }
function InitializeSetup(): Boolean;
var
  Waited: Integer;
begin
  Result := True;
  if RelaunchAsked() then
  begin
    Waited := 0;
    while CheckForMutexes(VoluteMutex) and (Waited < 10000) do
    begin
      Sleep(250);
      Waited := Waited + 250;
    end;
    Result := not CheckForMutexes(VoluteMutex);
    if not Result then
      Exec(ExpandConstant('{userpf}\Volute\volute.exe'), '', '', SW_SHOWNORMAL, ewNoWait, Waited);
  end;
end;

procedure CurUninstallStepChanged(CurUninstallStep: TUninstallStep);
begin
  if (CurUninstallStep = usPostUninstall) and not UninstallSilent() then
    MsgBox(CustomMessage('SettingsKept'), mbInformation, MB_OK);
end;
