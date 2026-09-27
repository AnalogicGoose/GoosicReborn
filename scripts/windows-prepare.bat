@echo off
rem Resolves dependencies and repairs their symlinks. `call` this after windows-env.bat.
rem
rem The order matters and is not interchangeable. The repair rewrites files inside
rem the dependency checkouts, so the checkouts have to exist first -- which is what
rem `swift package resolve` does without compiling anything. Running `swift build`
rem straight away would instead resolve and compile in one step, and the compile
rem would read the placeholder files the repair exists to replace.
swift package resolve --package-path "%GOOSIC_PACKAGE%" --scratch-path "%GOOSIC_SCRATCH%"
if errorlevel 1 exit /b 1

rem The repair is a shell script, because it reads git's own index to find what each
rem placeholder should have pointed at. Git for Windows ships the bash that runs it.
rem
rem It is located by path rather than by `where bash`, which finds Windows' own WSL
rem launcher first on most machines: that stub is not a shell, and with no distro
rem installed it fails with `execvpe(/bin/bash)` -- an error about a Linux path, from
rem a program nobody meant to call.
set "GOOSIC_BASH="
if exist "%ProgramFiles%\Git\bin\bash.exe" set "GOOSIC_BASH=%ProgramFiles%\Git\bin\bash.exe"
if not defined GOOSIC_BASH if exist "%ProgramFiles(x86)%\Git\bin\bash.exe" set "GOOSIC_BASH=%ProgramFiles(x86)%\Git\bin\bash.exe"
if not defined GOOSIC_BASH if exist "%LOCALAPPDATA%\Programs\Git\bin\bash.exe" set "GOOSIC_BASH=%LOCALAPPDATA%\Programs\Git\bin\bash.exe"
if not defined GOOSIC_BASH (
    echo windows-prepare: Git for Windows' bash was not found, so dependency symlinks >&2
    echo cannot be repaired and the build would compile against placeholder files. >&2
    echo Install Git for Windows, or run scripts/windows-fix-symlinks.sh yourself. >&2
    exit /b 1
)

"%GOOSIC_BASH%" "%~dp0windows-fix-symlinks.sh" "%GOOSIC_SCRATCH%\checkouts"
if errorlevel 1 exit /b 1
exit /b 0
