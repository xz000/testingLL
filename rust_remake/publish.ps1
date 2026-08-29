# ============================================================================
#  Steam 发布编译脚本（SteamPipe / steamcmd）
#
#  用途：把 client 编译成 release，收集产物（exe + steam_api64.dll），
#        生成 app_build VDF，再调用 steamcmd 上传到 Steam 后台。
#
#  用法：
#     powershell -ExecutionPolicy Bypass -File publish.ps1            # 编译+上传
#     powershell -ExecutionPolicy Bypass -File publish.ps1 -BuildOnly # 只编译+收集产物，不上传
#     powershell -ExecutionPolicy Bypass -File publish.ps1 -SetLive public  # 直接上 public
#
#  前置：
#    1) 已装 steamcmd（见 $SteamCmd），或让脚本从官网下载。
#    2) 到 Steamworks 后台创建应用，AppID=908660，建好 Depot（记下 DepotID）。
#    3) 填下面的配置节：$DepotId / $SteamUser（凭据用环境变量，别写死进脚本！）
#
#  注意：
#    - AppID 的三种位置各管各的（见文件头注释 / 说明文档）：
#        代码 const APP_ID        -> 运行时 init_app（非 Steam 启动时兜底）
#        steam_appid.txt          -> 本地直接跑 exe 的开发调试用，发布版【不打包】
#        本脚本的 VDF "AppID"     -> 真正"发布上传"绑定 appid 的地方
#    - 本脚本只负责编译+上传；实际发布操作会改动线上分支，运行前请再三确认
#      SetLive / DepotID / 凭据无误。
# ============================================================================

[CmdletBinding()]
param(
    # 上传后设为哪个分支：public / beta / [自定义分支名]；只构建不上传时忽略。
    [string]$SetLive = 'public',
    # 只编译+收集产物，不调用 steamcmd 上传（用于本地检查 staging 内容）。
    [switch]$BuildOnly,
    # 可选：覆盖默认的 steamcmd.exe 路径。
    [string]$SteamCmdExe = ''
)

$ErrorActionPreference = 'Stop'
Push-Location $PSScriptRoot

# ----------------------------------------------------------------------------
# 配置节（需自行填写 / 覆盖）
# ----------------------------------------------------------------------------
$AppId    = 908660
# Depot ID = 908661「Circle Brawl Content」（当前仅 Windows，语言 = 所有语言）。
# 未来加 Linux 等平台：为每个平台各新建一个 depot，并在 VDF 的 Depots 里列出多个。
$DepotId  = 908661

# Steam 登录账号（带 bot 的账号）。凭据从环境变量读，避免明文进仓库：
#   $env:STEAM_USER / $env:STEAM_PASS
$SteamUser = $env:STEAM_USER
$SteamPass = $env:STEAM_PASS

# steamcmd 位置；为空时自动探测常见路径，找不到则提示从官网下载。
# 仓库根（git 根）是 rust_remake 的上一级（含 steam_api64.dll / steam_appid.txt），
# steamcmd.exe 放在仓库根\steamcmd\ 下，符合 run-steam.ps1 对“根目录”的约定。
if (-not $SteamCmdExe) {
    $root = Split-Path $PSScriptRoot -Parent
    $SteamCmdExe = Join-Path $root 'steamcmd\steamcmd.exe'
}
# ----------------------------------------------------------------------------

$Staging = Join-Path $PSScriptRoot 'target\steam-pipe'
$Content = Join-Path $Staging 'content'
$OutDir  = Join-Path $Staging 'output'
$Vdf     = Join-Path $Staging "app_build_$AppId.vdf"

Write-Host '== 1/4 cargo build --release (client + steam) ==' -ForegroundColor Cyan
# 注意：native 命令的 stderr（如 cargo 编译进度）在 PS5.1+$ErrorActionPreference='Stop' 下若被
# 2>&1 重定向会误报为 NativeCommandError。这里不重定向，让输出直接透传，失败靠 $LASTEXITCODE 判断。
& cargo build --release -p client --features client/steam
if ($LASTEXITCODE -ne 0) { Write-Host '[FAIL] build 失败' -ForegroundColor Red; Pop-Location; exit 1 }

# ----------------------------------------------------------------------------
# 收集产物到 content/
# ----------------------------------------------------------------------------
Write-Host '== 2/4 收集产物到 staging ==' -ForegroundColor Cyan
if (Test-Path $Content) { Remove-Item $Content -Recurse -Force }
New-Item -ItemType Directory -Path $Content -Force | Out-Null
New-Item -ItemType Directory -Path $OutDir  -Force | Out-Null

