# AoE 视觉范围 vs 实际作用范围（核实 2026-09-13）

> 起因：天罚的视觉效果与实际作用范围差距偏大。
> 结论：根因是**爆炸特效环**（表现层 P3-3）从 `0.5×` 涨到 `2×` 半径，未按真实半径绘制；
> 已改为**精确半径**（`FxKind::Blast`：淡填充圆 + 亮描边，不随进度扩散）。
> 本文存档全部「有范围的 AoE」的实际半径与视觉对照，供后续回归。

---

## 0. 修复

| 项 | 之前 | 现在 |
|---|---|---|
| `CombatEvent::Explode` 视觉 | `Ring`：`radius*(0.5 → 2.0)`，最大 2× 真实半径 | `Blast`：恒为 `radius`（淡填充 + 2.5px 亮环），淡出 |
| 位置 | `client/src/fx.rs` `FxKind::Ring` | 新增 `FxKind::Blast`；`client/src/main.rs` 消费 `Explode` |

`Ring` 仍用于死亡/柱子尘（纯装饰，不代表范围），不受影响。

---

## 1. 实际作用范围（模拟侧真值）

| 效果 | 触发/来源 | **实际半径**（世界单位） | 代码 |
|---|---|---|---|
| **S001 天罚** | `W098bNova{Smiting}` | **250** | `world.rs` nova 分支 `explode_at(ppos, radius=effect.radius)` |
| S020 灾变 | `W098bNova{Catastrophe}` | 阶段 0/1 = **300**，阶段 2 = **400** | 同上（覆盖 effect.radius=300） |
| S021 虔诚 | `W098bNova{Devotion}` | 伤敌 **250**；**回血队友 500** | 伤敌 `explode_at(250)`；回血单独 500（`500²` 判定） |
| 陨石 | `ProjectileKind::DelayedBlast` | `radius`（定义 `radius_base=200`，随等级 `radius_delta`） | `explode_at(.., radius, .., DmgFalloff::Mul)` |
| 火球/名册弹爆炸 | `W098b{blast: Some(..)}`（如 200） | `blast` | 到期/命中 `explode_at(.., br, ..)` |
| 引力场 | `ProjectileKind::Gravity` | 投射物 `radius` | 场伤半径 = `radius` |
| 星域 | `ProjectileKind::Star` | 投射物 `radius` | 场伤半径 = `radius` |
| 点燃场（火球命中） | `ProjectileKind::Star`（`ignite`） | **75**（`spec aoe_radius_obj`） | 命中处生成 Star(75) |
| **冲锋/燃烧接触 AoE（098c `bA`/`SI`）** | `resolve_player_collisions`（S010A 冲锋 / S012A 燃烧） | **`160 × (1 + 0.12 × 范围精通)`** | `splash_damage(.., SI_RADIUS_BASE=160 …)` |

---

## 2. 视觉载体与一致性

| 效果 | 视觉 | 一致性 |
|---|---|---|
| S001/S020 nova | `Explode` → `Blast` 精确环 | ✅ 修复后精确 |
| S021 伤敌 250 | `Explode` 精确环 | ✅ |
| S021 **回血 500** | `HealPulse` → 绿色 `Blast` 精确环 | ✅（新增）|
| 陨石 | 落点环（`DelayedBlast.radius` 精确）+ 收缩内圈 | ✅ 落点环 = 实际 |
| 火球爆炸 | `Explode` 精确环 | ✅ |
| 点目标瞄准圈（客户端） | `skill_aim_hint`：`blast` / nova `radius` | ✅ 与实际爆炸同源同值 |
| 引力场/星域/点燃场 | 直接画投射物 `radius` | ✅ |
| **冲锋/燃烧接触 AoE** | `Splash` → 橙红 `Blast` 精确环（仅命中时）| ✅（新增）|

> 注：`skill_aim_hint` 取的是 **SkillEffect 的固定 radius**（非 `stats_at(level).radius`）；
> 模拟侧 nova 也用同一个固定 `radius`，二者一致，但**等级不改变 AoE 半径**（如设计需要成长请同步两处）。

---

## 3. 已补齐（本次）

1. **S021 虔诚 500 回血圈**：`CombatEvent::HealPulse{pos,radius}`（仅实际回血时发）→ 绿色 `Blast`。
2. **冲锋/燃烧接触 AoE**：`splash_damage` 改为返回命中数；`resolve_player_collisions` 在命中时发
   `CombatEvent::Splash{pos,radius}` → 橙红 `Blast`。单测：`splash_damage_reports_hit_count`。

## 3b. 仍可优化（可选）

1. 若未来 AoE 半径随等级成长：统一改由 `stats.radius` 驱动，并让客户端预览同步。

---

## 4. 回归入口

- 表现：`client/src/fx.rs`（`FxKind::Blast` 恒半径）、`client/src/main.rs`（`CombatEvent::Explode/PillarBreak`）。
- 真值：`game-core/src/world.rs`（`explode_at` 各调用点、`splash_damage`）、`game-core/src/skill.rs`（`W098bNova{radius}`、`Blast`）。
- 单测：`explode_multi_hit_emits_hattrick_or_vampire` 断言 `Explode` 事件存在。
