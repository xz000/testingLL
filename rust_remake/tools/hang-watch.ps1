<#
tools/hang-watch.ps1 —— 「进程卡死 → 自动转储」看门狗（零依赖：只用 Windows 自带 user32/dbghelp）。

用途
  真机（多机）联机测试时，任何一台客户端出现**真 hang**（窗口无响应 / 主循环被阻塞）时，
  自动把该进程的内存转储写到本仓库 `logs\hang-<时间>-pid<PID>.dmp`，并在旁边写同名 `.txt`
  记录上下文（何时卡的、什么模式启动的、判据是什么）。
  事后只要把 `logs/` 拷回来，就有现场可分析——不需要重现命令、不需要装 procdump/WinDbg。

判据（实测校准过；连续 `-ConsecutiveNeeded`（默认 3）次轮询都成立才转储，避免误报）
  1) **窗口探活失败（主判据）**：对该进程的顶层窗口发 `SendMessageTimeout(WM_NULL + WM_GETTEXT)`，
     超时（返回 0）即该窗口所在线程没在泵消息。**排除控制台窗口类**（`ConsoleWindowClass` /
     `PseudoConsoleWindow` 由 conhost 代泵，健康时也永远响应，不能当探针）。
     判定 = “有可探窗口，且至少一个超时”。
  2) **CPU 冻结（辅助判据）**：进程总 CPU 时间连续 `HangSeconds * 2` 秒完全不变。
     覆盖“所有线程都被阻塞”（此时 CPU 必然 0）；注意：只挂主线程时其它线程仍在跑，CPU 不会冻结。

  实测（用 SuspendThread 挂住 client.exe 主线程模拟真 hang）：
    - 健康：`Window Class`(游戏窗口)/`Winit Thread Event Target` 均 WM_NULL=1、WM_GETTEXT=1；
    - 挂住主线程：两者均 =0，CPU 仍有 1.0s/6s（所以判据 1 是必需的）；
    - 挂住全部线程：CPU 0s/6s（判据 2 生效）；
    - `ConsoleWindowClass` 两种情况都 =1 → 必须排除。
  注：`IsHungAppWindow` **不可靠**（实测挂住 16s 仍返回 false，除非有人先给它发消息），故不用它。

用法（一般由 run-steam.ps1 自动拉起，无需手敲）
  powershell -ExecutionPolicy Bypass -File tools/hang-watch.ps1 -Context "mode=menu target=full"

参数
  -ProcessName   监视的进程名（默认 client.exe）
  -GraceSeconds  只监视“此刻之后”启动的进程（默认 10s；避免抓到上一次残留的实例）
  -ProbeTimeoutMs 单窗口探活的超时（默认 1500ms）
  -HangSeconds   判定所需的连续异常秒数（默认 8；CPU 冻结用 2 倍）
  -ConsecutiveNeeded  连续多少次轮询都异常才转储（默认 3）——实测遇到过“关窗口瞬间”探活超时一次，
                取 3（≈6s，poll 2s）可避免“慢退出”误抓；真实 hang（实测持续 70s）晚 2s 无影响
  -PollMs        轮询间隔（默认 2000ms）
  -StartupGrace  进程启动后的“免判”秒数（默认 20s）——避开加载/建窗期的瞬时无响应
  -MaxMinutes    最长监视时长（默认 240 分钟；被监视进程全部退出则提前结束）
  -Context       写进 .txt 的上下文串（启动模式/参数等）
  -Full          转储改为 MiniDumpWithFullMemory（默认只带“线程栈+线程信息”，体积小得多）

退出
  被监视进程全部退出、或超过 MaxMinutes、或本进程被杀。运行情况另写 `logs\hang-watch-<时间>.log`。
#>
param(
    [string]$ProcessName = 'client.exe',
    [int]$GraceSeconds = 10,
    [int]$ProbeTimeoutMs = 1500,
    [int]$HangSeconds = 8,
    [int]$ConsecutiveNeeded = 3,
    [int]$PollMs = 2000,
    [int]$StartupGrace = 20,
    [int]$MaxMinutes = 240,
    [string]$Context = '',
    [switch]$Full
)

$ErrorActionPreference = 'Continue'
$root = Split-Path $PSScriptRoot -Parent
$logDir = Join-Path $root 'logs'
if (-not (Test-Path $logDir)) { New-Item -ItemType Directory -Force -Path $logDir | Out-Null }

