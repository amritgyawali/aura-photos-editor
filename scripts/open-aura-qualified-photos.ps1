# Open the saved 27-photo qualification project for local visual review.
[CmdletBinding()]
param()
$ErrorActionPreference = 'Stop'
$qualificationRoot = Split-Path -Parent $PSScriptRoot
$qualificationExe = Join-Path $qualificationRoot 'target\desktop-launch\debug\aura-desktop.exe'
$qualificationCatalog = Join-Path $qualificationRoot 'output\aura100-photo-baseline\catalog\catalog.sqlite'
$freshQualification = Join-Path $qualificationRoot 'output\aura100-revalidated\catalog\catalog.sqlite'
$freshManifest = Join-Path $qualificationRoot 'output\aura100-revalidated\results.json'
if (Test-Path -LiteralPath $freshManifest) {
    $qualificationEvidence = Get-Content -LiteralPath $freshManifest -Raw | ConvertFrom-Json
    if ($qualificationEvidence.completed -and $qualificationEvidence.originalsUnchanged -and (Test-Path -LiteralPath $freshQualification)) {
        $qualificationCatalog = $freshQualification
    }
}
$nextQualification = Join-Path $qualificationRoot 'output\aura-next100-qualified\catalog\catalog.sqlite'
$nextManifest = Join-Path $qualificationRoot 'output\aura-next100-qualified\results.json'
if (Test-Path -LiteralPath $nextManifest) {
    $nextEvidence = Get-Content -LiteralPath $nextManifest -Raw | ConvertFrom-Json
    if ($nextEvidence.completed -and $nextEvidence.originalsUnchanged -and $nextEvidence.next100Requested -and (Test-Path -LiteralPath $nextQualification)) {
        $qualificationCatalog = $nextQualification
    }
}
$deliveryQualification = Join-Path $qualificationRoot 'output\aura-2026-10-22\catalog\catalog.sqlite'
$deliveryManifest = Join-Path $qualificationRoot 'output\aura-2026-10-22\results.json'
if (Test-Path -LiteralPath $deliveryManifest) {
    $deliveryEvidence = Get-Content -LiteralPath $deliveryManifest -Raw | ConvertFrom-Json
    if ($deliveryEvidence.completed -and $deliveryEvidence.originalsUnchanged -and $deliveryEvidence.additions20261022Requested -and (Test-Path -LiteralPath $deliveryQualification)) {
        $qualificationCatalog = $deliveryQualification
    }
}
$advancedQualification = Join-Path $qualificationRoot 'output\aura-advanced-2026-10-22\catalog\catalog.sqlite'
$advancedManifest = Join-Path $qualificationRoot 'output\aura-advanced-2026-10-22\results.json'
if (Test-Path -LiteralPath $advancedManifest) {
    $advancedEvidence = Get-Content -LiteralPath $advancedManifest -Raw | ConvertFrom-Json
    if ($advancedEvidence.completed -and $advancedEvidence.originalsUnchanged -and $advancedEvidence.labTool -and (Test-Path -LiteralPath $advancedQualification)) {
        $qualificationCatalog = $advancedQualification
    }
}
if (-not (Test-Path -LiteralPath $qualificationExe) -or -not (Test-Path -LiteralPath $qualificationCatalog)) {
    throw 'Build AURA and complete the supplied-photo qualification first.'
}
$env:AURA_TEST_CATALOG = $qualificationCatalog
$env:WEBVIEW2_USER_DATA_FOLDER = Join-Path (Split-Path -Parent (Split-Path -Parent $qualificationCatalog)) 'webview-review'
Remove-Item Env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS -ErrorAction SilentlyContinue
$qualificationProcess = Start-Process -FilePath $qualificationExe -WorkingDirectory $qualificationRoot -WindowStyle Hidden -PassThru
Add-Type @'
using System;
using System.Runtime.InteropServices;
public static class AuraQualificationWindow {
    [DllImport("user32.dll")] public static extern bool ShowWindowAsync(IntPtr window, int command);
    [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr window);
}
'@
$qualificationDeadline = [DateTime]::UtcNow.AddSeconds(45)
do {
    $qualificationProcess.Refresh()
    if ($qualificationProcess.HasExited) { throw 'AURA exited before opening the photo review window.' }
    if ($qualificationProcess.MainWindowHandle -ne [IntPtr]::Zero) {
        [AuraQualificationWindow]::ShowWindowAsync($qualificationProcess.MainWindowHandle, 5) | Out-Null
        [AuraQualificationWindow]::SetForegroundWindow($qualificationProcess.MainWindowHandle) | Out-Null
        Write-Output ('AURA is open with the qualified photo catalogue. Process: ' + $qualificationProcess.Id)
        exit 0
    }
    Start-Sleep -Milliseconds 200
} while ([DateTime]::UtcNow -lt $qualificationDeadline)
throw 'AURA did not expose its review window within 45 seconds.'
