---
name: windows-acceptance
description: 在 Windows 電腦上實機驗收聽字（安裝、升級、移除、注音、語音浮窗、自動更新、AI 潤稿），並把結果記到 GitHub issue。用在 issue 標著「status：待實機驗收」、要驗收某個 PR 或發行前的測試版時。
argument-hint: "[issue 編號或安裝程式版本]"
---

# Windows 實機驗收 $ARGUMENTS

CI（`windows.yml`）已經在拋棄式的 Windows 機器上做過安裝、升級與移除，但它沒有麥克風、沒有使用者焦點、不登入任何服務。實機驗收補的是這些。CI 結果與實機結果要分開回報，沒做到的項目寫「未驗收」，不要推測結果。

## 1. 準備

1. 讀要驗收的 issue（`status: 待實機驗收` 或 `testing` 標籤），列出它的「驗收項目」。沒有指定 issue 時，用 [checklist.md](checklist.md) 的「基本」一節。
2. 取得要驗收的安裝程式：PR 的 `Tingzi-windows` 工作流程產物，或草稿 release 的 `Tingzi-Setup-<版本>-x64.exe`。記下 commit 或版本，並用 `Get-FileHash -Algorithm SHA256` 核對 `.sha256`。
3. 驗收要在維護者的 Windows 電腦上做。需要執行安裝程式或錄音時，請維護者操作或同意後再透過遠端工作階段執行；安裝程式會要求系統管理員權限。

## 2. 驗收

依 [checklist.md](checklist.md) 中與這次變更相關的分組逐項做。每一項記下：

- 做了什麼（步驟）、預期結果、實際結果，判定為「通過」「失敗」或「未驗收」。
- 只用自造、可以公開的文字（例如「今天測試文字輸入」）。不附錄音、金鑰、登入資料、私人詞表，截圖不能露出完整使用者路徑或私人內容。

失敗時，先確認可以重現兩次，再收集 `%TEMP%` 裡的「Setup Log <日期>」或設定頁的診斷資訊，去掉私人內容。

## 3. 記錄結果

1. 在 issue 留言或更新「結果紀錄」一節：環境（Windows 版本、x64、辨識方式、聽字版本或 commit）、逐項結果表、失敗項目的重現步驟。寫法依 `github-writing` skill。
2. 全部通過：移除 `status: 待實機驗收` 標籤，關閉 issue（`state_reason` 為 completed）。
3. 有失敗：issue 保持開啟；新的缺陷另開 bug issue，並互相連結。
4. 回報給維護者時，分清楚「CI 驗證過」「實機驗收通過」「未驗收」三類。
