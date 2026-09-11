# 修复路线（基于代码复核的审计结论）

日期：2026-09-11
方法：**以 Rust 代码为准逐条读码确认**，不采信规划文档（多份文档互相冲突/已过期，见文末）。
标记：✅ = 已读码确认；🔍 = 子代理报告、尚未逐一复核，需进一步验证。

> 优先级原则（沿用项目约定）：**先修正确性（帧同步/状态分叉），再修崩溃与联机健壮性，最后谈手感与新内容**。
> 每批应可独立提交、过完整门禁（`check.ps1`：build+test+clippy，非 steam 与 steam feature）。

---

## 一、已确认问题清单

### A. 崩溃 / 远程 DoS（可被网络或 CLI 触发）

| # | 级别 | 位置 | 问题 | 状态 |
|---|------|------|------|------|
| A1 | High | `client/src/main.rs:6314/6319` + `:3451` + `:1803/1812` | CLI `--steam-host/--steam-join` 取消/失败后 `app` 仍为 `SteamHost/SteamJoin` 且 `steam_active=false`，下一帧落入“单机带 AI”分支 `compute_inputs()`，对恒空的 `bot_targets/bot_rngs` 取下标 panic | ✅ |
| A2 | High | `game-core/src/netcode.rs:167-168` | `let n = r.u32()? as usize; Vec::with_capacity(n)` 无上限，报文头声明大值即巨量分配 → abort | ✅ |
| A3 | High | `net/src/proto.rs:208` | 快照长度 `world_bytes.len() as u16`，>64KiB 静默截断 → 重连快照损坏 | ✅ |
| A4 | Med | `net/src/frame.rs:61`、`net/src/proto.rs:292`、`game-core/src/world_ser.rs:414` | 报文 count / `last_hit_by` 等解码无上限/无边界校验 | 🔍（`world_ser:414` 已确认无 `<np` 校验） |
| A5 | Med | `game-core/src/world_ser.rs:369-538` | 反序列化不校验数值合法性（`out_dist=0`、`radius=0`、NaN 因子）→ 模拟内除零/`from_num` panic | 🔍 |

### B. 帧同步正确性（最高优先级）

| # | 级别 | 位置 | 问题 | 状态 |
|---|------|------|------|------|
| B1 | High | `net/`（全库） | **无校验和/分歧检测**：`FrameData` 无 hash，无周期性 state hash；两端静默分叉不可发现，只能靠重连快照纠偏。这是锁步最大架构缺口 | ✅ |
| B2 | High | `game-core/src/world.rs:1384/1395` | 模拟路径使用 f64 libm `cos/sin`，且 `emit_angle` 为 f64 进快照；其余几何走确定性 `cordic` → 跨平台非逐位一致，可静默 desync | ✅ |
| B3 | Med | `game-core/src/rng.rs:37` | `next_fix()` 用 `as i32 as i64` 符号扩展，实际落在 (-1,1) 而非 [0,1)，约一半取值为负，扭曲所有调用点分布（非 desync，是正确性） | ✅ |

### C. 联机健壮性

| # | 级别 | 位置 | 问题 | 状态 |
|---|------|------|------|------|
| C1 | High | `net/src/lockstep.rs:1038-1042/1069-1073` + `1131-1135` | client `pending` 无上限插入；缺口每帧重请求，超出 host `frame_buf_capacity=60`(`:114`) 后永不应答（客户端有 stale 兜底，但 pending 期间增长） | ✅ |
| C2 | Med | `net/src/lockstep.rs:479-486` | `drain_cfg` 丢弃 socket 中**所有**类型的包，非仅 PlayerCfg（靠 client 重发自愈，故降为 Med） | ✅ |
| C3 | High | `client/src/main.rs:3825/3711` | Esc/Q 退对局直接 `reset_to_main_menu()`，**从不 `leave_lobby`** → Steam 大厅席位泄漏成幽灵成员 | ✅ |
| C4 | High | `net/src/lockstep.rs:588-605` | 输入/房间包按 `index` 写状态，不校验 `from` 是否该槽合法 peer → 可冒名注入/篡改输入 | ✅ |
| C5 | Med | `client/src/main.rs:3990-4048` 等 | 配置同步 `ClientWait/HostGather` 无超时，host 掉线可永久软锁 | 🔍 |
| C6 | Med | `net/src/lockstep.rs:369`、`:243-247`；`net-steam/src/session.rs` 多处 | `orig_to_new().unwrap()`、`takeover` 数组越界、`lock().unwrap()` 中毒 panic 面 | 🔍 |

