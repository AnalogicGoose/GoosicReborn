@echo off
rem Builds the service and the shell, then runs the shell against it.
setlocal
pushd "%~dp0.."
cargo build -p goosic-service || exit /b 1
call "%~dp0windows-build.bat" || exit /b 1
set "GOOSIC_SERVICE_PATH=%CD%\target\debug\goosic-service.exe"
popd
rem The Swift runtime DLLs are not beside the executable.
set "PATH=%PATH%;%LOCALAPPDATA%\Programs\Swift\Runtimes\6.3.3\usr\bin"
"%LOCALAPPDATA%\goosic-swift-build\x86_64-unknown-windows-msvc\debug\goosic-swift.exe"
