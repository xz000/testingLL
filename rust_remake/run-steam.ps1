param(
    [ValidateSet('host','join','menu')] [string]$Mode = 'menu',
    [int]$Players = 2,
    [string]$LobbyId = ''
)

$ErrorActionPreference = 'Continue'
Push-Location $PSScriptRoot

Write-Host '== cargo build -p client --features client/steam =='
cargo build -p client --features client/steam 2>&1 | Out-Host
if ($LASTEXITCODE -ne 0) { Write-Host 'build failed.'; Pop-Location; exit 1 }

$exe = Join-Path $PSScriptRoot 'target\debug\client.exe'

# stage steam_api64.dll next to exe (repo root or steamworks-sys build out)
$root = Split-Path $PSScriptRoot -Parent
$cands = @(
    (Join-Path $root 'steam_api64.dll'),
    (Join-Path $PSScriptRoot 'target\debug\build\steamworks-sys-*\out\steam_api64.dll')
)
$dll = $cands | Where-Object { $_ -and (Test-Path $_) } | Select-Object -First 1
if ($dll) {
    Copy-Item $dll (Join-Path $PSScriptRoot 'target\debug\steam_api64.dll') -Force
    Write-Host '[ok] steam_api64.dll staged'
} else {
    Write-Host 'WARN: steam_api64.dll not found (place one at repo root).'
}
# steam_appid.txt next to exe too
$appid = Join-Path $root 'steam_appid.txt'
if (Test-Path $appid) { Copy-Item $appid (Join-Path $PSScriptRoot 'target\debug\steam_appid.txt') -Force }

if ($Mode -eq 'menu') {
    # 进 Steam 版主菜单：按 3 进入 Steam 大厅，H 创建 / J 自动加入（无需输房间号）。
    Write-Host '== Steam MENU (按 3 进入大厅，H 建厅 / J 自动加入) =='
    $argsList = @()  # 不传参数 → 主菜单
} elseif ($Mode -eq 'host') {
    Write-Host "== Steam HOST --players $Players =="
    $argsList = @('--steam-host','--players',"$Players")
} elseif ($LobbyId -eq '') {
    Write-Host '== Steam JOIN (auto-find host lobby) =='
    $argsList = @('--steam-join')
} else {
    Write-Host "== Steam JOIN manual lobby $LobbyId =="
    $argsList = @('--steam-join',"$LobbyId")
}

# 前台运行（&），让 stderr（panic/错误/[steam-join] 日志）直接进当前控制台，便于排查 client 加入失败。
& $exe @argsList
Pop-Location
