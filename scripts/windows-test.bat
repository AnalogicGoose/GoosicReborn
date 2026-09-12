@echo off
rem Runs the Swift test suite on Windows.
rem
rem `make test-swift` cannot stand in for this: the Makefile's uname branch does not
rem cover Windows, and make is not part of the toolchain.
setlocal
call "%~dp0windows-env.bat" || exit /b 1
call "%~dp0windows-prepare.bat" || exit /b 1
swift test --package-path "%GOOSIC_PACKAGE%" --scratch-path "%GOOSIC_SCRATCH%" %*
