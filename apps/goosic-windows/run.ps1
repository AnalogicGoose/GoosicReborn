# Builds the service and the Windows shell, then launches the shell against that service.
# Usage (from anywhere): .\apps\goosic-windows\run.ps1
# Native tools print progress and warnings on stderr; failures are read from their exit codes.
$ErrorActionPreference = 'Continue'
$root = Resolve-Path "$PSScriptRoot\..\.."
Set-Location $root

# A running copy locks its own files, so the build could not replace them.
Get-Process Goosic.Windows -ErrorAction SilentlyContinue | Stop-Process -Force

cargo build -p goosic-service
if ($LASTEXITCODE -ne 0) { throw 'goosic-service failed to build' }

dotnet build apps\goosic-windows\Goosic.Windows -c Debug -p:Platform=x64
if ($LASTEXITCODE -ne 0) { throw 'Goosic.Windows failed to build' }

$env:GOOSIC_SERVICE_PATH = "$root\target\debug\goosic-service.exe"
$exe = Get-ChildItem "$root\apps\goosic-windows\Goosic.Windows\bin\x64\Debug" -Recurse -Filter Goosic.Windows.exe |
    Sort-Object LastWriteTime -Descending | Select-Object -First 1
Start-Process $exe.FullName
