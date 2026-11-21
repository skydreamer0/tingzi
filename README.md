<div align="center">

**繁體中文** · [English](README.en.md)

<img src="assets/brand/tingzi.svg" width="96" height="96" alt="聽字的橫耳鍵圖示">

# 聽字｜Tingzi

**說完就能打，錯字用注音改。**

Windows 上的繁體中文語音與注音輸入法。<br>
Traditional Chinese voice and Zhuyin input for Windows.

[![最新版本](https://img.shields.io/github/v/release/skydreamer0/tingzi?label=%E6%9C%80%E6%96%B0%E7%89%88%E6%9C%AC)](https://github.com/skydreamer0/tingzi/releases/latest)
[![平台](https://img.shields.io/badge/%E5%B9%B3%E5%8F%B0-Windows%2010%20%2F%2011%20x64-0078D4)](#下載與安裝)
[![授權](https://img.shields.io/github/license/skydreamer0/tingzi?label=%E6%8E%88%E6%AC%8A)](LICENSE)
[![Windows 建置](https://github.com/skydreamer0/tingzi/actions/workflows/windows.yml/badge.svg)](https://github.com/skydreamer0/tingzi/actions/workflows/windows.yml)
[![macOS 建置](https://github.com/skydreamer0/tingzi/actions/workflows/macos.yml/badge.svg)](https://github.com/skydreamer0/tingzi/actions/workflows/macos.yml)

[下載](https://github.com/skydreamer0/tingzi/releases/latest) ·
[Windows 使用手冊](windows/README.md) ·
[macOS（準備中）](macos/README.md) ·
[回報問題](https://github.com/skydreamer0/tingzi/issues) ·
[參與開發](CONTRIBUTING.md)

</div>

![聽字的三個步驟：點聽字圖示說話、文字插入游標、用注音改同音字](docs/images/how-it-works.svg)

把游標放在記事本、瀏覽器或任何文字欄位，點一下螢幕邊緣的聽字圖示說話，說完文字就插入游標位置。語音辨識難免寫錯同音字，這時切到聽字內建的注音輸入法，在原處打注音改字，不必切換到其他工具或另外安裝輸入法。

## 為什麼用聽字

- **語音和注音在同一個工具裡**：大段文字用說的，同音字用注音改。改過一次的詞，下次口述會自動寫對。
- **自行決定錄音的去向**：可以完全在自己的電腦辨識（本機 Whisper，含針對台灣華語的 Breeze ASR 25），也可以用自己選的線上服務。
- **針對繁體中文**：自動補上中文標點、去掉「嗯、呃」和口吃重複；注音支援大千與倚天鍵盤，長句會依前後文選字。
- **開放原始碼**：程式以 MIT 授權釋出，內建的注音詞庫來自 McBopomofo 的公開詞庫。

## 下載與安裝

| 平台 | 狀態 | 取得方式 |
| --- | --- | --- |
| Windows 10／11（x64） | 預覽版 | [Releases](https://github.com/skydreamer0/tingzi/releases/latest) 下載 `Tingzi-Setup-<版本>-x64.exe`（0.7.7 以前是 ZIP） |
| macOS 11 以上 | 準備中，尚未實機驗收 | 只有語音輸入，沒有注音；自 0.10.0 起可在 [Releases](https://github.com/skydreamer0/tingzi/releases/latest) 下載 `Tingzi-<版本>-macos.dmg`，拖到「應用程式」資料夾，見 [macOS 說明](macos/README.md) |

**Windows 安裝步驟**

1. 從 [Releases](https://github.com/skydreamer0/tingzi/releases/latest) 下載 `Tingzi-Setup-<版本>-x64.exe`。
2. 雙擊它，依畫面按「下一步」；Windows 詢問是否允許變更時按「是」。
3. 裝好後語音浮窗會出現在螢幕右側，第一次開啟會帶你選麥克風和辨識方式。按 Windows＋空白鍵可以切換到「聽字・注音與語音輸入法」。

安裝畫面會顯示正在解開安裝檔、複製程式檔與註冊輸入法，或把聽字加進鍵盤清單。安裝程式會把聽字加進你的鍵盤清單（缺少中文（台灣）時一併加入），缺少 Microsoft Edge WebView2 Runtime 時也會自動下載，不必另外安裝 .NET。已安裝的電腦再執行新版安裝程式就是升級；要移除時，到 Windows 的「設定 → 應用程式 → 已安裝的應用程式」按「解除安裝」。0.7.7 以前的版本是 ZIP 加 `InstallTingzi.cmd`。更多細節見 [Windows 使用手冊](windows/README.md)。

> [!NOTE]
> 這是預覽版。安裝程式沒有數位簽章，Windows 可能會顯示「Windows 已保護您的電腦」，按「其他資訊」再按「仍要執行」。下載頁附有 `.sha256` 檔，可以核對下載的檔案是否完整。
>
> macOS 版只有本機簽章，沒有 Apple 的開發者簽章，也沒有經過 Apple 公證。第一次開啟會被擋下，要到「系統設定 > 隱私權與安全性」按「強制打開」（macOS 14 以前在 Finder 按右鍵選「打開」）。

## 快速上手

1. 點一下記事本正文，把游標放在要插字的地方。
2. 點螢幕邊緣的聽字圖示，或按 **Ctrl＋Alt＋R** 開始說話；再按一次停止。
3. 文字插入游標位置，同時複製到剪貼簿。
4. 有同音字寫錯時，按 **Windows＋空白鍵** 切到「聽字・注音與語音輸入法」，刪掉寫錯的字，在原處打注音，按 **↓** 選字，按 **Enter** 送出。

若想按住快捷鍵說話、放開時送出，可以在設定的「一般」頁改成「按住說話，放開送出」。錄音快捷鍵也在同一頁更換。

## 功能

### 首次設定與更新

- **首次設定**：第一次開啟時，設定視窗會依序帶你選麥克風並測試收音、選辨識方式，並在記事本口述一句、用注音改一個字。每一步都可以略過，之後可在「關於」頁重新開始。
- **自動更新（Windows）**：每天最多檢查一次新版，找到時在背景下載並核對安裝程式，詢問後才安裝；設定與資料都會保留。可以在「關於」頁關閉，改成手動檢查。

### 語音輸入

![語音浮條的組成、換了欄位時的編輯卡與六種狀態](docs/images/voice-bar.svg)

- **常駐浮條**：平常只是螢幕邊緣的一顆圖示，可以拖到左側或右側；錄音、辨識、潤稿時展開顯示狀態，並標示這一句用的是線上還是本機辨識。 編輯區與浮條會往圖示所在的一側收合。
- **換欄也不會貼錯**：錄音途中換了欄位，文字不會貼到別的地方，而是留在可以修改的編輯卡裡，也已複製到剪貼簿。
- **整理口語**：去掉句首的「嗯、呃」和口吃重複，補上中文標點，不改寫句子。
- **自訂詞與取代規則**：人名、專有名詞優先寫對；固定聽錯的詞或英文大小寫，可以用「聽到的字 → 要換成的字」修正。
- **從改字學詞**：口述寫錯的詞用注音改對一次，之後同音的口述就會自動寫成這個詞。
- **AI 潤稿（選用，預設關閉）**：把辨識出的文字交給你自己設定的 OpenAI 格式服務，整理改口、贅詞與標點；浮條會註明每一句有沒有潤稿，紀錄裡可以取回潤稿前的文字，服務出錯時照常插入原文。
- **錄音快捷鍵**：預設 **Ctrl＋Alt＋R**，可以在設定頁直接按下想用的組合來更換，也能用 F1 到 F24 單鍵。
- **辨識紀錄**：最近 50 則只存在自己的電腦，可以從浮窗取回；**Alt＋Shift＋Z** 把上一則再貼一次。
- **依程式設定**：替個別程式指定繁體或簡體、是否整理口語、是否潤稿，以及辨識完成後怎麼處理。

### 注音輸入法

- **內建注音引擎**：不需要安裝其他輸入法，支援標準（大千）與倚天鍵盤。
- **整句選字**：依字詞的常用程度與「哪些詞常接在一起」的統計選字，例如「程式編輯」不會變成「城市編輯」。
- **在句中改字**：組字時用 ← → 移到要改的字，只改那一個；Tab 強制斷詞，選字時按 Tab 看近音字，也能找回大千或倚天鍵盤上按到隔壁鍵的字。
- **標點、符號表與範本**：Shift 加標點鍵打全形標點，`` ` `` 可在有無組字時開啟符號表；游標在尚未送出的符號後，按 ↑ 可換成別的符號，括號後按 ↓ 可換成同側括號。常用的地址或回覆可以存成範本。
- **首碼快打**：只打每個字的第一個注音再按空白鍵，就能叫出之前打過的整句（需開啟本機學習）。

完整的按鍵表與設定說明見 [Windows 使用手冊](windows/README.md)。

### 其他

- **繁體／簡體輸出**：語音結果和注音送出的字都照設定轉換；浮條與系統匣也能把剪貼簿文字簡繁互轉。
- **依系統外觀**：深色、淺色模式與 Windows 強調色。
- **設定即時套用**：「一般」、「語音」、「注音」、「隱私」、「關於」五頁，變更後立即生效，不需要另外儲存。

## 辨識方式與隱私

在設定的「語音」頁選擇辨識方式。不同方式的錄音去向不一樣：

| 辨識方式 | 錄音送到哪裡 | 需要什麼 |
| --- | --- | --- |
| 本機 Whisper | 留在自己的電腦 | 在設定頁一鍵下載模型與 whisper.cpp；下載前列出來源、大小與授權，不用的模型可以刪除，也不必另外安裝 Visual C++ 執行階段。有 NVIDIA 顯示卡可以另外下載加速元件 |
| 自選線上服務 | 你填的 OpenAI 格式轉寫服務（例如 OpenAI、Groq），會一併送出自訂詞 | 自備金鑰，費用依該服務計算；網址是 `localhost` 時錄音留在自己的電腦 |
| Codex 線上辨識（非官方） | ChatGPT | 這台電腦上有效的 Codex 登入 |

> [!WARNING]
> 「Codex 線上辨識」透過第三方的非官方元件 [codex-asr](https://crates.io/crates/codex-asr)，沿用這台電腦上 Codex 的 ChatGPT 登入來辨識錄音。這不是 OpenAI 提供或支援的功能，OpenAI 調整服務時可能隨時無法使用。需要穩定運作時，請改用本機 Whisper 或自選線上服務。

開啟 AI 潤稿時，送出的是辨識出的**文字**（不是錄音），連同自訂詞和你寫的補充說明；沒開啟時不會送出。金鑰存在 Windows 認證管理員，不寫進設定檔。辨識紀錄、從改字學到的詞和本機學習資料都只存在自己的電腦，可以在「隱私」頁清除。

「關於」頁的自動更新開啟時，聽字每天最多向 GitHub 查詢一次有沒有新版，只送出目前的版本號；關閉後不會自動連線。

聽字會在個人資料夾的 `tingzi-log.txt` 留一份操作記錄，寫下何時用什麼開始錄音（快捷鍵、浮窗按鈕或選單）、要貼進哪個程式、用哪種方式辨識、花了多久、哪裡失敗或當掉，方便回報問題。注音輸入法另外記在 `tingzi-ime-log.txt`，寫下在哪個程式啟用、哪些按鍵沒能交給輸入法、組字被程式中斷等，不記你按了哪些鍵或打出的字。記錄裡沒有你說的話或打的字、錄音和金鑰，只存在自己的電腦，超過 256 KB 會換新檔，舊的只留一份。在「關於」頁或系統匣選單按「回報問題…」，會先列出要附上的內容讓你看過、刪改；按「到 GitHub 回報」才會開啟瀏覽器的回報頁，送不送出由你決定。

## 文件

| 我想… | 看這裡 |
| --- | --- |
| 安裝、升級、移除，或查按鍵 | [Windows 使用手冊](windows/README.md) |
| 建置或試用準備中的 macOS 版 | [macOS 說明](macos/README.md) |
| 建置、測試、了解原始碼 | [開發指南](docs/development/README.md) |
| 看接下來要做什麼 | [一般用戶體驗規格](docs/specs/general-user-experience.md) |
| 所有文件 | [文件索引](docs/README.md) |

## 參與開發

歡迎回報問題與提交修改。最簡單的方式是在設定的「關於」頁按「回報問題…」，聽字會整理好版本、系統、設定和最近的操作記錄，你補上操作步驟與預期、實際結果即可；語音內容和私人詞庫請用自製範例代替。流程見[貢獻指南](CONTRIBUTING.md)。資安問題請不要公開回報，改依[資安政策](SECURITY.md)私下通知。

聽字由兩部分組成：`windows/` 是 C++ 寫的 Windows 文字服務（注音輸入法），`app/` 是 Rust（Tauri 2）寫的語音浮窗與設定頁。在 Windows 安裝 Rust 與 Visual Studio C++ 工具後，可以先跑浮窗的測試：

```powershell
cd app/src-tauri
cargo test
```

完整建置與打包步驟見[開發指南](docs/development/README.md)。

## 致謝

- [McBopomofo 小麥注音](https://github.com/openvanilla/McBopomofo)：內建的注音字詞資料（MIT 授權）
- [whisper.cpp](https://github.com/ggml-org/whisper.cpp) 與 [Whisper](https://github.com/openai/whisper) 模型：本機語音辨識
- [Breeze ASR 25](https://huggingface.co/MediaTek-Research/Breeze-ASR-25)（聯發科創新基地）：針對台灣華語與中英夾雜的辨識模型
- [codex-asr](https://crates.io/crates/codex-asr)：Codex 線上辨識
- [Tauri](https://tauri.app/)：語音浮窗與設定頁
- [中文維基百科](https://zh.wikipedia.org/)：整句選字使用的詞語搭配統計（`pairs.txt`）由維基百科的文章統計而來

## 授權

聽字的程式以 [MIT 授權](LICENSE)釋出。以下內容依各自的授權：

- 注音字詞資料取自 McBopomofo，MIT 授權。
- 詞語搭配統計 `windows/standalone-data/pairs.txt` 由中文維基百科統計而來，依 [CC BY-SA 4.0](https://creativecommons.org/licenses/by-sa/4.0/deed.zh-hant) 授權，說明見同目錄的 `pairs-NOTICE.txt`。
- 其他第三方元件的授權列在安裝包隨附的 `THIRD-PARTY-NOTICES.txt`。

除了上述 McBopomofo 的詞庫，聽字不含其他輸入法的程式或詞庫。聽字是獨立專案，與 OpenAI、Microsoft、Apple 沒有關聯。
