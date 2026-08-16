# 交接记录 / 下一步（离线续接用）

> 唯一的当前权威续接入口。**2026-08-15 晚**重写为最新快照。
> 阅读顺序：先看本文件 → 缺背景再翻 `RECONNECT.md`（重连）/ `ROADMAP.md`（三功能）/
> `LATENCY_MASKING.md`（手感）/ `ATTRIBUTE_SYSTEM.md`（4.6b）/ `NET_REWRITE.md`（网络重写）/
> `LOCKSTEP_FOUNDATION.md`（基座）/ `UI_MENUS.md`（界面）。

## 当前状态（全绿）
- **单测 93 全绿 = game-core 79 + net 11 + client 3**；`cargo build --workspace`、`cargo test --workspace`、`cargo clippy --workspace -- -D warnings` 均绿、工作区干净。
- 技术栈：`game-core`（定点确定性核心）+ `net`（proto/handshake/lockstep 三层）+ `client`（ggez）。定点 `fixed=1.28`、三角 `cordic`；`Balance` 数值收敛层已建。
- 测试计数几乎每次新增/重连切片都在涨（79/11/3）。**续接时以 `cargo test --workspace` 为准，别信本文件里的静态数字。**

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

## 三大功能（ROADMAP.md）当前进度
- **单机技能试验场**：✅ 已可用（无 AI + 靶子 + 开局配置；`--solo` 或菜单按 1）。
- **局域网对战**：✅ lockstep 打通（多局配置同步/重连切片/开局配置已在 UDP 验证）；缺"中途退出的真实掉线 UI"（见重连剩余）。
- **Steamworks 对战**：未接；`Transport` 抽象已就绪，靠实现 `SteamTransport` 复用局域网这套（含重连）。

## 界面（UI_MENUS.md）
- M0 主菜单已做（三大入口）。暂停/退出菜单、单机调试辅助（无敌/重置CD/伤害数字）**未做**，规划在 UI_MENUS。

## 待议 / 下一步（按我此前优先级建议排序）
1. **把重连接进 client 真实运行时**（RECONNECT 残余）：
   - host 端真实「上行超时判定 → 自动 mark_dropped」（目前是显式调用）。
   - host 端把 World 快照周期 save + 应答 ReconnectReq 返回 Snapshot。
   - client 端重连入口：掉线 → 重连按钮 → 拉 Snapshot → 重建 World → set_start_seq 接回。
   - host 掉线处理（快照给候补 / 回大厅）可后做。
2. **暂停/退出菜单 + 单机调试辅助**（体验层，见效快，无网络顾虑；联网暂停=本地暂停交互、不暂停时间，退出按 RECONNECT 离场处理）。
3. **4.6b 属性系统**（`ATTRIBUTE_SYSTEM.md`：法抗/护甲/移速血量成长等；网络层已就绪，只加 PlayerConfig 字段 + 合成）。
4. **4.7 完整回滚重放**（`LATENCY_MASKING.md` 阶段二；决策门：先真机感受乐观预测，跳变明显再做）。
5. **Steamworks**（最终目标必含重连；先局域网验证重连，再接 SteamTransport）。

## 常用命令
```
cargo test  --workspace                # 回归（93 全绿基线）
cargo clippy --workspace -- -D warnings
cargo run -p client -- --solo          # 单机试验场
powershell -File multi-launch.ps1 -Players 3   # 局域网多开（-Fast 加速局终看多局）
powershell -File check.ps1             # 一键 build+test+clippy
```

## ⚠ 环境大坑（务必再读）
git 仓库根在**上级 `testingLL/`**，`rust_remake/` 只是子目录。`core.hooksPath` 用绝对路径指到 `rust_remake/.githooks`；pre-commit 用 `$0` 定位项目根。已端到端验证：坏代码拦得住、干净提交放行。
