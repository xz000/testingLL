# 开局/房间设置与 098c 对齐审计（2026-09-13）

> 目标：**尽量与 098c 对齐**。本文只做总结 + 待办清单，**未改代码**。
> 关联：`ROOM_UI_REVIEW.md`（房间 UI/回车）、`GAMEPLAY_FIX_PLAN.md`、`game-core/src/meta.rs`、`client/src/settings_ui.rs`。
> 真相源：`../098c/out/war3map_pretty.j`。

---

## 0. 098c 的权威设置清单

**`-constants 1`（设置 1..9，`war3map_pretty.j:18760-18786`）**

| # | 名称 | 全局变量 | 默认 | 说明 |
|---|---|---|---|---|
| 1 | Lava Damage | `To[0]`（显示 `*10`） | .9/s | 出界/岩浆伤害 |
| 2 | Damage Multiplier | `Gn[0]` | 1 | 全局伤害倍率 |
| 3 | Knockback Multiplier | `Hn[0]` | 1 | 全局击退倍率 |
| 4 | Shop Time | `uo` | 30 | **局间**商店时长 |
| 5 | Shop Time initial | `Uo` | 40 | **首轮**商店时长 |
| 6 | Shrink Time per player | `wo` | 10 | 收缩节奏（延迟 = wo×√存活） |
| 7 | Arena (0 is random) | `to` | 0 | 地图形状 |
| 8 | Pillar (0 is random) | `Po` | 0 | 柱子 |
| 9 | HP Regeneration | `In[0]` | 0.5/s | 基础回血 |

**`-constants 2`（设置 10..17，`18799-18813`）**

| # | 名称 | 全局 | 默认 |
|---|---|---|---|
| 10 | Kill Point Reward | `ko` | 1 |
| 11 | Assist Point Reward | `Ko` | 1 |
| 12 | Kill Gold Reward | `lo` | 1 |
| 13 | Assist Gold Reward | `Lo` | 1 |
| 14 | Win Point Reward | `mo` | 2 |
| 15 | Win Gold Reward | `Mo` | 2 |
| 16 | Damage Gold Reward | `po` | 1 |
| 17 | Gold per round | `qo` | 10 |

**命令**：`-gold <Qo>`（初始金，`18382`）、`-ice`（切换 `Ka` 开关，`18391`）、
`-no reward`（把 `Mo`/`po`/`lo` 置 0，`18609`）、`-league`、`-cam`、`-duel` 等。

**关键：098c 没有「名次奖励」（第 2/3 名…发金）。**

---

## 0.5 原则：设置改动 = core + UI + 设置串 + 测试，**四处同步**

**配置改动必须连着 UI 一起改**（用户已强调）。每个设置项牵涉四处：
1. **core 逻辑**：`game-core/src/meta.rs`（字段/结算）、`game-core/src/world.rs`；
2. **UI 行**：`client/src/settings_ui.rs` 的 `rows()`/`label()`/`hint()`/`value()`/`nudge()`/`enum_tiers()`/`num_range()`/`int_range()`；
3. **设置串**：`MatchConfig::to_meta_string`/`from_meta_string`（必要时 bump `ROOM_SETTINGS_SCHEMA`）；
4. **测试**：core 单测 + client 护栏/源码扫描。

> 只改 1、不改 2/3 → UI 会显示“无效”或“多余”的行（正是本次审计发现的毛病）；
> 反过来，**UI 只能暴露我们真正支持的项**（例：地图形状）。

---

## 1. 字段对照表（098c ↔ 我们）

