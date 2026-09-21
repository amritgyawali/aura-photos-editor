# Open the native application with its bundled UI; no dev server is required.
[CmdletBinding()]
param(
    [switch]$Rebuild,
    [switch]$InstallShortcuts,
    [switch]$NoLaunch
)

$ErrorActionPreference = 'Stop'
$projectRoot = Split-Path -Parent $PSScriptRoot
$desktopExe = Join-Path $projectRoot 'target\desktop-launch\debug\aura-desktop.exe'
$logDir = Join-Path $projectRoot '.work-checks\launcher'
$launchLock = $null
$ownsLock = $false
$buildNotice = $null

function Invoke-BuildStep {
    param([string]$Command, [string[]]$CommandArguments, [string]$LogName)
    $logPath = Join-Path $logDir $LogName
    Write-Host "Running $Command $CommandArguments (log: $logPath)"
    # Windows PowerShell otherwise treats native stderr progress as a terminating error.
    $previousPreference = $ErrorActionPreference
    $ErrorActionPreference = 'Continue'
    try {
        & $Command @CommandArguments > $logPath 2>&1
        $buildExit = $LASTEXITCODE
    } finally {
        $ErrorActionPreference = $previousPreference
    }
    if ($buildExit -ne 0) {
        throw "$Command failed (exit $buildExit). See $logPath"
    }
}

