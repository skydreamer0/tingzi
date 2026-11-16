# 參與聽字開發

先讀[開發者入口](docs/development/README.md)，確認建置需求與現有目錄。產品名稱與系統識別見[產品識別](docs/specs/tingzi-product.md)。

## 回報問題

使用 [GitHub Issue](https://github.com/skydreamer0/tingzi/issues) 回報。儲存庫提供[問題回報模板](.github/ISSUE_TEMPLATE/bug-report.yml)。填入 Windows 版本、程式版號或 commit、重現步驟、預期結果、實際結果與辨識模式。

以下是使用合成文字的回報範例，不需要附錄音：

> Windows 11 x64，0.6.0 開發版，本機辨識模式。在記事本欄位 A 開始錄音，停止前改點欄位 B。預期結果複製至剪貼簿並展開預覽，不自動插入 B。實際結果未展開預覽。以「今天測試文字輸入」重現兩次。未附私人文字、錄音或登入資料。

不要在問題或附件貼出帳號、token、私人詞表、診斷原始資料或本機完整使用者路徑。截圖先遮除私人內容。

## 提交變更

1. 在 Issue 說明問題、預期行為、驗收方式與不包含的範圍。較大的行為或結構變更先提出規格供討論。
2. 執行適合該變更的檢查，記錄 commit、指令、結果與未驗證項目。不要用編譯成功代替互動驗收。
3. 執行 `git diff --check` 與 `git status --short`，檢查新增檔案與個人資料。
4. 透過 [GitHub Pull Requests](https://github.com/skydreamer0/tingzi/pulls) 提交變更，連結對應 Issue。說明使用者可觀察的影響與驗證結果，以及升級、移除或回復受到的影響。

不要提交其他廠商的輸入法程式或詞庫、私人詞表、錄音、語音模型、登入狀態、憑證或本機驗收原始資料。`.gitignore` 已排除常見產物，提交前仍須人工檢查。測試資料使用自造且可公開的內容。

## 授權

`windows/float/AudioNotes/LICENSE` 僅涵蓋相應衍生元件。NAudio、codex-asr 及其他依賴各有授權。全庫授權尚待作者選定，不能據第三方 MIT 檔案宣稱全庫為 MIT。公開、發版與授權選定須另行確認。
