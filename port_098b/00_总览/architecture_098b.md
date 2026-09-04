# 架构设计：ggez 复刻 Warlock 0.98b（2D，文档版）

> 本文档是**纯设计**（不出代码），配套 `port_plan_098b.md`（里程碑/验证）。
> 约束（来自用户 2026-09-04 决策）：
> - **只做文档规划，暂不写实现代码**。
> - **最终联机目标 = Steamworks**（Rust `steamworks` crate，P2P 中继）。**明确不做局域网**。
> - 因此架构把 sim 设计为**确定性 + 输入可序列化**，为将来 Steamworks 接入留口；但本期不实现任何网络传输。

---

## 1. 设计原则（架构地基）

1. **单一时间常量 `TICK = 0.03`**：所有逻辑步进以 `TICK` 为唯一时间单位，渲染帧率完全解耦。
2. **sim 层确定性**：`sim/` 内不调用 `SystemTime`、不读真实时钟、不碰任何 I/O。所有随机来自 `World` 持有的** seeded PRNG**（复活抖动 `10±0.5s` 等可复现）。这是为将来 Steamworks 联机（锁步/回滚）预埋的硬约束。
3. **数据驱动技能**：28 个技能的数值全部来自 `port_spec_098b.json`，代码里不硬编码 `speed/radius/cd`。
4. **无 ECS**：实体规模小（同屏 <~200），用 `Vec<Entity>` + 系统函数遍历，胜过 ECS 的心智负担。
5. **输入抽象 + 传输抽象（双层）**：`PlayerInput` 抽象"输入来源"（键盘/AI），`Transport` 抽象"输入如何到达"（本地直连 / 将来 Steamworks）。本期只实现"本地直连"，`Transport` 留 trait 不实现。
6. **渲染只读 sim 快照**：`render()` 不修改任何 sim 状态，只读取 + 用 `alpha` 插值。

---

## 2. 分层架构（依赖单向朝下）

```
┌──────────────────────────────────────────────────────────┐
│ main.rs  — ggez EventLoop + GameState（薄壳）              │
│   持有 World 快照；转发输入事件；每帧调 update→render        │
├──────────────────────────────────────────────────────────┤
│ L4  Presentation   ui/render.rs, ui/interp.rs, ui/hud.rs   │
│   ● 只读 World；alpha 插值绘制圆/线/文字；不写 sim          │
├──────────────────────────────────────────────────────────┤
│ L3  Orchestration game/modes.rs, game/shop.rs,            │
│                    game/spawn.rs, game/input.rs           │
│   ● 比赛流程：模式 En1–5、开局购物 Wo=40、击杀/复活 to=10   │
│   ● 把"输入事件"转成"Intent"塞进 World；触发胜负判定        │
├──────────────────────────────────────────────────────────┤
│ L2  Simulation     sim/world.rs, sim/hero.rs,             │
│   sim/projectile.rs, sim/aoe.rs, sim/beam.rs, sim/field.rs│
│   ● step(TICK)：积分→碰撞→结算→DoT→despawn（纯函数式推进） │
├──────────────────────────────────────────────────────────┤
│ L1  Spell system   spell/spec.rs, spell/registry.rs,     │
│   spell/cast.rs, spell/control.rs                         │
│   ● 反序列化 port_spec_098b.json；code→施法函数+impact 表   │
├──────────────────────────────────────────────────────────┤
│ L0  Foundation     engine/tick.rs, engine/math.rs,        │
│   engine/rng.rs, data/port_spec_098b.json                 │
│   ● TICK 常量、圆-圆/点-线段碰撞、确定性 PRNG、spec 加载    │
└──────────────────────────────────────────────────────────┘
```
依赖规则：**上层可调用下层，下层绝不反调上层**。`sim/` 不知道 `ui/`、`game/` 的存在；`game/` 通过 `World` 公共接口驱动 `sim/`。

---

## 3. 核心数据模型（设计示意，非编译代码）