try {
    New-Item -ItemType Directory -Force -Path $logDir | Out-Null
    $launchLock = [System.Threading.Mutex]::new($false, 'Local\AURA-Desktop-Launcher')
    try { $ownsLock = $launchLock.WaitOne(0) }
    catch [System.Threading.AbandonedMutexException] { $ownsLock = $true }
    if (-not $ownsLock) { exit 0 }

    $running = Get-Process -Name aura-desktop -ErrorAction SilentlyContinue |
        Where-Object { $_.Path -eq $desktopExe } | Select-Object -First 1
    if ($Rebuild -and $running) {
        throw 'Close AURA before rebuilding, then run Start AURA.cmd -Rebuild again.'
    }

    $needsBuild = $Rebuild -or -not (Test-Path -LiteralPath $desktopExe)
    if (-not $needsBuild -and -not $running) {
        $builtAt = (Get-Item -LiteralPath $desktopExe).LastWriteTimeUtc
        # Scan only build inputs, never the large Cargo caches or node_modules.
        foreach ($inputPath in @(
            'crates', 'assets', 'models', 'ui\src', 'ui\src-tauri\src',
            'ui\src-tauri\icons', 'ui\src-tauri\capabilities',
            'Cargo.toml', 'Cargo.lock', 'rust-toolchain.toml', '.cargo',
            'ui\package.json', 'ui\package-lock.json', 'ui\index.html',
            'ui\tsconfig.json', 'ui\vite.config.ts',
            'ui\src-tauri\Cargo.toml', 'ui\src-tauri\Cargo.lock',
            'ui\src-tauri\build.rs', 'ui\src-tauri\tauri.conf.json'
        )) {
            $inputFullPath = Join-Path $projectRoot $inputPath
            if (Test-Path -LiteralPath $inputFullPath) {
                # Directory timestamps also catch files being removed or renamed.
                $changed = @(Get-Item -LiteralPath $inputFullPath; Get-ChildItem -LiteralPath $inputFullPath -Recurse) |
                    Where-Object { $_.LastWriteTimeUtc -gt $builtAt } | Select-Object -First 1
                if ($changed) { $needsBuild = $true; break }
            }
        }
    }

    if ($needsBuild) {
        Add-Type -AssemblyName System.Windows.Forms
        $buildNotice = New-Object System.Windows.Forms.NotifyIcon
        $buildNotice.Icon = [System.Drawing.SystemIcons]::Application
        $buildNotice.Text = 'Preparing AURA'
        $buildNotice.Visible = $true
        $buildNotice.ShowBalloonTip(10000, 'Preparing AURA', 'Building the app. This can take several minutes. AURA will open automatically.', [System.Windows.Forms.ToolTipIcon]::Info)
        foreach ($command in @('node.exe', 'npm.cmd', 'cargo.exe')) {
            if (-not (Get-Command $command -ErrorAction SilentlyContinue)) {
                throw "$command is missing. Install Node.js LTS and the pinned Rust toolchain with Visual Studio C++ Build Tools and a Windows SDK. See docs/photo-editing-quickstart.md."
            }
        }
        Push-Location (Join-Path $projectRoot 'ui')
        try {
            Invoke-BuildStep 'npm.cmd' @('ci') 'dependencies.log'
            Invoke-BuildStep 'npm.cmd' @('run', 'build') 'frontend.log'
        } finally { Pop-Location }
        Push-Location $projectRoot
        try {
            Invoke-BuildStep 'cargo.exe' @('build', '--locked', '--manifest-path', 'ui/src-tauri/Cargo.toml', '--target-dir', 'target/desktop-launch', '--features', 'custom-protocol', '-j', '1') 'desktop.log'
        } finally { Pop-Location }
    }

    if ($InstallShortcuts) {
        $shortcutShell = New-Object -ComObject WScript.Shell
        foreach ($folder in @(
            [Environment]::GetFolderPath('Desktop'),
            [Environment]::GetFolderPath('Programs')
        )) {
            $shortcut = $shortcutShell.CreateShortcut((Join-Path $folder 'AURA Photo Editor.lnk'))
            $shortcut.TargetPath = Join-Path $PSHOME 'powershell.exe'
            $shortcut.Arguments = '-NoProfile -WindowStyle Hidden -ExecutionPolicy Bypass -File "' + (Join-Path $PSScriptRoot 'start-aura.ps1') + '"'
            $shortcut.WorkingDirectory = $projectRoot
            $shortcut.IconLocation = (Join-Path $projectRoot 'ui\src-tauri\icons\icon.ico')
            $shortcut.Description = 'Open AURA Photo Editor'
            $shortcut.Save()
            Write-Host "Installed AURA Photo Editor shortcut in $folder"
        }
    }
    if ($NoLaunch) { exit 0 }

    if (-not $running) {
        $running = Start-Process -FilePath $desktopExe -WorkingDirectory $projectRoot -WindowStyle Hidden -PassThru
    }
    # Repeated clicks restore the same window instead of opening another catalog writer.
    Add-Type @'
using System;
using System.Runtime.InteropServices;
public static class AuraLauncherWindow {
    [DllImport("user32.dll")] public static extern bool IsIconic(IntPtr window);
    [DllImport("user32.dll")] public static extern bool ShowWindowAsync(IntPtr window, int command);
    [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr window);
}
'@
    $deadline = [DateTime]::UtcNow.AddSeconds(45)
    do {
        $running.Refresh()
        if ($running.HasExited) {
            throw "AURA exited during startup. See $env:APPDATA\AURA\logs for the engine log."
        }
        if ($running.MainWindowHandle -ne [IntPtr]::Zero) {
            $showCommand = if ([AuraLauncherWindow]::IsIconic($running.MainWindowHandle)) { 9 } else { 5 }
            [AuraLauncherWindow]::ShowWindowAsync($running.MainWindowHandle, $showCommand) | Out-Null
            $restoreDeadline = [DateTime]::UtcNow.AddSeconds(5)
            while ([AuraLauncherWindow]::IsIconic($running.MainWindowHandle)) {
                if ([DateTime]::UtcNow -ge $restoreDeadline) {
                    throw 'AURA is running but its window did not restore. Select AURA on the taskbar to open it.'
                }
                Start-Sleep -Milliseconds 100
            }
            if (-not [AuraLauncherWindow]::SetForegroundWindow($running.MainWindowHandle)) {
                (New-Object -ComObject WScript.Shell).AppActivate($running.Id) | Out-Null
            }
            Write-Host 'AURA is open.'
            exit 0
        }
        Start-Sleep -Milliseconds 250
    } while ([DateTime]::UtcNow -lt $deadline)
    throw "AURA did not open a window within 45 seconds. See $env:APPDATA\AURA\logs."
} catch {
    $message = $_.Exception.Message
    $message | Out-File -LiteralPath (Join-Path $logDir 'last-error.log') -Encoding utf8
    Write-Host $message -ForegroundColor Red
    Add-Type -AssemblyName System.Windows.Forms
    [System.Windows.Forms.MessageBox]::Show($message, 'AURA could not start', 'OK', 'Error') | Out-Null
    exit 1
} finally {
    if ($buildNotice) { $buildNotice.Dispose() }
    if ($ownsLock) { $launchLock.ReleaseMutex() }
    if ($launchLock) { $launchLock.Dispose() }
}
