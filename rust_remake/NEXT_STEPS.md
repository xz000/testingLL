# 交接记录 / 下一步（离线续接用）

> 唯一的当前权威续接入口。**2026-08-15 晚**重写为最新快照。
> 阅读顺序：先看本文件 → 缺背景再翻 `RECONNECT.md`（重连）/ `ROADMAP.md`（三功能）/
> `LATENCY_MASKING.md`（手感）/ `ATTRIBUTE_SYSTEM.md`（4.6b）/ `NET_REWRITE.md`（网络重写）/
> `LOCKSTEP_FOUNDATION.md`（基座）/ `UI_MENUS.md`（界面）。

## 当前状态（全绿）
- **单测 98 全绿 = game-core 79 + net 14 + client 5**；`cargo build --workspace`、`cargo test --workspace`、`cargo clippy --workspace -- -D warnings` 均绿、工作区干净。
- 技术栈：`game-core`（定点确定性核心）+ `net`（proto/handshake/lockstep 三层）+ `client`（ggez）。定点 `fixed=1.28`、三角 `cordic`；`Balance` 数值收敛层已建。
- 测试计数几乎每次新增/重连切片都在涨（79/14/5）。**续接时以 `cargo test --workspace` 为准，别信本文件里的静态数字。**

## 关键历史（速览，回滚/定位用）
- **tag 丢失 bug**（曾导致网络静默吞输入 + 旧测试假绿）→ 已修 + 防假绿纪律写入 PLAN「测试约定」。真机 4 窗口验证通过。
- **网络层已按「正确 lockstep」完整重写**：proto + handshake + lockstep 三层，删旧 session.rs。帧带 seq、host 等齐、client 严格按序、丢帧自动补发。
- **4.3 Balance**：手感/场地数值权威源收敛（base_speed/accel/decel/radii/hurt 等）。
- **4.4 判定**：当前 lockstep 承载「本地/内网多端逐位一致」，不背"显示上掩盖延迟"；真延迟掩盖（本地预测/回滚）另记 `LATENCY_MASKING.md`。
- **4.6 多局配置同步**：局域网多局经济/升级/洗点确定性同步 ✓（host 收齐广播 PlayerCfgAll、各端 apply）。
- **M0 主菜单 + M1 单机试验场**：`AppState`（MainMenu/Solo/LanHost/LanJoin）；Solo 无 AI + 不动靶子 + sandbox（不缩圈/不判结束）+ 开局配置；Lan 开局也先配置（round1 配置同步）。命令行 `--solo/--host/--join` 直通。

## 重连（RECONNECT.md 方案 A）—— 垂直切片已全部完成 ✅
| 切片 | 内容 | 提交 |
|------|------|------|
| 1 | host 掉线排除（mark_dropped 默认输入占位，不卡全队） | `3274e2e` |
| 2 | unmark_dropped + 快照重建 World + set_start_seq 接回 | `1ae894b` |
| 3a | proto：ReconnectReq / Snapshot / Resync | `c04f226` |
| 3b | game-core World 全量确定性序列化（world_to_bytes/from_bytes） | `5bbd333` |
| 3c | 重连测试改走 Packet::Snapshot 真字节链路 | `31e28e3` |

→ 重连主线（掉线不卡全队 + 真字节快照重连接回 + 两端逐位一致）已闭环、有测试锁死。

## 重连接入 client 真实运行时 ✅（本次会话完成，未提交）
把重连从“只有无头切片”接到真实运行的 ggez client 与 host，全部走真实 UDP：
- **net `lockstep.rs`**：
  - `HostLockstep` 加 `set_snapshot`/`current_snapshot`（周期保存快照）、`client_addr`（掉线后仍保留端点到槽位映射）、
    `idle_ticks` + `client_idle_ticks` + `auto_drop_idle(threshold)`（host 自动判定掉线）；`poll()` 改为：
    收到输入始终重记 peer、清零该端 idle；收到 `ReconnectReq` 按来源端点映射回槽位 → unmark + 回 `Snapshot` + 广播 `Resync(seq)`。
  - `ClientLockstep` 加 `send_reconnect_req` / `recv_snapshot` / `apply_resync`（对齐基线 seq）。
  - 语义锁定：**快照字节反映「已处理完 seq-1 帧」的世界，重连端从 seq 开始继续收帧**（seq = host.next_seq()）。
