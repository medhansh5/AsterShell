# Reproducible Idle Resource Measurement Harness for AsterShell
# Measures shell-host.exe and taskbar.exe separately over 10s (10 samples) + 3 start/stop cycles.

$ErrorActionPreference = "Stop"
$RepoRoot = Split-Path -Parent $PSScriptRoot
$ShellHostExe = Join-Path $RepoRoot "target\release\shell-host.exe"
$TaskbarExe = Join-Path $RepoRoot "target\release\taskbar.exe"

if (-not (Test-Path $ShellHostExe)) { throw "Missing $ShellHostExe; run cargo build --release -p shell-host -p taskbar" }
if (-not (Test-Path $TaskbarExe)) { throw "Missing $TaskbarExe; run cargo build --release -p shell-host -p taskbar" }

$os = Get-CimInstance Win32_OperatingSystem
$cpu = Get-CimInstance Win32_Processor | Select-Object -First 1
$logicalCores = [Environment]::ProcessorCount

Write-Host "=== ENVIRONMENT ==="
Write-Host "OS: $($os.Caption) (Build $($os.BuildNumber), Version $($os.Version))"
Write-Host "CPU: $($cpu.Name) ($logicalCores logical cores)"
Write-Host "Total RAM: $([math]::Round($os.TotalVisibleMemorySize / 1MB, 2)) GB"
Write-Host ""

function Get-Percentile($vals, $p) {
    $sorted = $vals | Sort-Object
    $idx = [math]::Min($sorted.Count - 1, [math]::Floor(($p / 100.0) * $sorted.Count))
    return $sorted[$idx]
}

# Start shell-host (which auto-spawns taskbar under Job Object)
$hostProc = Start-Process -FilePath $ShellHostExe -PassThru -WindowStyle Hidden
Start-Sleep -Seconds 2

$tbProc = Get-Process -Name "taskbar" -ErrorAction SilentlyContinue | Select-Object -First 1
if (-not $tbProc) {
    Write-Host "WARNING: taskbar.exe not auto-started; starting standalone for measurement."
    $tbProc = Start-Process -FilePath $TaskbarExe -ArgumentList "--no-supervisor" -PassThru -WindowStyle Hidden
    Start-Sleep -Seconds 2
}

$hostSamples = @()
$tbSamples = @()

$prevHostCpu = $hostProc.TotalProcessorTime.TotalSeconds
$prevTbCpu = $tbProc.TotalProcessorTime.TotalSeconds

for ($i = 1; $i -le 10; $i++) {
    Start-Sleep -Seconds 1
    $hostProc.Refresh()
    $tbProc.Refresh()

    $curHostCpu = $hostProc.TotalProcessorTime.TotalSeconds
    $curTbCpu = $tbProc.TotalProcessorTime.TotalSeconds

    $hostCpuPct = [math]::Round((($curHostCpu - $prevHostCpu) / $logicalCores) * 100.0, 2)
    $tbCpuPct = [math]::Round((($curTbCpu - $prevTbCpu) / $logicalCores) * 100.0, 2)

    $prevHostCpu = $curHostCpu
    $prevTbCpu = $curTbCpu

    $hostSamples += [PSCustomObject]@{
        RssMB = [math]::Round($hostProc.WorkingSet64 / 1MB, 2)
        PrivMB = [math]::Round($hostProc.PrivateMemorySize64 / 1MB, 2)
        Handles = $hostProc.HandleCount
        Threads = $hostProc.Threads.Count
        CpuPct = $hostCpuPct
    }
    $tbSamples += [PSCustomObject]@{
        RssMB = [math]::Round($tbProc.WorkingSet64 / 1MB, 2)
        PrivMB = [math]::Round($tbProc.PrivateMemorySize64 / 1MB, 2)
        Handles = $tbProc.HandleCount
        Threads = $tbProc.Threads.Count
        CpuPct = $tbCpuPct
    }
}

Write-Host "=== IDLE RESOURCE SUMMARY (10 samples @ 1s) ==="
foreach ($entry in @(
    @{ Name = "shell-host.exe"; Samples = $hostSamples },
    @{ Name = "taskbar.exe"; Samples = $tbSamples }
)) {
    $s = $entry.Samples
    $rssP50 = Get-Percentile ($s.RssMB) 50
    $rssP95 = Get-Percentile ($s.RssMB) 95
    $rssPeak = ($s.RssMB | Measure-Object -Maximum).Maximum
    $privP50 = Get-Percentile ($s.PrivMB) 50
    $hdlStart = $s[0].Handles
    $hdlEnd = $s[-1].Handles
    $hdlPeak = ($s.Handles | Measure-Object -Maximum).Maximum
    $thrPeak = ($s.Threads | Measure-Object -Maximum).Maximum
    $cpuP50 = Get-Percentile ($s.CpuPct) 50
    $cpuPeak = ($s.CpuPct | Measure-Object -Maximum).Maximum
    Write-Host "$($entry.Name): RSS p50=${rssP50}MB p95=${rssP95}MB peak=${rssPeak}MB | Priv p50=${privP50}MB | Handles start=$hdlStart end=$hdlEnd peak=$hdlPeak | Threads peak=$thrPeak | CPU p50=${cpuP50}% peak=${cpuPeak}%"
}

# Cleanup primary run
Stop-Process -Id $hostProc.Id -Force -ErrorAction SilentlyContinue
Start-Sleep -Milliseconds 500
Stop-Process -Name "taskbar" -Force -ErrorAction SilentlyContinue

# 3-cycle start/stop check
Write-Host ""
Write-Host "=== 3-CYCLE START/STOP CHECK ==="
for ($c = 1; $c -le 3; $c++) {
    $p = Start-Process -FilePath $ShellHostExe -PassThru -WindowStyle Hidden
    Start-Sleep -Seconds 1
    $tb = Get-Process -Name "taskbar" -ErrorAction SilentlyContinue | Select-Object -First 1
    $tbRss = if ($tb) { [math]::Round($tb.WorkingSet64 / 1MB, 2) } else { 0 }
    Stop-Process -Id $p.Id -Force -ErrorAction SilentlyContinue
    Start-Sleep -Milliseconds 400
    $rem = Get-Process -Name "taskbar" -ErrorAction SilentlyContinue
    $orphaned = if ($rem) { $rem.Count } else { 0 }
    if ($rem) { $rem | Stop-Process -Force -ErrorAction SilentlyContinue }
    Write-Host "Cycle ${c}: taskbar RSS=${tbRss}MB, orphaned_after_host_stop=${orphaned}"
}
