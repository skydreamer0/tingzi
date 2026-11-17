# 開發者入口

此頁記錄聽字目前的原始碼位置與本機操作入口。產品名稱與系統識別見[產品識別](../specs/tingzi-product.md)，待補功能見[一般用戶體驗規格](../specs/general-user-experience.md)。

## 原始碼地圖

| 路徑 | 用途 |
| --- | --- |
| `windows/service.cpp`、`profile.h`、`engine.h`、`learning.h` | C++ Windows TSF 文字服務、輸入法識別與自有引擎。 |
| `windows/setup.cpp`、`upgrade.cpp`、`InstallTingzi.cmd`、`RemoveTingzi.cmd` | 註冊、升級與移除。 |
| `windows/keyboard.cpp`、`learning_cli.cpp` | 切換輸入法與學習工具。 |
| `windows/editor.cpp` | 開發用的注音測試視窗；預設建置已不再編譯，也不隨安裝包發行。 |
| `app/src-tauri/` | 語音浮窗與設定頁的 Rust 後端（Tauri 2）：錄音、辨識（內建 codex-asr 與本機 Whisper）、貼入規則、設定、系統匣與單一執行個體。各檔案的職責見[遷移規格](../specs/app-migration.md)的架構表；測試寫在各模組裡，以 `cargo test` 執行。 |
| `app/src-tauri/src/sys/` | 向作業系統要求的每一件事，一個系統一個檔：`win.rs`（Win32、登錄檔、認證管理員）與 `mac.rs`（AppKit、Quartz 按鍵事件、鑰匙圈）。兩個檔提供同一組函式，其他模組只呼叫 `sys::`，不直接使用系統 API。 |
| `macos/build.sh`、`notices.py`、`README.md` | macOS 版的建置腳本（組出 `Tingzi.app` 與 ZIP）、第三方聲明產生器與使用說明。macOS 版只有語音浮窗，沒有注音輸入法。 |
| `.github/workflows/windows.yml` | 在 GitHub 的 Windows 電腦上執行 `cargo test`、`windows/build.cmd`、打包 ZIP 與安裝程式，再在那台用完即丟的電腦上以 `windows/test_installer.ps1` 安裝、升級與移除一次，並存成工作流程的產物。 |
| `.github/workflows/macos.yml` | 在 GitHub 的 Mac 上執行 `cargo test` 與 `macos/build.sh`，並把 ZIP 存成工作流程的產物。 |
| `app/ui/` | 浮窗（`index.html`）與設定頁（`settings.html`）的網頁介面，只顯示狀態與收集選擇。 |
| `windows/engine_test.cpp`、`learning_test.cpp` | 自有引擎與學習測試原始碼。 |
| `windows/segment_compare.cpp` | 開發用比對工具：同一串按鍵分別以「整句最高分」與「分批定案」兩種斷詞模式轉換並列出差異。分批定案預設關閉，可在 `HKCU\Software\Tingzi\Engine` 設 DWORD `StagedSegmentation=1` 啟用。 |
| `windows/build.cmd`、`package_release.py` | 完整 Windows 建置與明列檔案的 ZIP 打包入口。 |
| `windows/build_installer.py`、`windows/installer/` | 由發行 ZIP 做出單一安裝程式（Inno Setup 6）。`ChineseTraditional.isl` 是 Inno Setup 官方儲存庫 `is-6_4_3` 標籤的繁體中文翻譯，原樣收錄。 |
| `windows/licenses/` | 標準授權全文，打包時附給沒有自帶授權檔的 Rust 相依套件；macOS 的建置也使用這一份。 |
| `windows/standalone-data/pairs.txt`、`windows/tools/build_pairs.py` | 詞語搭配統計與產生它的腳本。腳本讀詞庫和一份語料（維基百科分卷或純文字檔），輸出常一起出現的詞對；語料本身不放進專案。授權見 `pairs-NOTICE.txt`。 |
| `assets/icons/` | 介面圖示集的 SVG 原稿；規範見[圖示集說明](../../assets/icons/README.md)。 |
| `assets/brand/` | 橫耳鍵品牌素材。Windows 建置使用 `windows/tingzi.svg` 與 `windows/tingzi.ico`。 |
| `assets/design/tokens.json`、`build_tokens.py` | 所有視窗共用的顏色、圓角、字級與動畫時間。改 `tokens.json` 後執行 `python assets/design/build_tokens.py`，會重寫浮窗與設定頁用的 `app/ui/tokens.css`，以及候選窗與輸入行用的 `windows/palette.h`；加 `--check` 只檢查兩個檔是否最新。 |
| `docs/specs/` | 產品識別與待實作規格。 |

