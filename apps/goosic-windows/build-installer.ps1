param(
    [ValidatePattern('^\d+\.\d+\.\d+$')][string]$Version = '0.2.1',
    [switch]$SkipPublish
)
$ErrorActionPreference = 'Stop'
$repo = (Resolve-Path "$PSScriptRoot\..\..").Path
$compiler = @(
    "$env:LOCALAPPDATA\Programs\Inno Setup 6\ISCC.exe",
    "${env:ProgramFiles(x86)}\Inno Setup 6\ISCC.exe",
    "$env:ProgramFiles\Inno Setup 6\ISCC.exe"
) | Where-Object { Test-Path -LiteralPath $_ } | Select-Object -First 1
if (-not $compiler) { throw 'Install Inno Setup 6: winget install --id JRSoftware.InnoSetup --exact' }
if (-not $SkipPublish) { & "$PSScriptRoot\package.ps1" -Version $Version -Platform x64 }
$ErrorActionPreference = 'Stop'
$dependencyDir = Join-Path $repo 'dist\installer-tools'
New-Item -ItemType Directory -Path $dependencyDir -Force | Out-Null
$bootstrapper = Join-Path $dependencyDir 'MicrosoftEdgeWebview2Setup.exe'
Invoke-WebRequest 'https://go.microsoft.com/fwlink/p/?LinkId=2124703' -OutFile $bootstrapper
$signature = Get-AuthenticodeSignature $bootstrapper
if ($signature.Status -ne 'Valid' -or $signature.SignerCertificate.Subject -notmatch 'O=Microsoft Corporation') {
    throw 'The WebView2 bootstrapper must have a valid Microsoft signature'
}
& $compiler "/DAppVersion=$Version" "$PSScriptRoot\installer.iss"
if ($LASTEXITCODE -ne 0) { throw 'Installer compilation failed' }
# The in-app updater installs a release's Setup only if its hash matches this file, so it is
# written here rather than by hand. Upload it with the Setup and the portable ZIP.
$dist = Join-Path $repo 'dist'
$sums = @("Goosic-$Version-windows-x64-setup.exe", "Goosic-$Version-windows-x64.zip") |
    Where-Object { Test-Path -LiteralPath (Join-Path $dist $_) } |
    ForEach-Object { "$((Get-FileHash (Join-Path $dist $_) -Algorithm SHA256).Hash.ToLowerInvariant())  $_" }
[IO.File]::WriteAllText((Join-Path $dist 'SHA256SUMS.txt'), (($sums -join "`n") + "`n"))
Get-Content (Join-Path $dist 'SHA256SUMS.txt')
