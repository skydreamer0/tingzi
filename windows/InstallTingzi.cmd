@echo off
setlocal EnableExtensions
rem Run this file from a complete extracted package, using Windows Run as administrator.
if "%ProgramW6432%"=="" exit /b 2
set "TARGET=%ProgramW6432%\Tingzi"
reg query "HKLM\SOFTWARE\Classes\CLSID\{B6ECF4B2-9955-4C24-9BA8-3F7641F0792B}\InprocServer32" /ve /reg:64 >nul 2>&1
if not errorlevel 1 (
  "%~dp0TingziVersion.exe" upgrade || exit /b
  call :voiceShortcuts || exit /b
  call :keyboards
  exit /b 0
)
reg query "HKLM\SOFTWARE\Classes\CLSID\{B6ECF4B2-9955-4C24-9BA8-3F7641F0792B}\InprocServer32" /ve /reg:32 >nul 2>&1
if not errorlevel 1 exit /b 9
for %%f in (THIRD-PARTY-NOTICES.txt pairs.txt pairs-NOTICE.txt TingziFloat\LICENSE TingziVersion.exe TingziKeyboard.exe TingziLearning.exe TingziFloat\TingziFloat.exe) do (
  if not exist "%~dp0%%f" exit /b 4
)
for %%f in (TingziSetup64.exe TingziSetup32.exe TingziTextService64.dll TingziTextService32.dll README.md RemoveTingzi.cmd VoiceShortcuts.ps1) do (
  if not exist "%~dp0%%f" exit /b 4
)
if not exist "%TARGET%\" mkdir "%TARGET%" || exit /b 5
for %%f in (TingziSetup64.exe TingziSetup32.exe TingziTextService64.dll TingziTextService32.dll README.md RemoveTingzi.cmd VoiceShortcuts.ps1) do (
  if not exist "%TARGET%\%%f" copy /y /b "%~dp0%%f" "%TARGET%\%%f" >nul || goto copyfailed
)
for %%f in (Tingzi.cin phrases.txt McBopomofo-LICENSE.txt ACKNOWLEDGEMENTS.md) do if exist "%~dp0%%f" if not exist "%TARGET%\%%f" (
  if not exist "%TARGET%\%%f" copy /y /b "%~dp0%%f" "%TARGET%\%%f" >nul || goto copyfailed
)
"%TARGET%\TingziSetup64.exe" install || goto failed
"%TARGET%\TingziSetup32.exe" install || goto failed
"%TARGET%\TingziSetup64.exe" status || goto failed
"%TARGET%\TingziSetup32.exe" status || goto failed
"%~dp0TingziVersion.exe" upgrade || goto failed
call :voiceShortcuts || exit /b
call :keyboards
echo Tingzi Zhuyin and voice input installed. Use Win+Space to select it.
if /i not "%~1"=="/quiet" pause
exit /b 0
:voiceShortcuts
powershell.exe -NoProfile -NonInteractive -File "%~dp0VoiceShortcuts.ps1"
exit /b
:keyboards
rem Windows lists the input method only for users who have it among their keyboards. Add it for the
rem account running this script; a failure is not fatal, the settings page can add it later.
"%~dp0TingziKeyboard.exe" enable >nul 2>&1
exit /b 0
:failed
echo Registration failed. Cleaning up the new installation.
"%TARGET%\TingziSetup32.exe" remove
"%TARGET%\TingziSetup64.exe" remove
if /i not "%~1"=="/quiet" pause
exit /b 6
:copyfailed
echo Package copy failed. Registration was not attempted.
rem Existing local files are retained after a failed first registration.
exit /b 7
