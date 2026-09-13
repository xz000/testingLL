# 表现层规划（更新 2026-09-13）

> 前提（PORT_098B_DECISIONS.md D13 / D14）：098c 大量表现是 **war3 引擎局限下的变通**；我们只复刻**信息与手感**，
> 用原生载体呈现。原则：**表现层全部是客户端本地效果，不进快照、不影响确定性**（不改 `PROTOCOL_VERSION`）。
> 关联：`AUDIO_PLAN.md` / `AUDIO_SCRIPT.md`（音效）· `UI_AUDIT.md` · `JASS_AUDIT_098c.md`。
> 代码位置：`client/src/main.rs`（绘制 `draw_scene` / HUD）、`client/src/ui.rs`（theme/text/HitRegistry）。

---

## 0. 进度总览

| 批次 | 内容 | 状态 |
|---|---|---|
| **P1** 飘字 + 事件横幅 | 伤害/治疗飘字；首杀/击杀/连杀/多重击杀/Ludicrous/Hattrick/Vampire/Silencer/Pancake/Burnout/Denied/LastSecondSave 横幅 | ✅ 已完成 |
| **P2** 音效 | 全部 098c 映射 cue + 战斗/UI/流程/商店（占位素材） | ✅ 已完成（素材待替换） |
| **P3** 命中/死亡/施法视觉 | 本文 §3 | ⬜ 待做 |
| **P4** HUD 增强 | 本文 §4 | ⬜ 待做 |
| **P5** 相机/预警 | 本文 §5 | ⬜ 待做 |
| **P6** 观战/回放 | 远期 | ⬜ 未启动 |

> **事件来源**：已有 `World::combat_events`（纯表现事件通道，不进快照/`state_hash`，每 `step` 清空）
> 与客户端上帧状态差分（`update_presentation`）。P3/P4 继续用这两条来源，**不新增同步字段**。

---

## 1. 目标与纪律（不变）

1. **信息可读**：伤害/治疗/状态/关键事件在 2D 俯视下"看得见"。
2. **反馈及时**：命中/死亡/施法/就绪有即时视觉与音效。
3. **确定性隔离**：表现层只做「只读 `World` → 生成客户端效果」；**禁止**把表现状态写进 `World`/快照。
   如需核心信号，走 `World::combat_events`（非序列化、非哈希、每 tick 清空）。
4. **不照搬 war3**：用原生图形/音效，不引入漂字多板/贴图法阵等引擎变通。
5. 表现层改动**不 bump 协议**；触碰快照/模拟才 bump。

---

## 2. 基础设施（P3 前置，一次性）

新增 `client/src/fx.rs`：**纯客户端特效系统**（不依赖 ggez 之外的库）。
```rust
pub enum FxKind { HitFlash, Ring, Spark, Debris, CastRing, ReadyPulse, ... }
pub struct Fx { pub kind: FxKind, pub pos: Vec2, pub life: f32, pub max_life: f32, pub color: Color, pub radius: f32 }
pub struct FxSystem { fx: Vec<Fx> }         // Game 上一个字段
impl FxSystem {
    pub fn spawn(&mut self, fx: Fx);         // 带上限（超出丢最旧）
    pub fn update(&mut self, dt: f32);       // 递减 life、清死亡
    // pub fn draw(&self, canvas, ctx, world_to_screen)   // 在世界层绘制（实体之后、HUD 之前）
}
```
- 位置用**世界坐标**，绘制时经现有 `world_to_screen` 换算（与飘字/玩家一致）。
- 上限（如 256）防刷屏；纯函数 `Fx::alpha()/scale()` 可单测。
- 事件源：
  - **伤害**：复用 `update_presentation` 的 hp 差分（`health_delta_text`）→ 受击者 `HitFlash` + 命中点 `Spark`。
  - **死亡**：`was_alive && !alive` → `Ring`（淡出）。
  - **柱子破碎 / 爆炸**：给 `World::combat_events` 增 `PillarBreak{pos}` / `Explode{pos,radius}`（非序列化），客户端消费。
- **绘制顺序**：场地 → 冰面/区域 → 柱子 → 玩家 → 弹体 → **特效** → 飘字/横幅 → HUD。

---

## 3. P3 · 命中 / 死亡 / 施法视觉（按价值排序）

### P3-1 命中闪光 + 弹体火花（小，先做）
- 受击的玩家：圆环闪白/红（`HitFlash`，0.15s 淡出；自伤红、他伤黄白，与飘字配色一致）。
- 弹体命中点：几枚小火花（`Spark`，0.2s，沿命中法线散射）。
- 数据：纯客户端 hp 差分（已有 `present_prev_hp`）；火花位置=受击者位置+半径。

### P3-2 死亡效果（小）
- 阵亡瞬间：`Ring`（扩散圆环，0.6s 淡出）+ 短暂残影（把该玩家上一帧位置画一个淡色圆，0.4s）。
- 数据：`was_alive && !alive`（已有）。淘汰后不再跟随（`alive=false`）。

