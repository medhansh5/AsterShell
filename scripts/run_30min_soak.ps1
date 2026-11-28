# AsterShell 30-Minute Stability & Resource Soak Test
# Captures shell-host.exe and taskbar.exe metrics at 0, 5, 10, 15, 20, 25, and 30 minutes.
# Handles changing taskbar.exe PIDs cleanly across supervisor crash-recovery restarts.

param(
    [int]$DurationMinutes = 30,
    [int]$IntervalMinutes = 5
)

$ErrorActionPreference = "Stop"
$RepoRoot = Split-Path -Parent $PSScriptRoot
$ShellHostExe = Join-Path $RepoRoot "target\release\shell-host.exe"
$OutCsv = Join-Path $RepoRoot "soak_30min_results.csv"

if (-not (Test-Path $ShellHostExe)) {
    throw "Missing $ShellHostExe; run: cargo build --release -p shell-host -p taskbar -p search-ui --locked"
}

$logicalCores = [Environment]::ProcessorCount
$hostProc = Start-Process -FilePath $ShellHostExe -PassThru -WindowStyle Hidden
Start-Sleep -Seconds 3

$tbProc = Get-Process -Name "taskbar" -ErrorAction SilentlyContinue | Select-Object -First 1
if (-not $tbProc) {
    Stop-Process -Id $hostProc.Id -Force -ErrorAction SilentlyContinue
    throw "taskbar.exe was not spawned by shell-host.exe"
}

$rows = @()
$steps = [math]::Floor($DurationMinutes / $IntervalMinutes)
$prevHostCpu = $hostProc.TotalProcessorTime.TotalSeconds
$prevTbCpu = $tbProc.TotalProcessorTime.TotalSeconds
$lastTbPid = $tbProc.Id
$tbRestarts = 0
$prevTime = Get-Date

for ($s = 0; $s -le $steps; $s++) {
    $elapsedMin = $s * $IntervalMinutes
    $now = Get-Date
    $hostProc.Refresh()
    if ($hostProc.HasExited) {
        throw "shell-host.exe exited unexpectedly at ${elapsedMin}m (exit code $($hostProc.ExitCode))"
    }

    # Re-acquire taskbar.exe in case supervisor restarted it after a crash
    $curTb = Get-Process -Name "taskbar" -ErrorAction SilentlyContinue | Select-Object -First 1
    if (-not $curTb) {
        Start-Sleep -Seconds 2
        $curTb = Get-Process -Name "taskbar" -ErrorAction SilentlyContinue | Select-Object -First 1
    }
    if (-not $curTb) {
        Stop-Process -Id $hostProc.Id -Force -ErrorAction SilentlyContinue
        throw "taskbar.exe missing and not recovered at ${elapsedMin}m"
    }

    $dt = ($now - $prevTime).TotalSeconds
    $hostCpuPct = if ($dt -gt 0.5) { [math]::Round((($hostProc.TotalProcessorTime.TotalSeconds - $prevHostCpu) / ($dt * $logicalCores)) * 100.0, 2) } else { 0.0 }

    if ($curTb.Id -ne $lastTbPid) {
        $tbRestarts++
        $lastTbPid = $curTb.Id
        $prevTbCpu = $curTb.TotalProcessorTime.TotalSeconds
        $tbCpuPct = 0.0
    } else {
        $tbCpuPct = if ($dt -gt 0.5) { [math]::Round((($curTb.TotalProcessorTime.TotalSeconds - $prevTbCpu) / ($dt * $logicalCores)) * 100.0, 2) } else { 0.0 }
        $prevTbCpu = $curTb.TotalProcessorTime.TotalSeconds
    }

    $prevHostCpu = $hostProc.TotalProcessorTime.TotalSeconds
    $prevTime = $now

    $row = [PSCustomObject]@{
        Timestamp       = $now.ToString("o")
        ElapsedMin      = $elapsedMin
        HostPid         = $hostProc.Id
        HostRssMB       = [math]::Round($hostProc.WorkingSet64 / 1MB, 2)
        HostCpuPct      = $hostCpuPct
        HostHandles     = $hostProc.HandleCount
        HostThreads     = $hostProc.Threads.Count
        TaskbarPid      = $curTb.Id
        TaskbarRestarts = $tbRestarts
        TaskbarRssMB    = [math]::Round($curTb.WorkingSet64 / 1MB, 2)
        TaskbarCpuPct   = $tbCpuPct
        TaskbarHandles  = $curTb.HandleCount
        TaskbarThreads  = $curTb.Threads.Count
    }
    $rows += $row
    Write-Host ("[{0,2}m] shell-host(PID={1}): {2}MB CPU={3}% H={4} T={5} | taskbar(PID={6},restarts={7}): {8}MB CPU={9}% H={10} T={11}" -f `
        $elapsedMin, $row.HostPid, $row.HostRssMB, $row.HostCpuPct, $row.HostHandles, $row.HostThreads, `
        $row.TaskbarPid, $row.TaskbarRestarts, $row.TaskbarRssMB, $row.TaskbarCpuPct, $row.TaskbarHandles, $row.TaskbarThreads)

    if ($s -lt $steps) {
        Start-Sleep -Seconds ($IntervalMinutes * 60)
    }
}

$rows | Export-Csv -Path $OutCsv -NoTypeInformation
Write-Host "Saved soak results to $OutCsv"

Stop-Process -Id $hostProc.Id -Force -ErrorAction SilentlyContinue
Start-Sleep -Milliseconds 500
$orphans = (Get-Process -Name "taskbar" -ErrorAction SilentlyContinue | Measure-Object).Count
if ($orphans -gt 0) { Get-Process -Name "taskbar" -ErrorAction SilentlyContinue | Stop-Process -Force }
Write-Host "Post-shutdown orphaned taskbar count: $orphans"
if ($orphans -gt 0) { throw "Orphaned taskbar processes remained after shutdown: $orphans" }
