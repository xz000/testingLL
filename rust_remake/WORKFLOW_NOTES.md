# 工程操作注意事项（踩坑记录）

本文件记录**做事的坑**（不是游戏内容），避免重复踩。

## 1. 用脚本改代码时：锚点只用 ASCII（2026-09-12，多次踩）

**现象**：用 Python 脚本做 `str.replace(锚点, 新内容)` 批量改代码时，
**含中文的锚点经常匹配不到** → 脚本"看起来成功"（甚至打印了 ok）但**实际什么都没改** ✗，
或改到一半断言失败导致**整个改动静默丢失** ✗。

**根因**：终端/控制台的编码不是 UTF-8（`cmd` 默认 GBK），
而且**探针输出里的中文是乱码** ✗ → 我"照着乱码猜原文"写出的锚点自然对不上 ✗。

**规则（强制）**：
1. **锚点只用 ASCII**：用行内可辨识的 ASCII 片段校验，例如
   `'m.profiles[0].gold, 40 + 30'`、`'pub fn enter_first_round'`、`'fn draw_menu'`。
2. **优先按行号改**：先探针打印行号 → 用 `set_line(n, ascii_sub, new)`（断言该行包含 `ascii_sub`）→
   **降序处理**多行编辑（避免行号漂移）。
3. **写入中文没有问题**：新建/替换的**内容**可以随便用中文 ✓；只有**匹配锚点**不能用中文 ✗。
4. **每个改动都要断言**：`assert s.count(锚点) == 1`，失败就整脚本中止（宁可不改，不可改一半 ✗）。
5. **改完必须跑门禁**：`cargo test --workspace` + 两套 `cargo clippy -D warnings`。

**已有的可靠套路**（本仓库多处在用）：
- 行号 + ASCII 校验：`set_line` / `ins_after`
- 结构断言式源码检测：`client/src/keys.rs` 的 `source_scan_tests`（找不到锚点就 panic ✓，
  重构改名会立刻暴露而不是静默失效 ✓）

## 2. 中文工具输出：先落文件再看

`cmd` 里 `python -c "...中文..."` 经常**静默失败** ✗。
可靠做法：写成脚本文件（`_q.py`）→ `python _q.py` → 输出**重定向到文件** → 再用 `read` 工具读 ✓。

## 3. 旧二进制误判

改完必须确认**跑的是新 exe**：`dir target\debug\client.exe` 看时间戳；
或直接在 exe 里搜新增字符串 ✓。曾被 9/6 的旧 exe 误导过 ✗。

## 4. PowerShell `Tee-Object` 默认写 UTF-16（日志乱码）

`run-steam.ps1` 曾用 `... 2>&1 | Tee-Object -FilePath $log` 同时看控制台+存盘，
但**Windows PowerShell 5.1 的 `Tee-Object` 不支持 `-Encoding`**，默认写成 **UTF-16LE + BOM**，
用 UTF-8 工具读就全乱码。而且 native exe 的 stderr 是 UTF-8，若 `[Console]::OutputEncoding` 不是 UTF-8，
PowerShell 会先按 GBK 解码 → 中文先错解再存盘。

**修复（已在 run-steam.ps1）**：设 `[Console]::OutputEncoding = UTF8`，逐行用
`[System.IO.File]::AppendAllText($log, "$_`r`n", (New-Object System.Text.UTF8Encoding($false)))` 追加（UTF-8 无 BOM）。

**验证**：`python -c "print(open('logs/console-xxx.log','rb').read(4))"` → 应为 ASCII/UTF-8，**不是** `\xff\xfe`。
进程内 `logging.rs` 的 `app-*.log` 一直就是 UTF-8，不受影响；
以后优先看 `app-*.log`（它才是带 ms 时间戳的诊断主文件）。

## 5. 源码扫描测试：`main.rs` 是 **CRLF**，`include_str!` 不规范化（2026-09-13，假通过）

`keys.rs` 的 `source_scan_tests` 用 `include_str!("main.rs")` 做结构断言。
`main.rs` 为 **CRLF** 换行，因此 `SRC.find("\n    }\n")` **永远匹配不到**，
`unwrap_or(剩余全文)` 就把“整段后续代码”当作函数体 —— 于是断言 `!body.contains(...)` **恒真**（假通过），
有些回归检测一直没有真正生效。

**修复**：改用 `fn_body(name)`（兼容 `\n    }\r\n` 与 `\n    }\n`，找不到闭合直接 panic），并修正全部 7 处提取点。
**教训**：源码扫描测试的“取函数体”必须对换行/缩进鲁棒，且“提取失败”应显式报错（而非 `unwrap_or(全文)`）。
