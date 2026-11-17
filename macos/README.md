# 聽字・語音輸入｜macOS（準備中）

macOS 版只有語音輸入：螢幕邊緣的聽字圖示、錄音、辨識、把文字貼回原本的程式，以及設定頁。注音輸入法是 Windows 的文字服務，macOS 版沒有；打注音請用 macOS 內建的注音輸入法。

**目前狀態：** 還沒有發行，也還沒有在 Mac 上實際執行驗收。程式碼已能以 macOS 為目標通過型別檢查；完整的編譯、測試與打包由 GitHub Actions 的 macOS 工作流程執行。下面的步驟是給想先試用或協助測試的人。

## 建置

需要 macOS 11 以上、Rust（stable）、Python 3 與 Xcode 命令列工具。在儲存庫根目錄執行：

```bash
bash macos/build.sh
```

完成後會有 `.build/macos/Tingzi.app` 和同一個資料夾裡的 ZIP。程式只做了本機簽章，沒有 Developer ID；拿到別台 Mac 上第一次開啟時，要在 Finder 對它按右鍵選「打開」。

只想跑測試或開發版：

```bash
cargo test --manifest-path app/src-tauri/Cargo.toml
```

```bash
cargo run --manifest-path app/src-tauri/Cargo.toml
```

## 第一次使用

1. 開啟 `Tingzi.app`。它是選單列程式，不會出現在 Dock；選單列和螢幕邊緣各有一顆聽字圖示。
2. 第一次錄音時，macOS 會詢問是否允許使用**麥克風**。
3. 第一次要把文字貼回其他程式時，macOS 會要求**輔助使用**權限（系統設定 > 隱私權與安全性 > 輔助使用）。允許之前，文字只會複製到剪貼簿並留在浮窗，自己按 Command＋V 貼上。
4. 重新建置後簽章會改變，macOS 可能再問一次這兩項權限。
5. 第一次在這台 Mac 開啟時，設定視窗會帶你選麥克風、選辨識方式，並在文字編輯裡練習一句。每一步都可以略過，之後可以在「關於」頁按「首次設定」重來。

## 和 Windows 版不同的地方

| 項目 | macOS |
| --- | --- |
| 錄音快捷鍵 | Control＋Option＋R；可在設定改成 Control＋Shift 或 Option＋Shift 加字母 |
| 再貼一次上一則 | Option＋Shift＋Z |
| 貼入方式 | 一律經剪貼簿送出 Command＋V |
| 換了地方就不貼 | 以「程式」判斷：錄音途中切到別的程式就不貼入。同一個程式裡換視窗或換欄位無法分辨，文字會貼到目前游標所在的地方 |
| 依程式設定 | 程式以應用程式名稱比對，例如 `Safari.app` |
| 貼入 Codex 草稿（Control＋Option＋V） | 鍵盤在 Codex 桌面版的任何欄位都會貼入，不分訊息框或搜尋框 |
| 自選服務的金鑰 | 存在登入鑰匙圈 |
| AI 潤稿的金鑰 | 存在登入鑰匙圈，和自選服務的金鑰分開 |
| 登入時啟動 | 在 `~/Library/LaunchAgents/app.tingzi.voice.plist` 寫一個啟動項目 |
| 個人資料 | `~/Library/Application Support/Tingzi` |
| 檢查更新 | 和 Windows 版一樣在「關於」頁檢查與提示，但還沒有 macOS 的安裝程式，所以按鈕是「開啟下載頁」 |
| 注音頁 | 只留下自訂詞；鍵盤配置、文字範本、本機學習等注音輸入法的項目不顯示 |

## 本機 Whisper

1. 安裝 whisper.cpp 的命令列工具，例如 `brew install whisper-cpp`，會得到 `/opt/homebrew/bin/whisper-cli`。也可以把 `whisper-cli` 放進 `~/Library/Application Support/Tingzi/whisper/`。
2. 到設定的「語音」頁選「本機 Whisper」，在模型清單按「下載」。下載前會列出來源、大小與授權，下載後核對檔案並試跑一次。自己準備的模型檔（檔名是 `ggml-<名稱>.bin`）也可以放進 `~/Library/Application Support/Tingzi/whisper/downloads/`。
3. 要用顯示卡辨識，開啟同一頁的「顯示卡加速」；`whisper-cli` 本身就支援，不必另外下載。

## AI 潤稿

選用，預設關閉，用法和 Windows 版相同：在設定的「語音」頁填入 OpenAI 格式的服務網址、模型名稱與金鑰，按「測試」試試，再打開開關。開啟後，辨識出的文字（不是錄音）會連同自訂詞和「補充說明」送到這個服務；網址是 `localhost`（例如 Ollama）時文字留在這台 Mac。服務出錯時照常貼入原文。每次口述完成時，浮條會註明這一句有沒有潤稿；浮窗的「最近紀錄」可以取回潤稿前的文字來比較。

## 還沒做的事

- 在實機上驗收錄音、三種辨識方式、AI 潤稿、貼入、快捷鍵、選單列選單與設定頁。
- Developer ID 簽章與公證；目前其他 Mac 開啟時會看到 Gatekeeper 警告。
- 應用程式圖示目前由 256 px 的圖放大，之後換成完整尺寸的圖示。
- 一份程式同時支援 Apple 晶片與 Intel；目前建置出來的只適用於建置它的那種 Mac。
