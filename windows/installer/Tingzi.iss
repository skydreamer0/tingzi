; 聽字・注音與語音輸入法：Windows 安裝程式（Inno Setup 6）。
; 不直接編譯這個檔，請用 windows/build_installer.py：它先核對發行 ZIP 的檔案清單，
; 再以 /DAppVersion、/DPayloadZip、/DOutputDir 呼叫 ISCC。
;
; 安裝程式只是把發行 ZIP 的內容交給既有的 InstallTingzi.cmd：註冊、升級與保留資料的規則都沿用它與
; TingziVersion.exe，這裡不重寫。安裝程式另外負責：WebView2 Runtime、在「設定 → 應用程式」登記，
; 以及解除安裝時詢問是否保留個人資料。

#ifndef AppVersion
  #error 請以 /DAppVersion=<版號> 指定版本
#endif
#ifndef PayloadZip
  #error 請以 /DPayloadZip=<發行 ZIP> 指定內容
#endif
#ifndef OutputDir
  #define OutputDir "."
#endif

#define AppName "聽字・注音與語音輸入法"
#define ServiceClsid "{B6ECF4B2-9955-4C24-9BA8-3F7641F0792B}"
#define WebView2Client "{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}"

[Setup]
; AppId 決定「設定 → 應用程式」裡的那一筆，發行後不可再改。
AppId={{D8B8670C-C282-48DE-8673-AFC4631934BA}
AppName={#AppName}
AppVersion={#AppVersion}
AppVerName={#AppName} {#AppVersion}
AppPublisher=Tingzi contributors
AppPublisherURL=https://github.com/skydreamer0/tingzi
AppSupportURL=https://github.com/skydreamer0/tingzi/issues
AppUpdatesURL=https://github.com/skydreamer0/tingzi/releases
VersionInfoVersion={#AppVersion}
VersionInfoProductName={#AppName}
; 安裝位置由產品識別規定（%ProgramFiles%\Tingzi），不讓使用者改。
DefaultDirName={commonpf64}\Tingzi
DisableDirPage=yes
DisableProgramGroupPage=yes
DisableWelcomePage=no
UsePreviousAppDir=no
PrivilegesRequired=admin
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
MinVersion=10.0
WizardStyle=modern
SetupIconFile=..\tingzi.ico
UninstallDisplayIcon={app}\tingzi.ico
UninstallDisplayName={#AppName}
OutputDir={#OutputDir}
OutputBaseFilename=Tingzi-Setup-{#AppVersion}-x64
Compression=lzma2/max
SolidCompression=yes
; 安裝程式只在 {app} 放圖示與解除安裝程式；其餘檔案由 InstallTingzi.cmd 放進版本子目錄，
; 正在使用的舊版不會被覆蓋，所以不需要關閉其他程式。
CloseApplications=no
RestartIfNeededByRun=no
; 解除安裝時可選擇刪除目前使用者的個人資料。
UsedUserAreasWarning=no
; 一律寫安裝記錄到 %TEMP%（檔名「Setup Log <日期> #<編號>.txt」），在別人電腦上裝不起來時可以請對方傳回來。
SetupLogging=yes

[Languages]
Name: "zh_TW"; MessagesFile: "ChineseTraditional.isl"

[Messages]
zh_TW.WelcomeLabel2=這會在電腦上安裝 [name/ver]。%n%n裝好後按 Win＋空白鍵即可切換到聽字的注音，按錄音快捷鍵即可語音輸入。已經裝過聽字的話，這會直接升級，偏好、詞表、學習資料與已下載的模型都會保留。

[Files]
Source: "..\tingzi.ico"; DestDir: "{app}"; Flags: ignoreversion
; 發行 ZIP 原封不動帶著，在 PrepareToInstall 解開並執行 InstallTingzi.cmd；註冊失敗時安裝程式會停下，不留下任何登記。
Source: "{#PayloadZip}"; DestName: "payload.zip"; Flags: dontcopy

[Run]
Filename: "{app}\v{#AppVersion}\TingziFloat\TingziFloat.exe"; Description: "開啟語音浮窗"; Flags: postinstall nowait skipifsilent runasoriginaluser; Check: IsFirstInstall
; 升級時若語音浮窗原本開著（例如從設定的「關於」頁更新），安裝完成後開啟新版。
Filename: "{app}\v{#AppVersion}\TingziFloat\TingziFloat.exe"; Flags: nowait runasoriginaluser; Check: FloatWasRunning

[UninstallRun]
; 先關掉語音浮窗，再用既有的移除腳本取消註冊並刪除捷徑。
Filename: "{sys}\taskkill.exe"; Parameters: "/f /im TingziFloat.exe"; Flags: runhidden; RunOnceId: "StopFloat"
Filename: "{cmd}"; Parameters: "/c """"{app}\RemoveTingzi.cmd"" /quiet"""; Flags: runhidden; RunOnceId: "RemoveTingzi"

[Code]
var
  WasInstalled: Boolean;
  DownloadPage: TDownloadWizardPage;
  InstallProgressPage: TOutputMarqueeProgressWizardPage;
  DeletePersonalData: Boolean;
  FloatWasRunning_: Boolean;
  UninstallRestartRequired: Boolean;

function MoveFileExForUninstall(ExistingName: String; NewName: Integer; Flags: Cardinal): Boolean;
  external 'MoveFileExW@kernel32.dll stdcall uninstallonly';

function UninstallFileAttributes(Name: String): Cardinal;
  external 'GetFileAttributesW@kernel32.dll stdcall uninstallonly';

function UninstallTickCount: Cardinal;
  external 'GetTickCount@kernel32.dll stdcall uninstallonly';

function UninstallRootIsLink: Boolean;
begin
  Result := UninstallFileAttributes(ExpandConstant('{app}')) and $400 <> 0;
  if Result then
    Log('Leaving reparse-point install root and its target untouched: ' + ExpandConstant('{app}'));
end;

function InitializeUninstall: Boolean;
begin
  Result := not UninstallRootIsLink;
end;

function IsProgramVersion(const Name: String): Boolean;
var
  I, Dots, Digits: Integer;
begin
  Result := False;
  if (Length(Name) < 6) or (Name[1] <> 'v') then exit;
  Dots := 0;
  Digits := 0;
  for I := 2 to Length(Name) do
    if Name[I] = '.' then
    begin
      if Digits = 0 then exit;
      Dots := Dots + 1;
      Digits := 0;
    end
    else if (Name[I] >= '0') and (Name[I] <= '9') then
      Digits := Digits + 1
    else
      exit;
  Result := (Dots = 2) and (Digits > 0);
end;

function IsProgramRootFile(const Name: String): Boolean;
var
  LowerName, Extension: String;
  PendingSuffix: Integer;
begin
  LowerName := Lowercase(Name);
  PendingSuffix := Pos('.tingzi-delete-', LowerName);
  if PendingSuffix > 0 then
    LowerName := Copy(LowerName, 1, PendingSuffix - 1);
  Result := Pos('|' + LowerName + '|',
    '|tingzisetup64.exe|tingzisetup32.exe|tingzitextservice64.dll|tingzitextservice32.dll|' +
    'readme.md|removetingzi.cmd|voiceshortcuts.ps1|tingzi.cin|phrases.txt|' +
    'mcbopomofo-license.txt|acknowledgements.md|tingzi.ico|tingzipad.exe|' +
    'tingzikeyboard.exe|tingzilearning.exe|tingziversion.exe|installtingzi.cmd|' +
    'tingzi.svg|pairs.txt|pairs-notice.txt|third-party-notices.txt|') > 0;
  if Result or (Length(LowerName) <> 12) then exit;
  Extension := Copy(LowerName, 9, 4);
  Result := (Copy(LowerName, 1, 5) = 'unins') and
    (LowerName[6] >= '0') and (LowerName[6] <= '9') and
    (LowerName[7] >= '0') and (LowerName[7] <= '9') and
    (LowerName[8] >= '0') and (LowerName[8] <= '9') and
    ((Extension = '.exe') or (Extension = '.dat') or (Extension = '.msg'));
end;

procedure RemoveOwnedFile(const Path: String);
var
  Pending, Unique: String;
  Attempt: Integer;
begin
  if DeleteFile(Path) then exit;
  Pending := Path;
  if Pos('.tingzi-delete-', ExtractFileName(Path)) = 0 then
  begin
    Unique := Path + '.tingzi-delete-' + IntToStr(UninstallTickCount);
    Attempt := 0;
    while FileExists(Unique + '-' + IntToStr(Attempt)) do
      Attempt := Attempt + 1;
    Unique := Unique + '-' + IntToStr(Attempt);
    if RenameFile(Path, Unique) then
    begin
      Pending := Unique;
      Log('Renamed locked owned file for removal: ' + Path + ' -> ' + Pending);
    end;
    if Pending = Path then
      Log('Could not rename locked owned file; scheduling original path: ' + Path);
  end;
  if not MoveFileExForUninstall(Pending, 0, 4) then
    RaiseException('Cannot schedule removal of ' + Pending + ': ' + SysErrorMessage(DLLGetLastError));
  Log('Removal pending reboot: ' + Pending);
  UninstallRestartRequired := True;
end;

procedure RemoveProgramTree(const Path: String);
var
  Entry: TFindRec;
  Child: String;
  Removed: Boolean;
begin
  if FindFirst(Path + '\*', Entry) then
  begin
    try
      repeat
        if (Entry.Name <> '.') and (Entry.Name <> '..') then
        begin
          Child := Path + '\' + Entry.Name;
          if (Entry.Attributes and FILE_ATTRIBUTE_DIRECTORY <> 0) and
             (Entry.Attributes and $400 = 0) then
            RemoveProgramTree(Child)
          else
          begin
            if Entry.Attributes and FILE_ATTRIBUTE_DIRECTORY <> 0 then
              Removed := RemoveDir(Child)
            else
            begin
              RemoveOwnedFile(Child);
              Removed := True;
            end;
            if not Removed then
            begin
              if not MoveFileExForUninstall(Child, 0, 4) then
                RaiseException('Cannot schedule removal of ' + Child + ': ' + SysErrorMessage(DLLGetLastError));
              Log('Removal pending reboot: ' + Child);
              UninstallRestartRequired := True;
            end;
          end;
        end;
      until not FindNext(Entry);
    finally
      FindClose(Entry);
    end;
  end;
  if DirExists(Path) and not RemoveDir(Path) then
  begin
    if not MoveFileExForUninstall(Path, 0, 4) then
      RaiseException('Cannot schedule removal of ' + Path + ': ' + SysErrorMessage(DLLGetLastError));
    Log('Removal pending reboot: ' + Path);
    UninstallRestartRequired := True;
  end;
end;

procedure RemoveOwnedProgramItems;
var
  Root, Child: String;
  Entry: TFindRec;
  OnlyOwnedItems: Boolean;
begin
  if UninstallRootIsLink then exit;
  Root := ExpandConstant('{app}');
  OnlyOwnedItems := True;
  if FindFirst(Root + '\*', Entry) then
  begin
    try
      repeat
        if (Entry.Name <> '.') and (Entry.Name <> '..') then
        begin
          Child := Root + '\' + Entry.Name;
          if (Entry.Attributes and FILE_ATTRIBUTE_DIRECTORY <> 0) and IsProgramVersion(Entry.Name) then
          begin
            if Entry.Attributes and $400 = 0 then
              RemoveProgramTree(Child)
            else if not RemoveDir(Child) then
              Log('Leaving version directory reparse point: ' + Child);
          end
          else if (Entry.Attributes and FILE_ATTRIBUTE_DIRECTORY = 0) and IsProgramRootFile(Entry.Name) then
            RemoveOwnedFile(Child)
          else
          begin
            OnlyOwnedItems := False;
            Log('Leaving unrelated install-root item: ' + Child);
          end;
        end;
      until not FindNext(Entry);
    finally
      FindClose(Entry);
    end;
  end;
  if OnlyOwnedItems and DirExists(Root) and not RemoveDir(Root) then
  begin
    if not MoveFileExForUninstall(Root, 0, 4) then
      RaiseException('Cannot schedule removal of ' + Root + ': ' + SysErrorMessage(DLLGetLastError));
    UninstallRestartRequired := True;
  end;
end;

function UninstallNeedRestart: Boolean;
begin
  RemoveOwnedProgramItems;
  Result := UninstallRestartRequired;
end;

function ServiceKey: String;
begin
  Result := 'SOFTWARE\Classes\CLSID\{#ServiceClsid}\InprocServer32';
end;

function IsFirstInstall: Boolean;
begin
  Result := not WasInstalled;
end;

function FloatWasRunning: Boolean;
begin
  Result := FloatWasRunning_;
end;

function WebView2Installed: Boolean;
var
  Version: String;
begin
  // Evergreen Runtime 的版本號記在 EdgeUpdate 的用戶端登錄裡；全機安裝在 32 位元的登錄區，個人安裝在 HKCU。
  Result := (RegQueryStringValue(HKLM32, 'SOFTWARE\Microsoft\EdgeUpdate\Clients\{#WebView2Client}', 'pv', Version) and (Version <> '') and (Version <> '0.0.0.0'))
    or (RegQueryStringValue(HKCU, 'Software\Microsoft\EdgeUpdate\Clients\{#WebView2Client}', 'pv', Version) and (Version <> '') and (Version <> '0.0.0.0'));
end;

function InitializeSetup: Boolean;
begin
  WasInstalled := RegKeyExists(HKLM64, ServiceKey);
  if (not WasInstalled) and RegKeyExists(HKLM32, ServiceKey) then
  begin
    // 與 InstallTingzi.cmd 的結束碼 9 同一種情況：只有 32 位元的註冊，無法判斷是哪一份安裝。
    MsgBox('這台電腦上有一份不完整的聽字註冊（只有 32 位元）。請先用舊版的 RemoveTingzi.cmd 移除，再執行這個安裝程式。', mbError, MB_OK);
    Result := False;
    exit;
  end;
  Result := True;
end;

function OnDownloadProgress(const Url, FileName: String; const Progress, ProgressMax: Int64): Boolean;
begin
  Result := True;
end;

procedure InitializeWizard;
begin
  DownloadPage := CreateDownloadPage('下載 WebView2 Runtime', '語音浮窗需要 Microsoft Edge WebView2 Runtime。', @OnDownloadProgress);
  if not WizardSilent then
    InstallProgressPage := CreateOutputMarqueeProgressPage('正在安裝聽字', '安裝完成前，請不要關閉這個視窗。');
end;

// 子程序沒有回報百分比，這裡只在實際換步驟時更新文字。
procedure ReportInstallStage(const Stage: String);
begin
  if not WizardSilent then
    InstallProgressPage.SetText(Stage, '');
end;

procedure ShowInstallProgress(const Stage: String);
begin
  if not WizardSilent then
  begin
    ReportInstallStage(Stage);
    InstallProgressPage.Show;
    InstallProgressPage.Animate;
  end;
end;

procedure HideInstallProgress;
begin
  if not WizardSilent then
    InstallProgressPage.Hide;
end;

function InstallWebView2: Boolean;
var
  Code: Integer;
begin
  Result := False;
  DownloadPage.Clear;
  // 微軟官方的常青版啟動程式，會再下載並安裝適合這台電腦的 Runtime。
  DownloadPage.Add('https://go.microsoft.com/fwlink/p/?LinkId=2124703', 'MicrosoftEdgeWebview2Setup.exe', '');
  DownloadPage.Show;
  try
    try
      DownloadPage.Download;
      Result := Exec(ExpandConstant('{tmp}\MicrosoftEdgeWebview2Setup.exe'), '/silent /install', '', SW_SHOW, ewWaitUntilTerminated, Code) and (Code = 0);
    except
      if not DownloadPage.AbortedByUser then
        Log(GetExceptionMessage);
    end;
  finally
    DownloadPage.Hide;
  end;
end;

// 沒有 WebView2 Runtime 時直接從微軟下載安裝，不另外詢問：語音浮窗和設定視窗少了它開不起來。
// 有畫面時顯示下載進度；無人值守安裝（/VERYSILENT）在 PrepareToInstall 裡安靜地下載。
// 下載失敗不擋安裝，注音照常可用。
function NextButtonClick(CurPageID: Integer): Boolean;
begin
  Result := True;
  if (CurPageID = wpReady) and (not WizardSilent) and (not WebView2Installed) then
  begin
    if not InstallWebView2 then
      MsgBox('WebView2 Runtime 沒有裝好，可能是網路無法連到微軟。聽字會繼續安裝，注音可以照常使用；語音浮窗需要它，之後可以到 https://developer.microsoft.com/microsoft-edge/webview2/ 下載「Evergreen Bootstrapper」再裝一次。', mbInformation, MB_OK);
  end;
end;

procedure InstallWebView2Quietly;
var
  Code: Integer;
begin
  if WebView2Installed then
    exit;
  try
    DownloadTemporaryFile('https://go.microsoft.com/fwlink/p/?LinkId=2124703', 'MicrosoftEdgeWebview2Setup.exe', '', nil);
    if Exec(ExpandConstant('{tmp}\MicrosoftEdgeWebview2Setup.exe'), '/silent /install', '', SW_HIDE, ewWaitUntilTerminated, Code) then
      Log('WebView2 Runtime setup exit code ' + IntToStr(Code))
    else
      Log('WebView2 Runtime setup could not start.');
  except
    Log('WebView2 Runtime download failed: ' + GetExceptionMessage);
  end;
end;

function UpdateReadyMemo(Space, NewLine, MemoUserInfoInfo, MemoDirInfo, MemoTypeInfo, MemoComponentsInfo, MemoGroupInfo, MemoTasksInfo: String): String;
var
  Action: String;
begin
  if WasInstalled then
    Action := '升級既有的聽字，保留偏好、錄音快捷鍵、詞表、學習資料與已下載的模型'
  else
    Action := '第一次安裝';
  Result := '版本：' + NewLine + Space + '{#AppVersion}（' + Action + '）' + NewLine + NewLine +
            '安裝位置：' + NewLine + Space + ExpandConstant('{app}') + '\v{#AppVersion}' + NewLine + NewLine +
            '會註冊的輸入法：' + NewLine + Space + '{#AppName}（中文（台灣），按 Win＋空白鍵切換）' + NewLine + NewLine +
            '開始功能表：' + NewLine + Space + '{#AppName} 資料夾，內有「語音輸入」與「注音與工具」';
  if not WebView2Installed then
    Result := Result + NewLine + NewLine + '注意：' + NewLine + Space + '沒有找到 WebView2 Runtime，按「安裝」時會詢問是否下載。';
end;

function InstallFailure(Code: Integer): String;
begin
  case Code of
    2: Result := '這不是 64 位元的 Windows。';
    4: Result := '安裝內容不完整，或既有的聽字註冊和這個版本對不上。請重新下載安裝程式。';
    5: Result := '無法建立安裝資料夾。';
    6: Result := '輸入法註冊失敗，已還原這次的變更。';
    7: Result := '複製程式檔失敗，沒有註冊輸入法。';
    8, 11: Result := '升級時更新登錄失敗。';
    9: Result := '另一個聽字安裝正在進行，或只有 32 位元的註冊。';
    10: Result := '讀寫安裝資料夾時出錯。';
  else
    Result := '安裝腳本回報錯誤。';
  end;
  Result := Result + '（代碼 ' + IntToStr(Code) + '）';
end;

procedure LogFile(const Path: String);
var
  Lines: TArrayOfString;
  I: Integer;
begin
  if LoadStringsFromFile(Path, Lines) then
    for I := 0 to GetArrayLength(Lines) - 1 do
      Log('  ' + Lines[I]);
end;

// 放進 PowerShell 單引號字串裡的路徑：路徑裡的單引號要寫兩次。
function PowerShellQuoted(Value: String): String;
begin
  StringChangeEx(Value, '''', '''''', True);
  Result := '''' + Value + '''';
end;

// 完成程式檔複製與輸入法註冊後，才讓 Inno Setup 登記應用程式清單。
function InstallPayload: String;
var
  Code: Integer;
  Payload, Archive, Output: String;
begin
  Result := '';
  ExtractTemporaryFile('payload.zip');
  Archive := ExpandConstant('{tmp}\payload.zip');
  Payload := ExpandConstant('{tmp}\payload');
  Output := ExpandConstant('{tmp}\install-output.txt');
  // 直接用 .NET 解開，不用 Expand-Archive：從 PowerShell 7 開啟安裝程式時，Windows PowerShell 會沿用
  // PowerShell 7 的模組路徑，載入它附的 Microsoft.PowerShell.Archive，結果什麼都沒解開卻回報成功。
  // 所以解開後還要確認安裝腳本真的在。
  if not Exec(ExpandConstant('{sys}\WindowsPowerShell\v1.0\powershell.exe'),
      '-NoProfile -NonInteractive -ExecutionPolicy Bypass -Command "$ErrorActionPreference = ''Stop''; Add-Type -AssemblyName System.IO.Compression.FileSystem; [System.IO.Compression.ZipFile]::ExtractToDirectory(' + PowerShellQuoted(Archive) + ', ' + PowerShellQuoted(Payload) + ')"',
      '', SW_HIDE, ewWaitUntilTerminated, Code) or (Code <> 0) or not FileExists(Payload + '\InstallTingzi.cmd') then
  begin
    Log('Unpacking payload.zip failed (code ' + IntToStr(Code) + ').');
    Result := '無法解開安裝內容（代碼 ' + IntToStr(Code) + '）。請重新下載安裝程式。';
    exit;
  end;
  ReportInstallStage('正在複製程式檔並註冊注音輸入法…');
  if not Exec(ExpandConstant('{cmd}'), '/c ""' + Payload + '\InstallTingzi.cmd" /quiet > "' + Output + '" 2>&1"', Payload, SW_HIDE, ewWaitUntilTerminated, Code) then
  begin
    Result := '無法執行安裝腳本。';
    exit;
  end;
  Log('InstallTingzi.cmd exit code ' + IntToStr(Code));
  LogFile(Output);
  if Code <> 0 then
  begin
    Result := InstallFailure(Code);
    exit;
  end;
  ReportInstallStage('正在完成安裝準備…');
  // 新版的語音浮窗和舊版用同一個單一執行個體名稱，舊版還開著時新版開不起來，所以先關掉舊版。
  // 設定每改一次就存檔，關掉不會遺失設定。taskkill 找到並結束程式時回傳 0。
  if WasInstalled and Exec(ExpandConstant('{sys}\taskkill.exe'), '/f /im TingziFloat.exe', '', SW_HIDE, ewWaitUntilTerminated, Code) and (Code = 0) then
  begin
    FloatWasRunning_ := True;
    Log('Closed the running voice window; it will be opened again after setup.');
  end;
end;

// 這裡回傳文字就會停止安裝並顯示原因，不會讓 Inno Setup 繼續登記。
function PrepareToInstall(var NeedsRestart: Boolean): String;
begin
  if WizardSilent then
    InstallWebView2Quietly;
  ShowInstallProgress('正在解開安裝檔…');
  try
    Result := InstallPayload;
  finally
    HideInstallProgress;
  end;
end;

// 個人資料夾以外，使用者可能換過的本機詞表與語音資料夾設定放在目前使用的版本子目錄裡。
// 保留個人資料時，先把它們複製到個人資料夾，避免隨程式目錄一起刪除。
procedure KeepUninstallFile(const Source, Destination: String);
begin
  if FileExists(Source) then
    if not ForceDirectories(ExtractFileDir(Destination)) or
       not FileCopy(Source, Destination, False) then
      RaiseException('Cannot preserve ' + Source + ' in ' + Destination);
end;

procedure KeepLocalWordLists;
var
  Active, Kept: String;
begin
  Kept := ExpandConstant('{localappdata}\Tingzi\kept-after-uninstall');
  KeepUninstallFile(ExpandConstant('{app}\Tingzi.cin'), Kept + '\legacy-root\Tingzi.cin');
  KeepUninstallFile(ExpandConstant('{app}\phrases.txt'), Kept + '\legacy-root\phrases.txt');
  if not RegQueryStringValue(HKLM64, ServiceKey, '', Active) then
    exit;
  Active := ExtractFileDir(Active);
  KeepUninstallFile(Active + '\Tingzi.cin', Kept + '\Tingzi.cin');
  KeepUninstallFile(Active + '\phrases.txt', Kept + '\phrases.txt');
  KeepUninstallFile(Active + '\TingziFloat\voice-root.txt', Kept + '\voice-root.txt');
end;

// 輸入法登記在整台電腦，但 Windows 只在「把它加進鍵盤清單」的使用者那裡列出。安裝程式以系統管理員身分執行，
// 所以用原本開啟安裝程式的使用者身分把聽字加進他的鍵盤清單；缺少中文（台灣）時 Windows 會一併加入。
// 電腦上的其他使用者第一次開啟語音浮窗時，聽字會替他們做同一件事。
procedure AddToUserKeyboards;
var
  Code: Integer;
begin
  if ExecAsOriginalUser(ExpandConstant('{app}\v{#AppVersion}\TingziKeyboard.exe'), 'enable', '', SW_HIDE, ewWaitUntilTerminated, Code) and (Code = 0) then
    Log('Added Tingzi to the keyboards of the user who ran setup.')
  else
  begin
    Log('Could not add Tingzi to the user''s keyboards (code ' + IntToStr(Code) + ').');
    if not WizardSilent then
      MsgBox('聽字已經裝好，但沒能自動加進你的鍵盤清單。請到「設定 → 時間與語言 → 語言與地區」，在「中文（台灣）」的「語言選項」裡按「新增鍵盤」，選「聽字・注音與語音輸入法」。', mbInformation, MB_OK);
  end;
end;

procedure CurStepChanged(CurStep: TSetupStep);
begin
  if CurStep = ssPostInstall then
  begin
    ShowInstallProgress('正在將聽字加入你的鍵盤清單…');
    try
      AddToUserKeyboards;
    finally
      HideInstallProgress;
    end;
  end;
end;

procedure CurUninstallStepChanged(CurUninstallStep: TUninstallStep);
begin
  if CurUninstallStep = usUninstall then
  begin
    DeletePersonalData := False;
    if not UninstallSilent then
      DeletePersonalData := MsgBox('要一併刪除你的個人資料嗎？' + #13#10#13#10 +
        '包括偏好設定、自訂詞、學習資料、辨識紀錄與已下載的語音模型（' + ExpandConstant('{localappdata}\Tingzi') + '）。' + #13#10#13#10 +
        '選「否」會保留這些資料，之後重新安裝可以接著用。', mbConfirmation, MB_YESNO or MB_DEFBUTTON2) = IDYES;
    if not DeletePersonalData then
      KeepLocalWordLists;
  end;
  if (CurUninstallStep = usPostUninstall) and DeletePersonalData then
  begin
    DelTree(ExpandConstant('{localappdata}\Tingzi'), True, True, True);
    RegDeleteKeyIncludingSubkeys(HKCU, 'Software\Tingzi');
  end;
end;
