# Rebuild helper for azure-sapi.
# The GNU Rust toolchain needs WinLibs MinGW (as/dlltool) on PATH to link the
# `windows`-crate cdylib. This script puts cargo + WinLibs on PATH and builds,
# then refreshes dist\.

$ErrorActionPreference = 'Stop'

$cargoBin = Join-Path $env:USERPROFILE '.cargo\bin'
$winlibsRoot = Join-Path $env:LOCALAPPDATA 'Microsoft\WinGet\Packages'
$winlibs = Get-ChildItem $winlibsRoot -Recurse -Filter 'gcc.exe' -ErrorAction SilentlyContinue |
    Where-Object { $_.FullName -like '*WinLibs*' } |
    Select-Object -First 1 | ForEach-Object { Split-Path $_.FullName -Parent }
if (-not $winlibs) { throw "WinLibs MinGW not found. Install with: winget install BrechtSanders.WinLibs.POSIX.UCRT" }

$env:PATH = "$cargoBin;$winlibs;$env:PATH"

Push-Location $PSScriptRoot\azure-sapi
try {
    cargo build --release
    if ($LASTEXITCODE -ne 0) { throw "cargo build failed" }
} finally {
    Pop-Location
}

$dist = Join-Path $PSScriptRoot 'dist'
New-Item -ItemType Directory -Path $dist -Force | Out-Null
$rel = Join-Path $PSScriptRoot 'azure-sapi\target\release'
Copy-Item "$rel\azure_sapi.dll", "$rel\setup.exe" $dist -Force
Write-Host "Built and copied to $dist"
