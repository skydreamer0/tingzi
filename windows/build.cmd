@echo off
setlocal
pushd "%~dp0.."
set "PRODUCT=%CD%\.build"
if not "%~1"=="" set "PRODUCT=%~f1"
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
rem Public word data, so a new computer can type Zhuyin right after installation.
"%SystemRoot%\System32\tar.exe" -xf "windows\standalone-data\public-data.zip" -C "%OUTPUT%" || exit /b 1
copy /y "windows\standalone-data\pairs.txt" "%OUTPUT%\" >nul || exit /b 1
copy /y "windows\standalone-data\pairs-NOTICE.txt" "%OUTPUT%\" >nul || exit /b 1
copy /y "windows\tingzi.svg" "%OUTPUT%\" >nul || exit /b 1
copy /y "windows\InstallTingzi.cmd" "%OUTPUT%\" >nul || exit /b 1
copy /y "windows\VoiceShortcuts.ps1" "%OUTPUT%\" >nul || exit /b 1
copy /y "windows\RemoveTingzi.cmd" "%OUTPUT%\" >nul || exit /b 1
rem The voice window. Source paths are remapped so the program does not carry this computer's folder names.
set "RUSTFLAGS=--remap-path-prefix=%USERPROFILE%=/_ --remap-path-prefix=%CD%=/_/src"
rem The app also records the folder it was built from, so it is built through a temporary drive letter.
set "DRIVE="
for %%d in (T S R Q P O N M) do if not defined DRIVE if not exist %%d:\ set "DRIVE=%%d"
if not defined DRIVE exit /b 3
subst %DRIVE%: "%CD%" || exit /b 3
cargo build --release --locked --manifest-path %DRIVE%:\app\src-tauri\Cargo.toml --target-dir "%PRODUCT%\app-target"
set "BUILT=%ERRORLEVEL%"
subst %DRIVE%: /d
if not "%BUILT%"=="0" exit /b 1
if not exist "%OUTPUT%\TingziFloat" mkdir "%OUTPUT%\TingziFloat"
copy /y "%PRODUCT%\app-target\release\Tingzi.exe" "%OUTPUT%\TingziFloat\TingziFloat.exe" >nul || exit /b 1
copy /y "LICENSE" "%OUTPUT%\TingziFloat\LICENSE" >nul || exit /b 1
cargo metadata --format-version 1 --locked --filter-platform x86_64-pc-windows-msvc --manifest-path app\src-tauri\Cargo.toml > "%PRODUCT%\cargo-metadata.json" || exit /b 1
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
if "%1"=="x64" cl %FLAGS% windows\keyboard.cpp /Fo"%OBJ%\keyboard.obj" /Fe"%OUTPUT%\TingziKeyboard.exe" /link /MANIFEST:EMBED ole32.lib uuid.lib || exit /b 1
if "%1"=="x64" cl %FLAGS% windows\learning_cli.cpp /Fo"%OBJ%\learning.obj" /Fe"%OUTPUT%\TingziLearning.exe" /link /MANIFEST:EMBED winsqlite3.lib shell32.lib ole32.lib uuid.lib || exit /b 1
if "%1"=="x64" cl %FLAGS% windows\upgrade.cpp /Fo"%OBJ%\upgrade.obj" /Fe"%OUTPUT%\TingziVersion.exe" /link /MANIFEST:EMBED /manifestinput:windows\compatibility.manifest advapi32.lib ole32.lib uuid.lib || exit /b 1
exit /b 0
