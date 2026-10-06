# Installs the latest AsciiMLN Windows release for the current user.
# Usage (from any PowerShell prompt):
#   irm https://raw.githubusercontent.com/wtrmlnv1/asciimln/main/install.ps1 | iex

$ErrorActionPreference = 'Stop'

$Repository = 'wtrmlnv1/asciimln'
$AssetNames = @('asciimln-windows-x86_64.exe', 'asciimln.exe')
$InstallDirectory = Join-Path $env:LOCALAPPDATA 'AsciiMLN'
$ExecutablePath = Join-Path $InstallDirectory 'asciimln.exe'

try {
    $release = Invoke-RestMethod -Uri "https://api.github.com/repos/$Repository/releases/latest" -Headers @{ Accept = 'application/vnd.github+json' }
} catch {
    throw "Could not find the latest AsciiMLN release. Ensure https://github.com/$Repository has a published release. $($_.Exception.Message)"
}

$asset = $release.assets | Where-Object { $_.name -in $AssetNames } | Select-Object -First 1
if (-not $asset) {
    throw "Release '$($release.tag_name)' does not contain a supported Windows executable ($($AssetNames -join ', '))."
}

New-Item -ItemType Directory -Force -Path $InstallDirectory | Out-Null
$temporaryPath = Join-Path $InstallDirectory "$($asset.name).download"

try {
    Invoke-WebRequest -Uri $asset.browser_download_url -OutFile $temporaryPath
    Move-Item -Force -LiteralPath $temporaryPath -Destination $ExecutablePath
} finally {
    if (Test-Path -LiteralPath $temporaryPath) {
        Remove-Item -Force -LiteralPath $temporaryPath
    }
}

$userPath = [Environment]::GetEnvironmentVariable('Path', 'User')
$pathEntries = @($userPath -split ';' | Where-Object { $_ })
if ($pathEntries -notcontains $InstallDirectory) {
    $newUserPath = (@($pathEntries) + $InstallDirectory) -join ';'
    [Environment]::SetEnvironmentVariable('Path', $newUserPath, 'User')
}

# Make the command available immediately in this PowerShell session too.
if (($env:Path -split ';') -notcontains $InstallDirectory) {
    $env:Path = "$InstallDirectory;$env:Path"
}

Write-Host 'AsciiMLN installed successfully.'
Write-Host 'Run: asciimln'
Write-Host 'Open a new terminal if this was run from a different shell.'
