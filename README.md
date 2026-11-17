<div align="center">

<img src="assets/brand/tingzi.svg" width="96" height="96" alt="聽字的橫耳鍵圖示">

# 聽字｜Tingzi

**說完就能打，錯字用注音改。**

Windows 上的繁體中文語音與注音輸入法。<br>
Traditional Chinese voice and Zhuyin input for Windows.

[![最新版本](https://img.shields.io/github/v/release/skydreamer0/tingzi?label=%E6%9C%80%E6%96%B0%E7%89%88%E6%9C%AC)](https://github.com/skydreamer0/tingzi/releases/latest)
[![平台](https://img.shields.io/badge/%E5%B9%B3%E5%8F%B0-Windows%2010%20%2F%2011%20x64-0078D4)](#下載與安裝)
[![授權](https://img.shields.io/github/license/skydreamer0/tingzi?label=%E6%8E%88%E6%AC%8A)](LICENSE)
[![macOS 建置](https://github.com/skydreamer0/tingzi/actions/workflows/macos.yml/badge.svg)](https://github.com/skydreamer0/tingzi/actions/workflows/macos.yml)

[下載](https://github.com/skydreamer0/tingzi/releases/latest) ·
[Windows 使用手冊](windows/README.md) ·
[macOS（準備中）](macos/README.md) ·
[回報問題](https://github.com/skydreamer0/tingzi/issues) ·
[參與開發](CONTRIBUTING.md)

</div>

![聽字的三個步驟：點聽字圖示說話、文字插入游標、用注音改同音字](docs/images/how-it-works.svg)

把游標放在記事本、瀏覽器或任何文字欄位，點一下螢幕邊緣的聽字圖示說話，說完文字就插入游標位置。語音辨識難免寫錯同音字，這時切到聽字內建的注音輸入法，在原地打注音改字就好，不用換工具，也不用另外安裝其他輸入法。

## 為什麼用聽字

- **語音和注音在同一個工具裡**：大段文字用說的，同音字用注音改。改過一次的詞，下次口述會自動寫對。
- **錄音去哪裡由你決定**：可以完全在自己的電腦辨識（本機 Whisper，含針對台灣華語的 Breeze ASR 25），也可以用自己選的線上服務。
- **針對繁體中文**：自動補上中文標點、去掉「嗯、呃」和口吃重複；注音支援大千與倚天鍵盤，長句會依前後文選字。
- **自由軟體**：以 MIT 授權釋出，內建的注音詞庫來自 McBopomofo 的公開詞庫。

## 下載與安裝

| 平台 | 狀態 | 取得方式 |
| --- | --- | --- |
| Windows 10／11（x64） | 預覽版 0.7.7 | [Releases](https://github.com/skydreamer0/tingzi/releases/latest) 下載 `Tingzi-<版本>-windows-x64.zip` |
| macOS | 準備中，尚未發行 | 只有語音輸入，沒有注音；目前只能[自行建置](macos/README.md) |

**Windows 安裝步驟**

1. 從 [Releases](https://github.com/skydreamer0/tingzi/releases/latest) 下載 ZIP，完整解壓縮。
2. 在解壓縮的資料夾，以系統管理員身分執行 `InstallTingzi.cmd`。
3. 按 Windows 鍵搜尋 **聽字**，開啟設定，在「語音」頁選麥克風和辨識方式。

語音浮窗需要 Microsoft Edge WebView2 Runtime，Windows 11 已內建，大多數 Windows 10 也已經有，不必另外安裝 .NET。要移除時，以系統管理員身分執行安裝目錄裡的 `RemoveTingzi.cmd`。升級與更多細節見 [Windows 使用手冊](windows/README.md)。

> [!NOTE]
> 這是預覽版。安裝包沒有數位簽章，Windows 可能會跳出警告；目前也還沒有單一安裝程式，安裝程式正在 [#7](https://github.com/skydreamer0/tingzi/issues/7) 進行。下載頁附有 `.sha256` 檔，可以核對 ZIP 是否完整。
>
> macOS 版自行建置出的 `Tingzi.app` 只有本機簽章，沒有 Developer ID，也沒有經過 Apple 公證，第一次開啟時要在 Finder 按右鍵選「打開」。

## 快速上手

1. 點一下記事本正文，把游標放在要插字的地方。
2. 點螢幕邊緣的聽字圖示，或按 **Ctrl＋Alt＋R** 開始說話；再按一次停止。
3. 文字插入游標位置，同時複製到剪貼簿。
4. 有同音字寫錯時，按 **Win＋空白鍵** 切到「聽字・注音與語音輸入法」，刪掉寫錯的字，在原處打注音，按 **↓** 選字，按 **Enter** 送出。

想按著講、放開就送出，可以在設定的「一般」頁改成「按住說話，放開送出」。

## 功能

### 語音輸入

![語音浮條的組成、換了欄位時的編輯卡與六種狀態](docs/images/voice-bar.svg)

- **常駐浮條**：平常只是螢幕邊緣的一顆圖示，可以拖到左側或右側；錄音、辨識、潤稿時展開顯示狀態，並標示這一句用的是線上還是本機辨識。
- **換欄也不會貼錯**：錄音途中換了欄位，文字不會貼到別的地方，而是留在可以修改的編輯卡裡，也已複製到剪貼簿。
- **整理口語**：去掉句首的「嗯、呃」和口吃重複，補上中文標點，不改寫句子。
- **自訂詞與取代規則**：人名、專有名詞優先寫對；固定聽錯的詞或英文大小寫，可以用「聽到的字 → 要換成的字」修正。
- **從改字學詞**：口述寫錯的詞用注音改對一次，之後同音的口述就會自動寫成這個詞。
- **AI 潤稿（選用，預設關閉）**：把辨識出的文字交給你自己設定的 OpenAI 格式服務，整理改口、贅詞與標點；服務出錯時照常插入原文。
- **辨識紀錄**：最近 50 則只存在自己的電腦，可以從浮窗取回；**Alt＋Shift＋Z** 把上一則再貼一次。
- **依程式設定**：替個別程式指定繁體或簡體、是否整理口語、是否潤稿，以及辨識完成後怎麼處理。

### 注音輸入法

- **內建注音引擎**：不需要安裝其他輸入法，支援標準（大千）與倚天鍵盤。
- **整句選字**：依字詞的常用程度與「哪些詞常接在一起」的統計選字，例如「程式編輯」不會變成「城市編輯」。
- **在句中改字**：組字時用 ← → 移到要改的字，只改那一個；Tab 強制斷詞，選字時按 Tab 看近音字。
- **標點、符號表與範本**：Shift 加標點鍵打全形標點，`` ` `` 開啟符號表；常用的地址或回覆可以存成範本。
- **首碼快打**：只打每個字的第一個注音再按空白鍵，就能叫出之前打過的整句（需開啟本機學習）。

完整的按鍵表與設定說明見 [Windows 使用手冊](windows/README.md)。

### 其他

- **繁體／簡體輸出**：語音結果和注音送出的字都照設定轉換；浮條與系統匣也能把剪貼簿文字簡繁互轉。
- **跟著系統外觀**：深色、淺色模式與 Windows 強調色。
- **設定改了就套用**：「一般」、「語音」、「注音」、「隱私」、「關於」五頁，不需要另外儲存。

## 辨識方式與隱私

在設定的「語音」頁選擇辨識方式。不同方式的錄音去向不一樣：

| 辨識方式 | 錄音送到哪裡 | 需要什麼 |
| --- | --- | --- |
| 本機 Whisper | 留在自己的電腦 | 在設定頁一鍵下載模型與 whisper.cpp；下載前列出來源、大小與授權。有 NVIDIA 顯示卡可以另外下載加速元件 |
| 自選線上服務 | 你填的 OpenAI 格式轉寫服務（例如 OpenAI、Groq），會一併送出自訂詞 | 自備金鑰，費用依該服務計算；網址是 `localhost` 時錄音留在自己的電腦 |
| Codex 線上辨識 | ChatGPT | 這台電腦上有效的 Codex 登入 |

開啟 AI 潤稿時，送出的是辨識出的**文字**（不是錄音），連同自訂詞和你寫的補充說明；沒開啟時不會送出。金鑰存在 Windows 認證管理員，不寫進設定檔。辨識紀錄、從改字學到的詞和本機學習資料都只存在自己的電腦，可以在「隱私」頁清除。

## 文件

| 我想… | 看這裡 |
| --- | --- |
| 安裝、升級、移除，或查按鍵 | [Windows 使用手冊](windows/README.md) |
| 建置或試用準備中的 macOS 版 | [macOS 說明](macos/README.md) |
| 建置、測試、了解原始碼 | [開發指南](docs/development/README.md) |
| 看接下來要做什麼 | [一般用戶體驗規格](docs/specs/general-user-experience.md) |
| 所有文件 | [文件索引](docs/README.md) |

## 參與開發

歡迎回報問題與提交修改。回報時請附上聽字版本、Windows 版本、操作步驟，以及預期與實際結果；語音內容和私人詞庫請用自製範例代替。流程見[貢獻指南](CONTRIBUTING.md)。

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

## 授權

聽字以 [MIT 授權](LICENSE)釋出。內建的第三方元件依各自條款，安裝包隨附 `THIRD-PARTY-NOTICES.txt`。套件不含任何其他廠商的輸入法程式或詞庫。