$stamp = Get-Date -Format 'yyyyMMdd-HHmmss'
$watchLog = Join-Path $logDir "hang-watch-$stamp.log"
$baseName = [System.IO.Path]::GetFileNameWithoutExtension($ProcessName)
$cutoff = (Get-Date).AddSeconds(-1 * $GraceSeconds)
$started = Get-Date
$deadline = $started.AddMinutes($MaxMinutes)

function Log([string]$m) {
    $line = '{0} {1}' -f (Get-Date -Format 'HH:mm:ss.fff'), $m
    try { [System.IO.File]::AppendAllText($watchLog, "$line`r`n", (New-Object System.Text.UTF8Encoding($false))) } catch {}
}

# ---- native 帮手：窗口探活 + 写转储 ----
$cs = @'
using System;
using System.Collections.Generic;
using System.Runtime.InteropServices;
using System.Text;

public static class HangWatch
{
    private delegate bool EnumWindowsProc(IntPtr hWnd, IntPtr lParam);

    [DllImport("user32.dll")]
    private static extern bool EnumWindows(EnumWindowsProc cb, IntPtr lParam);
    [DllImport("user32.dll")]
    private static extern uint GetWindowThreadProcessId(IntPtr hWnd, out uint pid);
    [DllImport("user32.dll")]
    private static extern bool IsWindow(IntPtr hWnd);
    [DllImport("user32.dll")]
    private static extern bool IsWindowVisible(IntPtr hWnd);
    [DllImport("user32.dll", CharSet = CharSet.Unicode)]
    private static extern int GetClassNameW(IntPtr hWnd, StringBuilder s, int n);
    [DllImport("user32.dll", SetLastError = true)]
    private static extern IntPtr SendMessageTimeout(IntPtr hWnd, uint msg, IntPtr wp, IntPtr lp,
                                                    uint flags, uint timeout, out IntPtr res);

    private const uint WM_NULL = 0x0000;
    private const uint WM_GETTEXT = 0x000D;
    private const uint SMTO_ABORTIFHUNG = 0x0002;
    private const uint SMTO_BLOCK = 0x0001;

    /// 控制台窗口由 conhost 代泵消息，健康时也“永远响应”，不能当探针。
    private static bool SkipClass(string cls)
    {
        if (cls == null) { return false; }
        if (cls == "ConsoleWindowClass") { return true; }
        if (cls == "PseudoConsoleWindow") { return true; }
        if (cls.IndexOf("GDI+ Hook Window") >= 0) { return true; }
        return false;
    }

    /// 返回 "probed=<n> timedOut=<m> first='<class>'"；n=0 表示没有可探窗口（无法判定）。
    public static string Probe(uint pid, uint timeoutMs)
    {
        int probed = 0;
        int bad = 0;
        string first = "";
        EnumWindowsProc cb = delegate(IntPtr h, IntPtr l)
        {
            uint p;
            GetWindowThreadProcessId(h, out p);
            if (p != pid) { return true; }
            if (!IsWindow(h)) { return true; }
            StringBuilder c = new StringBuilder(256);
            GetClassNameW(h, c, 256);
            string cls = c.ToString();
            if (SkipClass(cls)) { return true; }

            probed++;
            IntPtr r1, r2;
            IntPtr a = SendMessageTimeout(h, WM_NULL, IntPtr.Zero, IntPtr.Zero,
                                          SMTO_ABORTIFHUNG | SMTO_BLOCK, timeoutMs, out r1);
            IntPtr b = SendMessageTimeout(h, WM_GETTEXT, IntPtr.Zero, IntPtr.Zero,
                                          SMTO_ABORTIFHUNG | SMTO_BLOCK, timeoutMs, out r2);
            if (a == IntPtr.Zero || b == IntPtr.Zero)
            {
                bad++;
                if (first.Length == 0) { first = cls; }
            }
            return true;
        };
        EnumWindows(cb, IntPtr.Zero);
        GC.KeepAlive(cb);
        return String.Format("probed={0} timedOut={1} first='{2}'", probed, bad, first);
    }

    [DllImport("dbghelp.dll", SetLastError = true)]
    private static extern bool MiniDumpWriteDump(IntPtr hProcess, uint pid, IntPtr hFile, uint dumpType,
                                                 IntPtr exParam, IntPtr userStream, IntPtr cbParam);
    [DllImport("kernel32.dll", SetLastError = true)]
    private static extern IntPtr OpenProcess(uint access, bool inherit, uint pid);
    [DllImport("kernel32.dll", SetLastError = true)]
    private static extern bool CloseHandle(IntPtr h);
    [DllImport("kernel32.dll", SetLastError = true, CharSet = CharSet.Unicode)]
    private static extern IntPtr CreateFileW(string name, uint access, uint share, IntPtr sec,
                                             uint disp, uint flags, IntPtr tmpl);

