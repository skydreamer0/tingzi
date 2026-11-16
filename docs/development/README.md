# 開發者入口

此頁記錄聽字目前的原始碼位置與本機操作入口。產品名稱與系統識別見[產品識別](../specs/tingzi-product.md)，待補功能見[一般用戶體驗規格](../specs/general-user-experience.md)。

## 原始碼地圖

| 路徑 | 用途 |
| --- | --- |
| `windows/service.cpp`、`profile.h`、`engine.h`、`learning.h` | C++ Windows TSF 文字服務、輸入法識別與自有引擎。 |
| `windows/setup.cpp`、`upgrade.cpp`、`InstallTingzi.cmd`、`RemoveTingzi.cmd` | 註冊、升級與移除。 |
| `windows/editor.cpp`、`keyboard.cpp`、`learning_cli.cpp` | 打字板、切換輸入法與學習工具。 |
| `windows/float/TingziFloat.csproj`、`Program.cs` | .NET 8 Windows Forms 浮窗與啟動協調。 |
| `windows/float/VoiceSettings.cs`、`VoiceInsertionTarget.cs`、`CodexFocusProbe.cs` | 設定、插入策略與輸入欄辨識。 |
| `windows/float/AudioNotes/` | 錄音、辨識與浮窗繪製，保留元件 MIT 授權。 |
| `windows/tests/` | 以模擬輸入欄和剪貼簿驗證文字交付的 C# 可執行測試。 |
| `windows/engine_test.cpp`、`learning_test.cpp` | 自有引擎與學習測試原始碼。 |
| `windows/segment_compare.cpp` | 開發用比對工具：同一串按鍵分別以「整句最高分」與「分批定案」兩種斷詞模式轉換並列出差異。分批定案預設關閉，可在 `HKCU\Software\Tingzi\Engine` 設 DWORD `StagedSegmentation=1` 啟用。 |
| `windows/build.cmd`、`package_release.py` | 完整 Windows 建置與明列檔案的 ZIP 打包入口。 |
| `windows/float/ZhuyinOptions.cs` | 設定頁寫給注音服務的選項（鍵盤配置、選字方式、字級）與自訂詞的讀寫。 |
| `windows/float/Controls.cs` | 設定視窗自繪的元件：卡片、圓角按鈕、開關與產品標誌。 |
| `windows/float/Appearance.cs` | 色系（跟隨 Windows 或固定深淺色）、圖示繪製與登入啟動。 |
| `assets/icons/` | 介面圖示集的 SVG 原稿，建置時嵌入浮窗程式；規範見[圖示集說明](../../assets/icons/README.md)。 |
| `assets/brand/` | 橫耳鍵品牌素材。Windows 建置使用 `windows/tingzi.svg` 與 `windows/tingzi.ico`。 |
| `docs/specs/` | 產品識別與待實作規格。 |

目前沒有根目錄 `source/`、`tests/` 或 `tools/` 的統一建置架構。重整須先依規格確認相對路徑和發行依賴。

## 建置與檢查浮窗

在 Windows 安裝 .NET 8 SDK。執行環境另需 .NET 8 Desktop Runtime x64。浮窗使用 Windows Forms、WPF 互通與 NAudio 2.2.1，首次還原需要 NuGet 套件來源或已備妥的快取。

從儲存庫根目錄執行：

```powershell
dotnet build windows/float/TingziFloat.csproj -c Release
dotnet run --project windows/tests/VoiceDelivery.Tests.csproj -c Release
```

測試專案是可執行程式，使用 `dotnet run`。它連結部分產品原始碼，使用 `TestStubs.cs` 替代外部互動。測試成功不代表真實麥克風、線上辨識、Windows 焦點或 TSF 安裝已通過。

要手動啟動剛建置的浮窗，執行下列指令。已安裝的浮窗與開發版共用單一執行個體識別，先完成錄音並正常退出現有浮窗。

```powershell
dotnet run --project windows/float/TingziFloat.csproj -c Release
```

開發版浮窗不會替你安裝 TSF 或第三方辨識執行檔。線上模式需要另備 `codex-asr.exe` 與可用登入。本機模式需要既有 Whisper 工具與模型。不要為測試提交這些私人檔案。

## 完整建置與打包

`windows/build.cmd` 使用 Visual Studio 的 C++ x86 與 x64 工具、Windows SDK、`vswhere.exe`、資源編譯器與 .NET SDK。它從 `windows/` 的相對路徑建置 C++ 工具與 C# 浮窗。

此入口仍有本機依賴。第二個參數用來尋找 `audio-notes/vendor/codex-asr/LICENSE-MIT`，第三個參數是預建的 `codex-asr.exe`，第四個參數可指定輸出根目錄。第一個參數目前未使用。腳本從使用者 NuGet 快取複製 NAudio 授權，並以輸出根目錄下的 `offline` 目錄作為 publish 套件來源。它不是乾淨 clone 後可直接重現的發行流程。

`windows/package_release.py` 的介面如下：

```text
python windows/package_release.py <package-directory> <new-output.zip> --cargo-metadata <metadata.json>
```

`metadata.json` 須包含 codex-asr 依賴的 `packages` 清單與可讀取的 `manifest_path`。打包器讀取各依賴的授權檔，拒絕未審核的授權字串，建立 `THIRD-PARTY-NOTICES.txt`、檔案雜湊 manifest 與 ZIP 雜湊。既有輸出 ZIP 會被拒絕。打包器另會在輸入目錄寫入第三方聲明，測試時使用獨立 staging 目錄。

明列檔案不等於完整的隱私或授權審查。公開前仍須人工檢查 ZIP、來源歷史與第三方聲明。不要執行安裝腳本作為一般文件變更的驗證。

## 版本與授權界線

主分支的浮窗專案、升級目錄與打包器均標記程式版本 `0.6.6`，這是開發版資訊，不代表已發行。

AudioNotes 元件的授權見[MIT 原文](../../windows/float/AudioNotes/LICENSE)。NAudio 版本為 2.2.1。打包器記錄第三方 codex-asr 0.1.2 及其依賴，但預建執行檔來源仍須發版時核驗。專案整體授權、公開時間、可重現建置、簽章和試行驗收仍待確認。

可用 `git ls-files windows docs assets` 重新列出受版控的檔案。`bin/`、`obj/`、錄音、私人詞庫、模型與本機資料不屬於來源地圖。
