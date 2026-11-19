聽字 {{VERSION}} 是 1.0 之前的預覽版。{{SUMMARY}}

## 升級須知

- 已安裝舊版時，執行安裝程式即可升級，設定、自訂詞與學習資料都會保留。
- {{UPGRADE_NOTES}}

{{CHANGES}}

## 已知問題

- 安裝程式沒有數位簽章，Windows 可能會顯示警告；macOS 版沒有 Apple 的開發者簽章與公證，第一次開啟要到系統設定允許。
- {{KNOWN_ISSUES}}

## 下載與安裝

- `Tingzi-Setup-{{VERSION}}-x64.exe`：Windows 10 與 11（x64）的安裝程式，全新安裝與升級都使用這個檔案。
- `Tingzi-Setup-{{VERSION}}-x64.exe.sha256`：安裝程式的 SHA-256 雜湊值。
- `Tingzi-{{VERSION}}-macos.dmg`：macOS 11 以上的磁碟映像檔，Apple 晶片與 Intel 的 Mac 都能用。macOS 版只有語音輸入，還沒有實機驗收。
- `Tingzi-{{VERSION}}-macos.dmg.sha256`：磁碟映像檔的 SHA-256 雜湊值。

**Windows**

1. 下載安裝程式後，在 PowerShell 執行 `Get-FileHash .\Tingzi-Setup-{{VERSION}}-x64.exe -Algorithm SHA256`，確認結果是 `{{SHA256}}`。
2. 執行安裝程式。如果出現「Windows 已保護您的電腦」，按「其他資訊」，再按「仍要執行」。
3. 安裝完成後，按 Windows＋空白鍵選擇「聽字・注音與語音輸入法」。

詳細步驟、無人值守安裝與移除方式，見 Windows 使用手冊：https://github.com/skydreamer0/tingzi/blob/main/windows/README.md

**macOS**

1. 下載後在「終端機」執行 `shasum -a 256 Tingzi-{{VERSION}}-macos.dmg`，確認結果和 `.sha256` 檔裡的一樣。
2. 打開磁碟映像檔，把「Tingzi」拖到「應用程式」資料夾。
3. 第一次開啟會被 macOS 擋下，因為程式沒有 Apple 的開發者簽章。到「系統設定 > 隱私權與安全性」，在「安全性」一段按「強制打開」。

詳細步驟見 macOS 說明：https://github.com/skydreamer0/tingzi/blob/main/macos/README.md

## 隱私

選擇「本機 Whisper」時，錄音只在這台電腦上辨識。選擇「自選線上服務」時，錄音與自訂詞會送往所設定的服務，費用依該服務計算。選擇「Codex 線上辨識」時，錄音會經 Codex 送往 ChatGPT。開啟 AI 潤稿時，辨識出的文字會送往所設定的服務，錄音不會送出。

## 授權

聽字以 MIT 授權發佈。注音詞庫取自 McBopomofo（MIT）。詞語搭配統計由中文維基百科的文字計算而來，以 CC BY-SA 4.0 提供，只含統計數字，不含文章內容，說明見 `pairs-NOTICE.txt`。其他第三方元件的授權全文見 `THIRD-PARTY-NOTICES.txt`。

完整變更：https://github.com/skydreamer0/tingzi/compare/v{{PREVIOUS}}...v{{VERSION}}