```rust
// ---- 标量/几何 ----
const TICK: f32 = 0.03;                 // 唯一时间常量
const MAX_FRAME: f32 = 0.25;            // accumulator 上限，防螺旋 death
type Unit = f32;                        // 距离单位 = war3 单位；英雄移速 210 = 单位/秒

#[derive(Clone, Copy, Debug, Default)]
struct Vec2 { x: Unit, y: Unit }
impl Vec2 { fn lerp(a,b,alpha)->Self; fn len()->f32; fn dot()->f32; }

// ---- 实体 ----
enum Entity {
    Hero(Hero),
    Projectile(Projectile),   // 火球/陨石/弹跳弹/急行残影...
    Aoe(Aoe),                 // 岩浆/火球点燃/引力漩涡/黑洞（持续场）
    Beam(Beam),               // 闪电/锁链/喷火线（线段）
    Static(Static),           // 柱子（仅碰撞，无状态）
}

struct Hero {
    id: PlayerId,
    pos: Vec2, prev_pos: Vec2,   // prev_pos 供插值
    vel: Vec2,                   // 单位/秒
    facing: f32,
    hp: f32, max_hp: f32,
    mana: f32, max_mana: f32,    // 098b 魔法上限 = 10000
    radius: f32,                 // 碰撞半径（参照 Rv）
    buffs: Vec<Buff>,            // 反射盾/急行/定身/点燃...
    cd: [f32; SLOT_N],           // 8 槽剩余 CD（秒）
    lvl: [u8; SLOT_N],           // 各槽等级
    alive: bool, respawn_t: f32,
    rng: PrngState,              // 每英雄独立种子（确定性）
}

struct Projectile {
    id: u32, owner: PlayerId,
    pos: Vec2, prev_pos: Vec2, vel: Vec2,  // vel = 单位/秒（spec.speed）
    radius: f32,                           // spec.radius
    life: f32,                            // 剩余存活秒（spec.life 求值）
    kind: ProjKind,                       // Straight/Bounce/Dash/...
    impact: ImpactId,                     // 关联 registry 中的命中回调
    data: ProjData,                       // 等级 oi/vi、伤害系数、弹跳衰减...
}

struct Aoe {                       // 持续伤害/控制场
    pos: Vec2, radius: f32,
    life: f32,                     // 剩余秒
    dps: f32,                     // 每 tick 结算 dps*TICK（见 §6）
    kind: AoeKind,                // Lava/Ignite/Vortex/Blackhole/...
    owner: PlayerId,
}
struct Beam { a: Vec2, b: Vec2, life: f32, kind: BeamKind, owner: PlayerId }
struct Static { pos: Vec2, radius: f32 }   // 柱子
```

**Buff / Debuff**（统一建模，避免每种状态写一套）：
```rust
struct Buff {
    kind: BuffKind,        // Reflect/Haste/Stun/Ignite/Slow/...
    remain: f32,           // 剩余秒
    magnitude: f32,        // 反弹率/加速比/定身强度...
    src: PlayerId,
}
```

---

## 4. 单步数据流（step 管线）

`GameState.update()` 每帧：
```
dt = ctx.time.delta().min(MAX_FRAME)
acc += dt
while acc >= TICK {
    world.step(TICK)      // ← 唯一逻辑入口
    acc -= TICK
}
alpha = acc / TICK
ui::render(world, alpha)  // 只读 + 插值
```

`World::step(dt)`（dt 永远 = TICK）：
```
1. input.apply()        // 把本 tick 的 Intent 应用到 heroes（移动/施法/升级）
2. for hero  : hero.integrate(dt)   // pos += vel*dt；处理 buff（急行加速/定身清零）
3. for proj  : proj.integrate(dt)   // pos += vel*dt；life -= dt
4. collide()           // 圆-圆：proj×hero, proj×pillar, hero×pillar
                        //   点-线段：Beam×hero
5. resolve_hits()      // 命中 → 调 registry[impact](world, proj, target)
                        //   伤害/击退/反弹/链弹/定身...
6. for aoe  : aoe.tick(dt)          // DoT：target.hp -= dps*dt
7. for beam : beam.tick(dt)         // 持续型（喷火/锁链引导）
8. despawn()          // life<=0 / 死亡 的实体移除；英雄死亡→进入复活计时
9. spawn.resolve()    // 本 tick 新建的实体入表
```
**不变量**：步骤 1–9 全部只用 `dt`（=TICK）与 `World` 内部状态，绝不读真实时钟/I/O → 确定性。

