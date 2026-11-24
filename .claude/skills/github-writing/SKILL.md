---
name: github-writing
description: 聽字儲存庫的 issue、PR 說明、GitHub 留言與標籤寫法。新開或修改 issue、寫 PR 說明、加標籤、在 GitHub 留言或關閉 issue 之前使用。
---

# issue 與 PR 寫作

儲存庫是公開的。所有文字都用繁體中文、完整句子，寫需求與事實。

## 共同規則

- 不寫維護者或任何個人的名字。不要寫「某人希望……」「請某人實測」，改寫成「需求：……」「需要實機驗收」。
- 不要口語敘述對話經過，只寫需求、限制、結果。
- 用「使用者」，不用「用戶」；用「AI 潤稿」，不用「AI Refine」。功能名稱用介面上實際的按鈕與頁面名稱。
- issue 裡連到儲存庫文件時，用完整網址 `https://github.com/skydreamer0/tingzi/blob/main/<路徑>`，相對路徑在 issue 裡會失效。PR 說明與儲存庫內的文件可以用相對路徑。
- 不貼帳號、token、金鑰、私人詞表、錄音或含完整使用者路徑的內容。

## issue 段落

一張 issue 只放一個可以獨立驗收的需求。需求有好幾項時，開一張追蹤 issue，再把每項開成子 issue。

| 類型 | 段落 |
| --- | --- |
| 功能需求、缺陷 | 背景／需求／限制與不在範圍／待決定事項／驗收條件／狀態／相關 |
| 實機驗收 | 驗收項目／結果紀錄 |
| 技術評估 | 目的／方法／判定標準／參考 |

使用者回報的問題用 `.github/ISSUE_TEMPLATE/` 的表單，那些表單有自己的欄位。

## 標籤

每張 issue 一個類型標籤，加一個或多個功能區標籤，必要時加狀態標籤。

- 類型：`bug`、`enhancement`、`testing`（實機驗收）、`research`（技術評估）、`documentation`。
- 功能區：`area: 語音浮窗`、`area: 注音輸入法`、`area: 安裝與更新`、`area: AI 潤稿`。
- 狀態：`status: 待決定`（需要維護者決定才能做）、`status: 待實機驗收`（程式已合併，還沒在實機驗收）。

## issue 的開關

- 功能與缺陷 issue 在 PR 合併後關閉，`state_reason` 用 completed；不做時用 not_planned 並寫原因。
- 合併後還沒實機驗收的項目，加進實機驗收的追蹤 issue（目前是 [#6](https://github.com/skydreamer0/tingzi/issues/6)「實機驗收：0.8.0 與之後合併的功能」）的勾選清單，寫明 PR 編號與要驗收什麼。
- 追蹤 issue（例如 #46 選字品質）與實機驗收 issue 保持開啟，加 `status: 待實機驗收`；驗收全部通過才關閉並移除這個標籤。
- 暫緩的 issue 保持開啟，留言寫「狀態：暫緩」與原因。

## PR

- 標題與 commit 第一行用英文祈使句，描述使用者看得到的改變，不加 `feat:` 之類的前綴。
- 說明依 `.github/pull_request_template.md` 的段落填寫，CI 驗證與實機驗收分開寫，沒驗證的項目明列。
- 有對應的功能或缺陷 issue 時寫 `Closes #<編號>`；連到追蹤 issue 或實機驗收 issue 時寫 `Refs #<編號>`，讓它保持開啟。
