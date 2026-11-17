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

[UninstallDelete]
; RemoveTingzi.cmd 只刪根目錄的檔案；各版本的子目錄由這裡刪。
Type: filesandordirs; Name: "{app}\v0.*"
Type: dirifempty; Name: "{app}"

[Code]
var
  WasInstalled: Boolean;
  DownloadPage: TDownloadWizardPage;
  DeletePersonalData: Boolean;
  FloatWasRunning_: Boolean;

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

function NextButtonClick(CurPageID: Integer): Boolean;
begin
  Result := True;
  if (CurPageID = wpReady) and (not WebView2Installed) and (not WizardSilent) then
  begin
    if MsgBox('這台電腦沒有 Microsoft Edge WebView2 Runtime，語音浮窗和設定視窗需要它才能開啟（注音輸入不需要）。' + #13#10#13#10 +
              '要現在從微軟下載並安裝嗎？大約需要 2 MB 的下載，之後會再自動取得其餘部分。', mbConfirmation, MB_YESNO) = IDYES then
    begin
      if not InstallWebView2 then
        MsgBox('WebView2 Runtime 沒有裝好。聽字會繼續安裝，注音可以照常使用；之後可以到 https://developer.microsoft.com/microsoft-edge/webview2/ 下載「Evergreen Bootstrapper」再裝一次。', mbInformation, MB_OK);
    end;
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

// 在複製任何檔案、登記應用程式清單之前註冊輸入法。這裡回傳文字就會停止安裝並顯示原因，
// 不會留下半套的安裝；AfterInstall 裡丟出例外只會顯示錯誤，安裝照樣完成。
function PrepareToInstall(var NeedsRestart: Boolean): String;
var
  Code: Integer;
  Payload, Archive, Output: String;
begin
  Result := '';
  ExtractTemporaryFile('payload.zip');
  Archive := ExpandConstant('{tmp}\payload.zip');
  Payload := ExpandConstant('{tmp}\payload');
  Output := ExpandConstant('{tmp}\install-output.txt');
  if not Exec(ExpandConstant('{sys}\WindowsPowerShell\v1.0\powershell.exe'),
      '-NoProfile -NonInteractive -ExecutionPolicy Bypass -Command "Expand-Archive -LiteralPath ''' + Archive + ''' -DestinationPath ''' + Payload + ''' -Force"',
      '', SW_HIDE, ewWaitUntilTerminated, Code) or (Code <> 0) then
  begin
    Result := '無法解開安裝內容（代碼 ' + IntToStr(Code) + '）。請重新下載安裝程式。';
    exit;
  end;
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
  // 新版的語音浮窗和舊版用同一個單一執行個體名稱，舊版還開著時新版開不起來，所以先關掉舊版。
  // 設定每改一次就存檔，關掉不會遺失設定。taskkill 找到並結束程式時回傳 0。
  if WasInstalled and Exec(ExpandConstant('{sys}\taskkill.exe'), '/f /im TingziFloat.exe', '', SW_HIDE, ewWaitUntilTerminated, Code) and (Code = 0) then
  begin
    FloatWasRunning_ := True;
    Log('Closed the running voice window; it will be opened again after setup.');
  end;
end;

// 個人資料夾以外，使用者可能換過的本機詞表與語音資料夾設定放在目前使用的版本子目錄裡。
// 保留個人資料時，先把它們複製到個人資料夾，避免隨程式目錄一起刪除。
procedure KeepLocalWordLists;
var
  Active, Kept: String;
begin
  if not RegQueryStringValue(HKLM64, ServiceKey, '', Active) then
    exit;
  Active := ExtractFileDir(Active);
  Kept := ExpandConstant('{localappdata}\Tingzi\kept-after-uninstall');
  ForceDirectories(Kept);
  if FileExists(Active + '\Tingzi.cin') then FileCopy(Active + '\Tingzi.cin', Kept + '\Tingzi.cin', False);
  if FileExists(Active + '\phrases.txt') then FileCopy(Active + '\phrases.txt', Kept + '\phrases.txt', False);
  if FileExists(Active + '\TingziFloat\voice-root.txt') then FileCopy(Active + '\TingziFloat\voice-root.txt', Kept + '\voice-root.txt', False);
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
