@echo off
rem Runs the Swift test suite on Windows. Same environment as windows-build.bat;
rem `make test-swift` cannot be used here because the Makefile's uname branch does
rem not cover Windows and make is not part of the toolchain.
setlocal
set "PATH=%PATH%;C:\Program Files (x86)\Microsoft Visual Studio\Installer"
call "C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools\VC\Auxiliary\Build\vcvars64.bat" >nul
if errorlevel 1 exit /b 1

set "SWIFT_VERSION=6.3.3"
set "SWIFT_ROOT=%LOCALAPPDATA%\Programs\Swift"
set "SDKROOT=%SWIFT_ROOT%\Platforms\%SWIFT_VERSION%\Windows.platform\Developer\SDKs\Windows.sdk"
set "PATH=%PATH%;%SWIFT_ROOT%\Toolchains\%SWIFT_VERSION%+Asserts\usr\bin;%SWIFT_ROOT%\Runtimes\%SWIFT_VERSION%\usr\bin"
set "SCUI_DEFAULT_BACKEND=WinUIBackend"

set GIT_CONFIG_COUNT=1
set GIT_CONFIG_KEY_0=core.symlinks
set GIT_CONFIG_VALUE_0=false

swift test --package-path "%~dp0..\apps\goosic-swift" --scratch-path "%LOCALAPPDATA%\goosic-swift-build" %*
