# 參與聽字開發

先讀[開發者入口](docs/development/README.md)，確認建置需求與現有目錄。產品名稱與系統識別見[產品識別](docs/specs/tingzi-product.md)。

## 回報問題

使用 [GitHub Issue](https://github.com/skydreamer0/tingzi/issues) 回報。儲存庫提供[問題回報模板](.github/ISSUE_TEMPLATE/bug-report.yml)與[功能需求模板](.github/ISSUE_TEMPLATE/feature-request.yml)。回報問題時填入平台、作業系統版本、程式版號或 commit、辨識方式、重現步驟、預期結果與實際結果。

以下是使用合成文字的回報範例，不需要附錄音：

> Windows 11 x64，0.7.7，本機辨識模式。在記事本欄位 A 開始錄音，停止前改點欄位 B。預期結果複製至剪貼簿並展開預覽，不自動插入 B。實際結果未展開預覽。以「今天測試文字輸入」重現兩次。未附私人文字、錄音或登入資料。

資安問題請不要開公開的 Issue，改依[資安政策](SECURITY.md)私下回報。

不要在問題或附件貼出帳號、token、私人詞表、診斷原始資料或本機完整使用者路徑。截圖先遮除私人內容。

## 提交變更

1. 在 Issue 說明問題、預期行為、驗收方式與不包含的範圍。較大的行為或結構變更先提出規格供討論。
2. 執行適合該變更的檢查，記錄 commit、指令、結果與未驗證項目。不要用編譯成功代替互動驗收。
3. 執行 `git diff --check` 與 `git status --short`，檢查新增檔案與個人資料。
4. 透過 [GitHub Pull Requests](https://github.com/skydreamer0/tingzi/pulls) 提交變更，依 [PR 範本](.github/pull_request_template.md) 填寫說明並連結對應 Issue：使用者可觀察的影響、驗證結果與尚未驗證的項目，以及升級、移除或回復受到的影響。
5. 使用者看得到的改變，在 [`CHANGELOG.md`](CHANGELOG.md) 的「尚未發行」底下加一項：放進新增、變更、修正、移除等分類，以「功能區：」開頭，寫成完整的句子，句末附 PR 或 Issue 編號。只改文件、測試或建置流程時不用加。

## 發行說明

發行新版時，在推送版本標籤之前，把 `CHANGELOG.md` 的「尚未發行」改成版號與日期（例如 `## [0.8.0] - 2026-11-20`），並在上方加回一個空的「尚未發行」。推送標籤後，`.github/workflows/release.yml` 會依 [`.github/release-notes.md`](.github/release-notes.md) 的範本建立草稿：版號、前一版的比較連結、安裝程式的雜湊值與該版在 `CHANGELOG.md` 的段落都會自動填入。發佈前在草稿裡填好 `{{SUMMARY}}`（一兩句本版重點）、`{{UPGRADE_NOTES}}` 與 `{{KNOWN_ISSUES}}`，沒有內容的項目整行刪除，確認沒有留下任何 `{{`。

- 設定頁「關於」會以純文字顯示說明的前 4000 個字，所以重點與升級須知放在最前面，不要用表格或 HTML 註解。
- 以中性、完整的句子描述使用者看得到的改變，用介面上實際的按鈕與頁面名稱，不用標語式的標題。
- 簽章、公證與尚未在實機驗證的項目要如實寫明。
- 不要勾選「Set as a pre-release」：設定頁的檢查更新只會找到最新的正式 release。

不要提交其他廠商的輸入法程式或詞庫、私人詞表、錄音、語音模型、登入狀態、憑證或本機驗收原始資料。`.gitignore` 已排除常見產物，提交前仍須人工檢查。測試資料使用自造且可公開的內容。

## 授權

聽字以 MIT 授權釋出，全文見根目錄的 `LICENSE`；送出的貢獻視為以相同授權提供。語音浮窗使用的 codex-asr 及其他 Rust 套件各有授權，打包時收錄在 `THIRD-PARTY-NOTICES.txt`。請不要加入授權與 MIT 不相容的程式碼或詞庫。