| 098c 旋钮 | 我们的字段 | 默认 | 当前状态 |
|---|---|---|---|
| 初始金 `Qo` | `starting_gold` | 20 | ✅ 生效（默认已在 `bd8ae5a` 修正） |
| 每轮金 `qo` | `gold_per_round` | 10 | ✅ 生效 |
| 击杀/助攻/胜利/伤害金 `lo/Lo/Mo/po` | `gold_per_kill/assist/round_win/most_damage` | 1/1/2/1 | ⚠️ meta 会读，但**开局被丢弃**（见 S1） |
| 得分 `ko/Ko/mo` | `score_per_kill/assist/round_win` | 1/1/2 | ⚠️ 同上，**开局被丢弃** |
| 首轮时长 `Uo` | `first_round_time_secs`(40) + `shopping_time_secs`(40) | 40 | ⚠️ **冗余**：UI 改 `first_round_time_secs`，但 gameplay 读的是 `shopping_time_secs`（不可编辑、不被搬运）→ UI 行**无效** |
| 局间时长 `uo` | `between_rounds_time_secs`(30) + `learn_time_secs`(30) | 30 | ⚠️ **冗余**：靠 `between_rounds`→`match_learn_secs`→`learn_time_secs` 间接生效；`learn_time_secs` 不可编辑 |
| 伤害倍率 `Gn` | `damage_mult` | 1 | ❌ **装饰**：gameplay **从不读取** |
| 击退倍率 `Hn` | `knockback_mult` | 1 | ❌ **装饰** |
| 岩浆伤害 `To[0]` | `lava_damage_mult` | 1 | ❌ **装饰** |
| 柱子 `Po` | `pillar_mode` | 1 | ❌ **装饰**：世界生成只看 seed，不读该项 |
| 地图形状 `to` | `arena_shape` | 0 | ⚪ **不接线，但 UI 保留**：我们**只有圆形**（短期不做其他形状）；`ArenaShape` 行**保留但置灰**（只显示“圆形”、不可改），作为远期形状扩展的占位 |
| 冰面 `-ice` | `ice_mode`（0/1/2） | 1 | ❌ **装饰**；且 098c 是**开关**，我们是三档 |
| 收缩 `wo` | `shrink_delay_secs`(10) + `shrink_ring_secs`(10) | 10/10 | ✅ **有意不同**：098c 的单 `wo` 是 War3 限制下的做法；我们用延迟+每环两个旋钮的连续收缩，**不按 098c 调整** |
| 回血 `In` | `base_regen` | 0.5 | ✅ 生效 |
| `-no reward` | `gold_rewards_enabled` | true | ❌ **装饰**：从不读取；且语义比 098c 宽（098c 只清 `Mo/po/lo`） |
| — | `place_rewards` | 空 | ❌ **非 098c**（要删） |
| 模式/轮数/队伍 | `game_mode`/`total_rounds`/`team_count`/`win_score` | 1/3/1/10 | ✅ 生效 |

---

## 1b. 时长字段专项（深挖：每个字段到底谁在读）

全仓库扫描后的结论：**4 个字段其实是 2 个概念的重复副本**，而且“UI 改的那个”与“gameplay 读的那个”各在一半。

### 概念 A：首轮商店时长（098c `Uo=40`）

| 字段 | 谁读 | 默认 | 结论 |
|---|---|---|---|
| `shopping_time_secs` | ✅ gameplay：`meta.rs:958 begin_first_round_config`；client `FASTROUND` 覆写 (`main.rs:915`) | 40 | **实际生效的那个** |
| `first_round_time_secs` | ❌ 只被 UI/序列化/测试引用（`settings_ui.rs:309/343`、serde 槽 4） | 40 | **UI 改的那个，但没人读 → 无效** |

→ 二者是同一个概念的副本。**保留一个即可**。建议保留 `first_round_time_secs`（名字/UI/098c 对齐），让 meta 改读它，删 `shopping_time_secs`。

### 概念 B：局间商店时长（098c `uo=30`）

| 字段 | 谁读 | 默认 | 结论 |
|---|---|---|---|
| `learn_time_secs` | ✅ gameplay：`meta.rs:780 finish_round`（回合后 Learning 倒计时）；client `match_config()` (`1923`) / `FASTROUND` (`914`) | 30 | **实际生效的那个** |
| `between_rounds_time_secs` | ⚠️ 不直接读；经 client `publish_room_cfg`→`match_learn_secs`(`6486/6550`)→`match_config().learn_time_secs`→`learn_time_secs` 间接生效 | 30 | **UI 改的那个，绕了一大圈** |

→ 同一概念。建议保留 `between_rounds_time_secs`（UI/098c 对齐），让 meta 直接读它，删 `learn_time_secs`
（并顺手把 `match_learn_secs`/`host_set_learn`/大厅键 `learn` 改名为 between-rounds）。

