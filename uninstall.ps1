#Requires -Version 5.1
<#
.SYNOPSIS
    Fully uninstalls Stellaris Natural Voice (azure-sapi): removes every registry
    entry and file it created. Self-contained - does NOT need setup.exe.

.DESCRIPTION
    Removes the per-user (HKCU) footprint:
      * COM class    HKCU\Software\Classes\CLSID\{2F9B4A17-8C3D-4E62-A1F5-7D0E9B34C8A2}
      * Voice tokens HKCU\...\Speech\Voices\Tokens\Azure{Thomas,Christopher,Aria,Guy,Jenny}
      * The classic default-voice pointer, if it points at one of these voices
      * The modern OneCore default-voice override, if it points at one of these
    Removes files:
      * %LOCALAPPDATA%\AzureSapi   (azure_sapi.dll, engine.log, ...)
    Also detects machine-wide (HKLM) leftovers from older/alternate installs and,
    when run elevated, removes those too.

    After running, Windows reverts to its normal built-in voices (David/Zira/Mark).
    Restart Stellaris to pick up the change.

.PARAMETER KeepFiles
    Do not delete %LOCALAPPDATA%\AzureSapi (leave the DLL and log in place).

.PARAMETER Yes
    Skip the confirmation prompt.

.EXAMPLE
    powershell -ExecutionPolicy Bypass -File .\uninstall.ps1

.EXAMPLE
    # Remove machine-wide leftovers too (run in an elevated PowerShell):
    powershell -ExecutionPolicy Bypass -File .\uninstall.ps1 -Yes
#>
[CmdletBinding()]
param(
    [switch]$KeepFiles,
    [switch]$Yes
)

$ErrorActionPreference = 'Stop'

$Clsid     = '{2F9B4A17-8C3D-4E62-A1F5-7D0E9B34C8A2}'
$Tokens    = 'AzureThomas', 'AzureChristopher', 'AzureAria', 'AzureGuy', 'AzureJenny'
$AssetsDir = Join-Path $env:LOCALAPPDATA 'AzureSapi'

function Step($m) { Write-Host "    $m" }

Write-Host ""
Write-Host "Stellaris Natural Voice - full uninstall"
Write-Host "========================================"
Write-Host "This will remove:"
Write-Host "  - the 5 Azure voices and the COM engine from your user registry (HKCU)"
Write-Host "  - the default-voice pointers it set (classic + OneCore)"
if (-not $KeepFiles) { Write-Host "  - the folder $AssetsDir (DLL + log)" }
Write-Host ""

if (-not $Yes) {
    $ans = Read-Host "Proceed? [y/N]"
    if ($ans -notin @('y', 'Y', 'yes', 'Yes')) { Write-Host "Cancelled."; return }
}

# Warn early if the engine DLL is locked (an app is using the voice).
$dll = Join-Path $AssetsDir 'azure_sapi.dll'
if (Test-Path $dll) {
    try { $fs = [System.IO.File]::Open($dll, 'Open', 'ReadWrite', 'None'); $fs.Close() }
    catch { Write-Warning "azure_sapi.dll is in use. Close Stellaris (and any TTS app) first, or the file may not delete." }
}

# --- 1. Per-user (HKCU) registry --------------------------------------------
Write-Host "[1/3] Removing per-user registry (HKCU)..."

$comPath = "HKCU:\Software\Classes\CLSID\$Clsid"
if (Test-Path $comPath) { Remove-Item $comPath -Recurse -Force; Step "removed COM class $Clsid" }

foreach ($t in $Tokens) {
    foreach ($base in @(
            'HKCU:\SOFTWARE\Microsoft\Speech\Voices\Tokens',
            'HKCU:\SOFTWARE\Microsoft\Speech_OneCore\Voices\Tokens')) {
        $p = Join-Path $base $t
        if (Test-Path $p) { Remove-Item $p -Recurse -Force; Step "removed voice token $t ($([System.IO.Path]::GetFileName($base)))" }
    }
}