---

## 5. 技能系统（数据驱动 + impact 注册表）

```
port_spec_098b.json
   │  (spell/spec.rs 反序列化)
   ▼
SpellSpec { code, name, cooldown_s{l1,lmax,levels}, max_level,
            cast_range, cast_time, aoe_radius_obj,
            projectile?{kind, speed, radius, life, impact},
            effect?{kind:buff/dash, dur, ...}, dot_seconds? }
   │
   ├─ spell/cast.rs  :: cast(world, caster, slot)
   │    读 spec → 生成 Projectile/Aoe/Buff/Beam，挂到 World
   │
   └─ spell/registry.rs  :: IMPACTS: Map<ImpactId, fn(&mut World, &Projectile, &Hit)>
        Xi(火球)   → 即时伤害 + 创建 Ignite AoE（xc）
        Ji(弹跳弹) → 伤害×0.8 递归弹向下一目标至最小值
        qI→PI→LI   → 区域伤害链（AoE 内多目标按 jI 结算）
        ... 其余见 abilities_control_098b.md 附录 + jass_deobf.md §七
```

- **施法入口**：`cast()` 根据 `spec.cast_range`/`cooldown_s`/`max_level` 与英雄 `lvl[slot]`、`cd[slot]` 决定能否放、生成什么。
- **升级形态 S032–S036**：用 `Fa/Ga/Ha/ga` 标志位选择 handler 分支（见 `mechanics_098b.md` 调度树），形态名取自 `abilities_tooltips_098b.md`。
- **impact 表是"机制真相"的唯一映射点**：所有"命中后发生什么"集中在此，便于逐技能对照 `abilities_*.md` 核对。

---

## 6. tick 无关性规则（移植正确性核心，重申）

| 数值类型 | 在 spec/object 中的表达 | 移植处理 |
|----------|------------------------|----------|
| 速度 `speed` | 单位/秒（1000/900/1300…） | `pos += vel * TICK` |
| 持续时间 | 已是秒（`kO(...,N*jn)`, buff `dur`） | `life -= TICK` 倒计时 |
| 伤害 `KI`/`jI` | **每次命中**一次性值 | 命中即结算一次结果，**不乘 TICK**（`*.03` 是几何换算常量） |
| DoT（点燃 `xc`/陨石 `nB`） | 持续秒数 `dot_seconds` | 统一 `dps = 总伤害/dot_seconds`，每 tick `hp -= dps*TICK` |

> 一句话：**速率乘 TICK；一次性不乘；每秒乘 TICK。** 这样 30/60/144Hz 下逻辑逐位一致。

---

## 7. 渲染与插值（L4）

- `GameState` 在 `step()` 前把每个实体的 `prev_pos = pos`；`step()` 只更新 `pos`。
- `render(world, alpha)`：`draw_pos = lerp(prev_pos, pos, alpha)`，`alpha = acc/TICK`。
- 一切绘制用 ggez 的 `Mesh`/`Rect`/`Line`/`Text`：英雄=填充圆+朝向线段，投射物=小圆，AoE=半透明圆，Beam=线段，柱子=实心圆，岩浆=色块。
- 本期**无图片/音频资产**；若将来要音效走 `rodio`（可选，不在本期）。

---

## 8. 输入抽象 + 传输抽象（为 Steamworks 留口，不做 LAN）