- **client `netlink.rs`（NetLink）**：加 `stale_ticks` + `bump_stale`/`stale_ticks`（掉线探测）、`try_reconnect`（发 ReconnectReq + 收 Snapshot）、
  `align_after_reconnect`（收 Resync 对齐基线）；`step_frame` 成功时清零 stale。
- **client `main.rs`（真实运行时）**：
  - host 端：每帧 `auto_drop_idle(HOST_DROP_TICKS)`（超时自动掉线不卡全队）；每 `SNAPSHOT_EVERY` 帧 `set_snapshot(world_to_bytes, next_seq)`。
  - client 端：收到权威帧即推进；没收到则 `bump_stale` + 乐观预测；`stale >= CLIENT_STALE_TICKS` → `conn_dropped` 冻结并显示重连 UI；
    按 **R** 触发 `try_reconnect` → 拉快照 → `align_after_reconnect` → 重建 World → 清输入残留 → 恢复 lockstep。
  - 常量：`HOST_DROP_TICKS=180`、`SNAPSHOT_EVERY=30`、`CLIENT_STALE_TICKS=180`。
- **新增测试锁死（防假绿）**：net `reconnect_snapshot_and_resync_roundtrip` + `host_auto_drops_idle_client`；client `netlink_reconnect_flow_resumes_identical_worlds`
  （真实 UDP，host+1 client 掉线→快照重连接回→两端逐位一致）。合起来把“host 自动判活 + 快照应答 + client 掉线重连”整条链路锁住。

→ 重连在真实运行时已闭环（host 自动掉线、快照应答、client 按 R 拉快照重建回接）。

## 单机启动死循环 bug + 修复（本次会话 +1，未提交）
真机/多开排查发现 `--solo` **无法启动**（进程存活但无窗口、无 `[main]` 日志），追查到底：
`main()` 命令行解析里 `--solo` 分支**漏了 `i += 1`**，导致 `while i < args.len()` 死循环，单机永远到不了建窗。
- 修复：补 `i += 1`；把解析抽成纯函数 `parse_app_from_args(&[String]) -> AppState`，加回归单测 `solo_parse_does_not_hang_and_selects_solo`（锁死 `--solo` 不再死循环）。
- 顺带体验改善：配置/学习面板**默认预选第一个技能树**（`learn_tree_key` 默认 C 键树），按数字键绑技能立即可用，不必先想到按字母选树；
  Solo 开局配置加 **15 秒超时自动默认开始**（`PRE_GAME_TIMEOUT_SECS`，显示提示），避免窗口没焦点/按键收不到时单机卡死。

## 局域网体验补完（ROADMAP M2）—— 起步
- **对局结束可回主菜单** ✅（本次）：`MatchPhase::Finished` 结算界面新增“按 Q 返回主菜单”（`reset_to_main_menu` 重建 2 玩家沙盒世界/meta、清空联网与运行状态），不再死屏幕。Esc 暂用不了（ggez0.10 的 `NamedKey::Escape` 需直接 import winit，先只用 Q）。
- 待做：玩家名/开局房间界面、局域网发现（可选）、对接续的下一个体验闭环。

## Steam 前置基础（本次，为换 SteamTransport 铺路）—— 已落 + 已测
- **`Peer` 抽象升级**：`net::transport::Peer` 由单一 `Udp(SocketAddr)` 扩为
  `Udp(SocketAddr)` + `Steam { id: u64, conn: Option<u32> }`（id 作稳定身份/SteamID/重连身份，见 RECONNECT 挂点 2）。
  UDP 传输路径不变（lockstep/handshake 只按 `Peer` 判等/转发，不关心变体）。
