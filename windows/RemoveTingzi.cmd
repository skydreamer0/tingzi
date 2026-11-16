@echo off
setlocal EnableExtensions
if "%ProgramW6432%"=="" exit /b 2
set "TARGET=%ProgramW6432%\Tingzi"
if not exist "%TARGET%\TingziSetup64.exe" exit /b 3
if exist "%TARGET%\VoiceShortcuts.ps1" powershell.exe -NoProfile -NonInteractive -File "%TARGET%\VoiceShortcuts.ps1" -Remove
"%TARGET%\TingziSetup32.exe" remove || exit /b 4
"%TARGET%\TingziSetup64.exe" remove || exit /b 4
echo Switch to another input method and close applications using Tingzi before removing the files.
for %%f in (TingziTextService64.dll TingziTextService32.dll TingziPad.exe) do if exist "%TARGET%\%%f" del /q "%TARGET%\%%f"
for %%f in (TingziTextService64.dll TingziTextService32.dll TingziPad.exe) do if exist "%TARGET%\%%f" (
  echo A Tingzi component is in use. Close its applications, then run removal again.
  exit /b 8
)
rem Local dictionaries, voice roots and user data remain available for a later installation.
for %%f in (TingziSetup64.exe TingziSetup32.exe VoiceShortcuts.ps1 ACKNOWLEDGEMENTS.md README.md McBopomofo-LICENSE.txt) do if exist "%TARGET%\%%f" del /q "%TARGET%\%%f"
(
  del /q "%TARGET%\RemoveTingzi.cmd"
  rd "%TARGET%"
  if errorlevel 1 echo Some files are still in use. Close their applications before removing the remaining Tingzi files.
  if /i not "%~1"=="/quiet" pause
  exit /b
)
