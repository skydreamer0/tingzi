# 聽字圖示集

介面上的按鈕圖示。每個圖示畫在 24×24 的格子上，使用 2 px 圓頭線條、不填色，顏色一律取 `currentColor`，由程式依目前色系上色。這套線條與圓角和[橫耳鍵](../brand/README.md)的筆畫一致。

| 檔案 | 用途 |
| --- | --- |
| `mic.svg` | 麥克風 |
| `stop.svg` | 停止 |
| `settings.svg` | 設定 |
| `close.svg` | 關閉 |
| `copy.svg` | 複製 |
| `edit.svg` | 編輯 |
| `keyboard.svg` | 切換注音輸入法 |
| `pad.svg` | 打字板 |
| `learn.svg` | 本機學習 |
| `language.svg` | Windows 語言設定 |
| `check.svg` | 儲存 |
| `back.svg` | 返回 |
| `convert.svg` | 簡繁轉換 |
| `audio-file.svg` | 辨識音檔 |
| `power.svg` | 登入時啟動 |
| `theme.svg` | 色系 |
| `retry.svg` | 改用本機辨識 |
| `trash.svg` | 捨棄 |
| `float.svg` | 顯示浮條 |
| `exit.svg` | 結束 |
| `info.svg` | 關於 |
| `privacy.svg` | 隱私 |
| `plus.svg` | 加入 |
| `export.svg` | 匯出 |
| `folder.svg` | 開啟資料夾 |
| `link.svg` | 開啟網頁 |

## 新增或修改圖示

- 維持 `viewBox="0 0 24 24"`、線寬 2、圓頭圓角，四周留 3 px 邊界。
- 程式內建的繪製器只支援 `path`（`M L H V C Z` 指令，大小寫皆可）、`circle`、`rect`（可帶 `rx`）和 `line`。不要使用圓弧指令、`transform`、漸層或文字。
- 需要實心的部分在該元素加 `fill="currentColor"`。
- 檔案會嵌入語音浮窗程式；改完重新建置即可。
