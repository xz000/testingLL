# 本地一键多开：开 1 个 host 窗口 + (N-1) 个 join 窗口，方便真机手测帧同步联网。
# 用法：
#   powershell -ExecutionPolicy Bypass -File multi-launch.ps1            # 默认 2 开，端口 5199
#   powershell -ExecutionPolicy Bypass -File multi-launch.ps1 -Players 4 # 4 开
#   powershell -ExecutionPolicy Bypass -File multi-launch.ps1 -Players 4 -Port 9001
#
# 说明：
#   每个窗口都是一个独立 ggez 进程。host 是玩家 0，clients 依次是玩家 1..N-1。
#   任何窗口被关闭，不影响其它窗口；全部关闭即结束多开。
#   手测目标：不阻塞（等齐输入）、两端画面一致、手感正常。

param(
    [int]$Players = 2,
    [int]$Port = 5199
)

if ($Players -lt 2) { $Players = 2 }
if ($Players -gt 8) { Write-Host "最多 8 人，已钳制为 8。" -ForegroundColor Yellow; $Players = 8 }
if ($Port -lt 1 -or $Port -gt 65535) { Write-Host "端口非法，已钳制为 5199。" -ForegroundColor Yellow; $Port = 5199 }

Push-Location $PSScriptRoot
$exe = Join-Path (Get-Location) 'target\debug\client.exe'
if (-not (Test-Path $exe)) {
    Write-Host "未找到 $exe，先 cargo build -p client ..." -ForegroundColor Cyan
    cargo build -p client
    if ($LASTEXITCODE -ne 0) { Write-Host "build 失败。" -ForegroundColor Red; Pop-Location; exit 1 }
}
Pop-Location

$host_args = @("--host", "$Port", "--players", "$Players")
Write-Host "启动 host（玩家 0）：client $($host_args -join ' ')" -ForegroundColor Green
Start-Process -FilePath $exe -ArgumentList $host_args

# 留一点时间让 host 先开房
Start-Sleep -Milliseconds 600

for ($p = 1; $p -lt $Players; $p++) {
    $addr = "127.0.0.1:$Port"
    $join_args = @("--join", "$addr")
    Write-Host "启动 client（玩家 $p）：--join $addr" -ForegroundColor Cyan
    Start-Process -FilePath $exe -ArgumentList $join_args
    Start-Sleep -Milliseconds 400
}

Write-Host ""
Write-Host "已启动 $Players 个窗口（1 host + $($Players-1) clients）。手测要点："
Write-Host "  - 帧同步不应阻塞（大家都顺畅、无人等最慢者卡死）。"
Write-Host "  - 各窗口画面/位置一致（锁步正确）。"
Write-Host "  - 关闭窗口后进程自动结束。"
exit 0