### D. 游戏逻辑正确性

| # | 级别 | 位置 | 问题 | 状态 |
|---|------|------|------|------|
| D1 | Med | `game-core/src/player.rs:395-398` | 减益时长只对 `Tied|Scorched` 除以 `debuff_dur_div`；`Slow/Pancake/Weakened/Silenced` 落入 `_` 被 `buff_dur_mult` **延长**，与同文件 `381-390` Mirror 减益列表自相矛盾 | ✅ |
| D2 | Med | `game-core/src/world.rs:818-823` | 对所有玩家 `caster.advance(dt)` 无 `alive` 检查，死亡未清 caster → 尸体施法 | ✅ |
| D3 | Med | `game-core/src/player.rs:791-821` | `reset_state` 未清 `lava_boot_cd/phoenix_remaining/windwalk_cd/aegis_charged/respawn_at/on_ice` 等回合瞬态，漏进下一局 | 🔍 |
| D4 | Low | `game-core/src/world.rs:1260` | `Periodic` 散布未防 `interval<=0` 死循环 | 🔍 |

### E. 死代码 / UI 误导

| # | 级别 | 位置 | 问题 | 状态 |
|---|------|------|------|------|
| E1 | Med | `client/src/main.rs`（`878/4487/5069/5615` 等） | `steam_build_done` 全仓只有 `=false`，永不为 true → “配好/按 P 确认”分支与显示全是死逻辑 | ✅ |
| E2 | Low | `client/src/main.rs:5360` | `steam_sess` 为 None 时仍置 `steam_list_searching=true`，推进逻辑依赖 sess → 可能永久显示“搜索中…” | ✅ |
| E3 | Low | 全仓 | 死字段/函数：`net_ready`、`steam_create_players`、`BOTS`、`draw_pre_game`、`shrink_speed`/`SHRINK_SPEED`、`ItemDef.sell`、`rng.next_fix_signed`、`proto.input_body` | 🔍 |

---

## 二、文档 vs 代码（“文档不一定对”成立）

- 明显过期：`ROADMAP.md`、`UI_MENUS.md`、`PLAYTEST.md`、`ATTRIBUTE_SYSTEM.md`（Solo/菜单/Steam 栈/测试数全部对不上）。
- 直接冲突：
  - 计分：`098C_DIFF.md`（胜2/杀1/助1）vs `PORT_098B_DECISIONS.md`（2/2/1）。
  - 熔岩成长：`098C_DIFF.md A8`（恒定 9/s）vs `PORT_098B_DECISIONS.md D9`（×round）。
  - 移动模型：`D14`（已定 accel/decel）vs `098C_DIFF.md A2`（仍列待拍板）。
  - Mana：`D3`（无蓝）vs `ATTRIBUTE_SYSTEM.md`（“加蓝”）。
- **权威序**：以 **代码 + `098C_DIFF.md` + `resume.md`** 为准；其余视为历史记录。

---

## 三、修复批次（逐步执行）

