# Open a copy of the two-photo colour-match catalogue for visual review.
[CmdletBinding()]
param()
$ErrorActionPreference = 'Stop'
$referenceProjectRoot = Split-Path -Parent $PSScriptRoot
$referenceEvidenceRoot = Join-Path $referenceProjectRoot 'output\aura-internet-reference-2026-10-22'
$referenceRun = $null
foreach ($referenceCandidate in @('ui-qualified', 'ui-controls', 'ui-retry')) {
    $referenceResultsPath = Join-Path $referenceEvidenceRoot ($referenceCandidate + '\results.json')
    if (Test-Path -LiteralPath $referenceResultsPath) {
        $referenceResults = Get-Content -LiteralPath $referenceResultsPath -Raw | ConvertFrom-Json
        if ($referenceResults.completed -and $referenceResults.originalsUnchanged) {
            $referenceRun = Join-Path $referenceEvidenceRoot $referenceCandidate
            break
        }
    }
}
if (-not $referenceRun) { throw 'Complete the Internet colour-reference qualification first.' }
$adaptiveReferenceRun = Join-Path $referenceProjectRoot 'output\aura-adaptive-ui-2026-10-22'
$adaptiveReferenceManifest = Join-Path $adaptiveReferenceRun 'results.json'
if (Test-Path -LiteralPath $adaptiveReferenceManifest) {
    $adaptiveReferenceEvidence = Get-Content -LiteralPath $adaptiveReferenceManifest -Raw | ConvertFrom-Json
    if ($adaptiveReferenceEvidence.completed -and $adaptiveReferenceEvidence.originalsUnchanged -and
        (Test-Path -LiteralPath (Join-Path $adaptiveReferenceRun 'catalog\qualified.sqlite'))) {
        $referenceRun = $adaptiveReferenceRun
    }
}
$referenceExecutable = Join-Path $referenceProjectRoot 'target\desktop-launch\debug\aura-desktop.exe'
$referenceQualified = Join-Path $referenceRun 'catalog\qualified.sqlite'
if (-not (Test-Path -LiteralPath $referenceExecutable) -or -not (Test-Path -LiteralPath $referenceQualified)) {
    throw 'The tested application or saved reference catalogue is missing.'
}
$referenceReviewId = [DateTime]::UtcNow.ToString('yyyyMMdd-HHmmss-fff')
$referenceReviewCatalog = Join-Path $referenceRun ('catalog\review-' + $referenceReviewId + '.sqlite')
Copy-Item -LiteralPath $referenceQualified -Destination $referenceReviewCatalog
$env:AURA_TEST_CATALOG = $referenceReviewCatalog
$env:WEBVIEW2_USER_DATA_FOLDER = Join-Path $referenceRun ('webview-review-' + $referenceReviewId)
Remove-Item Env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS -ErrorAction SilentlyContinue
$referenceProcess = Start-Process -FilePath $referenceExecutable -WorkingDirectory $referenceProjectRoot -WindowStyle Hidden -PassThru
Add-Type @'
using System;
using System.Runtime.InteropServices;
public static class AuraReferenceReviewWindow {
    [DllImport("user32.dll")] public static extern bool ShowWindowAsync(IntPtr window, int command);
    [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr window);
}
'@
$referenceDeadline = [DateTime]::UtcNow.AddSeconds(45)
do {
    $referenceProcess.Refresh()
    if ($referenceProcess.HasExited) { throw 'AURA exited before opening the reference review window.' }
    if ($referenceProcess.MainWindowHandle -ne [IntPtr]::Zero) {
        [AuraReferenceReviewWindow]::ShowWindowAsync($referenceProcess.MainWindowHandle, 5) | Out-Null
        [AuraReferenceReviewWindow]::SetForegroundWindow($referenceProcess.MainWindowHandle) | Out-Null
        Write-Output ('AURA reference review is open. Choose Match a look. Process: ' + $referenceProcess.Id)
        exit 0
    }
    Start-Sleep -Milliseconds 200
} while ([DateTime]::UtcNow -lt $referenceDeadline)
throw 'AURA did not expose its review window within 45 seconds.'
