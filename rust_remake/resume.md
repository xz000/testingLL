# 交接 · 下次从这里继续

> 创建/更新 2026-08-30（同日第二次更新）。把当前进度、已解决、待办整理成一份可接续的记录。
> 阅读顺序：本文 → `NEXT_STEPS.md` / `WORK_BACKLOG.md`（历史累计）。

## 0. 项目与命令

- 项目根：`C:/Users/xvzan/Documents/testingLL/rust_remake`
- 常用命令：
  ```
  cargo test --workspace                                  # 回归（当前 146 全绿：5+101+31+9）
  cargo test -p client --features client/steam            # steam 版 client 测试
  cargo clippy --workspace -- -D warnings
  cargo clippy -p client --features client/steam -- -D warnings
  powershell -File check.ps1                              # 一键 build+test+clippy（pre-commit 也会跑）
  powershell -File run-steam.ps1 -Mode menu               # Steam 版主菜单（H 建房 / J 加入）
  powershell -File multi-launch.ps1 -Players 2 -Fast      # 本机 LAN 双开（-Fast=FASTROUND，局间 3s、4 局）
  ```
- 提交会触发 pre-commit 钩子跑回归（`rust_remake/.githooks/pre-commit` → `check.ps1`）。
  若 `target\debug\client.exe` 被游戏窗口占用会构建失败（拒绝访问）→ 先关掉 client 窗口再提交，或 `SKIP_HOOKS=1 git commit`。

## 1. 当前 git 状态（工作区干净）

分支 `master`，本地 HEAD 为 `e7acb59`，基于 `838c7af` 重建的干净主线（已排除有 bug 的 `de4f6a5`，用批 A 替代）：

```
e7acb59  chore(client): 删除配置同步阶段每帧刷屏的 CLIENT SEND 诊断日志（根因已修，保留每局一次日志）
fab3ba2  fix(game-core): 自爆(F)自残数值对齐 Unity——最多自扣10血、保底留1血(GetHurt min(10,hp-1))，取代固定扣到1血；加低血量保底测试
860e975  fix(client,net): 不满员手动倒计时跨端同步——host按回车后随RosterReady心跳广播剩余毫秒、client端显示同一倒计时并按U可取消（最后LOCK秒锁定）、协议RosterReady加manual_ms字段
c315baa  fix(client,net): 局间技能同步彻底修复——self_index缓存lan_my_index上报正确profile、host等client崩溃保护、host配置同步drain在途旧包+稳定等待、FASTROUND学习3s
ee85329  feat(game-core,client): 闪电(D1)画面效果——world记录瞬态lightning_visual(0.1s)，client画蓝线
6b6ce38  diag(client): 局间技能同步诊断日志（cfg-sync apply匹配/teardown前后skill_levels）
9fb61bc  1                                        ← 移动目标修复（提交信息误写成"1"，内容正确）
a7b7886  fix(client): 施法改为持续重发直到被世界接受——修复 client 放技能迟滞
d651708  refactor(client,meta): 统一首局与局间的配置学习阶段（首局倒计时归零开战、去按P、局间补 Steam 配置同步）
6556bc6  fix(game-core): 弹体撞柱子必须被挡下（火球穿柱）   ← 摘自"他"
84c6f1a  fix(client/steam): 不满员时房主按回车走 5 秒倒计时  ← 摘自"他"
b441d9e  fix(client): 移动目标电平量、仅施法进入前摇才清（批 A，修复右键丢指令）
838c7af  (原 HEAD)
```

被排除（不在 master，可随时 cherry-pick 找回）：
- `de4f6a5` 施法/移动 take（有 bug，被批 A 取代）
- `11c5a8e` 技能按树解锁定价（用户暂缓，待定定价模型）
- `4fc060c` 根因分析文档（`GAMEPLAY_FIX_PLAN.md`，内容仍可参考）

> 远程 `origin/master` 与本地已 diverged，用户对远程无顾虑，未 push。

## 2. 已解决（2026-08-30 本轮）

### 2.1 局间技能"没学到"（高优先级，`c315baa`）✅ 验证通过
现象：局间配置界面改技能，下一局没生效（之前 d651708 引入局间配置同步后出现）。**两个根因叠加**。已分别经 **LAN 双开 `FASTROUND`** 与 **Steam 双机（真机）** 验证：局间绑定全部保留、host/client 一致。