### 概念 C：场地收缩 —— **有意不同，不调整**

098c 只有 `wo` 一个旋钮（延迟 = wo×√存活）。这是 **War3 引擎限制下的实现**；我们采用「收缩延迟 + 每环时长」
两个旋钮的**连续收缩**模型，**不向 098c 看齐**（用户已裁定）。

- `shrink_delay_secs`（延迟，`world.rs:2920/3032`）
- `shrink_ring_secs`（每环时长，`world.rs:978`）

→ **保留两个旋钮，本审计不涉及收缩。**

### 运行时字段（保留，不算重复）
- `learn_remaining`（`meta.rs:665/682/907`）：当前配置期剩余秒数，HUD `main.rs:3763` 显示。
- `pending_first_round`：区分“首局配置”与“局间配置”。

### 小结：哪些“两边都不需要/重复”
- `shopping_time_secs` 与 `first_round_time_secs`：**同一概念，删一个**。
- `learn_time_secs` 与 `between_rounds_time_secs`：**同一概念，删一个**。
（收缩的 2 个旋钮是**有意设计**，不算重复。）

> 设置串里这 4 个时长占了 4 个槽位（index 2..5）；合并后应为 2 个 → 影响 `ROOM_SETTINGS_SCHEMA`（见 B3）。

---

## 2. 严重问题（重点）

### S1（最严重）开局重建 `meta` 时丢弃大部分设置  — ✅ **已修**
`main.rs:stage_world_for_participants`（`1938`，在 `4970/5128` 被首局调用）用
`MatchState::new(self.match_config(), ...)` 重建 meta；而 `match_config()`（`1919`）**只拷 8 个字段**
（`total_rounds/learn_time_secs/gold_per_round/starting_gold/place_rewards/game_mode/base_regen/team_count`），
其余 `..Default::default()`。后果：Steam 房主在 `O` 编辑器里改的
**击杀/助攻/胜利/伤害最高金币、三个得分项**在**开局瞬间被丢回默认**（房间面板/大厅串里还对，但落地的对局不用）。
> 注：`publish_room_cfg` 里 `self.meta.config = self.match_cfg.clone()`（`6496/5969`）会被这个重建覆盖。
>
> **修复（已提交）**：`match_config()` 改为直接返回 `authored_match_cfg(&self.match_cfg, self.match_teams)`
> —— 完整沿用房间设置，仅保留“国王模式强制两队”派生；两个 stage 调用点（`stage_world_for_participants`、`finish_enter_steam_mode`）
> 自动拿到完整配置。回归测试 `authored_match_cfg_keeps_room_settings_and_king_teams`。

### S2 若干设置为“装饰”，gameplay 从不读取
`damage_mult`、`knockback_mult`、`lava_damage_mult`、`pillar_mode`、`ice_mode`、
`gold_rewards_enabled`、`first_round_time_secs` 在 `game-core` 里**只有序列化/默认值/测试引用**，
没有任何对局逻辑读取。UI 给出这些行会误导玩家。
（例外：`arena_shape` 我们**短期不实现其他形状**，不接线，但**保留 UI 行并置灰**（只显示“圆形”），
作为远期形状扩展的占位。）

### S3 字段冗余 / 非 098c 项
- 时长 4 个字段表达 2 个概念（`shopping_time_secs`/`learn_time_secs` 与 `first_round_time_secs`/`between_rounds_time_secs`）。
- `place_rewards` 是移植发明的（098c 无名次金）。
- （收缩的 2 个旋钮是**有意设计**，不计入。）

### S4 参与奖“时点”（已修）
098c：首轮商店只有 `Qo=20`；`qo` 在**回合结束** `aI` 才发（`22904`）。
旧实现 `grant_opening_gold` 在**首轮配置**就发 `初始金+首轮参与奖`=30 → 始终比 098c 多 10。
**已修**：`grant_opening_gold` 只 `give_starting_gold()`；`gold_per_round` 仍由 `finish_round` 发。

---

## 3. 待办清单（每项 = core + UI + 设置串 + 测试，四处同步）

