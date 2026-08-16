# Steam 联机一键运行：确保 steam_api64.dll + steam_appid.txt 就位、用 steam feature 构建，再启动 host/join。
# 用法（各在一台电脑，登不同 Steam 账号）：
#   powershell -ExecutionPolicy Bypass -File run-steam.ps1 -Mode host   [--Players 2]
#   powershell -ExecutionPolicy Bypass -File run-steam.ps1 -Mode join
param(
    [ValidateSet('host','join')] [string]$Mode = 'host',
    [int]$Players = 2
)
$ErrorActionPreference = 'Stop'
Push-Location $PSScriptRoot

# 1) 用 steam feature 构建 client（默认构建不带 Steam）。
Write-Host "== cargo build -p client --features client/steam ==" -ForegroundColor Cyan
cargo build -p client --features client/steam
if ($LASTEXITCODE -ne 0) { Write-Host "build 失败。" -ForegroundColor Red; Pop-Location; exit 1 }

$exe = Join-Path (Get-Location) 'target\debug\client.exe'
# 2) 把 steam_api64.dll（从 repo 根或 steamworks-sys build out 拿一份）放到 exe 旁边。
$dllCandidates = @(
    (Join-Path '..\..' 'steam_api64.dll'),            # 仓库根 testingLL/steam_api64.dll
    (Get-ChildItem 'target\debug\build\steamworks-sys-*\out\steam_api64.dll' -ErrorAction SilentlyContinue | Select-Object -First 1 -ExpandProperty FullName)
)
$dllSrc = $dllCandidates | Where-Object { Test-Path $_ } | Select-Object -First 1
if ($dllSrc) {
    Copy-Item $dllSrc (Join-Path 'target\debug' 'steam_api64.dll') -Force
    Write-Host "[ok] steam_api64.dll -> target\debug\steam_api64.dll"
} else {
    Write-Host "WARN: 未找到 steam_api64.dll（放一份到仓库根即可）。" -ForegroundColor Yellow
}
# 3) steam_appid.txt 放到 exe 旁边（配合 init_app 双保险）。
$appidSrc = Join-Path '..\..' 'steam_appid.txt'
if (Test-Path $appidSrc) { Copy-Item $appidSrc (Join-Path 'target\debug' 'steam_appid.txt') -Force }

# 4) 启动。
if ($Mode -eq 'host') {
    Write-Host "== Steam HOST（玩家0）--steam-host --players $Players ==" -ForegroundColor Green
    & $exe "--steam-host" "--players" "$Players"
} else {
    Write-Host "== Steam JOIN --steam-join（自动搜 host 大厅）==" -ForegroundColor Green
    & $exe "--steam-join"
}
Pop-Location
