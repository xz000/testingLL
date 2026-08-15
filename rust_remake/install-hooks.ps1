# 把本仓库 git hooks 指向项目内的 .githooks（提交前自动跑 check.ps1 回归）。
# 用法：
#   powershell -ExecutionPolicy Bypass -File install-hooks.ps1        # 安装
#   powershell -ExecutionPolicy Bypass -File install-hooks.ps1 -Off   # 卸载(恢复默认)
#
# 说明：hooksPath 是本仓库级配置（--local），只影响当前仓库，不上传到云端。

param([switch]$Off)

Push-Location $PSScriptRoot

if ($Off) {
    git config --unset-all core.hooksPath 2>$null
    Write-Host "已卸载：恢复为默认 .git/hooks。" -ForegroundColor Green
} else {
    $rel = ".githooks"
    git config core.hooksPath $rel
    if ($LASTEXITCODE -eq 0) {
        Write-Host "已安装：core.hooksPath = $rel" -ForegroundColor Green
        Write-Host "现在每次 git commit 前都会自动跑 check.ps1（build+fmt+test+clippy）。"
        Write-Host "想跳过可用：  SKIP_HOOKS=1 git commit ..."
    } else {
        Write-Host "安装失败。" -ForegroundColor Red
        Pop-Location
        exit 1
    }
}

Pop-Location
exit 0
