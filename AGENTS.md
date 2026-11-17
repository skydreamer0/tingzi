# AGENTS.md

這份檔案給在這個儲存庫工作的 AI 程式代理（Codex、Claude Code 等）。先讀完再動手；細節以連結的文件為準。Claude Code 透過 `CLAUDE.md` 匯入這份檔案，規則只寫在這裡。

## 專案是什麼

聽字（Tingzi）是繁體中文的語音與注音輸入工具，由兩個部分組成：

| 部分 | 位置 | 語言 | 平台 |
| --- | --- | --- | --- |
| 注音輸入法（Windows TSF 文字服務、引擎、安裝與升級工具） | `windows/` | C++ | 只有 Windows |
| 語音浮窗與設定頁（錄音、辨識、貼入、AI 潤稿、系統匣） | `app/src-tauri/`（Rust，Tauri 2）、`app/ui/`（HTML／CSS／JS） | Rust、網頁 | Windows 與 macOS 共用 |

macOS 版只有語音浮窗，沒有注音輸入法。各檔案的職責見 `docs/development/README.md` 的原始碼地圖與 `docs/specs/app-migration.md` 的架構表。

## 程式的分層

- 向作業系統要求的事一律放在 `app/src-tauri/src/sys/`：`win.rs`（Win32、登錄檔、認證管理員）與 `mac.rs`（AppKit、Quartz、鑰匙圈）提供同一組函式。其他模組只呼叫 `sys::`，不要直接呼叫系統 API，也不要在其他模組加 `#[cfg(windows)]` 分支來繞過它。新增系統功能時，兩個檔都要補上。
- `app/ui/` 只顯示狀態與收集選擇，邏輯放在 Rust。
- 顏色、圓角、字級與動畫時間由 `assets/design/tokens.json` 產生。改了它要執行 `python assets/design/build_tokens.py`，重寫 `app/ui/tokens.css` 與 `windows/palette.h`；不要手改這兩個產物。

## 建置與測試

**雲端容器（Linux）編譯不了這個程式。** `sys/` 只有 Windows 與 macOS 的實作，Linux 也缺 Tauri 需要的 GTK 套件，`cargo check` 會在 `gdk-sys` 失敗。在雲端工作時：

- 能在本機跑的檢查：`python assets/design/build_tokens.py --check`、`git diff --check`，以及 Python 腳本本身的語法。
- Rust 與 C++ 的編譯和測試靠 GitHub Actions。推送分支、開 PR 後讀 CI 結果，不要說「測試通過」除非看到 CI 是綠的。

GitHub Actions（只在 `app/**`、`macos/**`、`windows/**` 或工作流程檔有變動時觸發；只改文件的 PR 不會跑 CI）：

- `.github/workflows/macos.yml`：在 Mac 上 `cargo test --locked` 與 `bash macos/build.sh`，產物是 `Tingzi.app` 的 ZIP。
- `.github/workflows/windows.yml`：在 Windows 上 `cargo test --locked`、`windows\build.cmd`、打包 ZIP 再做成單一安裝程式，並實際安裝、升級、移除一次。
- `.github/workflows/release.yml`：推送 `v*` 標籤時觸發，建置並檢查安裝程式，只把 `Tingzi-Setup-<版本>-x64.exe` 與 `.sha256` 附到該標籤的**草稿** release。見下方「發行」。

在 Windows 電腦上（需要 Rust stable 與 Visual Studio C++ x86／x64 工具；不需要 .NET）：

```powershell
cd app/src-tauri
cargo test
cargo build --release
```

```powershell
windows\build.cmd
python windows/package_release.py .build/package .build/Tingzi-<版本>-windows-x64.zip --cargo-metadata .build/cargo-metadata.json
python windows/build_installer.py .build/Tingzi-<版本>-windows-x64.zip .build/installer
windows/test_installer.ps1 -Installer .build/installer/Tingzi-Setup-<版本>-x64.exe
```

`build_installer.py` 需要 Inno Setup 6.3 以上，輸出安裝程式與它的 `.sha256`。`test_installer.ps1` 會在這台電腦上實際安裝、升級並移除，只在可拋棄的機器（例如 CI）或要做實機驗收時執行。

C++ 引擎與學習的測試（`windows/engine_test.cpp`、`learning_test.cpp`）由 `windows/build_standalone.cmd` 編譯，`build.cmd` 不會編譯它們。

在 Mac 上（需要 Rust、Python 3 與 Xcode 命令列工具，不需要完整的 Xcode）：

```bash
bash macos/build.sh
```

輸出 `.build/macos/Tingzi.app` 與 ZIP，只做本機簽章，沒有 Developer ID，也沒有公證。在 Windows 上無法編譯 macOS 版；改了 `sys/mac.rs` 要靠 macOS 工作流程確認。

`cargo test` 只驗證邏輯（貼入規則、簡繁轉換、設定、潤稿請求等）。真實的麥克風、線上辨識、Windows 焦點、TSF 註冊與 macOS 權限都要在實機上驗收；回報時分清楚哪些是 CI 驗證過的、哪些還沒有實機驗收。不要用執行安裝腳本來驗證一般的文件變更。

## 發行

