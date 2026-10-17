# soak-report.ps1 -- summarize multiplayer soak logs across machines.
#
# Usage:
#   powershell -NoProfile -ExecutionPolicy Bypass -File tools/soak-report.ps1
#   powershell ... tools/soak-report.ps1 -Path 'logs4/*.log'
#   powershell ... tools/soak-report.ps1 -Csv > soak.csv
#
# It only reads logs; it never modifies game state.
# ASCII-only on purpose: Windows PowerShell 5.1 misreads BOM-less UTF-8 .ps1 files.

param(
    [Parameter(Position = 0)]
    [string[]]$Path = @(),
    [switch]$All,
    [switch]$Csv
)

$ErrorActionPreference = 'Stop'

if ($PSScriptRoot) {
    Push-Location (Join-Path $PSScriptRoot '..')
}

$files = @()
if (-not $Path -or $Path.Count -eq 0) {
    # Default: newest console log per known machine dir (logs, logs_2, logs_3).
    # Use -All to include every match, or -Path to point at specific files/dirs.
    foreach ($d in @('logs', 'logs_2', 'logs_3')) {
        if (-not (Test-Path $d)) { continue }
        $cand = @(Get-ChildItem -Path (Join-Path $d '*console*.log') -File -ErrorAction SilentlyContinue |
            Sort-Object LastWriteTime -Descending)
        if ($All) {
            $files += $cand
        } elseif ($cand.Count -gt 0) {
            $files += $cand[0]
        }
    }
} else {
    foreach ($p in $Path) {
        $files += Get-ChildItem -Path $p -File -ErrorAction SilentlyContinue
    }
}
$files = @($files | Sort-Object FullName -Unique)

if ($files.Count -eq 0) {
    Write-Host "No log files matched. Pass -Path 'dir/*.log'." -ForegroundColor Yellow
    exit 1
}

function Get-Count {
    param([string[]]$Lines, [string]$Pattern)
    return @($Lines | Select-String -Pattern $Pattern).Count
}

function Get-MaxInt {
    param([string[]]$Lines, [string]$Pattern)
    $max = 0
    foreach ($line in $Lines) {
        if ($line -match $Pattern) {
            $v = [int]$Matches[1]
            if ($v -gt $max) { $max = $v }
        }
    }
    return $max
}

$rows = @()

foreach ($f in $files) {
    $lines = @(Get-Content -Path $f.FullName -Encoding UTF8)

    # Valve SDK debug assertions (non-fatal).
    $isLocked = Get-Count $lines '!IsLocked'
    $iface = Get-Count $lines 'interfacemap'

    # SteamAPI lifetime: should be 1 (startup only) when E2 adopt works.
    $sessionReady = Get-Count $lines '\[steam\] session ready'
    $adopt = Get-Count $lines 'adopt transport back into session'

    # Liveness / recovery.
    $noFrames = Get-Count $lines 'NO frames for'
    $cfgWait = Get-Count $lines '\[steam-client\] ClientWait (?!sending)'
    $autoDrop = Get-Count $lines 'AUTO-DROP'
    $desync = Get-Count $lines 'DESYNC|panicked|thread .* panicked'
    $phaseBStall = Get-Count $lines 'Phase-B STALL|cannot determine a valid new host'

    # Migration flow.
    $elect = Get-Count $lines 'host gone \(probe timeout\), elected new host='
    $iamHost = Get-Count $lines 'TAKEOVER: I am new host'
    $superseded = Get-Count $lines 'SUPERSEDED:'
    $migrated = Get-Count $lines 'migrated to new host'
    $resumed = Get-Count $lines 'resumed from snapshot'
    $rejoined = Get-Count $lines 'resumed from snapshot seq='

    # P7 residual signal: election result must equal Steam lobby_owner.
    $ownerMismatch = 0
    $ownerZero = 0
    foreach ($line in $lines) {
        if ($line -match 'elected new host=(\d+).*lobby_owner=(\d+)') {
            $elected = [uint64]$Matches[1]
            $owner = [uint64]$Matches[2]
            if ($owner -eq 0) {
                $ownerZero++
            } elseif ($elected -ne $owner) {
                $ownerMismatch++
            }
        }
    }

    # Traffic peaks from [stat] lines.
    $queuedMax = Get-MaxInt $lines 'queued=(\d+)'
    $reqMax = Get-MaxInt $lines '\[stat\] client .*\breq=(\d+)'
    $pendingMax = Get-MaxInt $lines '\[stat\] client .*\bpending=(\d+)'

    $fail = ($desync -gt 0) -or ($phaseBStall -gt 0) -or ($autoDrop -gt 0) -or ($ownerMismatch -gt 0)
    $warn = (-not $fail) -and (($isLocked + $iface) -gt 2 -or $noFrames -gt 0 -or $cfgWait -gt 0 -or $sessionReady -gt 1)
    $verdict = if ($fail) { 'FAIL' } elseif ($warn) { 'WARN' } else { 'OK' }

    $rows += [pscustomobject]@{
        Machine       = $f.Name
        Time          = $f.LastWriteTime.ToString('MM-dd HH:mm')
        Lines         = $lines.Count
        IsLocked      = $isLocked
        interfacemap  = $iface
        SessionReady  = $sessionReady
        Adopt         = $adopt
        NoFrames      = $noFrames
        CfgWaitTO     = $cfgWait
        AutoDrop      = $autoDrop
        Desync        = $desync
        Stall         = $phaseBStall
        Elect         = $elect
        OwnerMismatch = $ownerMismatch
        OwnerZero     = $ownerZero
        IamHost       = $iamHost
        Superseded    = $superseded
        Migrated      = $migrated
        Resumed       = $rejoined
        QueuedMax     = $queuedMax
        ReqMax        = $reqMax
        PendingMax    = $pendingMax
        Verdict       = $verdict
    }
}

if ($Csv) {
    $rows | ConvertTo-Csv -NoTypeInformation
    exit 0
}

Write-Host ""
Write-Host "== Health ==" -ForegroundColor Cyan
$rows | Select-Object Machine, Time, Lines, Verdict, IsLocked, interfacemap, SessionReady, Adopt,
    NoFrames, CfgWaitTO, AutoDrop, Desync, Stall | Format-Table -AutoSize

Write-Host "== Migration ==" -ForegroundColor Cyan
$rows | Select-Object Machine, Time, Elect, OwnerMismatch, OwnerZero, IamHost, Superseded,
    Migrated, Resumed | Format-Table -AutoSize

Write-Host "== Traffic peaks ==" -ForegroundColor Cyan
$rows | Select-Object Machine, Time, QueuedMax, ReqMax, PendingMax | Format-Table -AutoSize

Write-Host "Notes:" -ForegroundColor DarkGray
Write-Host "  Verdict FAIL: DESYNC/panic, Phase-B STALL, AUTO-DROP, or election != lobby_owner." -ForegroundColor DarkGray
Write-Host "  Verdict WARN: >2 SDK assertions, NO frames, ClientWait timeout, or session re-init." -ForegroundColor DarkGray
Write-Host "  SessionReady should be 1 per machine (E2 adopt keeps SteamAPI alive)." -ForegroundColor DarkGray

$failed = @($rows | Where-Object { $_.Verdict -eq 'FAIL' }).Count
if ($failed -gt 0) {
    exit 2
}
exit 0
