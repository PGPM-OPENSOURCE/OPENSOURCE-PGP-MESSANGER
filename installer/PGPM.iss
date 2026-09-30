; PGPM setup wizard (Inno Setup 6), bundled with the official GnuPG installer.
; Build from the repo root, after PyInstaller has produced dist\PGPM.exe:
;   iscc /DAppVersion=0.5.7 /DGnuPGSetup=gnupg-w32-2.5.24_20260923.exe installer\PGPM.iss
; installer\redist\<GnuPGSetup> must be the signature-verified file from gnupg.org.
; Output: dist\PGPM-Setup.exe
; /DTEST_NO_GNUPG builds a test variant that pretends GnuPG is missing and runs a
; 3-second dummy instead of the GnuPG installer (never ship it).

#ifndef AppVersion
  #define AppVersion "0.0.0"
#endif
#ifndef GnuPGSetup
  #error Pass /DGnuPGSetup=gnupg-w32-<version>_<date>.exe
#endif
; "gnupg-w32-2.5.24_20260923.exe" -> "2.5.24"
#define GnuPGVersion Copy(GnuPGSetup, 11, Pos("_", GnuPGSetup) - 11)

[Setup]
AppId={{15AF3C24-BB7E-4813-BA04-23D9BD3EA356}
AppName=PGPM
AppVersion={#AppVersion}
AppVerName=PGPM {#AppVersion}
AppPublisher=PGPM
AppPublisherURL=https://github.com/PGPM-OPENSOURCE/OPENSOURCE-PGP-MESSANGER
; per-user install (no admin); only the GnuPG step asks for elevation
PrivilegesRequired=lowest
DefaultDirName={autopf}\PGPM
DisableProgramGroupPage=yes
DisableWelcomePage=no
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
; a running PGPM holds this mutex — Setup asks the user to close it first
AppMutex=PGPM_single_instance_mutex
OutputDir=..\dist
OutputBaseFilename=PGPM-Setup
SetupIconFile=..\assets\icon.ico
UninstallDisplayIcon={app}\PGPM.exe
UninstallDisplayName=PGPM
WizardStyle=modern dark
; no wizard logo images (empty = hidden)
WizardImageFile=
WizardSmallImageFile=
Compression=lzma2/max
SolidCompression=yes

[Tasks]
Name: "desktopicon"; Description: "Create a desktop icon"; Flags: unchecked
Name: "gnupg"; Description: "Install GnuPG {#GnuPGVersion} (required)"; Flags: unchecked

[Files]
Source: "..\dist\PGPM.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "redist\{#GnuPGSetup}"; Flags: dontcopy

[Icons]
Name: "{autoprograms}\PGPM"; Filename: "{app}\PGPM.exe"
Name: "{autodesktop}\PGPM"; Filename: "{app}\PGPM.exe"; Tasks: desktopicon

[Code]
var
  GnuPGFound: Boolean;
  GnuPGPage: TOutputProgressWizardPage;

// GnuPG's registered install dir (Gpg4win registers it too), else a default location
function FindGnuPG(): String;
var
  Dir: String;
begin
  Result := '';
#ifdef TEST_NO_GNUPG
  exit;
#endif
  if RegQueryStringValue(HKLM64, 'SOFTWARE\GnuPG', 'Install Directory', Dir) and
     FileExists(AddBackslash(Dir) + 'bin\gpg.exe') then begin
    Result := Dir; exit;
  end;
  if RegQueryStringValue(HKLM32, 'SOFTWARE\GnuPG', 'Install Directory', Dir) and
     FileExists(AddBackslash(Dir) + 'bin\gpg.exe') then begin
    Result := Dir; exit;
  end;
  if FileExists(ExpandConstant('{commonpf64}\GnuPG\bin\gpg.exe')) then
    Result := ExpandConstant('{commonpf64}\GnuPG')
  else if FileExists(ExpandConstant('{commonpf32}\GnuPG\bin\gpg.exe')) then
    Result := ExpandConstant('{commonpf32}\GnuPG');
end;

function GnuPGItem(): Integer;
var
  i: Integer;
begin
  Result := -1;
  for i := 0 to WizardForm.TasksList.Items.Count - 1 do
    if Pos('GnuPG', WizardForm.TasksList.ItemCaption[i]) > 0 then begin
      Result := i; exit;
    end;
end;

function GnuPGChecked(): Boolean;
var
  i: Integer;
begin
  i := GnuPGItem();
  Result := (i >= 0) and WizardForm.TasksList.Checked[i];
end;

// Next stays disabled on the options page until "Install GnuPG" is checked
procedure TasksClickCheck(Sender: TObject);
begin
  if WizardForm.CurPageID = wpSelectTasks then
    WizardForm.NextButton.Enabled := GnuPGChecked();
end;

// no logo on the Welcome / Finished pages: let their text use the whole page width
procedure SpanPage(L: TNewStaticText; Page: TNewNotebookPage);
begin
  L.Left := ScaleX(32);
  L.Width := Page.ClientWidth - ScaleX(64);
end;

procedure InitializeWizard();
begin
  WizardForm.WizardBitmapImage.Visible := False;
  WizardForm.WizardBitmapImage2.Visible := False;
  SpanPage(WizardForm.WelcomeLabel1, WizardForm.WelcomePage);
  SpanPage(WizardForm.WelcomeLabel2, WizardForm.WelcomePage);
  SpanPage(WizardForm.FinishedHeadingLabel, WizardForm.FinishedPage);
  SpanPage(WizardForm.FinishedLabel, WizardForm.FinishedPage);
  GnuPGFound := FindGnuPG() <> '';
  WizardForm.TasksList.OnClickCheck := @TasksClickCheck;
  GnuPGPage := CreateOutputProgressPage('Installing GnuPG',
    'PGPM uses GnuPG for all encryption and for hardware keys.');
end;

procedure CurPageChanged(CurPageID: Integer);
var
  i: Integer;
begin
  if CurPageID = wpSelectTasks then begin
    i := GnuPGItem();
    if GnuPGFound and (i >= 0) then begin
      WizardForm.TasksList.ItemCaption[i] := 'GnuPG is already installed';
      WizardForm.TasksList.Checked[i] := True;
      WizardForm.TasksList.ItemEnabled[i] := False;
    end;
    WizardForm.NextButton.Enabled := GnuPGChecked();
  end;
end;

function NextButtonClick(CurPageID: Integer): Boolean;
begin
  Result := True;
  if (CurPageID = wpSelectTasks) and not GnuPGFound and not GnuPGChecked() then begin
    SuppressibleMsgBox('PGPM needs GnuPG. Check "Install GnuPG" to continue.',
      mbInformation, MB_OK, IDOK);
    Result := False;
  end;
end;

// Ready page summary: don't claim GnuPG will be installed when it's already there
function UpdateReadyMemo(Space, NewLine, MemoUserInfoInfo, MemoDirInfo, MemoTypeInfo,
  MemoComponentsInfo, MemoGroupInfo, MemoTasksInfo: String): String;
var
  Tasks: String;
  Parts: array of String;
  i: Integer;
begin
  Tasks := MemoTasksInfo;
  if GnuPGFound then
    StringChangeEx(Tasks, 'Install GnuPG {#GnuPGVersion} (required)',
                   'GnuPG is already installed', True);
  Parts := [MemoUserInfoInfo, MemoDirInfo, MemoTypeInfo, MemoComponentsInfo,
            MemoGroupInfo, Tasks];
  Result := '';
  for i := 0 to GetArrayLength(Parts) - 1 do
    if Parts[i] <> '' then begin
      if Result <> '' then
        Result := Result + NewLine + NewLine;
      Result := Result + Parts[i];
    end;
end;

// Runs the bundled GnuPG installer (elevated) before PGPM's files are copied;
// a non-empty result stops Setup with that message.
function PrepareToInstall(var NeedsRestart: Boolean): String;
var
  Code: Integer;
  Ok: Boolean;
begin
  Result := '';
  if GnuPGFound or not WizardIsTaskSelected('gnupg') then
    exit;
  GnuPGPage.SetText('Installing GnuPG {#GnuPGVersion}...',
    'Allow the Windows permission prompt if one appears.');
  GnuPGPage.Show;
  try
#ifdef TEST_NO_GNUPG
    Ok := Exec(ExpandConstant('{cmd}'), '/c ping -n 4 127.0.0.1 >nul', '', SW_HIDE,
               ewWaitUntilTerminated, Code);
    GnuPGFound := Ok;
#else
    ExtractTemporaryFile('{#GnuPGSetup}');
    Ok := ShellExec('runas', ExpandConstant('{tmp}\{#GnuPGSetup}'), '/S', '',
                    SW_SHOWNORMAL, ewWaitUntilTerminated, Code);
    GnuPGFound := Ok and (FindGnuPG() <> '');
#endif
  finally
    GnuPGPage.Hide;
  end;
  if not Ok then
    Result := 'GnuPG was not installed: ' + SysErrorMessage(Code) + #13#10#13#10 +
              'PGPM needs GnuPG. Run Setup again and allow the Windows permission prompt.'
  else if not GnuPGFound then
    Result := 'The GnuPG installer finished (exit code ' + IntToStr(Code) +
              ') but GnuPG could not be found.';
end;
