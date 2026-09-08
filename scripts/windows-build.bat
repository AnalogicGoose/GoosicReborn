@echo off
rem Builds the Swift shell on Windows.
rem
rem `vcvars64` supplies link.exe and the Windows SDK. Without it swiftc reports a
rem missing CLI tool `link`, and then -- confusingly -- an inability to load the
rem standard library, because vcvars aborts before setting anything up when
rem vswhere.exe is not on PATH.
setlocal
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

rem Windows refuses symlink creation without Developer Mode, and several SwiftPM
rem dependencies carry symlinks. Checking them out as plain files lets the
rem checkout succeed; scripts\windows-fix-symlinks.sh then restores the contents.
set GIT_CONFIG_COUNT=1
set GIT_CONFIG_KEY_0=core.symlinks
set GIT_CONFIG_VALUE_0=false

swift build --package-path "%~dp0..\apps\goosic-swift" --scratch-path "%LOCALAPPDATA%\goosic-swift-build" %*