- Windows 版只發佈單一安裝程式 `Tingzi-Setup-<版本>-x64.exe` 與它的 `.sha256`，不再另外提供 ZIP 或腳本。檔名不要改：設定頁的檢查更新與自動更新靠這個檔名找安裝程式，並用 `.sha256` 核對。
- 推送版本標籤只會產生草稿 release。推送標籤、按「Publish release」或任何對外發佈之前，都必須先取得維護者明確同意。
- 不要勾選「Set as a pre-release」：設定頁只會找到最新的正式 release。
- 完整步驟見 `.claude/skills/release/SKILL.md`，發行說明的寫法見 `CONTRIBUTING.md` 的「發行說明」。

## 改版號

版號寫在好幾個地方，要一起改：`app/src-tauri/Cargo.toml`、`Cargo.lock` 裡 `tingzi-app` 那一筆、`app/src-tauri/tauri.conf.json`、`windows/package_release.py` 的 `VERSION`、`windows/upgrade.cpp`（升級時認得的舊版目錄清單與新版目錄），以及 `README.md`、`README.en.md`、`windows/README.md`、`docs/development/README.md` 裡提到版本的文字。`CHANGELOG.md` 的「尚未發行」同時改成新版號與日期，並更新文末的比較連結。改完用 `git grep -n "<舊版號>"` 確認沒有遺漏。

## 文件

- 文件以繁體中文為主。根目錄有 `README.md`（繁體中文）與 `README.en.md`（英文），兩份結構相同；改了其中一份，另一份要同步。
- 使用者文件放在 `windows/README.md`（Windows 使用手冊）與 `macos/README.md`；開發者文件在 `docs/development/README.md`；規格在 `docs/specs/`。`docs/README.md` 是索引，新增文件時要加進去。
- 只寫已經做好的功能。還在 PR 或規劃中的事，要寫明是「準備中」並連到 issue 或規格。
- 寫給一般使用者看的文字，用介面上實際的按鈕與頁面名稱，用完整的句子，避免技術術語。
- 行為改變時，同一個 PR 裡更新受影響的使用手冊與 README。
- 使用者看得到的改變，同一個 PR 裡在 `CHANGELOG.md` 的「尚未發行」加一項。發行說明依 `.github/release-notes.md` 的範本寫，規則見 `CONTRIBUTING.md` 的「發行說明」。

## 第三方服務、模型與版本

建議或寫死任何第三方模型名稱、API 參數、套件或工具版本之前，先查該服務的官方現行文件，不要憑記憶。模型會被淘汰，參數規則會變（例如 OpenAI 的推理模型會拒絕 `temperature`）。AI 潤稿目前的請求格式與已知的模型差異記在 `docs/specs/ai-refine.md`，改 `refine.rs` 時一起更新它。

下載本機模型的來源、版本與 SHA-256 固定寫在 `app/src-tauri/src/fetch.rs`；換來源時要一併更新雜湊與授權說明。

## GitHub 上的文字

儲存庫是公開的。寫 issue、PR、留言與發行說明時：

- 不寫維護者或其他個人的名字，寫成需求或事實陳述（例如「需求：……」「需要實機驗收」），不要寫成「某人希望……」的口語敘述。
- issue 裡連到儲存庫文件時用完整的 `https://github.com/skydreamer0/tingzi/blob/main/...` 網址。
- issue 的段落、標籤（`area: ...`、`status: 待決定`、`status: 待實機驗收`）與開關規則見 `.claude/skills/github-writing/SKILL.md`。

## 提交與 PR

- commit 訊息用英文，第一行是一句描述使用者看得到的改變的祈使句，例如 `Learn words from the user's fixes to dictations`，不加 `feat:` 之類的前綴。
- PR 說明依 `.github/pull_request_template.md` 填寫：使用者可觀察的影響、怎麼驗證的（哪些是 CI、哪些是實機）、還沒驗證的項目，以及對升級、移除或回復的影響。有對應 issue 就連結。流程見 `CONTRIBUTING.md`。
- 提交前執行 `git diff --check` 與 `git status --short`。

## 不能提交的東西

不要提交其他廠商的輸入法程式或詞庫、私人詞表、錄音（`*.wav`）、語音模型、登入狀態、金鑰、憑證、`voice-settings.json`、`.dev-data/` 裡的本機驗收資料，或含有本機完整使用者路徑的內容。`.gitignore` 已排除常見產物，提交前仍要人工檢查。測試資料用自造、可以公開的內容。

不要加入授權與 MIT 不相容的程式碼或詞庫。Rust 相依套件的授權由 `windows/package_release.py` 審核，遇到清單以外的授權字串會停下來；新增相依套件時要確認它通得過。

## 換行

`.sh` 檔一律用 LF（`.gitattributes` 已設定），否則在 macOS 上會執行失敗。

## 專案的流程說明（skill）

固定的多步驟流程寫在 `.claude/skills/<名稱>/SKILL.md`。Claude Code 會自動載入；其他代理在做對應的事之前，先讀那份檔案再照著做。

| 名稱 | 什麼時候用 |
| --- | --- |
| `release` | 準備發行新版：改版號與 `CHANGELOG.md`、推送標籤、檢查草稿 release |
| `windows-acceptance` | 在 Windows 電腦上做實機驗收，並把結果記到 issue |
| `github-writing` | 新開或修改 issue、寫 PR 說明、在 GitHub 上留言 |