目前沒有根目錄 `source/`、`tests/` 或 `tools/` 的統一建置架構。重整須先依規格確認相對路徑和發行依賴。

## 建置與檢查浮窗

在 Windows 安裝 Rust（stable，含 `cargo`）與 Visual Studio 的 C++ 工具；不再需要 .NET SDK 或 NuGet。浮窗是 Tauri 2 程式，執行時使用 Microsoft Edge WebView2 Runtime（Windows 11 已內建）。首次建置時 cargo 會從 crates.io 下載相依套件。

在 `app/src-tauri` 執行：

```powershell
cargo test
cargo build --release
```

`cargo test` 在 Windows 目前有 68 個測試，涵蓋貼入規則、簡繁轉換與啟動參數等邏輯。測試成功不代表真實麥克風、線上辨識、Windows 焦點或 TSF 安裝已通過。

要手動啟動剛建置的浮窗，執行建置輸出裡的 `Tingzi.exe`。已安裝的浮窗與開發版共用單一執行個體識別；先把環境變數 `TINGZI_RECORD_KEY` 設為一個字母（例如 `T`）再啟動，開發版會改用該字母當錄音鍵並使用另一組執行個體名稱，可以和已安裝的版本同時執行。`--show`、`--settings`、`--edit`、`--open-wav` 與 `--transcribe` 等啟動參數見[遷移規格](../specs/app-migration.md)的「開發與測試」。

AI 潤稿的評測句在 `app/src-tauri/refine-cases/cases.json`。`cargo test` 只確認回應的檢查不會擋掉合格的潤稿；要看真實模型處理口頭禪、改口與數字的表現，在填好潤稿服務的電腦上執行 `python app/src-tauri/refine-cases/try_refine.py <Tingzi.exe 的路徑>`，每一句會用到一次服務的額度。說明見 [AI 潤稿規格](../specs/ai-refine.md)的「評測句」。

開發版浮窗不會替你安裝 TSF。線上辨識已內建，不需要另備 `codex-asr.exe`，但需要這台電腦上可用的 Codex 登入。本機模式需要既有 Whisper 工具與模型。不要為測試提交這些私人檔案。

## 建置 macOS 版

macOS 版要在 Mac 上建置，需要 Rust、Python 3 與 Xcode 命令列工具：

```bash
bash macos/build.sh
```

腳本以 `cargo build --release --locked` 建置 `app/src-tauri`，組出 `.build/macos/Tingzi.app`（執行檔、`Info.plist`、圖示、字音表 `phrases.txt` 與第三方聲明），做本機簽章，再壓成 ZIP。第三方聲明沿用 `windows/package_release.py` 的授權審核清單，遇到清單以外的授權字串會停下來。使用方式與目前的限制見 [macOS 說明](../../macos/README.md)。

在 Windows 上無法編譯 macOS 版（錄音與加密的相依套件需要 Apple 的工具與 SDK）。改動 `sys/mac.rs` 後，以推送到 GitHub 觸發的 macOS 工作流程確認編譯與測試。

## 完整建置與打包

`windows/build.cmd` 使用 Visual Studio 的 C++ x86 與 x64 工具、Windows SDK、`vswhere.exe`、資源編譯器與 Rust 的 `cargo`。它先建置 C++ 的文字服務與工具，再以 `cargo build --release --locked` 建置 `app/src-tauri`，把結果複製為套件裡的 `TingziFloat\TingziFloat.exe`，最後寫出 `cargo-metadata.json`。建置時會重新對應原始碼路徑，執行檔不含建置電腦的資料夾名稱。

從儲存庫根目錄執行。唯一的參數是輸出資料夾，可省略，預設為 `.build`：

```powershell
windows\build.cmd
python windows/package_release.py .build/package .build/Tingzi-<版本>-windows-x64.zip --cargo-metadata .build/cargo-metadata.json
```