# exe
$exe = Join-Path $PSScriptRoot 'target\release\client.exe'
if (-not (Test-Path $exe)) { Write-Host "[FAIL] 找不到 $exe" -ForegroundColor Red; Pop-Location; exit 1 }
Copy-Item $exe (Join-Path $Content 'client.exe') -Force
Write-Host "[ok] client.exe"

# steam_api64.dll（从 steamworks-sys 的编译输出目录找；找不到就提示手放一个）
$dllCands = @(
    (Join-Path $PSScriptRoot 'target\release\build\steamworks-sys-*\out\steam_api64.dll'),
    (Join-Path (Split-Path $PSScriptRoot -Parent) 'steam_api64.dll'),
    (Join-Path $PSScriptRoot 'steam_api64.dll')
)
$dll = $dllCands | Where-Object { Test-Path $_ } | Select-Object -First 1
if ($dll) {
    Copy-Item $dll (Join-Path $Content 'steam_api64.dll') -Force
    Write-Host "[ok] steam_api64.dll"
} else {
    Write-Host '[WARN] 未找到 steam_api64.dll，请把官方 dll 放到 staging content 根目录。' -ForegroundColor Yellow
}

# 【发布版刻意不复制 steam_appid.txt】—— 玩家从 Steam 客户端启动，无需该文件。

Write-Host "`n--- staging content 内容 ---"
Get-ChildItem $Content | Select-Object Name, Length | Format-Table -AutoSize

if ($BuildOnly) {
    Write-Host "`n[BuildOnly] 已编译+收集完成，未上传。" -ForegroundColor Green
    Pop-Location; exit 0
}

# ----------------------------------------------------------------------------
# 生成 app_build VDF
# ----------------------------------------------------------------------------
Write-Host '== 3/4 生成 app_build VDF ==' -ForegroundColor Cyan
if ($DepotId -eq 'REPLACE_WITH_YOUR_DEPOT_ID') {
    Write-Host '[FAIL] 请先填写脚本顶部 $DepotId（Steamworks 后台的 Depot ID）。' -ForegroundColor Red
    Pop-Location; exit 1
}

$esc = { param($p) ($p -replace '\', '\\') -replace '"', '\"' }
# VDF 里路径要用 /，避免反斜杠转义坑。
$contentRoot = ($Content -replace '\\', '/')
$outRoot     = ($OutDir  -replace '\\', '/')

$vdfBody = @"
"AppBuild"
{
	"AppID" "$AppId"
	"Desc" "rust_remake build ($(Get-Date -Format 'yyyy-MM-dd HH:mm'))"
	"BuildOutput" "$outRoot"
	"ContentRoot" "$contentRoot"
	"SetLive" "$SetLive"
	"Depots"
	{
		"$DepotId"
		{
			"FileMapping"
			{
				"LocalPath" "*"
				"DepotPath" "."
				"recursive" "1"
			}
		}
	}
}
"@
$vdfBody | Set-Content -Path $Vdf -Encoding UTF8
Write-Host "[ok] $Vdf"
Write-Host ($vdfBody)

# ----------------------------------------------------------------------------
# 调用 steamcmd 上传
# ----------------------------------------------------------------------------
Write-Host '== 4/4 steamcmd 上传 ==' -ForegroundColor Cyan
# 凭据：优先环境变量（自动化/CI 用）；否则每次上传交互输入（密码隐藏显示、不回显）。
if (-not $SteamUser) {
    $SteamUser = Read-Host 'Steam 登录账号'
}
if (-not $SteamPass) {
    $sec = Read-Host 'Steam 密码' -AsSecureString
    $bstr = [System.Runtime.InteropServices.Marshal]::SecureStringToBSTR($sec)
    $SteamPass = [System.Runtime.InteropServices.Marshal]::PtrToStringUni($bstr)
    [System.Runtime.InteropServices.Marshal]::ZeroFreeBSTR($bstr)
}
if (-not (Test-Path $SteamCmdExe)) {
    Write-Host "[FAIL] 找不到 steamcmd：$SteamCmdExe" -ForegroundColor Red
    Write-Host '       从官网下载并解压到仓库根目录的 steamcmd\ 下（与 steam_api64.dll 同级），或用 -SteamCmdExe 指定路径。'
    Pop-Location; exit 1
}

& $SteamCmdExe +login "$SteamUser" "$SteamPass" +run_app_build $Vdf +quit
if ($LASTEXITCODE -ne 0) {
    Write-Host '[FAIL] steamcmd 返回非零退出码，请检查凭据 / DepotID / 网络。' -ForegroundColor Red
    Pop-Location; exit 1
}
Write-Host "`n✅ 上传完成。到 Steamworks 后台「Builds」确认，并决定何时设为默认。"
Pop-Location
