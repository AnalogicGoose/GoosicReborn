@echo off
rem Shared environment for the Windows Swift scripts. `call` this, do not run it.
rem
rem `vcvars64.bat` needs the Visual Studio Installer directory on PATH to find
rem vswhere. Without it the script aborts before setting anything up, and swiftc
rem reports a missing CLI tool `link` and then an inability to load the standard
rem library -- neither of which names the cause.
set "PATH=%PATH%;C:\Program Files (x86)\Microsoft Visual Studio\Installer"
call "C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools\VC\Auxiliary\Build\vcvars64.bat" >nul
if errorlevel 1 exit /b 1

set "SWIFT_VERSION=6.3.3"
set "SWIFT_ROOT=%LOCALAPPDATA%\Programs\Swift"
rem The installer sets SDKROOT machine-wide, so a shell started before it was
rem installed inherits a stale environment and cannot find the standard library.
set "SDKROOT=%SWIFT_ROOT%\Platforms\%SWIFT_VERSION%\Windows.platform\Developer\SDKs\Windows.sdk"
set "PATH=%PATH%;%SWIFT_ROOT%\Toolchains\%SWIFT_VERSION%+Asserts\usr\bin;%SWIFT_ROOT%\Runtimes\%SWIFT_VERSION%\usr\bin"
set "SCUI_DEFAULT_BACKEND=WinUIBackend"

rem Windows refuses to create symlinks without Developer Mode, and several SwiftPM
rem dependencies carry them. Checking them out as plain files lets the checkout
rem succeed; windows-prepare.bat then restores the contents.
set GIT_CONFIG_COUNT=1
set GIT_CONFIG_KEY_0=core.symlinks
set GIT_CONFIG_VALUE_0=false

set "GOOSIC_PACKAGE=%~dp0..\apps\goosic-swift"
set "GOOSIC_SCRATCH=%LOCALAPPDATA%\goosic-swift-build"
exit /b 0
