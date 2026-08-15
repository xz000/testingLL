# 把本仓库 git hooks 指向项目内的 .githooks（提交前自动跑 check.ps1 回归）。
# 用法：
#   powershell -ExecutionPolicy Bypass -File install-hooks.ps1        # 安装
#   powershell -ExecutionPolicy Bypass -File install-hooks.ps1 -Off   # 卸载(恢复默认)
#
# 说明：
#   - 本项目的 git 仓库根在上级 testingLL/，本项目位于其子目录 rust_remake/。
#     故 core.hooksPath 必须用【绝对路径】指向 rust_remake/.githooks，
#     否则相对路径会被解析到仓库根 testingLL/.githooks（不存在，钩子不生效）。
#   - hooksPath 是本仓库级配置（--local），只影响当前仓库，不上传。

param([switch]$Off)

Push-Location $PSScriptRoot
$abs_githooks = Join-Path (Get-Location) '.githooks'

if ($Off) {
    git config --unset-all core.hooksPath 2>$null
    Write-Host "已卸载：恢复为默认 .git/hooks。" -ForegroundColor Green
} else {
    git config core.hooksPath $abs_githooks
    if ($LASTEXITCODE -eq 0) {
        Write-Host "已安装：core.hooksPath = $abs_githooks" -ForegroundColor Green
        Write-Host "现在每次 git commit 前都会自动跑 check.ps1（build+test+clippy）。"
        Write-Host "想跳过可用：  SKIP_HOOKS=1 git commit ..."
        Write-Host "验证可用：    git rev-parse --git-path hooks"
    } else {
        Write-Host "安装失败。" -ForegroundColor Red
        Pop-Location
        exit 1
    }
}

Pop-Location
exit 0