### P3-3 柱子破碎 / 爆炸（中）
- 柱子 HP 归零被移除时：在柱心生成 `Debris`（数枚小方块向外飞散 + 尘圈，0.5–0.8s）。
- AoE 爆炸（新星/陨石/弹体爆炸）：`Explode` 事件 → 扩散圆环 + 边缘白闪。
- 数据：给 `combat_events` 增 `PillarBreak{pos}` 与 `Explode{pos,radius}`（`explode_at`/柱子移除处 push）。
  纯客户端消费；不进快照。

### P3-4 施法前摇指示（中）
- 现有：瞄准线 / AoE 圆（点目标技能）。补充：
  - **前摇进度**：施法者在读条时脚下画**收缩圆**（剩余 windup 比例）；对**敌方**施法也显示（可读性）。
  - **落点脉冲**：预判落点（点目标）画脉动圆，随距离/延迟明暗变化（可选）。
- 数据：客户端读 `world.players[i].caster`（前摇/后摇状态与剩余时间），只读。

---

## 4. P4 · HUD 增强

### P4-1 技能就绪脉冲（小，价值高）
- 冷却从 `>0` 跨到 `0` 的那一帧：该技能槽**高亮闪现**（`ReadyPulse`，0.4s 金边/白闪），并可选播 `ui_ready`。
- 数据：客户端缓存「上一帧各槽冷却是否就绪」，比对边沿（`world.players[me]` 的技能冷却/skill_levels）。

### P4-2 施法条 / 前摇进度（小）
- 现为圆环；补一条**施法条**（底条 + 填充 = 前摇完成度），放在 HUD 技能栏上方；后摇用不同色。
- 数据：读 `caster` 状态（只读）。

### P4-3 状态图标行（中，可读性最高）
- 把玩家身上的 buff 显性化为一排**小图标**（放在血条旁）：
  `Tied`(束缚) / `Silenced`(沉默) / `Scorched`(灼烧) / `Slow`(减速) / `Weakened`(削弱) /
  `LavaShield`(岩浆护盾) / `Aegis`(守护之盾) / `Stealth`(隐身) / `Pancake`(肉饼) / `Boost`(疾跑) / `Mirror`(镜像)…
- 图标用 `ui::theme` 上色 + 单字/符号（不引外部贴图）；剩余时间用一圈扇形/淡出表示（可选）。
- 数据：只读 `world.players[*].buffs`（这是世界状态，读没问题）。
- **单测**：纯函数 `active_status_icons(&Player) -> Vec<(icon_id, color)>`（不依赖 ggez）。

### P4-4 目标/自机强调（小）
- 自机：脚下细环 + 队伍色描边（已有头像/队伍色，补自机环）。
- 当前选中/待施法目标：目标点/目标玩家画十字或菱形标记。

---

## 5. P5 · 相机与可读性（后续）

- **跟随自身**（可选开关）：相机默认自由，提供"跟随我"快捷（如 `Home`）。
- **出界/缩圈预警**：场地边缘描边变红 + 下一环倒计时；出界者屏幕边泛红（与 `combat_lava`/`flow_oob_warn` 配套）。
- **目标点标记**：右键移动目标点画一个小旗/十字（跟随 `move_target`）。

---

## 6. 分步实施（每步可编译/提交/过门禁）

- [x] **P3-0 基础设施**：`client/src/fx.rs`（`FxKind`/`Fx`/`FxSystem` + 上限 + `alpha/progress` 单测）；
      `Game` 增 `fx: FxSystem`；绘制接入（实体后、HUD 前）。
- [x] **P3-1**：命中闪光 + 火花（hp 差分驱动）。
- [x] **P3-2**：死亡圆环 + 残影。
- [x] **P3-3a**：`combat_events` 增 `PillarBreak`/`Explode`（core，非序列化）+ 单测。
- [x] **P3-3b**：柱子碎裂粒子 / 爆炸圆环（客户端消费）。
- [ ] **P3-4**：施法前摇收缩圆 + 落点脉冲。
- [x] **P4-1**：技能就绪脉冲（边沿检测 `ready_pulse_edge` + 单测）。
- [ ] **P4-2**：施法条。
- [x] **P4-3**：状态图标行（纯函数 `active_status_icons` + 单测 + 绘制）。
- [ ] **P4-4**：自机环 / 目标标记。
- [ ] **P5**：出界/缩圈预警 + 移动目标标记（+ 可选跟随）。

---

## 7. 测试与门禁

- 纯函数单测：`Fx::alpha/scale`、`active_status_icons`、就绪边沿检测（`skill_ready_edges`）、
  `combat_events` 新变体的产生（game-core 侧）。
- 每个子步：`cargo build --workspace` + `cargo test --workspace` + `clippy -D warnings`（默认与 steam）。
- **性能**：特效数量设上限；避免每帧分配大 Vec；`FxSystem::update` O(n)。

## 8. 风险

- **刷屏**：伤害/灼烧每帧触发 → 闪光/火花**必须限流**（沿用飘字的 `PRESENTATION_MIN_DELTA` 与 cooldown）。
- **可读性过载**：特效应克制（低透明度、短时），避免遮挡实体；提供"特效强度"本地设置（未来并入设置界面）。
- **与确定性无关**：任何特效/图标只读世界 + 客户端字段，**不得**写入 `World`/快照。
