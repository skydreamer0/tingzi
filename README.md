<img src="assets/brand/tingzi.svg" width="80" height="80" alt="聽字的橫耳鍵圖示">

# 聽字｜Tingzi

**說完就能打，錯字用注音改。**

聽字・注音與語音輸入法是 Windows 上的繁體中文輸入工具。把游標放在記事本、瀏覽器或其他文字欄位，說完一句就會插入文字；遇到同音字，直接用注音重新選字。

Traditional Chinese voice and Zhuyin input for Windows.

[Windows 使用手冊](windows/README.md) · [回報問題](https://github.com/skydreamer0/tingzi/issues) · [參與開發](CONTRIBUTING.md)

## 功能

- **語音輸入**：螢幕右側常駐一顆麥克風，點一下開始、再點一下停止。辨識完成時，若游標還在原欄位，文字只插入一次，同時複製到剪貼簿。
- **換欄也不會貼錯**：錄音途中換了欄位，文字會留在浮窗預覽，讓你自己決定貼到哪裡。
- **注音修字**：用 Win＋空白鍵切到「聽字・注音與語音輸入法」，在要改的位置打注音選字。
- **本機或線上辨識**：可選本機 Whisper，或透過第三方 `codex-asr` 使用線上辨識。浮條會標示每一句實際用了哪個引擎。

## 目前狀態

主分支為 **0.6.0 開發版**，尚未發布安裝包。要試用請依[開發指南](docs/development/README.md)自行建置。接下來要補齊的項目列在[一般用戶體驗規格](docs/specs/general-user-experience.md)。

## 安裝（自行建置的套件）

1. 準備 x64 Windows 10/11，以及 [.NET 8 Desktop Runtime（x64）](https://dotnet.microsoft.com/download/dotnet/8.0)。
2. 完整解壓縮套件，以系統管理員身分執行 `InstallTingzi.cmd`。
3. 開始功能表搜尋 **聽字**，開啟設定，在「語音輸入」選麥克風和辨識方式。

移除時以系統管理員身分執行安裝目錄裡的 `RemoveTingzi.cmd`。詳細步驟見 [Windows 使用手冊](windows/README.md)。

## 第一次輸入

1. 點一下記事本正文，把游標放在要插字的地方。
2. 點右側麥克風，或按 **Ctrl＋Alt＋R** 開始說話；再按一次停止。
3. 文字會插入游標位置。要改同音字，按 **Win＋空白鍵** 選「聽字・注音與語音輸入法」，打注音重新選字。

**錄音會送去哪裡？** 選「本機 Whisper」時在自己的電腦辨識，工具與模型須自行配置。選「線上辨識」時，`codex-asr` 會把該段錄音送往 ChatGPT 辨識，需要有效的登入。

## 參與專案

回報問題時，請附上版本、Windows 版本、操作步驟，以及預期與實際結果。語音內容和私人詞庫請用自製範例代替。[貢獻指南](CONTRIBUTING.md) · [開發與建置](docs/development/README.md) · [文件索引](docs/README.md)

## 授權

全庫授權尚待維護者選定。`AudioNotes` 衍生檔案採用 [MIT 授權](windows/float/AudioNotes/LICENSE)；NAudio、codex-asr 等第三方元件依各自條款，套件隨附 `THIRD-PARTY-NOTICES.txt`。內建的注音字詞資料來自 McBopomofo，依其 MIT 授權提供。套件不含任何其他廠商的輸入法程式或詞庫。