再由 ZIP 做出單一安裝程式，需要 [Inno Setup](https://jrsoftware.org/isinfo.php) 6.3 以上：

```powershell
python windows/build_installer.py .build/Tingzi-<版本>-windows-x64.zip .build/installer
```

它解開 ZIP、逐一核對 `release-manifest.json` 的雜湊，再以 ISCC 編譯 `windows/installer/Tingzi.iss`，輸出 `Tingzi-Setup-<版本>-x64.exe` 與它的 `.sha256`。發行時兩個檔都要上傳到 GitHub Releases，檔名不要改：設定頁的「下載並安裝」靠 `Tingzi-Setup-` 開頭、`-x64.exe` 結尾找安裝程式，並用同名加 `.sha256` 的檔核對；少了 `.sha256` 就只開下載頁。升級時安裝程式會先關掉開著的語音浮窗（新舊版用同一個單一執行個體名稱），安裝完成後開啟新版。版號取自 ZIP 的清單，不必另外修改。安裝程式原封不動帶著 ZIP，在開始複製檔案前解開到暫存資料夾並執行 `InstallTingzi.cmd /quiet`（失敗時停止安裝、不登記應用程式清單，腳本的輸出寫進安裝記錄），所以註冊與升級的規則仍只寫在 `InstallTingzi.cmd` 與 `upgrade.cpp`；安裝程式只另外處理 WebView2 Runtime、應用程式清單的登記，以及解除安裝時是否刪除 `%LOCALAPPDATA%\Tingzi` 與 `HKCU\Software\Tingzi`。`AppId` 決定應用程式清單裡的那一筆，發行後不可更改。

發行新版時，改好版號（見下方）、把 `CHANGELOG.md` 的「尚未發行」改成該版並合併進 main 後，推送 `v<版本>` 標籤。`.github/workflows/release.yml` 會在 GitHub 的 Windows 電腦上建置、檢查安裝程式，再把 `Tingzi-Setup-<版本>-x64.exe` 和 `.sha256` 附到該標籤的**草稿** release。標籤和建置出來的版號不一致時會停下來。草稿的說明由 `.github/release_notes.py` 依 `.github/release-notes.md` 範本與 `CHANGELOG.md` 產生，寫法見[貢獻指南](../../CONTRIBUTING.md)的「發行說明」。草稿只有維護者看得到；補好摘要、升級須知與已知問題，按「Publish release」後使用者才看得到，設定頁的檢查更新也是從那時起才會找到它。也可以在 Actions 頁手動執行這個工作流程，指定已存在的標籤。

同一版重新安裝時，`upgrade.cpp` 遇到內容不同的程式檔，會先把舊檔改名（加上 `.replaced-<數字>`；執行中的程式和已載入的 DLL 也可以改名），再放入新檔；舊檔在沒有程式使用時刪除，或排到下次開機刪除。

`windows/package_release.py` 的介面如下：

```text
python windows/package_release.py <package-directory> <new-output.zip> --cargo-metadata <metadata.json>
```

`metadata.json` 是 `build.cmd` 以 `cargo metadata` 產生的檔案，須包含各 Rust 相依套件的 `packages` 清單與可讀取的 `manifest_path`。打包器讀取各依賴的授權檔（沒有附授權檔的套件改附 `windows/licenses/` 的標準全文），拒絕未審核的授權字串，建立 `THIRD-PARTY-NOTICES.txt`、檔案雜湊 manifest 與 ZIP 雜湊。既有輸出 ZIP 會被拒絕。打包器另會在輸入目錄寫入第三方聲明，測試時使用獨立 staging 目錄。

明列檔案不等於完整的隱私或授權審查。公開前仍須人工檢查 ZIP、來源歷史與第三方聲明。不要執行安裝腳本作為一般文件變更的驗證。

## 版本與授權界線

主分支的浮窗專案（`app/src-tauri`）、升級目錄與打包器均標記程式版本 `0.7.7`，這是開發版資訊，不代表已發行。

浮窗內建第三方 codex-asr 0.1.2。打包器把它與其他 Rust 相依套件的名稱、版本與授權記錄在套件的 `notices/rust-dependencies.json` 與 `THIRD-PARTY-NOTICES.txt`。專案整體授權、公開時間、可重現建置、簽章和試行驗收仍待確認。

可用 `git ls-files app windows docs assets` 重新列出受版控的檔案。`target/`、`.build/`、錄音、私人詞庫、模型與本機資料不屬於來源地圖。