- **根因 1（client 上报错 profile）**：`self_index()`（`main.rs:1197`）在配置同步期间 `net_link` 被 `mem::take` 置 None → 落到 `None` 分支返回 `PLAYER_ID=0`。LAN client（本是 player 1）`local_player_cfg()` 上报的是 pid=0（host）配置，自己的绑定没上报，host 广播回来又把本地绑定覆盖成空。
  - 修复：加 `lan_my_index` 缓存，`take net_link` 时缓存 `link.my_index()`，`self_index()` 的 `None` 分支用它（与 Steam 的 `steam_active` 同款防护）。
- **根因 2（host 配置收集竞态）**：host 比 client 早归零进 HostGather，收到**上一局在途旧包**就 `all_cfgs()` 满足、广播旧配置，把 client 本轮新绑技能覆盖（所以早几局绑的 R/F 保留、本轮新绑的 E/T 丢）。
  - 修复：host 进配置同步先 `drain_cfg()`（`net/lockstep.rs`，丢弃在途 PlayerCfg）+ `reset_cfgs()` 清残留再收本轮；`host_cfg_drained` 标记保证只 drain 一次；保留 `HOST_CFG_SETTLE_TICKS=15` 帧稳定等待兜底。
- **附带修复**：host 等 client 加入期间误走单机 AI 分支崩溃（`compute_inputs` 访问空 `bot_targets`）——`Fighting` else 分支加 `if self.net_host.is_some() { return }`。
- **FASTROUND** 局间学习从 1s → 3s（方便手测）。

### 2.2 闪电（D1·雷电）画面效果（`ee85329`）✅
对照 Unity `TestSkillLightning.cs`：伤害/击退数值与 Unity 一致，只缺画面效果（Unity 有 `LineRenderer::Drawline`）。已给 `World` 加瞬态 `lightning_visual`（0.1s，不参与确定性/序列化），client 画亮蓝线，加断言锁死。
> 注：闪电是 `needs_point` 技能，按 D 后需左键确认目标才施放。

### 2.3 F 技能（蓄力自爆）核对 ✅
对照 `TestSkill03.cs`/`SelfExplodeScript.cs`：伤害/击退逻辑与数值正确，非缺失。用户"没击退"多为半径小/施法者自残导致体验差。
> 低优先待核：Rust 自残固定扣到 `self_stay=1`，Unity 是 `GetHurt(min(10,hp-1))`（hp>11 只扣 10）。

### 2.5 不满员倒计时跨端同步 + 可取消（`860e975`）✅ 已实现（待真机复验）
现象：host 按回车启动不满员手动倒计时只在 host 本地倒数，client 看不到（显示"已就绪：等其他人就绪"），两端不同步。

- **协议**：`Packet::RosterReady` 增加 `manual_ms: u16` 字段（host 手动倒计时剩余毫秒，0=未激活），随 host 每帧广播的 RosterReady 心跳**原子下发**（避免独立小包在 Steam P2P 下丢包导致 client 看不到倒计时）。encode 在 entries 后追加 2 字节 BE；decode 尾部不足 2 字节回退 0（兼容极短/旧包）。
- **host**：每帧广播前按 `steam_manual_countdown` 计算 `manual_ms`（`steam_countdown*1000` 向上取整）。
- **client**：`recv_room_inbox`/`recv_roster_ready` 返回带 `manual_ms`；持久记录到 `steam_manual_ms`（仅收新快照时更新，避免没收包的帧闪烁）。界面新增 `steam_manual_ms>0` 分支显示同一倒计时（"房主已确认，X 秒后进配置"），进入 LOCK 秒显示"即将开始（不可取消）"。
- **取消与锁定**：client 在手动倒计时期间按 U → toggle `local_ready=false` → 上行 RoomState → host 收到后 `!underfull_ready` 撤销（已有逻辑，无需改）。锁定窗口：client 端公共与分支的 `locked` 都加 `manual_ms>0 && manual_ms<=LOCK` 判定，最后 2 秒内不可按 U 取消，与 host 端锁定一致（防止最后两秒取消导致两端不同步）。
- **清理**：`reset_to_main_menu`/`enter_steam_mode` 重置 `steam_manual_ms=0`。
> 测试：net 31 全绿（新增 manual_ms 随 RosterReady 同步/复位断言）；workspace 146 全绿；clippy 默认+steam 全绿。

