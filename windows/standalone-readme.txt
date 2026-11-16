Tingzi standalone Zhuyin RC 0.1.0-rc2
Windows x64 with x86 application support. Unsigned review candidate.

Extract all files. In an elevated 64-bit PowerShell, run:
  powershell.exe -NoProfile -File .\InstallStandalone.ps1
This registers and adds only the standalone profile to the current user's input methods.
It does not select or replace a keyboard. Select 聽字 獨立注音 RC manually with Win+Space.
The same package can retry installation after a registration failure.
A different installed version is refused without replacing files.

From the extracted package or installed directory, run elevated:
  powershell.exe -NoProfile -File .\RemoveStandalone.ps1
Removal unregisters only the standalone identifiers. Files and learning stay local.

Learning starts off. Run the installed TingziStandaloneLearning.exe on to opt in.
Other commands are off, status, export NEW.tsv, forget CONTEXT READING, clear --confirm.
Learning is isolated at %LOCALAPPDATA%\TingziStandaloneRC\preferences.sqlite3.
The service conservatively disables learning for private scopes and unsupported applications.
Two committed explicit corrections in the same context change the next ranking.
Esc cancellations do not learn. An isolated 你/妳 choice does not reorder globally.

Use standard Taiwan Zhuyin physical keys. SU3 produces 你. Down opens candidates.
2 locks 妳 in preedit. Enter commits once. U then Space previews 一.
Esc dismisses a candidate menu first, then cancels the preedit.
End edits the final syllable in 我愛你. Shift alone toggles Chinese/English.
The candidate includes public McBopomofo data and its license. See data-provenance.json.
No vendor program, dictionary or learning file is included or called.
No voice component, account, shortcut or original IME setting is changed.

Validation covers engine behavior, SQLite reopening, and packaged DLL TSF contracts with controlled doubles.
Installation, uninstallation, real TSF text-store behavior, UI placement and application compatibility remain unverified.
This candidate has not been installed on the production machine.

Native installation when PowerShell scripts are unavailable:
Open Command Prompt as administrator and cd to the extracted complete package.
For a new installation, create the directory and copy the complete package:
  mkdir "%ProgramW6432%\TingziStandaloneRC"
  copy /b * "%ProgramW6432%\TingziStandaloneRC\"
Stop if that directory already contains different files. Do not overwrite them.
Run each command separately and stop on any reported failure:
  "%ProgramW6432%\TingziStandaloneRC\TingziStandaloneSetup64.exe" install
  "%ProgramW6432%\TingziStandaloneRC\TingziStandaloneSetup32.exe" install
  "%ProgramW6432%\TingziStandaloneRC\TingziStandaloneSetup64.exe" status
  "%ProgramW6432%\TingziStandaloneRC\TingziStandaloneSetup32.exe" status
To unregister this RC, run the two installed setup executables with remove.
No change to PowerShell execution policy is needed for these native steps.
