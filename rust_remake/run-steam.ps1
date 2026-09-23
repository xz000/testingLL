param(
    [ValidateSet('host','join','menu')] [string]$Mode = 'menu',
    [int]$Players = 2,
    [string]$LobbyId = '',
    # 界面语言（传给 client 的 --lang）：auto=跟随 Steam（默认）/ zh=中文 / en=英文。
    # 不传（''）则不覆盖，完全按 Steam + 本地设置决定。
    [ValidateSet('auto','zh','en','')] [string]$Lang = '',
    # 版本：`full` = 正式版（AppID 908660）/ `demo` = 试玩版（AppID 1042120）。
    # 决定编译期 feature（`client/demo` → AppID 1042120 且关工坊/成就/天梯）与 stage 的 steam_appid.txt。
    [ValidateSet('full','demo')] [string]$Target = 'full'
)

$ErrorActionPreference = 'Continue'
Push-Location $PSScriptRoot

# 版本差异只在这三样：编译 feature、AppID、界面标识（见 client/src/appid.rs）。
if ($Target -eq 'demo') {
    $Features = 'client/demo'
    $AppId    = '1042120'
} else {
    $Features = 'client/steam'
    $AppId    = '908660'
}

Write-Host "== cargo build -p client --features $Features [target=$Target] =="
cargo build -p client --features $Features 2>&1 | Out-Host
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
# steam_appid.txt：**直接按 $Target 写入**（不拷仓库根那份）。
# 理由：它是“脱离 Steam 直跑 exe”时告诉 Steam 客户端“这是哪个 app”的文件；
# 代码里的 AppID 已由 feature 编译期决定（见 appid.rs），两者必须一致，
# 否则 demo 开发跑会以正式版身份初始化（云/工坊/统计全作用到 908660 上）。
$appidPath = Join-Path $PSScriptRoot 'target\debug\steam_appid.txt'
Set-Content -Path $appidPath -Value $AppId -Encoding ASCII -NoNewline
Write-Host "[ok] steam_appid.txt = $AppId"

if ($Mode -eq 'menu') {
    # 进 Steam 版主菜单：按 3 进入 Steam 大厅，H 创建 / J 自动加入（无需输房间号）。
    Write-Host "== Steam MENU [$Target]（按 3 进入大厅，H 建厅 / J 自动加入）=="
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

# 语言覆盖（可选）：传给 client 的 --lang（auto/zh/en）。不传则不动。
if ($Lang -ne '') {
    Write-Host "== lang override: --lang $Lang =="
    $argsList += @('--lang', $Lang)
}

# 前台运行（&）并把控制台输出同时写到 logs/（进程内 logging 已带 ms 时间戳；
# 这里兜底捕获 net-steam 等库内直接用 eprintln! 的行）。
# 注意：本机 PowerShell 5.1 的 Tee-Object **不支持 -Encoding**，默认写 UTF-16LE（乱码）；
# 且 native exe 的 UTF-8 stderr 需先告诉 PowerShell 用 UTF-8 解码（否则中文先被错解）。
# 故这里：设 [Console]::OutputEncoding=UTF8 + 逐行用 UTF8（无 BOM）追加到文件。
$logDir = Join-Path $PSScriptRoot 'logs'
New-Item -ItemType Directory -Force -Path $logDir | Out-Null
$stamp = Get-Date -Format 'yyyyMMdd-HHmmss'
$logFile = Join-Path $logDir "console-$Mode-$Target-$stamp.log"
Write-Host "== log -> $logFile =="
$prevEnc = [Console]::OutputEncoding
try { [Console]::OutputEncoding = [System.Text.Encoding]::UTF8 } catch {}
$utf8NoBom = New-Object System.Text.UTF8Encoding($false)
& $exe @argsList 2>&1 | ForEach-Object {
    Out-Host $_
    try { [System.IO.File]::AppendAllText($logFile, "$_`r`n", $utf8NoBom) } catch {}
}
try { [Console]::OutputEncoding = $prevEnc } catch {}
Pop-Location