### 2.6 自爆(F)自残数值对齐 Unity（`fab3ba2`）✅
现象：Rust 自爆固定把施法者扣到 1 血；Unity `SelfExplodeScript` 是 `GetHurt(min(10,hp-1))`——**最多自扣 10 血、保底留 1 血**（满血 100 自爆后剩 90，而非 1）。
- 修复：`world.rs` 自残从 `p.hp = self_stay` 改为 `dmg = min(hp - self_stay, 10); hp -= dmg`（`self_stay=1` 保底；上限 10 与 Unity 硬编码一致）。敌人伤害/击退逻辑不变。
- 测试：`f_self_explode_hurts_enemies_and_self` 断言改为“满血自爆剩 max_hp-10”；新增 `f_self_explode_low_hp_floor_is_self_stay`（hp=5 自爆保底留 1）。
> 只改 game-core（世界层确定性），不影响协议；workspace 现 **147 全绿**（5+102+31+9）。

### 2.4 之前几轮已解决（历史）
1. **右键丢指令 / 施法迟滞**（`b441d9e` + `a7b7886`）：根因都是"只发一次被 host 帧同步输入缓存覆盖"。修复：移动目标、施法都改**电平量持续重发**，由 `note_self_cast` 在自己角色 `is_busy` false→true 边沿清掉。
2. **统一首局/局间配置学习阶段**（`d651708`）：首局也进 `MatchPhase::Learning`，倒计时归零→配置同步→统一开战（去按 P）；局间（含 Steam）也补配置同步。单机首局保留手动+超时兜底。
3. **下一局走向上一局移动目标**（`9fb61bc`）：配置同步前清 `player_target` + `pending_cast`。
4. **火球撞柱、不满员倒计时**（`6556bc6` / `84c6f1a`）。

## 3. 待办（下次优先）

| 优先级 | 事项 | 说明 |
|---|---|---|
| ~~**高**~~ | ~~不满员倒计时跨端同步 + 可取消~~ | ✅ **已实现（`860e975`，§2.5）**，待真机双账号复验：host 按回车 → client 显示同一倒计时、按 U 可取消、最后 2 秒锁定。 |
| ~~**高**~~ | ~~自爆自残数值对齐（§2.3）~~ | ✅ **已实现（`fab3ba2`，§2.6）**：最多自扣 10 血、保底留 1 血，对齐 Unity `GetHurt(min(10,hp-1))`。 |
| ~~**中**~~ | ~~客户端诊断日志精简~~ | ✅ **已实现（`e7acb59`）**：删除每帧刷屏的 `CLIENT SEND`，保留 `HOST COLLECT`/`bind`/`teardown` 等每局一次日志。 |
| **高·待拍板** | **技能定价（购买升级技能）** | 现在买技能不花金币，经济闭环缺失。**需用户先拍板定价模型**（按树解锁 / 按技能 / 仅升级收费 / 组合）。**等 warlock brawl 地图数值到手后按其复刻（§5）。** 建议下一步先做。 |
| **高·规划中** | **回放功能（方案A·确定性对局回放）** | Dota2/War3 式回放：host 记 seed+玩家配置+每帧输入，回放用确定性世界重建；存储小、可暂停/快进/倒带。方案已定（§5），待定范围/触发/入口。 |
| 低 | LAN 首局无配置阶段 | 局域网是备胎，但合并后首局没进 Learning（LAN host/join 没调 `begin_first_round_config`） |
| 低 | `draw_pre_game` 清理 | 已 `#[allow(dead_code)]` 保留；首局已改走 Learning 界面，需把"准备状态面板"并入 `draw_meta_overlay` 的 Learning 臂后删除旧函数 |
| 低 | Steam 配置学习阶段保活 | 倒计时/配置期间 Steam 不专门心跳（局间原本也如此），双机测是否断开 |

## 4. 关键实现备注（下次改前必读）