> 原则（见 §0.5）：每个设置项都要同时动 **core 逻辑 / UI 行 / 设置串 / 测试**；
> 每项独立可编译提交；改动设置串的项 bump `ROOM_SETTINGS_SCHEMA`。
> 下面每项明写 `[core]`/`[UI]`/`[schema]`/`[test]` 四处要做什么。

### 阶段 A — 确定性 bug（低风险，先做）
- [x] **A1（S1）✅ 已完成：开局别丢设置**
  - `[client]` `match_config()` → `authored_match_cfg(&self.match_cfg, self.match_teams)`；完整沿用房间设置，保留国王两队派生。
  - `[UI]` 无 —— 但**此后 UI 里那些行真正生效**。
  - `[test]` `authored_match_cfg_keeps_room_settings_and_king_teams`（含国王→两队）。
- [x] **A2 参与奖时点（S4）✅ 已完成**
  - `[core]` `grant_opening_gold` 只发 `starting_gold`；`gold_per_round` 留在 `finish_round`。
  - `[UI]` 无。
  - `[test]` 同步 `meta.rs` 多处期望值（首商店 20 而非 30）；`round_start_gives_participation_gold` 改名 `opening_grants_starting_gold_only`。
- [x] **A3 `gold_rewards_enabled` ✅ 已完成**
  - `[core]` 按 098c `-no reward`：`kill_gold()/win_gold()/damage_gold()` 关闭时归零（`lo/Mo/po`）；点数、助攻金、每轮金不变。
  - `[UI]` `settings_ui::hint(GoldRewardsEnabled)` 文案改为精确语义。
  - `[test]` `no_reward_disables_kill_win_damage_gold_only`。

### 阶段 B — 去冗余 / 去非 098c（含 UI 撤行 + schema bump）
- [ ] **B1 删 `place_rewards`**
  - `[core]` 字段 + `to/from_meta_string` 槽 + `finish_round` 分支。
  - `[UI]` 本就没有行（段 3 已删）；确认 `hint` 无残留。
  - `[client]` `match_place_rewards`/`host_set_place_reward`/大厅键、`auto_place_rewards`(+测试)。
  - `[schema]` bump。
- [x] **B2 时长字段合并 ✅ 已完成**
  - `[core]` 删 `shopping_time_secs`/`learn_time_secs`；`begin_first_round_config` 读 `first_round_time_secs`、`finish_round` 读 `between_rounds_time_secs`。
  - `[UI]` 保留 `FirstRoundSecs`/`BetweenRoundsSecs` 两行（名字已对）。
  - `[client]` 删 `init_learn_secs`；`match_learn_secs` 初值用 `STEAM_DEFAULT_LEARN_SECS`；`FASTROUND` 改用新字段名。
  - `[schema]` 设置串去 2 槽（learn/shopping），`ROOM_SETTINGS_SCHEMA` **1 → 2**；不兼容旧串（已确认）。
  - `[test]` 更新往返/默认值断言（`wrong_schema` 改用当前 schema 构造）。
- [ ] **B2b（可选）** `match_learn_secs`/`host_set_learn`/大厅键 `learn` → between-rounds 命名。
- [ ] **B3** `ROOM_SETTINGS_SCHEMA`：B2 已 1→2；B1 删名次金时再 2→3（每步都能解析当前历史串）。
- [ ] **B4** 全量 `settings_ui` 文案/hint 复查（删已不存在项、修正时长行名）。

### 阶段 C — 把 098c 有、但我们没接的旋钮接进对局（每项都要动 UI）
- [ ] **C1 `damage_mult`（设置 2）**：`[core]` 伤害结算乘它；`[UI]` 保留行；`[test]` 倍率。
- [ ] **C2 `knockback_mult`（设置 3）**：`[core]` 击退初速乘它；`[UI]` 保留行。
- [ ] **C3 `lava_damage_mult`（设置 1）**：`[core]` 出界伤害乘它；`[UI]` 保留行。
- [ ] **C4** `first_round_time_secs` 由 B2 自动生效。
- [ ] **C5 `pillar_mode`（设置 8）**：`[core]` `_layout_obstacles` 读取；`[UI]` 行保留（档位对齐 098c `Po`）。
- [ ] **C6 `arena_shape` —— 不接线，UI 保留但置灰**
  - 我们**只有圆形**、短期不新增形状 → 不接 098c 的 `to`（这是**远期计划**）。
  - `[UI]` `ArenaShape` 行**保留**，但置灰/锁定（只显示“圆形”、不可改）——可复用在 `is_readonly()` 上或新增一个“置灰/锁定”概念。
  - `[core]` 字段保留（默认 0）；不为非 098c 的形状做预留。
