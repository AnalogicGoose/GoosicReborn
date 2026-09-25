#ifndef AppVersion
  #define AppVersion "0.2.2"
#endif

[Setup]
AppId={{B676925B-C362-48FD-8486-1A0735400D53}
AppName=Goosic
AppVersion={#AppVersion}
AppPublisher=AnalogicGoose
AppPublisherURL=https://github.com/AnalogicGoose/GoosicReborn
DefaultDirName={localappdata}\Programs\Goosic
DefaultGroupName=Goosic
PrivilegesRequired=lowest
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
MinVersion=10.0.17763
OutputDir=..\..\dist
OutputBaseFilename=Goosic-{#AppVersion}-windows-x64-setup
Compression=lzma2
SolidCompression=yes
WizardStyle=modern
UninstallDisplayIcon={app}\Goosic.Windows.exe
LicenseFile=..\..\LICENSE-GPL-3.0
CloseApplications=yes
RestartApplications=no

[Tasks]
Name: desktopicon; Description: "Create a desktop shortcut"; Flags: unchecked

[Files]
Source: "..\..\dist\Goosic-{#AppVersion}-windows-x64\*"; DestDir: "{app}"; Flags: ignoreversion recursesubdirs createallsubdirs
Source: "..\..\dist\installer-tools\MicrosoftEdgeWebview2Setup.exe"; Flags: dontcopy

[Icons]
Name: "{group}\Goosic"; Filename: "{app}\Goosic.Windows.exe"
Name: "{userdesktop}\Goosic"; Filename: "{app}\Goosic.Windows.exe"; Tasks: desktopicon

[Run]
Filename: "{app}\Goosic.Windows.exe"; Description: "Launch Goosic"; Flags: nowait postinstall skipifsilent
; The in-app updater runs Setup silently with /RELAUNCH=1 after closing Goosic, and expects it
; back once the new files are in place.
Filename: "{app}\Goosic.Windows.exe"; Flags: nowait; Check: RelaunchAfterUpdate

[Code]
function RelaunchAfterUpdate: Boolean;
begin
  Result := WizardSilent and (ExpandConstant('{param:RELAUNCH|0}') = '1');
end;

function HasWebView2: Boolean;
var Version: String;
begin
  Result := (RegQueryStringValue(HKLM32, 'SOFTWARE\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}', 'pv', Version) or
    RegQueryStringValue(HKCU, 'SOFTWARE\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}', 'pv', Version)) and
    (Version <> '') and (Version <> '0.0.0.0');
end;

function PrepareToInstall(var NeedsRestart: Boolean): String;
var ExitCode: Integer;
begin
  Result := '';
  if not HasWebView2 then begin
    ExtractTemporaryFile('MicrosoftEdgeWebview2Setup.exe');
    if not Exec(ExpandConstant('{tmp}\MicrosoftEdgeWebview2Setup.exe'), '/silent /install', '', SW_HIDE, ewWaitUntilTerminated, ExitCode) then
      Result := 'Microsoft WebView2 could not be started. Install the WebView2 Evergreen Runtime and run Setup again.'
    else if not HasWebView2 then
      Result := 'Microsoft WebView2 could not be installed. Check your internet connection and run Setup again.';
  end;
end;
