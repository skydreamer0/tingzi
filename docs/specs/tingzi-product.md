# 聽字產品識別

本文件記錄產品對外與對系統使用的名稱，修改時須一併檢查安裝、升級與移除。

## 名稱與圖示

- 產品名稱：**聽字・注音與語音輸入法**，簡稱 **聽字**，英文 **Tingzi**。
- 圖示：橫耳鍵，素材在 [`assets/brand/`](../../assets/brand/README.md)。Windows 建置使用內容相同的 `windows/tingzi.ico` 與 `windows/tingzi.svg`。
- 浮窗上的錄音、停止、複製等按鈕保留功能圖示，不以品牌圖示取代。

## 系統識別

| 項目 | 值 |
| --- | --- |
| TSF 文字服務 CLSID | `{B6ECF4B2-9955-4C24-9BA8-3F7641F0792B}` |
| TSF 設定檔 GUID | `{CD4317C3-FFB3-4D28-BD1A-7BD8480EBB65}` |
| 安裝根目錄 | `%ProgramFiles%\Tingzi`，各版本在 `v<版號>` 子目錄 |
| 使用者資料 | `%LOCALAPPDATA%\Tingzi`（`voice-settings.json`、學習資料庫） |
| 使用者登錄機碼 | `HKCU\Software\Tingzi` |
| 開始功能表資料夾 | `聽字・注音與語音輸入法` |
| 獨立注音測試版 | 使用另一組 CLSID／GUID 與 `TingziStandaloneRC` 目錄，可與主產品並存 |

更換 CLSID 或設定檔 GUID 會讓 Windows 視為另一個輸入法，除非有意為之，否則不要更動。

## 升級與移除的要求

- 升級保留本機詞表、`voice-root.txt`、使用者設定與學習資料，且輸入法清單中只有一個聽字。
- 移除時只刪除確認屬於本產品的捷徑與程式檔，使用者資料預設保留。
- 版號變動時同步檢查 `upgrade.cpp` 的版本目錄、`VoiceShortcuts.ps1`、`package_release.py` 與使用手冊。

## 發行前待決事項

全庫授權、程式碼簽章、可重現建置流程與發行管道由維護者決定。套件內容以 `package_release.py` 的明列清單為準，發行前仍須人工檢查隱私與第三方聲明。
