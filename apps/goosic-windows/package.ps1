# Builds a self-contained Goosic for Windows and zips it, ready to hand to a tester.
# Usage: .\apps\goosic-windows\package.ps1 [-Version 0.2.1] [-Platform x64|ARM64]
#
# The folder runs on a Windows 10 (1809) or later machine with nothing else installed: .NET and
# the Windows App SDK travel inside it, and WebView2 is part of Windows. The service and the rules
# library sit beside Goosic.Windows.exe, which is where the app looks for them.
param(
    [ValidatePattern('^\d+\.\d+\.\d+(-[A-Za-z0-9.]+)?$')][string]$Version = '0.2.1',
    [ValidateSet('x64', 'ARM64')][string]$Platform = 'x64'
)
# Native tools print progress and warnings on stderr; failures are read from their exit codes.
$ErrorActionPreference = 'Continue'
$root = Resolve-Path "$PSScriptRoot\..\.."
Set-Location $root

$rid = if ($Platform -eq 'ARM64') { 'win-arm64' } else { 'win-x64' }
$cargoTarget = if ($Platform -eq 'ARM64') { 'aarch64-pc-windows-msvc' } else { 'x86_64-pc-windows-msvc' }
$name = "Goosic-$Version-windows-$($Platform.ToLower())"
$dist = Join-Path $root 'dist'
$out = Join-Path $dist $name

cargo build --release --target $cargoTarget -p goosic-service -p goosic-shell-support-ffi
if ($LASTEXITCODE -ne 0) { throw 'the Rust service failed to build' }

# The project builds its own copy of the rules for the host; the release one from above is what ships.
if (-not ([IO.Path]::GetFullPath($out).StartsWith([IO.Path]::GetFullPath($dist) + [IO.Path]::DirectorySeparatorChar))) {
    throw 'Package output must be inside dist'
}
if (Test-Path $out) { Remove-Item -LiteralPath $out -Recurse -Force -ErrorAction Stop }
dotnet publish apps\goosic-windows\Goosic.Windows -c Release -r $rid -p:Platform=$Platform `
    -p:SelfContained=true -p:WindowsAppSDKSelfContained=true -p:Version=$($Version -replace '-.*$', '') `
    -p:InformationalVersion=$Version -o $out
if ($LASTEXITCODE -ne 0) { throw 'Goosic.Windows failed to publish' }

Copy-Item "target\$cargoTarget\release\goosic-service.exe" $out -Force
Copy-Item "target\$cargoTarget\release\goosic_shell_support_ffi.dll" $out -Force
Copy-Item LICENSE,LICENSE-GPL-3.0,THIRD_PARTY_NOTICES.md $out -ErrorAction Stop
Get-ChildItem $out -Filter *.pdb -Recurse | Remove-Item -Force

$zip = "$out.zip"
if (Test-Path $zip) { Remove-Item -Force $zip }
Compress-Archive -Path "$out\*" -DestinationPath $zip
"Packaged $zip"
