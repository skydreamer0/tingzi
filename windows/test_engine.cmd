@echo off
rem Builds and runs the engine, learning and word-choice tests against the bundled word data.
rem   windows\test_engine.cmd [output folder]
rem The word-choice benchmark fails when a category gets fewer sentences right than
rem windows\bench\baseline.tsv records; see windows\bench\README.md.
setlocal
chcp 65001 >nul
pushd "%~dp0.."
set "BUILD=%CD%\.build\engine-test"
if not "%~1"=="" set "BUILD=%~f1"
set "DATA=%BUILD%\data"
set "VSWHERE=%ProgramFiles(x86)%\Microsoft Visual Studio\Installer\vswhere.exe"
for /f "usebackq tokens=*" %%v in (`"%VSWHERE%" -latest -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath`) do set "VSROOT=%%v"
if not defined VSROOT exit /b 2
call "%VSROOT%\VC\Auxiliary\Build\vcvarsall.bat" x64 >nul || exit /b 1
if exist "%BUILD%" rmdir /s /q "%BUILD%"
mkdir "%DATA%" || exit /b 1
"%SystemRoot%\System32\tar.exe" -xf "windows\standalone-data\public-data.zip" -C "%DATA%" || exit /b 1
copy /y "windows\standalone-data\pairs.txt" "%DATA%\" >nul || exit /b 1
set "FLAGS=/nologo /std:c++17 /O2 /MT /EHsc /utf-8 /DUNICODE /D_UNICODE /DNOMINMAX"
set "LIBS=winsqlite3.lib shell32.lib ole32.lib uuid.lib advapi32.lib"
cl %FLAGS% windows\engine_test.cpp /Fo"%BUILD%\engine-test.obj" /Fe"%BUILD%\engine-test.exe" /link %LIBS% || exit /b 1
cl %FLAGS% windows\learning_test.cpp /Fo"%BUILD%\learning-test.obj" /Fe"%BUILD%\learning-test.exe" /link %LIBS% || exit /b 1
cl %FLAGS% windows\bench\benchmark.cpp /Fo"%BUILD%\benchmark.obj" /Fe"%BUILD%\benchmark.exe" /link %LIBS% || exit /b 1
"%BUILD%\engine-test.exe" "%DATA%" || exit /b 1
"%BUILD%\learning-test.exe" "%DATA%" "%BUILD%\learning" || exit /b 1
"%BUILD%\benchmark.exe" "%DATA%" windows\bench\sentences.tsv windows\bench\baseline.tsv --write-baseline "%BUILD%\baseline.tsv" > "%BUILD%\benchmark.md"
set "RESULT=%ERRORLEVEL%"
type "%BUILD%\benchmark.md"
popd
exit /b %RESULT%