# Clear default-voice pointers ONLY if they point at one of our voices.
$defaults = @(
    @{ Key = 'HKCU:\SOFTWARE\Microsoft\Speech\Voices';                    Value = 'DefaultTokenId'; Label = 'classic default voice' },
    @{ Key = 'HKCU:\SOFTWARE\Microsoft\Speech_OneCore\Voices';            Value = 'DefaultTokenId'; Label = 'OneCore default voice' },
    @{ Key = 'HKCU:\SOFTWARE\Microsoft\Speech_OneCore\Settings\TextToSpeech'; Value = 'Voice';      Label = 'OneCore TextToSpeech override' }
)
foreach ($d in $defaults) {
    $cur = (Get-ItemProperty -Path $d.Key -Name $d.Value -ErrorAction SilentlyContinue).$($d.Value)
    if ($cur -and ($cur -match '\\Tokens\\Azure(Thomas|Christopher|Aria|Guy|Jenny)')) {
        Remove-ItemProperty -Path $d.Key -Name $d.Value -ErrorAction SilentlyContinue
        Step "cleared $($d.Label)"
    }
}

# --- 2. Files ----------------------------------------------------------------
if ($KeepFiles) {
    Write-Host "[2/3] Keeping files (-KeepFiles): $AssetsDir"
}
else {
    Write-Host "[2/3] Removing files..."
    if (Test-Path $AssetsDir) {
        try { Remove-Item $AssetsDir -Recurse -Force; Step "removed $AssetsDir" }
        catch { Write-Warning "Could not remove $AssetsDir ($($_.Exception.Message)). Close apps using the voice and delete it manually." }
    }
    else { Step "nothing at $AssetsDir" }
}

# --- 3. Machine-wide (HKLM) leftovers (older/alternate installs) -------------
Write-Host "[3/3] Checking for machine-wide (HKLM) leftovers..."
$hklmTokenBases = @(
    'HKLM:\SOFTWARE\Microsoft\Speech\Voices\Tokens',
    'HKLM:\SOFTWARE\Microsoft\Speech_OneCore\Voices\Tokens'
)
$hklmHits = @()
foreach ($t in $Tokens) {
    foreach ($base in $hklmTokenBases) { if (Test-Path (Join-Path $base $t)) { $hklmHits += (Join-Path $base $t) } }
}
if (Test-Path "HKLM:\SOFTWARE\Classes\CLSID\$Clsid") { $hklmHits += "HKLM:\SOFTWARE\Classes\CLSID\$Clsid" }

if ($hklmHits.Count -eq 0) {
    Step "none found (clean)"
}
else {
    $isAdmin = ([Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()
    ).IsInRole([Security.Principal.WindowsBuiltinRole]::Administrator)
    if ($isAdmin) {
        foreach ($t in $Tokens) {
            foreach ($base in $hklmTokenBases) { Remove-Item (Join-Path $base $t) -Recurse -Force -ErrorAction SilentlyContinue }
        }
        Remove-Item "HKLM:\SOFTWARE\Classes\CLSID\$Clsid" -Recurse -Force -ErrorAction SilentlyContinue
        foreach ($k in @(
                'HKLM:\SOFTWARE\Microsoft\Speech\Voices',
                'HKLM:\SOFTWARE\WOW6432Node\Microsoft\Speech\Voices',
                'HKLM:\SOFTWARE\Microsoft\Speech_OneCore\Voices')) {
            $d = (Get-ItemProperty -Path $k -Name DefaultTokenId -ErrorAction SilentlyContinue).DefaultTokenId
            if ($d -and ($d -match '\\Tokens\\Azure')) { Remove-ItemProperty -Path $k -Name DefaultTokenId -ErrorAction SilentlyContinue }
        }
        Step "removed machine-wide leftovers"
    }
    else {
        Write-Warning "Found machine-wide (HKLM) leftovers, but this window is not elevated:"
        $hklmHits | ForEach-Object { Write-Host "      $_" }
        Write-Host "      Re-run this script in an elevated (Administrator) PowerShell to remove them."
    }
}

Write-Host ""
Write-Host "Done. Stellaris Natural Voice has been removed."
Write-Host "Restart Stellaris - it will use a standard Windows voice again."