- **施法/移动语义**：`local_player_input()`（`main.rs`）里 `player_target` 和 `pending_cast` 都是**电平量**（读而不清）；由 `note_self_cast()` 在自己角色 `is_busy` false→true 边沿时清 `player_target` + `pending_cast`。不要改回 `take()`（会丢指令）。
- **`self_index()` 与 `mem::take`**：`self_index()` 依赖 `net_link`/`steam_my_index`。配置同步（`Fighting` 分支）会把 `net_link` `mem::take` 临时置 None，此时靠缓存的 `lan_my_index`（LAN）或 `steam_my_index`（Steam）拿序号，不能回落到 `PLAYER_ID=0`。
- **首局/局间统一**：配置学习统一走 `MatchPhase::Learning`；`Learning` 臂处理：单机首局手动 / 其余倒计时；归零后联机设 `net_cfg`、单机 `teardown_round_end`。`pre_game_config` 字段仍保留，用于 `Fighting` 分支的 `stage_first`（首局重建 world）。
- **host 配置同步竞态防护**：host 进 HostGather 首帧 `drain_cfg()` + `reset_cfgs()`（`host_cfg_drained` 只跑一次），再收本轮；`HOST_CFG_SETTLE_TICKS=15` 稳定等待兜底。改配置同步逻辑务必保留这两道防护，否则局间绑定会被上一局在途旧包覆盖。
- **meta 层**：`begin_first_round_config` / `finish_first_round_config` / `is_first_config` / `tick_learning`（首局归零走 `enter_first_round` 不 +round 不发奖）。
- **世界层跨局残留**：`world.reset_round()` 已清 `projectiles`、`players.reset_state()` 清 `move_target`/buffs/control/dash（有测试 `reset_round_clears_projectiles_and_move_targets`）。**跨局火球不残留**（若再遇到，查是否 client 侧又上行旧输入，而非世界层）。
- **闪电可视化**：`world.lightning_visual`（瞬态，`step` 开头递减归零清空，不参与序列化）供 client 画线。若加其他即时技能特效可仿照。

---

## 5. 项目背景与大方向（下次会话必读）

### 5.1 项目本质
本作是**复刻魔兽3地图《Warlock Brawl》**（作者本人在做）。目前所有技能/伤害/成长数值对照的是** Unity 版原素材**（`TestSkill*.cs` / `Sender.cs` / `SelfExplodeScript.cs` 等）。**待作者找到 warlock brawl 地图文件后，按地图内实际数值逐一复刻**（可覆盖/校准现有 Unity 对照数值）。→ 找地图 = 推进技能/数值复刻的关键前提。

### 5.2 两大方向（已讨论，互不依赖，优先级待最终敲定）
1. **技能定价（购买升级技能）**：买技能现在不花金币，经济闭环缺失。需拍板定价模型后实施。改动在 meta 层（已有 `spend_gold`/`upgrade_cost`/`upgrade_skill_spends_gold_and_fails_when_poor` 基础）。**建议先做**（补核心玩法闭环）。
2. **回放功能（方案A·确定性对局回放）**：Dota2/War3 式。**建议放技能定价后做**；若近期需多人真机调手感/查联机 bug，可提前（回放也是 lockstep 调试/回归利器）。

### 5.3 回放方案要点（下次实施可参考）
- 架构基础：`game-core` 世界完全确定性——`World::new(player_count, seed)`（每轮 `round_seed` 递增）；host 每帧广播 `Frame { seq, entries:[(玩家, 输入字节)] }`，client 解码喂 `world.step(inputs, dt)`；`World: Clone` 已具备、`world_ser` 有序列化。
- **录制端=host（权威）**：局头记 `(round_seed, player_count, 各玩家配置)`，之后每帧记 `Frame.entries`，局结束写 `.rec` 文件。
- **回放端**：读 `.rec` → `World::new(seed)` → 逐帧喂记录的输入 → 复用现有渲染管线；倒带=重放到第 N 帧（确定性，天然支持）。
- **待定设计点**：① 范围（仅对局内战斗复盘 vs 含配置阶段）；② 触发方式（建房勾选自动录像 / 局内按键）；③ 回放入口（主菜单加"观战/回放"浏览 `.rec` 列表）。