### 批次 1 — 隔离正确性小修（低风险，先做）  ✅ 已完成
- [x] RNG 符号扩展修正（`game-core/src/rng.rs:37`）：`as i32` → `as u32`，`next_fix()` 恢复 [0,1)；加回归测试。
- [x] 减益时长分类（`game-core/src/player.rs:395`）：只有「沉默」÷`debuff_dur_div`；减益不再被 `buff_dur_mult` 延长；增益×mult、化身 dur_mult 保留。
- [x] 死亡清 caster / advance 加 `alive` 门（`game-core/src/world.rs:818`、`record_death`）：杜绝尸体施法。
- [x] `reset_state` 补齐回合瞬态：`lava_boot_cd/phoenix_remaining/windwalk_cd/aegis_charged/respawn_at/on_ice`（`player.rs`）。
- [x] `steam_build_done` 死逻辑删除（`client/src/main.rs`）：字段恒 false 且显示全在 dead 的 `draw_pre_game`，已移除字段与各处引用，net API 传字面量 `false`。

### 批次 2 — 崩溃/DoS 加固  ✅ 已完成
- [x] 解码预分配加上限（防巨量分配）：`netcode.rs`（queued 按 `min(n, 包长)`）、`frame.rs`/`proto.rs`（count 按 remaining/3）。
- [x] 快照长度 `u16`→`u32`（`proto.rs` 编解码）：>64KiB 世界快照不再静默截断；加回归测试（70KB 往返）。
- [x] `world_ser.rs:414` `last_hit_by` 加 `<np` 边界校验（与玩家 id 同规）。
- [x] 修 CLI Steam 取消/失败崩溃：4 处取消/失败分支统一 `app=MainMenu`；`compute_inputs` 前置校验 `bot_targets/bot_rngs` 长度并加空世界保护。

### 批次 3 — 帧同步正确性（最高价值，需真机验证）  ✅ 已完成（自动 Resync 留待观察后跟进）
- [x] `world.rs:1384/1395` f64 libm `cos/sin` → 确定性 `crate::fix::{cos,sin}`（CORDIC）；f64 仅保留「常数×k+角度状态」的 IEEE 四则运算。
- [x] 新增周期性世界状态哈希 + 分歧检测：
  - `world_ser::state_hash(w)`（序列化字节 FNV-1a 64，纯整数确定）。
  - 协议新增 `Packet::StateHash{seq,hash}`；host 每 `SNAPSHOT_EVERY` 帧广播 host 应用完 `seq` 后的哈希。
  - client（Steam / LAN 各一路）推进到同 seq 时比对自身哈希，不一致 → 大声日志 + HUD 红条 + `desync_detected`。
  - 加测试：proto 往返、lockstep 缓存/取用、state_hash 确定性。
- [ ] **待跟进（需真机观察）**：确认哈希无误报后，再做「不一致即自动 Resync+快照重基线」。
      本批次只做「检测+警示」而不自动重连，避免误报导致踢人。A5（反序列化数值合法性）亦未含。

### 批次 4 — 联机健壮性
- [ ] `pending` 加上限 + 超时/降级；`try_advance` 缺口退避与计数。
- [ ] `drain_cfg` 只丢 `PlayerCfg`，其余包回投递。
- [ ] 退出对局补 `leave_lobby`（Esc/Q 路径）。
- [ ] 配置同步 `ClientWait/HostGather` 加超时回退。
- [ ] 输入序号来源校验（`slot_of` 同时校验 `from`）。

### 批次 5 — 文档与死代码清理
- [ ] 依代码事实重写/归档过期文档，消解上表冲突。
- [ ] 删除确认无用的死字段/函数。

---

## 四、进度记录

- 2026-09-11：建立本文件；批次 1 开工。
- 2026-09-11：**批次 1 全部完成并过门禁**（workspace test/clippy + steam test/clippy 全绿）。
- 2026-09-11：**批次 2 全部完成并过门禁**。下一步：批次 3（帧同步正确性，含周期性世界校验和）。
  注：A5（反序列化数值合法性校验，如 out_dist/radius 除零、NaN 因子）为 🔍 项，未含在批次 2，后续补。
- 2026-09-11：**批次 3 完成并过门禁**（确定性 trig + 周期性世界哈希与分歧检测；自动 Resync 待真机观察后跟进）。
  下一步：批次 4（联机健壮性）。
