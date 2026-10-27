param(
    [Parameter(Mandatory=$true)][string]$GhidraRoot,
    [Parameter(Mandatory=$true)][string]$JdkRoot,
    [Parameter(Mandatory=$true)][string]$ProjectRoot,
    [Parameter(Mandatory=$true)][string]$EvidenceRoot,
    [switch]$ImportWorker
)
$ErrorActionPreference='Stop'
$runner=Join-Path (Resolve-Path -LiteralPath $GhidraRoot).Path 'support\analyzeHeadless.bat'
$env:JAVA_HOME=(Resolve-Path -LiteralPath $JdkRoot).Path
$project=(Resolve-Path -LiteralPath $ProjectRoot).Path
$evidence=(Resolve-Path -LiteralPath $EvidenceRoot).Path
$scripts=(Resolve-Path -LiteralPath (Join-Path $PSScriptRoot 'ghidra')).Path
# Ghidra's project URL rejects path components beginning with a dot.
if (($project -split '[\\/]') | Where-Object { $_.StartsWith('.') }) {
    throw 'Choose a persistent project directory with no dot-prefixed path components.'
}
$arguments=@($project,'EvotoWorker735')
if($ImportWorker) { $arguments+=@('-import','F:\evoto\Evoto-worker.exe') }
else { $arguments+=@('-process','Evoto-worker.exe') }
$arguments+=@('-noanalysis','-max-cpu','2','-scriptPath',$scripts)
foreach($pair in @(@('target-functions.tsv','targeted-decompilation.json'),@('blemish-functions.tsv','blemish-decompilation.json'),@('caller-functions.tsv','caller-decompilation.json'),@('eye-functions.tsv','eye-decompilation.json'))) {
    $arguments+=@('-postScript','AuraTargetedTrace.java',(Join-Path $evidence $pair[0]),(Join-Path $evidence $pair[1]))
}
& $runner @arguments
if($LASTEXITCODE -ne 0) { throw "Ghidra exited with code $LASTEXITCODE; inspect its log." }