    /// 成功返回空串；失败返回原因（含 Win32 错误码）
    public static string Dump(uint pid, string path, bool full)
    {
        const uint PROCESS_ALL_ACCESS = 0x1F0FFF;
        const uint GENERIC_WRITE = 0x40000000;
        const uint CREATE_ALWAYS = 2;
        const uint FILE_ATTRIBUTE_NORMAL = 0x80;
        const uint MiniDumpNormal = 0x0;
        const uint MiniDumpWithFullMemory = 0x2;
        const uint MiniDumpWithThreadInfo = 0x1000;

        IntPtr hProc = OpenProcess(PROCESS_ALL_ACCESS, false, pid);
        if (hProc == IntPtr.Zero) { return "OpenProcess failed err=" + Marshal.GetLastWin32Error(); }
        IntPtr hFile = CreateFileW(path, GENERIC_WRITE, 0, IntPtr.Zero, CREATE_ALWAYS, FILE_ATTRIBUTE_NORMAL, IntPtr.Zero);
        if (hFile == IntPtr.Zero || hFile == (IntPtr)(-1))
        {
            CloseHandle(hProc);
            return "CreateFile failed err=" + Marshal.GetLastWin32Error();
        }
        uint type = full
            ? (MiniDumpNormal | MiniDumpWithFullMemory | MiniDumpWithThreadInfo)
            : (MiniDumpNormal | MiniDumpWithThreadInfo);
        bool ok = MiniDumpWriteDump(hProc, pid, hFile, type, IntPtr.Zero, IntPtr.Zero, IntPtr.Zero);
        int err = ok ? 0 : Marshal.GetLastWin32Error();
        CloseHandle(hFile);
        CloseHandle(hProc);
        return ok ? "" : ("MiniDumpWriteDump failed err=" + err);
    }
}
'@

try {
    Add-Type -TypeDefinition $cs -Language CSharp -ErrorAction Stop
} catch {
    Log "FATAL: Add-Type failed: $($_.Exception.Message)"
    exit 2
}

Log "watch start: proc=$baseName grace=${GraceSeconds}s probe=${ProbeTimeoutMs}ms hang=${HangSeconds}s consecutive=${ConsecutiveNeeded} poll=${PollMs}ms max=${MaxMinutes}min full=$($Full.IsPresent) context=[$Context]"
Log "repo=$root dumpDir=$logDir"

# pid -> @{ cpu=总CPU秒; cpuSince=上一次CPU变化的时刻; bad=连续判定卡死次数; dumped=是否已转储; seen=首次发现时刻 }
$state = @{}
$sawAny = $false
$loopStart = Get-Date

