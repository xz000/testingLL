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
- （收缩的 2 个旋钮是**有意设计**，不计入。）

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
- [ ] **B2** 时长字段合并为 098c 的两个（见 §1b）：保留 `first_round_time_secs`(Uo)/`between_rounds_time_secs`(uo)，
      删 `shopping_time_secs`/`learn_time_secs`；`meta::begin_first_round_config` 改读前者、`meta::finish_round` 改读后者。
      同步：client `FASTROUND`(`main.rs:914-915`)、`match_config()`/`match_learn_secs` 链路、`STEAM_DEFAULT_LEARN_SECS` 护栏测试。
      测试：`meta.rs` 首轮/局间时长用例改指新字段（`1464/1465/1588/1589`）。
- [ ] **B2b** （可选）把 `match_learn_secs`/`host_set_learn`/大厅键 `learn` 重命名为 between-rounds，消除“learn=局间”歧义。
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
- [ ] **D2** 冰面：3 档 (关闭/随机/每局必有) vs 098c 的开关？柱子同理（098c `Po` 0 随机）。
- [ ] **D3** S2 那些“装饰”项：接进 gameplay（阶段 C）还是先从 UI 撤下，避免继续误导？
- [ ] **D4** 设置串 schema 是否需要兼容旧串（若 bump，旧客户端不能加入）？

> 收缩（原 D1）已裁定：**有意不同，不调整**。

---

## 5. 建议动手顺序

目标：先修“设置根本不生效”的确定性 bug，再做去冗余，最后接线 098c 旋钮。

1. **S1（阶段 A1）— 最高优先级**：让首局直接用完整 `match_cfg`，
   `stage_world_for_participants` 改传 `self.match_cfg.clone()`（host）／客户端从大厅串还原的 `match_cfg`。
   ⚠️ **注意**：`match_config()` 里有一个隐含派生——`team_count = if mode==4 {2} else {match_teams}`（国王模式自动两队）；
   改为直接用 `match_cfg` 后，需保证这个派生不丢（在 game_mode 变更时同步 `match_cfg.team_count`，或保留该派生）。
   测试：编辑器改击杀金/得分 → 开局后 `meta.config` 保留。
2. **S4 + A2**：参与奖时点（`grant_opening_gold` 只发初始金）+ `-no reward` 语义。小改 meta，改几个单测。
3. **B2 → B1**：先合并时长字段（B2），再删名次金（B1）；`ROOM_SETTINGS_SCHEMA` 每步 bump。
4. **C1~C3**：把伤害/击退/岩浆倍率接进结算（这类“装饰项”最能被玩家察觉）。
5. **C5~C7 + D2~D4**：柱子/冰面/地图形状与剩余决策点。

> 收缩不在列表中（有意不同）。

---

## 6. 记录
- 2026-09-13：初版审计（未改代码）。含 S1~S4 与阶段 A~D 清单。
- 2026-09-13：补充 §1b「时长字段专项」——确认 4 个时长字段实为 2 个概念的重复副本，
  列为“两边都不需要的重复项”；细化 B2（合并字段）与 B2b（重命名 learn→between-rounds）。
- 2026-09-13：收缩**移出对齐范围**：098c 的单 `wo` 是 War3 限制的产物，我们用连续收缩两旋钮，
  **有意不同、不调整**；删除原决策点 D1。
