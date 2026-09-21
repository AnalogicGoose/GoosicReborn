@echo off
rem Builds the Swift shell on Windows. See windows-env.bat for why the environment
rem needs the setup it does, and windows-prepare.bat for the symlink repair.
setlocal
call "%~dp0windows-env.bat" || exit /b 1
call "%~dp0windows-prepare.bat" || exit /b 1
swift build --package-path "%GOOSIC_PACKAGE%" --scratch-path "%GOOSIC_SCRATCH%" %*
