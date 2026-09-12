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

rem The repair is a shell script because it reads git's own index. Git for Windows
rem ships the bash that runs it; if it is not on PATH there is nothing to repair
rem with, and saying so beats compiling against placeholders and failing later with
rem an error that names a missing symbol instead.
set "GOOSIC_BASH="
for /f "delims=" %%B in ('where bash 2^>nul') do if not defined GOOSIC_BASH set "GOOSIC_BASH=%%B"
if not defined GOOSIC_BASH (
    echo windows-prepare: bash was not found on PATH, so dependency symlinks cannot be repaired. >&2
    echo Install Git for Windows, or run scripts/windows-fix-symlinks.sh yourself. >&2
    exit /b 1
)

"%GOOSIC_BASH%" "%~dp0windows-fix-symlinks.sh" "%GOOSIC_SCRATCH%\checkouts"
if errorlevel 1 exit /b 1
exit /b 0
