param(
    [ValidateSet('host','join','menu')] [string]$Mode = 'menu',
    [int]$Players = 2,
    [string]$LobbyId = '',
    # 界面语言（传给 client 的 --lang）：auto=跟随 Steam（默认）/ zh=中文 / en=英文。
    # 不传（''）则不覆盖，完全按 Steam + 本地设置决定。
    [ValidateSet('auto','zh','en','')] [string]$Lang = '',
    # 版本：`full` = 正式版（AppID 908660）/ `demo` = 试玩版（AppID 1042120）。
    # 决定编译期 feature（`client/demo` → AppID 1042120 且关工坊/成就/天梯）与 stage 的 steam_appid.txt。
    [ValidateSet('full','demo')] [string]$Target = 'full',
    # 诊断（R0）：加 `--netdiag` 启动 → 打印大厅 owner/成员、peer 连接状态、大厅聊天探针。
    [switch]$NetDiag
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

# R0 诊断（可选）：--netdiag → 网络诊断日志（大厅 owner/成员、peer 连接状态、大厅聊天探针）。
if ($NetDiag) {
    Write-Host '== netdiag enabled: --netdiag =='
    $argsList += @('--netdiag')
}

# 前台运行（&）并把控制台输出同时写到 logs/（进程内 logging 已带 ms 时间戳；
# 这里兜底捕获 net-steam 等库内直接用 eprintln! 的行）。
# 注意：本机 PowerShell 5.1 的 Tee-Object **不支持 -Encoding**，默认写 UTF-16LE（乱码）；
# 且 native exe 的 UTF-8 stderr 需先告诉 PowerShell 用 UTF-8 解码（否则中文先被错解）。
# 故这里：设 [Console]::OutputEncoding=UTF8 + 逐行用 UTF8（无 BOM）追加到文件。
$logDir = Join-Path $PSScriptRoot 'logs'
New-Item -ItemType Directory -Force -Path $logDir | Out-Null
$runStart = Get-Date

# ── 卡死自动转储（看门狗）──
# 背景：真机出现过“继任房主窗口无响应（真 hang）”，但日志只是“静静停住”，无法定位。
# 启动游戏前先拉起 tools/hang-watch.ps1：本机任一实例“窗口无响应”或“CPU 冻结 ≥8s”
# 连续两次 → 自动 MiniDumpWriteDump 到 logs\hang-<时间>-pid<PID>.dmp，并写同名 .txt 上下文。
# 事后只要拷 logs/ 就有现场，不需要外部工具。只监视本脚本启动之后的 client.exe。
$watcher = Join-Path $PSScriptRoot 'tools\hang-watch.ps1'
if (Test-Path $watcher) {
    $ctxDesc = "mode=$Mode target=$Target players=$Players lang=$Lang netdiag=$($NetDiag.IsPresent) args=$($argsList -join ' ')"
    try {
        Start-Process -FilePath 'powershell' -WindowStyle Hidden -ArgumentList @(
            '-NoProfile', '-ExecutionPolicy', 'Bypass',
            '-File', ("`"$watcher`""),
            '-GraceSeconds', '10',
            '-HangSeconds', '8',
            '-Context', ("`"$ctxDesc`"")
        ) | Out-Null
        Write-Host '== hang-watch: 已启动卡死看门狗（转储写到 logs\hang-*.dmp）==' -ForegroundColor Yellow
    } catch {
        Write-Host "WARN: hang-watch 启动失败：$($_.Exception.Message)" -ForegroundColor Yellow
    }
} else {
    Write-Host "WARN: 未找到 $watcher，跳过卡死自动转储。" -ForegroundColor Yellow
}

$stamp = Get-Date -Format 'yyyyMMdd-HHmmss'
$logFile = Join-Path $logDir "console-$Mode-$Target-$stamp.log"
Write-Host "== log -> $logFile =="
$prevEnc = [Console]::OutputEncoding
try { [Console]::OutputEncoding = [System.Text.Encoding]::UTF8 } catch {}
$utf8NoBom = New-Object System.Text.UTF8Encoding($false)
& $exe @argsList 2>&1 | ForEach-Object {
    # 注意：`Out-Host $_` 在本机 PowerShell 会报 PositionalParameterNotFound（`-InputObject` 非位置参数）
    # → 控制台每行都刷错（但日志文件仍照常写）。改用显式 `-InputObject`。
    Out-Host -InputObject $_
    try { [System.IO.File]::AppendAllText($logFile, "$_`r`n", $utf8NoBom) } catch {}
}
try { [Console]::OutputEncoding = $prevEnc } catch {}

# 本次运行是否抓到卡死转储？
$dumps = @(Get-ChildItem (Join-Path $logDir 'hang-*.dmp') -ErrorAction SilentlyContinue | Where-Object { $_.LastWriteTime -ge $runStart })
if ($dumps.Count -gt 0) {
    Write-Host ''
    Write-Host '!! 本次检测到卡死并已自动转储：' -ForegroundColor Red
    foreach ($d in $dumps) {
        $info = [System.IO.Path]::ChangeExtension($d.FullName, '.txt')
        Write-Host ("   {0}  ({1:N1} MB)" -f $d.FullName, ($d.Length / 1MB))
        if (Test-Path $info) { Write-Host "   上下文：$info" }
    }
    Write-Host '   拷 logs/ 时请把 hang-*.dmp 与同名 .txt 一起拿走。' -ForegroundColor Yellow
} else {
    Write-Host '[hang-watch] 本次无可用的卡死转储（正常）。' -ForegroundColor DarkGray
}
Pop-Location
