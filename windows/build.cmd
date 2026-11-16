@echo off
setlocal
pushd "%~dp0.."
set "PRODUCT=%CD%\.build"
if not "%~4"=="" set "PRODUCT=%~f4"
set "OUTPUT=%PRODUCT%\package"
set "VSWHERE=%ProgramFiles(x86)%\Microsoft Visual Studio\Installer\vswhere.exe"
for /f "usebackq tokens=*" %%v in (`"%VSWHERE%" -latest -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath`) do set "VSROOT=%%v"
if not defined VSROOT exit /b 2
if not exist "%OUTPUT%" mkdir "%OUTPUT%"
call :build x64 64
if errorlevel 1 exit /b 1
call :build x86 32
if errorlevel 1 exit /b 1
copy /y "windows\README.md" "%OUTPUT%\" >nul || exit /b 1
copy /y "windows\tingzi.svg" "%OUTPUT%\" >nul || exit /b 1
copy /y "windows\InstallTingzi.cmd" "%OUTPUT%\" >nul || exit /b 1
copy /y "windows\VoiceShortcuts.ps1" "%OUTPUT%\" >nul || exit /b 1
copy /y "windows\RemoveTingzi.cmd" "%OUTPUT%\" >nul || exit /b 1
if not exist "%PRODUCT%\offline" mkdir "%PRODUCT%\offline"
dotnet publish windows\float\TingziFloat.csproj -c Release --nologo -p:DebugType=None -p:DebugSymbols=false -p:PathMap="%CD%=/_/src" --source "%PRODUCT%\offline" -p:BaseIntermediateOutputPath="%PRODUCT%/float-obj/" -p:BaseOutputPath="%PRODUCT%/float-bin/" -o "%OUTPUT%\TingziFloat" || exit /b 1
copy /y "windows\float\AudioNotes\LICENSE" "%OUTPUT%\TingziFloat\LICENSE" >nul || exit /b 1
copy /y "%USERPROFILE%\.nuget\packages\naudio\2.2.1\license.txt" "%OUTPUT%\TingziFloat\NAudio-LICENSE.txt" >nul || exit /b 1
if "%~3"=="" (echo Supply the prebuilt codex-asr.exe as the third argument. & exit /b 1)
copy /y "%~3" "%OUTPUT%\TingziFloat\codex-asr.exe" >nul || exit /b 1
copy /y "%~2\audio-notes\vendor\codex-asr\LICENSE-MIT" "%OUTPUT%\TingziFloat\codex-asr-LICENSE.txt" >nul || exit /b 1
popd
exit /b 0

:build
setlocal
call "%VSROOT%\VC\Auxiliary\Build\vcvarsall.bat" %1 >nul || exit /b 1
set "OBJ=%PRODUCT%\%1"
if not exist "%OBJ%" mkdir "%OBJ%"
set "FLAGS=/nologo /std:c++17 /O2 /MT /EHsc /utf-8 /DUNICODE /D_UNICODE /DNOMINMAX"
rc /nologo /i windows /fo "%OBJ%\tingzi.res" windows\tingzi.rc || exit /b 1
cl %FLAGS% /LD windows\service.cpp "%OBJ%\tingzi.res" /Fo"%OBJ%\service.obj" /Fe"%OUTPUT%\TingziTextService%2.dll" /link /def:windows\exports.def /implib:"%OBJ%\service.lib" winsqlite3.lib shell32.lib ole32.lib uuid.lib oleaut32.lib advapi32.lib user32.lib gdi32.lib ole32.lib uuid.lib || exit /b 1
cl %FLAGS% windows\setup.cpp /Fo"%OBJ%\setup.obj" /Fe"%OUTPUT%\TingziSetup%2.exe" /link /MANIFEST:EMBED /manifestinput:windows\compatibility.manifest /MANIFESTUAC:"level='asInvoker' uiAccess='false'" ole32.lib advapi32.lib shell32.lib uuid.lib || exit /b 1
if "%1"=="x64" cl %FLAGS% windows\editor.cpp "%OBJ%\tingzi.res" /Fo"%OBJ%\editor.obj" /Fe"%OUTPUT%\TingziPad.exe" /link /SUBSYSTEM:WINDOWS advapi32.lib user32.lib gdi32.lib imm32.lib winsqlite3.lib shell32.lib ole32.lib uuid.lib || exit /b 1
if "%1"=="x64" cl %FLAGS% windows\keyboard.cpp /Fo"%OBJ%\keyboard.obj" /Fe"%OUTPUT%\TingziKeyboard.exe" /link /MANIFEST:EMBED ole32.lib uuid.lib || exit /b 1
if "%1"=="x64" cl %FLAGS% windows\learning_cli.cpp /Fo"%OBJ%\learning.obj" /Fe"%OUTPUT%\TingziLearning.exe" /link /MANIFEST:EMBED winsqlite3.lib shell32.lib ole32.lib uuid.lib || exit /b 1
if "%1"=="x64" cl %FLAGS% windows\upgrade.cpp /Fo"%OBJ%\upgrade.obj" /Fe"%OUTPUT%\TingziVersion.exe" /link /MANIFEST:EMBED /manifestinput:windows\compatibility.manifest advapi32.lib ole32.lib uuid.lib || exit /b 1
exit /b 0
