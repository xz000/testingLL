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
| 地图形状 `to` | `arena_shape` | 0 | ❌ **装饰**（仅支持圆形） |
| 冰面 `-ice` | `ice_mode`（0/1/2） | 1 | ❌ **装饰**；且 098c 是**开关**，我们是三档 |
| 收缩 `wo` | `shrink_delay_secs`(10) + `shrink_ring_secs`(10) | 10/10 | ⚠️ 生效（world 直接读 `match_cfg`），但 098c 只有 1 个旋钮 |
| 回血 `In` | `base_regen` | 0.5 | ✅ 生效 |
| `-no reward` | `gold_rewards_enabled` | true | ❌ **装饰**：从不读取；且语义比 098c 宽（098c 只清 `Mo/po/lo`） |
| — | `place_rewards` | 空 | ❌ **非 098c**（要删） |
| 模式/轮数/队伍 | `game_mode`/`total_rounds`/`team_count`/`win_score` | 1/3/1/10 | ✅ 生效 |

---

## 2. 严重问题（重点）

### S1（最严重）开局重建 `meta` 时丢弃大部分设置
`main.rs:stage_world_for_participants`（`1938`，在 `4970/5128` 被首局调用）用
`MatchState::new(self.match_config(), ...)` 重建 meta；而 `match_config()`（`1919`）**只拷 8 个字段**
（`total_rounds/learn_time_secs/gold_per_round/starting_gold/place_rewards/game_mode/base_regen/team_count`），
其余 `..Default::default()`。后果：Steam 房主在 `O` 编辑器里改的
**击杀/助攻/胜利/伤害最高金币、三个得分项**在**开局瞬间被丢回默认**（房间面板/大厅串里还对，但落地的对局不用）。
> 注：`publish_room_cfg` 里 `self.meta.config = self.match_cfg.clone()`（`6496/5969`）会被这个重建覆盖。

### S2 若干设置为“装饰”，gameplay 从不读取
`damage_mult`、`knockback_mult`、`lava_damage_mult`、`pillar_mode`、`ice_mode`、`arena_shape`、
`gold_rewards_enabled`、`first_round_time_secs` 在 `game-core` 里**只有序列化/默认值/测试引用**，
没有任何对局逻辑读取。UI 给出这些行会误导玩家。

### S3 字段冗余 / 非 098c 项
- 时长 4 个字段表达 2 个概念（`shopping_time_secs`/`learn_time_secs` 与 `first_round_time_secs`/`between_rounds_time_secs`）。
- `place_rewards` 是移植发明的（098c 无名次金）。
- 收缩 2 个旋钮 vs 098c 1 个。

### S4 参与奖“时点”（上轮已确认，待改）
098c：首轮商店只有 `Qo=20`；`qo` 在**回合结束** `aI` 才发（`22904`）。
我们 `grant_opening_gold` 在**首轮配置**就发 `初始金+首轮参与奖`=30 → 始终比 098c 多 10。

---

## 3. 待办清单

> 原则：每项独立可编译/提交；能加单测的加单测；改动设置串 schema 的项要 bump `ROOM_SETTINGS_SCHEMA`。

### 阶段 A — 确定性 bug（低风险，先做）
- [ ] **A1** 开局别丢设置：让首局直接用完整 `match_cfg`（不再用子集 `match_config()` 重建 meta）；
      或把 `match_config()` 改为 `self.match_cfg.clone()`（客户端则用从大厅串还原的 `match_cfg`）。
      涉及 `main.rs:1938/1919/4970/5128/6895`。测试：加一条“编辑器改击杀金→开局后 `meta.config.gold_per_kill` 生效”。
- [ ] **A2** `gold_rewards_enabled` 接进经济（或删行）：按 098c `-no reward` 语义，在 `finish_round`/`register_kill`/`register_assists`
      里把 `Mo/po/lo` 视为 0（点数不受影响），或按我们更宽的“全部金币归零”实现并明确。
- [ ] **A3** 参与奖时点（S4）：`grant_opening_gold` 只发 `starting_gold`；`gold_per_round` 保留在 `finish_round`。
      同步 `meta.rs` 4~5 个测试期望值。

### 阶段 B — 去冗余 / 去非 098c（schema bump 一次做）
- [ ] **B1** 删除 `place_rewards`：字段 + `to_meta_string`/`from_meta_string` 槽位 + `finish_round` 分支
      (`meta.rs:735`) + 客户端 `match_place_rewards`/`host_set_place_reward`/大厅键 + `auto_place_rewards`(+测试) + 相关 meta 测试。
- [ ] **B2** 时长字段合并为 098c 的两个：保留 `first_round_time_secs`(Uo)/`between_rounds_time_secs`(uo)，
      删 `shopping_time_secs`/`learn_time_secs`；`begin_first_round_config` 用前者、`finish_round` 用后者。
      测试：`meta.rs` 首轮时长用例改指 `first_round_time_secs`。
- [ ] **B3** `ROOM_SETTINGS_SCHEMA` +1，更新 `to/from_meta_string` 与 `room_settings_meta_string_roundtrip` 断言。
- [ ] **B4** UI 提示/文案同步（`settings_ui::hint`）删除已不存在项、修正时长行名。

### 阶段 C — 把 098c 有、但我们没接的旋钮真正接进对局
- [ ] **C1** `damage_mult`（设置 2）→ 所有伤害结算乘上它（找 `damage_player`/`warlock_ki_impact` 入口）。
- [ ] **C2** `knockback_mult`（设置 3）→ 击退初速乘上它（`push_knockback`/KI 公式）。
- [ ] **C3** `lava_damage_mult`（设置 1）→ 出界/岩浆伤害乘上它（`world.step` 出界分支）。
- [ ] **C4** `first_round_time_secs` → 由 B2 后自动生效。
- [ ] **C5** `pillar_mode`（设置 8）：0 随机/1 每局必有/2? —— 接进 `_layout_obstacles`（当前只看 seed）。
- [ ] **C6** `arena_shape`（设置 7）：至少 `0=圆`；其余形状未实现则从 UI 撤下或标注“未实现”。
- [ ] **C7** 冰面：098c 是 `-ice` 开关；考虑把 `ice_mode` 收敛为开关（决策点 D2）。

### 阶段 D — 决策点（需你拍板）
- [ ] **D1** 收缩：合并为 098c 的单旋钮 `wo`（延迟=wo×√存活），还是保留我们的两旋钮？
- [ ] **D2** 冰面：3 档 (关闭/随机/每局必有) vs 098c 的开关？柱子同理（098c `Po` 0 随机）。
- [ ] **D3** S2 那些“装饰”项：接进 gameplay（阶段 C）还是先从 UI 撤下，避免继续误导？
- [ ] **D4** 设置串 schema 是否需要兼容旧串（若 bump，旧客户端不能加入）？

---

## 4. 记录
- 2026-09-13：初版审计（未改代码）。含 S1~S4 与阶段 A~D 清单。