```rust
// L3 game/input.rs
trait PlayerInput {                       // 输入"来源"
    fn poll(&self, world: &World) -> Intent;
}
struct KeyboardInput { bindings: KeyMap }  // 本地玩家
struct AIInput { policy: AiPolicy }        // 电脑对手
// （将来）struct NetInput { buffer: Inbox } // 从 Transport 收来的远端 Intent

#[derive(Serialize, Deserialize, Clone)]   // 必须可序列化 → 将来走网络
struct Intent {                            // 一 tick 内的玩家意图
    move_dir: Vec2,                        // 归一化方向
    cast_slot: Option<u8>,                // 想放的技能槽
    aim: Vec2,                            // 瞄准点（朝向）
    buy: Option<TechId>,                  // 购物/升级意图
}

// 传输抽象（本期只实现 Local，trait 留口给 Steamworks）
trait Transport {                          // 输入"如何到达"
    fn send(&mut self, from: PlayerId, intent: &Intent);
    fn recv(&mut self) -> Vec<(PlayerId, Intent)>;
}
struct LocalTransport;                    // 本地直连：键盘/AI 直接进 World
// （将来）struct SteamTransport { client: steamworks::Client } // P2P 中继
```
- **本期**：`LocalTransport` + `KeyboardInput`/`AIInput`。sim 用 `Intent` 驱动，完全不感知来源。
- **将来接 Steamworks**：实现 `SteamTransport`（用 `steamworks` crate 的 `networking_messages` 发 `Intent`）；联机模型选**锁步**（最简单，因 sim 已固定步长+确定性）或**回滚**（手感更好但复杂）。决策推迟到真正做联机时，但**当前架构已满足其前提**（确定性 sim + 可序列化 Intent + 固定步长）。
- **不做局域网**：不引入任何 LAN 发现/广播逻辑；只认 Steamworks 一种传输。

---

## 9. 模块/文件清单（细化自 port_plan）

```
src/
  main.rs                 // ggez 入口；持有 World + Transport + Inputs
  engine/{tick,math,rng}.rs        // TICK、碰撞、确定性 PRNG
  sim/{world,hero,projectile,aoe,beam,field}.rs
  spell/{spec,registry,cast,control}.rs
  game/{modes,shop,spawn,input}.rs
  ui/{render,interp,hud}.rs
data/
  port_spec_098b.json     // 由 ce_old98b/build_port_spec.py 生成
```
（本期只产出设计；代码实现待用户授权后按 `port_plan_098b.md` 的 M1–M5 推进。）

---

## 10. 移植保真度核对清单（实现时逐条对）

- [ ] 所有位移用 `vel*TICK`；英雄移速 210、冲撞 `Hr=1300*0.03=39/tick`、衰减 `jr=1.56` 对齐 `mechanics_098b.md` 引擎常量表。
- [ ] 复活 `to=10±0.5s`、开局购物 `Wo=40`、模式 `En1–5`、`-C#` 17 项从 `ed()` 默认值读取。
- [ ] 28 技能数值（speed/radius/life/cd/aoe）逐条对账 `port_spec_098b.json`。
- [ ] 控制/位移技行为对账 `abilities_control_098b.md`（反射盾反弹、弹跳弹 -20%/跳、冲撞定身 0.5s…）。
- [ ] 持续时间对账 `abilities_durations_098b.md`；CD 校准对账 `abilities_calibration_098b.md`（tooltip≈L1，反射盾≈L2）。
- [ ] 手感旁证：`Hr` 与 CE `ThrustVel` 一致 → 冲刺手感已对齐经典版。
- [ ] 确定性：固定种子 + 固定输入 → 多渲染帧率下终态一致（回放测试）。

## 11. 风险/未知（沿用 port_plan §10）

DoT settle 时机（`xc`/`nB`）→ 统一 `DPS×TICK`；`do/Do/fo/go/ar/nr/la` 常量语义未核 → 先占位；岩浆/柱子精确布局 → 从 `mechanics_098b.md` 重建；Steamworks 联机模型（锁步 vs 回滚）推迟决策，但架构已满足前提。
