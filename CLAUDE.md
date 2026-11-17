@AGENTS.md

## Claude Code

- 規則都寫在上面匯入的 `AGENTS.md`，要改規則就改那份檔案，不要在這裡另寫一份。
- 專案的 skill 在 `.claude/skills/`：發版用 `/release <版本>`（只能手動執行），實機驗收用 `/windows-acceptance <issue>`，寫 issue 與 PR 時會自動載入 `github-writing`。
- 在雲端容器推送分支後，用 GitHub 工具讀 CI 結果再回報，不要輪詢或等待。
