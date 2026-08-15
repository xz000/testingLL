# 一键回归检查：build + test + clippy，任一失败立即非零退出。
# 用法：  powershell -ExecutionPolicy Bypass -File check.ps1
#         或 .\check.ps1
#
# 说明：
#   - 门禁包含 build / test / clippy(-D warnings)。三者当前全绿，可安全当提交门禁。
#   - cargo fmt 检查默认【关闭】：历史代码未按 rustfmt 归一，开它会误报大量 diff。
#     想临时开（例如整库格式化后）用：  -FmtCheck  （或 plain cargo fmt 先行格式化）。
#   - pre-commit 钩子调用本脚本（build+test+clippy 全程）。

param([switch]$FmtCheck)

$ErrorActionPreference = 'Stop'
Push-Location $PSScriptRoot

function Run([string]$what, [scriptblock]$cmd) {
    Write-Host "`n===== $what =====" -ForegroundColor Cyan
    & $cmd
    if ($LASTEXITCODE -ne 0) {
        Write-Host "`n[FAIL] $what 失败（退出码 $LASTEXITCODE）。已中止后续步骤。" -ForegroundColor Red
        Pop-Location
        exit 1
    }
    Write-Host "[ok] $what 通过。" -ForegroundColor Green
}

Run "cargo build --workspace"      { cargo build --workspace }
Run "cargo test --workspace"       { cargo test --workspace }
Run "cargo clippy --workspace -- -D warnings" { cargo clippy --workspace -- -D warnings }
if ($FmtCheck) {
    Run "cargo fmt --check"        { cargo fmt --check }
}

Write-Host "`n✅ 全部回归通过。可以安全提交。" -ForegroundColor Green
Pop-Location
exit 0
