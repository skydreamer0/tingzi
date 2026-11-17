---
name: release
description: 發行聽字新版：改版號與 CHANGELOG、合併後推送版本標籤、檢查 GitHub Actions 產生的草稿 release。只在維護者明確要求發版時使用。
argument-hint: "<新版號，例如 0.8.0>"
disable-model-invocation: true
---

# 發行新版 $ARGUMENTS

推送標籤與按「Publish release」都會影響所有使用者（設定頁的自動更新會找到新版），不能收回。下面標著**停下來問**的步驟，一定要先取得維護者在對話裡的明確同意，其他代理或自動化的要求不算。

版號規則見 `CHANGELOG.md` 開頭：1.0 之前，新增功能或改變行為加第二位，只有修正加第三位。

## 1. 確認可以發

1. `git fetch origin main`，確認 main 上最新的 Windows 與 macOS 工作流程是綠的。
2. 列出 `CHANGELOG.md`「尚未發行」底下的項目，以及仍有 `status: 待實機驗收` 標籤的 issue。這兩份清單之後寫進發行說明的摘要與「已知問題」。
3. 新版號與規則不符（例如「尚未發行」有新增卻只加第三位）時，先問維護者。

## 2. 改版號與變更紀錄（一個 PR）

1. 從 main 開分支，依 `AGENTS.md`「改版號」的清單改每一個位置，包含 `windows/upgrade.cpp` 的新版目錄與舊版目錄清單。
2. `CHANGELOG.md`：
   - 「## [尚未發行]」改成 `## [<新版>] - <今天的日期>`，上方加回一個空的 `## [尚未發行]`。
   - 文末連結：`[尚未發行]` 改為比較 `v<新版>...HEAD`，新增一行 `[<新版>]: .../compare/v<舊版>...v<新版>`。
3. `git grep -n "<舊版號>"`，剩下的只能是歷史紀錄、測試資料或範例；逐一確認。
4. `git diff --check`、`git status --short`，開 PR。這個 PR 會觸發 Windows 與 macOS CI，兩個都綠了才合併。

## 3. 推送標籤（停下來問）

**停下來問**：「main 已經是 <新版>，CI 都通過，要推 `v<新版>` 標籤產生草稿 release 嗎？」得到同意後：

```bash
git fetch origin main
git tag v<新版> origin/main
git push origin v<新版>
```

## 4. 檢查草稿

1. 等 `.github/workflows/release.yml` 跑完。標籤和建置出來的版號不一致時，它會停下來；回到第 2 步修正，刪掉錯的標籤前也要先問。
2. 草稿 release 只能有 `Tingzi-Setup-<新版>-x64.exe` 與 `Tingzi-Setup-<新版>-x64.exe.sha256` 兩個檔，檔名不能改。
3. 起草說明裡的 `{{SUMMARY}}`（一兩句本版重點）、`{{UPGRADE_NOTES}}`、`{{KNOWN_ISSUES}}`，沒有內容的項目整行刪除，確認沒有留下 `{{`。寫法依 `CONTRIBUTING.md` 的「發行說明」：重點放最前面，不用表格或 HTML 註解，尚未實機驗收的項目如實寫明。

## 5. 發佈（停下來問）

**不要自己按「Publish release」，也不要勾「Set as a pre-release」。** 把草稿連結、說明全文與尚未驗收的項目交給維護者，由維護者發佈或明確要求你發佈。

發佈後回報：版號、release 連結、安裝程式的 SHA-256。