- **证明“换传输底层零改动”**：新增头测试 `lockstep_over_steam_peers_preserves_determinism`——用假想 `FakeSteamTransport`
  （以 `Peer::Steam` 为端点的内存邮箱）跑 `HostLockstep + ClientLockstep`，两端按序推进 + 逐位一致。
  这验证了将来 `SteamTransport`（真实 SteamNetworkingSockets/大厅）可直接复用现有 lockstep/多局/重连逻辑。
- 方向性决定：**局域网“房间列表/广播发现”不做**（Steam 大厅天然提供），玩家名按 Steam 昵称（局域网允许缺省）。优先保证 Steam 联机。

## host 提早收人修复（本次会话 +1，未提交）
多开实测发现：LAN host 在「开局配置」阶段从不 poll_join，要等 host 窗口按 Space 进入 Fighting 才开始收 client →
无头/手快时先到的 client 会握手超时（100 次后 panic 退出）。已把 host 收人逻辑抽成 `poll_host_join_phase()`，
并在「开局配置」与 Fighting 两阶段都调用，host 等人时就开始收人。
实测：`multi-launch.ps1 -Players 2` 客户端 **attempt 1 即 join_handshake OK**（原先 100 次超时），两端按 Space 均进入配置同步。

## 三大功能（ROADMAP.md）当前进度
- **单机技能试验场**：✅ 已可用（无 AI + 靶子 + 开局配置；`--solo` 或菜单按 1）。
- **局域网对战**：✅ lockstep 打通（多局配置同步/重连切片/开局配置已在 UDP 验证）；缺"中途退出的真实掉线 UI"（见重连剩余）。
- **Steamworks 对战**：未接；`Transport` 抽象已就绪，靠实现 `SteamTransport` 复用局域网这套（含重连）。

## 界面（UI_MENUS.md）
- M0 主菜单已做（三大入口）。暂停/退出菜单、单机调试辅助（无敌/重置CD/伤害数字）**未做**，规划在 UI_MENUS。

## 待议 / 下一步（按我此前优先级建议排序）
1. **✅ 已办（未提交）**：重连接进 client 真实运行时（见上节“重连接入 client 真实运行时”）。
   剩可后做：真机多窗口手动验证重连手感 / host 掉线处理（快照给候补 / 回大厅）。
2. **暂停/退出菜单 + 单机调试辅助**（体验层，见效快，无网络顾虑；联网暂停=本地暂停交互、不暂停时间，退出按 RECONNECT 离场处理）。
3. **4.6b 属性系统**（`ATTRIBUTE_SYSTEM.md`：法抗/护甲/移速血量成长等；网络层已就绪，只加 PlayerConfig 字段 + 合成）。
4. **4.7 完整回滚重放**（`LATENCY_MASKING.md` 阶段二；决策门：先真机感受乐观预测，跳变明显再做）。
5. **Steamworks**（最终目标必含重连；先局域网验证重连，再接 SteamTransport）。

> 建议先跑一次 `multi-launch.ps1` 真机多开，手动停掉一个 client 看 host 自动掉线 + 其余继续；再手动按 R 重连接回。
> 注意多窗口是给**真人操作**的：键盘输入只发给**有焦点的那一个窗口**，需逐个点选窗口后按 Space 完成各自开局配置，
> 才能让 host 收齐配置并开打。无头环境无法聚焦窗口，故逻辑用 UDP 单测（`host_and_two_clients_sync_over_udp`/`netlink_reconnect_flow_resumes_identical_worlds`）锁死。

## 常用命令
```
cargo test  --workspace                # 回归（97 全绿基线）
cargo clippy --workspace -- -D warnings
cargo run -p client -- --solo          # 单机试验场
powershell -File multi-launch.ps1 -Players 3   # 局域网多开（-Fast 加速局终看多局；可手动停窗看重连）
powershell -File check.ps1             # 一键 build+test+clippy
```

## ⚠ 环境大坑（务必再读）
git 仓库根在**上级 `testingLL/`**，`rust_remake/` 只是子目录。`core.hooksPath` 用绝对路径指到 `rust_remake/.githooks`；pre-commit 用 `$0` 定位项目根。已端到端验证：坏代码拦得住、干净提交放行。