- [ ] **C7 冰面**：098c 是 `-ice` 开关，我们三档 → 见 D2。

### 阶段 D — 决策点（需你拍板）
- [ ] **D2** 冰面粒度（开关 vs 三档）；柱子 `Po`（0 随机）对齐。
- [ ] **D3** 其余“装饰”项：C 阶段接线；若某项也不打算实现（如 `arena_shape`）→ 从 UI 撤下。
- [ ] **D4** schema bump 是否需要兼容旧串（若 bump，旧客户端不能加入）。

> 收缩（原 D1）已裁定：**有意不同，不调整**。

---

## 5. 建议动手顺序

目标：先修“设置根本不生效”的确定性 bug，再做去冗余，最后接线 098c 旋钮。
**每步都同时改 core + UI + 设置串 + 测试**（见 §0.5）。

1. **S1（阶段 A1）✅ 已完成**：`match_config()` 直接用完整 `match_cfg`（`authored_match_cfg`），
   国王模式强制两队派生保留。（core 无改；UI 无改，但此后 UI 各行真正生效。）
2. **S4 + A3 ✅ 已完成**：参与奖时点（只发初始金）+ `-no reward` 语义（只清击杀/胜利/伤害金）。
3. **B2 → B1**（含 UI 文案 + schema bump）：B2 ✅ 已完成（时长合并，schema→2）；B1（删名次金，schema→3）待做。
4. **C1~C3**：伤害/击退/岩浆倍率接进结算（core + UI 保留行）。
5. **C5~C7 + D2~D4**：柱子/冰面 + 地图形状（`arena_shape` 行保留但**置灰**只显示“圆形”，不接线；远期再扩）。

> 收缩不在列表中（有意不同）。

---

## 6. 记录
- 2026-09-13：初版审计（未改代码）。含 S1~S4 与阶段 A~D 清单。
- 2026-09-13：补充 §1b「时长字段专项」——确认 4 个时长字段实为 2 个概念的重复副本，
  列为“两边都不需要的重复项”；细化 B2（合并字段）与 B2b（重命名 learn→between-rounds）。
- 2026-09-13：收缩**移出对齐范围**：098c 的单 `wo` 是 War3 限制的产物，我们用连续收缩两旋钮，
  **有意不同、不调整**；删除原决策点 D1。
- 2026-09-13：重整为“**配置 + UI 同步改**”组织（§0.5 四处同步原则）；每项清单明写 core/UI/schema/test；
  `arena_shape` 定为**不接线、从 UI 撤下**（我们只有圆形，短期不新增形状）。
- 2026-09-13：修正上条：`arena_shape` 行**保留但置灰**（只显示“圆形”、不可改），
  作为远期形状扩展的占位（用户：远期计划，先置灰/单一选项即可）。
- 2026-09-13：**A1/S1 已完成**（`match_config()` 改用完整 `match_cfg` + 国王两队派生；新增回归测试）。
- 2026-09-13：**A2/S4 已完成**（开局只发初始金；`qo` 留在回合结束）。
- 2026-09-13：**A3 已完成**（`-no reward` = 关击杀/胜利/伤害金（lo/Mo/po），点数与助攻/每轮金不变）；
  game-core 235 + client+steam 48 测试绿。**阶段 A 全部完成**。
- 2026-09-13：**B2 已完成**（时长字段合并：删 `shopping_time_secs`/`learn_time_secs`，meta 直接读 `first_round_time_secs`/`between_rounds_time_secs`；
  `ROOM_SETTINGS_SCHEMA` 1→2；不兼容旧串）。B1（删 `place_rewards`）待做。