while ($true) {
    if ((Get-Date) -gt $deadline) { Log 'exit: 达到 MaxMinutes'; break }

    $procs = @()
    try { $procs = @(Get-Process -Name $baseName -ErrorAction SilentlyContinue) } catch {}

    $live = @()
    foreach ($p in $procs) {
        $st = $null
        try { $st = $p.StartTime } catch { continue }
        if ($st -lt $cutoff) { continue }   # 本次测试之前启动的旧实例，不监视
        $sawAny = $true
        $live += $p.Id

        if (-not $state.ContainsKey($p.Id)) {
            $state[$p.Id] = @{ cpu = -1.0; cpuSince = (Get-Date); bad = 0; dumped = $false; start = $st; seen = (Get-Date) }
            Log "watch pid=$($p.Id) startTime=$($st.ToString('HH:mm:ss'))"
        }
        $s = $state[$p.Id]

        # 刚启动的阶段不参与判定（加载大素材/建窗时可能瞬时“无响应”）。
        if (((Get-Date) - $s.seen).TotalSeconds -lt $StartupGrace) { continue }

        $cpu = -1.0
        try { $cpu = $p.TotalProcessorTime.TotalSeconds } catch { continue }
        $now = Get-Date

        # CPU 是否冻结
        if ($s.cpu -lt 0 -or ($cpu - $s.cpu) -gt 0.05) {
            $s.cpu = $cpu
            $s.cpuSince = $now
        }
        $frozenSecs = ($now - $s.cpuSince).TotalSeconds
        $cpuFrozenLong = ($frozenSecs -ge ($HangSeconds * 2))

        # 判据 1：窗口探活（主判据）
        $probe = ''
        $timedOut = 0
        $probed = 0
        try {
            $probe = [HangWatch]::Probe([uint32]$p.Id, [uint32]$ProbeTimeoutMs)
            if ($probe -match 'probed=(\d+) timedOut=(\d+)') { $probed = [int]$Matches[1]; $timedOut = [int]$Matches[2] }
        } catch {}
        $winHung = ($probed -gt 0) -and ($timedOut -gt 0)

        $hung = $winHung -or $cpuFrozenLong
        if ($hung) {
            $s.bad = $s.bad + 1
            $why = @()
            if ($winHung) { $why += ("window-probe-timeout[$probe]") }
            if ($cpuFrozenLong) { $why += ("cpu-frozen-{0:N1}s" -f $frozenSecs) }
            Log "pid=$($p.Id) 可疑卡死($($why -join '+')) 连续=$($s.bad)/$ConsecutiveNeeded cpu=$([math]::Round($cpu,1))s"
        } else {
            if ($s.bad -gt 0) { Log "pid=$($p.Id) 恢复正常（撤销可疑计数） probe=[$probe]" }
            $s.bad = 0
        }

        if ($s.bad -ge $ConsecutiveNeeded -and -not $s.dumped) {
            $s.dumped = $true
            # 若已有别的 watcher（如同一机器上多个 run-steam.ps1 各拉一个）为这个 pid 转储过，就不重复。
            $existing = @(Get-ChildItem (Join-Path $logDir "hang-*-pid$($p.Id).dmp") -ErrorAction SilentlyContinue)
            if ($existing.Count -gt 0) {
                Log "pid=$($p.Id) 已有转储（$($existing[0].Name)）→ 本 watcher 不再重复"
                continue
            }
            $dmpStamp = Get-Date -Format 'yyyyMMdd-HHmmss'
            $dmp = Join-Path $logDir "hang-$dmpStamp-pid$($p.Id).dmp"
            $info = Join-Path $logDir "hang-$dmpStamp-pid$($p.Id).txt"
            Log "pid=$($p.Id) 判定卡死 → 开始转储 $dmp"
            $err = [HangWatch]::Dump([uint32]$p.Id, $dmp, $Full.IsPresent)
            if ($err -eq '') {
                $sz = 0
                try { $sz = (Get-Item -LiteralPath $dmp).Length } catch {}
                Log ("pid={0} 转储完成：{1} ({2:N1} MB)" -f $p.Id, $dmp, ($sz / 1MB))
            } else {
                Log "pid=$($p.Id) 转储失败：$err"
            }
            $txt = @(
                "hang dump context",
                "time        = $(Get-Date -Format 'yyyy-MM-dd HH:mm:ss')",
                "pid         = $($p.Id)",
                "process     = $baseName",
                "startTime   = $($s.start.ToString('yyyy-MM-dd HH:mm:ss'))",
                "cpuSeconds  = $([math]::Round($cpu,1))",
                "windowProbe = $probe   (probed=可探窗口数；timedOut=其中超时数；first=第一个超时的窗口类)",
                "cpuFrozenSec= $([math]::Round($frozenSecs,1))",
                "winHung     = $winHung",
                "dumpFile    = $(Split-Path $dmp -Leaf)",
                "dumpError   = $err",
                "context     = $Context",
                "watchLog    = $(Split-Path $watchLog -Leaf)",
                "如何分析：用 cdb/WinDbg 打开 dmp 看所有线程栈（cdb 命令：~*k）",
                "          或把 .dmp 交给同事/AI；重点看主线程停在哪（我们的循环 / Steam / 驱动 / 磁盘）。"
            )
            try { [System.IO.File]::WriteAllLines($info, $txt, (New-Object System.Text.UTF8Encoding($false))) } catch {}
        }
    }

    foreach ($k in @($state.Keys)) { if ($live -notcontains $k) { $state.Remove($k) } }

    if (($live.Count -eq 0) -and $sawAny) { Log 'exit: 被监视进程已全部退出'; break }
    if ((-not $sawAny) -and ((Get-Date) - $loopStart).TotalSeconds -gt ($GraceSeconds + 60)) {
        Log 'exit: 未等到目标进程启动（超时）'; break
    }

    Start-Sleep -Milliseconds $PollMs
}

Log 'watch end'
exit 0
