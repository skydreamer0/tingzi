@echo off
setlocal
pushd "%~dp0.."
set "OUTPUT=%CD%\.dev-data\standalone-rc2\package"
set "BUILD=%CD%\.dev-data\standalone-rc2"
set "VSWHERE=%ProgramFiles(x86)%\Microsoft Visual Studio\Installer\vswhere.exe"
for /f "usebackq tokens=*" %%v in (`"%VSWHERE%" -latest -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath`) do set "VSROOT=%%v"
if not defined VSROOT exit /b 2
if not exist "%OUTPUT%" mkdir "%OUTPUT%"
call :build x64 64
if errorlevel 1 exit /b 1
call :build x86 32
if errorlevel 1 exit /b 1
popd
exit /b 0
:build
setlocal
call "%VSROOT%\VC\Auxiliary\Build\vcvarsall.bat" %1 >nul || exit /b 1
set "OBJ=%BUILD%\%1"
if not exist "%OBJ%" mkdir "%OBJ%"
set "FLAGS=/nologo /std:c++17 /O2 /MT /EHsc /utf-8 /DTINGZI_STANDALONE /DUNICODE /D_UNICODE /DNOMINMAX"
rc /nologo /i windows /fo "%OBJ%\tingzi.res" windows\tingzi.rc || exit /b 1
cl %FLAGS% /LD windows\service.cpp "%OBJ%\tingzi.res" /Fo"%OBJ%\service.obj" /Fe"%OUTPUT%\TingziStandaloneService%2.dll" /link /def:windows\exports.def /implib:"%OBJ%\service.lib" winsqlite3.lib shell32.lib ole32.lib uuid.lib oleaut32.lib advapi32.lib user32.lib gdi32.lib || exit /b 1
cl %FLAGS% windows\setup.cpp /Fo"%OBJ%\setup.obj" /Fe"%OUTPUT%\TingziStandaloneSetup%2.exe" /link /MANIFEST:EMBED /manifestinput:windows\compatibility.manifest /MANIFESTUAC:"level='asInvoker' uiAccess='false'" ole32.lib advapi32.lib shell32.lib uuid.lib || exit /b 1
cl %FLAGS% windows\engine_test.cpp /Fo"%OBJ%\engine-test.obj" /Fe"%OBJ%\engine-test.exe" /link winsqlite3.lib shell32.lib ole32.lib uuid.lib advapi32.lib || exit /b 1
cl %FLAGS% windows\learning_test.cpp /Fo"%OBJ%\learning-test.obj" /Fe"%OBJ%\learning-test.exe" /link winsqlite3.lib shell32.lib ole32.lib uuid.lib advapi32.lib || exit /b 1
cl %FLAGS% windows\standalone_learning_restart.cpp /Fo"%OBJ%\restart-test.obj" /Fe"%OBJ%\restart-test.exe" /link winsqlite3.lib shell32.lib ole32.lib uuid.lib advapi32.lib || exit /b 1
cl %FLAGS% windows\standalone_activation_test.cpp /Fo"%OBJ%\activation-test.obj" /Fe"%OBJ%\activation-test.exe" /link winsqlite3.lib shell32.lib ole32.lib uuid.lib oleaut32.lib advapi32.lib user32.lib gdi32.lib || exit /b 1
if "%1"=="x64" cl %FLAGS% windows\learning_cli.cpp /Fo"%OBJ%\learning.obj" /Fe"%OUTPUT%\TingziStandaloneLearning.exe" /link winsqlite3.lib shell32.lib ole32.lib uuid.lib advapi32.lib || exit /b 1
exit /b 0
