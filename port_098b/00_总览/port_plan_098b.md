# 移植计划：用 ggez 复刻 Warlock 0.98b（2D）

> 主参照：`mechanics_098b.md`（098b 机制总览）+ `port_spec_098b.json`（28 技能机器可读规格）。
> 配套深扒：`abilities_consolidated_098b.md` / `abilities_control_098b.md` / `abilities_durations_098b.md` / `abilities_calibration_098b.md`。
> 差异参照：`compare_ce_098b.md` / `compare_ce_098b_objects.md`（与 CE 1.10B 对齐，仅用于"哪些要改"，不是移植依据）。

---

## 0. 结论先行（先回答最关键的两个问题）

**Q1：用 ggez 做 2D 复刻是否合适？**
合适。Warlock 的所有机制都发生在 X-Y 平面：**没有 Z 轴**——
- 投射物、AoE、冲撞、锁链、闪电、岩浆、柱子、复活全用 2D 圆/线段/矩形表达。
- 英雄只有"位置 + 朝向 + 状态(buff/debuff)"，不需要骨骼动画或 3D 碰撞。
- 所有"角色/火球/陨石"用 2D 形状（圆/多边形/线段）代表即可，零 3D 建模成本。

**Q2：伤害计算该按 30 帧还是 60 帧？**
**两者都不直接绑定帧率**。正确做法是：
- **逻辑层固定步长 `TICK = 0.03s`（≈33.3Hz）**，与渲染帧率彻底解耦。
- 渲染用 **60fps（ggez 默认 winit 事件循环）**，逻辑步进通过 **accumulator + 插值** 同步到渲染。
- 所有"速度"以 **单位/秒** 存储，每 tick 位移 = `速度 × TICK`。这样 30 或 60 渲染帧下逻辑完全一致（确定性）。
- 理由：098b 本体就是 0.03s 逻辑步（源码 71 处 `速度*0.03`）。我们不是"选 30/60"，而是**直接复刻它的 33.3Hz**，再用插值让画面在 60fps 下平滑。

> 简言之：**逻辑 33.3Hz 固定步，渲染 60fps 插值**。把 `TICK` 作为全工程唯一时间常量。

---

## 1. 目标与范围

### 1.1 目标
在 Rust + ggez 0.9.3 中，复刻 098b 的**对战核心手感**：
- 多英雄（默认 2–10 人，由 `wn=9` 上下可调）在同一张含柱子/岩浆的场地内互投法术。
- 28 个技能（S000–S031 + 升级形态 S032–S036）的数值与机制对齐 `port_spec_098b.json`。
- 击杀 → 掉落 → 复活（`to=10s` 基础）循环。
- 开局购物（`Wo=40s` 开商店）、技能升级、模式选择（En1–5）。

### 1.2 明确不在本期范围（避免范围蔓延）
- **联机 / 网络同步**：ggez 自身无网络层。本期只做**单机 / 本地热座 / AI 对手**。架构上把"输入"抽象成 `PlayerInput` 接口，为将来接 `renet` 等留口，**但不实现 netcode**。
- **美术资产**：全部用程序化 2D 图形（圆、线、矩形、文字标签）。不引入图片/音频文件（除非后续要音效，用 `rodio` 可选）。
- **地图编辑器 / 自定义模式**：先用 098b 默认场地（固定柱子布局 + 岩浆带）。
- **完整的 CE 差异回灌**：只在数值明显影响手感时参考 `compare_ce_*.md`，不双向同步。

---

## 2. 技术栈与运行模型

| 项 | 选择 | 说明 |
|----|------|------|
| 语言 | Rust (edition 2021) | ggez 原生 |
| 框架 | ggez 0.9.3 | winit + wgpu + rodio 后端，事件循环即渲染循环 |
| 时间模型 | 固定步长 `TICK=0.03` + accumulator | 见 §3 |
| 插值 | `alpha = acc / TICK` | 渲染位置 = `lerp(prev, curr, alpha)` |
| 碰撞 | 圆-圆（英雄/投射物/AoE）、点-线段（闪电） | 全 2D，无空间哈希也够（实体数 <~200） |
| 实体存储 | `Vec<Entity>` + 类型枚举，**不引 ECS** | 实体规模小，ECS 是过度设计；系统函数直接遍历 |
| 输入 | 抽象 `PlayerInput`（键位/AI/将来网络） | 解耦输入来源 |
| 随机数 | 确定性 PRNG（如 `rand_chacha` 固定种子） | 复活抖动 `10±0.5s` 等需可复现 |

### 2.1 为什么不用 ECS
实体类型固定（英雄、投射物、AoE 场、闪电链、岩浆格、柱子），总数小（同屏 <200），且行为高度耦合于"命中即时结算"。用 `Vec` + 每系统遍历比 ECS 心智负担低、调试直观。若将来要插件化技能，再考虑。

---

## 3. 核心循环：固定步长 + 插值

```
const TICK: f32 = 0.03;
const MAX_FRAME: f32 = 0.25; // 防螺旋death，单帧最多补 8 tick

fn update(&mut self, ctx) {
    let dt = ctx.time.delta().as_secs_f32();
    self.acc += dt.min(MAX_FRAME);
    while self.acc >= TICK {
        self.step(TICK);          // 纯逻辑：推进所有实体一 tick
        self.acc -= TICK;
    }
    let alpha = self.acc / TICK;  // 用于渲染插值
    self.render(ctx, alpha);
}

fn step(&mut self, dt: f32) {     // dt 永远是 0.03
    for hero  in &mut heroes   { hero.update(dt); }
    for proj  in &mut projectiles { proj.integrate(dt); check_hit(); }
    for aoe   in &mut aoes     { aoe.tick(dt); }
    for field in &mut fields   { field.damage(dt); }  // DoT 用 DPS*dt
    spawn_queue.drain(...);
    despawn_dead();
}
```

**关键不变量**：`step()` 内所有位移/伤害只依赖 `dt`（= TICK），绝不读 `ctx.time.delta()`。这样 30/60/144Hz 显示器下逻辑逐位一致。

---

## 4. 实体模型

```rust
struct Vec2 { x: f32, y: f32 }              // 单位 = "war3 距离单位"；英雄移速 210 = 单位/秒

enum Entity {
    Hero(Hero),
    Projectile(Projectile),   // 火球/陨石/弹跳弹/急行残影...
    AoeField(Aoe),            // 岩浆/火球点燃地带/引力漩涡/黑洞
    Beam(Beam),               // 闪电/锁链/喷火线（线段）
    Static(Static),           // 柱子（碰撞用，无状态）
}

struct Hero {
    pos: Vec2, prev_pos: Vec2,   // prev_pos 供插值
    vel: Vec2,                   // 单位/秒
    facing: f32,
    hp: f32, mana: f32,         // 098b 魔法上限 10000
    radius: f32,                 // 英雄碰撞半径（参照 Rv）
    buffs: Vec<Buff>,           // 反射盾/急行/定身/点燃...
    cd: [f32; SLOT_N],          // 各技能槽剩余 CD（秒）
    spell_levels: [u8; SLOT_N],
    alive: bool, respawn_t: f32,
}

struct Projectile {
    pos, prev_pos, vel: Vec2,    // vel = 单位/秒（来自 spec speed）
    radius: f32,                 // spec.radius（火球25/陨石.../弹跳35）
    life: f32,                   // 剩余存活秒（spec.life 解析为常量或公式）
    owner: PlayerId,
    impact: ImpactFn,            // 命中回调（见 §6）
    kind: ProjKind,              // Straight/Bounce/Dash/...
    data: ProjData,             // 等级 oi、伤害系数、弹跳衰减等
}
```

**从 spec 加载数值**：`port_spec_098b.json` 中每个技能的 `projectile.speed/radius/life/impact` 直接反序列化为 `Projectile` 初始值。`speed`（如 1000/900）已是**单位/秒**，无需再乘 0.03——`integrate(dt)` 内做 `pos += vel * dt`。

---

## 5. tick 无关性规则（移植正确性核心）

098b 的数值在 spec/object 里有两种表达，混用会错：

1. **速度**：object/spec 里的 `speed`（1000、900、1300…）是 **单位/秒**。
   → 每 tick：`pos += vel * TICK`。
2. **持续时间**：JASS 里 `kO(...,N*jn,...)`、buff `dur` 等已是**秒**（如 致残 `(4+0.25*级)*jn`）。
   → 直接当秒倒计时，`life -= dt`。
3. **伤害 `KI`/`jI`**：伤害基数本身是**每次命中**的值；`kI = 100*gX*JI*0.03` 里的 `*0.03` 是**几何换算常量**（把"速率×时间"空间化），**不是每 tick 结算**。
   → 命中即结算一次 `KI` 公式结果，不要乘 TICK。
4. **DoT（持续伤害：火球点燃 `xc()`、陨石点燃 `nB()`）**：JASS 里 settle 时机未完全确认（疑为每 tick 或每 interval）。
   → **移植策略：统一写成 `DPS × TICK` 每 tick 扣血**，与帧率无关且等价于"每秒 DPS"。spec 里 `dot_seconds` 给出持续秒数，`dot_dps = 总伤害 / dot_seconds`。

> 一句话：**凡是"速率"乘 TICK；凡是"一次性"不乘；凡是"每秒"乘 TICK。** 伤害公式 `KI` 的一次性结果直接施加。

---

## 6. 技能实现：数据驱动 + impact 回调

`port_spec_098b.json` 已把每个技能收敛成结构化字段。实现分两层：

**A. 通用投射物管线**（覆盖 80% 技能）
`eO`（分配ID）→ `VO`（生成圆形实体）→ `IO/bO`（赋速度）→ 每 tick 位移 → `hv[id]` 命中回调 → 命中后 `xO`（销毁/爆炸/AoE）。

对应代码：
```rust
fn cast_projectile(spec: &SpellSpec, caster: &Hero) -> Projectile {
    Projectile {
        vel: dir(caster.facing) * spec.projectile.speed,
        radius: spec.projectile.radius,
        life: eval(spec.projectile.life, caster),  // 公式求值
        impact: spec.projectile.impact,            // 关联 ImpactFn
        ..
    }
}
```
命中时按 `impact` 分派（映射表见 `abilities_control_098b.md` 附录 + `jass_deobf.md` 的 impact 绑定表）：
- `Xi`(火球) → 即时伤害 + 创建点燃 AoE 场（`xc`）。
- `Ji(Cc)`(弹跳弹) → 伤害×0.8 递归弹向下一个目标，直到最小值。
- `qI→PI→LI`（区域伤害链）→ AoE 内多目标按 `jI` 结算。

**B. 非投射物技能**（控制/位移/buff）走各自的 handler，逻辑见 `abilities_control_098b.md`：
- 冲撞/急行/瞬移：`dash` 用 `Hr=1300*0.03=39/tick`（存 1300 单位/秒即可），衰减 `jr=1.56`。
- 反射盾：给 owner 加 `reflect` buff，持续 `(2.6+0.2*vi)*jn`，期间来袭弹体反向。
- 引力/锁链/闪电：生成 `Beam`（线段）或 `AoeField`（漩涡 5*jn 秒）。
- 喷火/致残/灾变：即时 AoE + 定身 debuff。

---

## 7. 模块架构（建议文件划分）

```
src/
  main.rs            // ggez 入口：EventLoop + State
  engine/
    tick.rs          // TICK 常量、accumulator、step() 调度
    interp.rs        // 渲染插值（alpha）
    rng.rs           // 确定性随机（复活抖动等）
    math.rs          // Vec2、圆-圆、点-线段碰撞
  sim/
    world.rs         // 持有所有实体 + step()
    hero.rs          // 英雄更新、CD、buff、复活
    projectile.rs    // 投射物积分 + 命中分派
    aoe.rs           // AoE 场/DoT（DPS*TICK）
    beam.rs          // 闪电/锁链/喷火线段
    field.rs         // 岩浆/柱子静态地形
  spell/
    spec.rs          // 反序列化 port_spec_098b.json
    registry.rs      // code(S000..) -> 施法函数 + impact 表
    cast.rs          // 通用投射物施法管线
    control.rs       // 控制/位移/buff handler
  game/
    modes.rs         // En1–5 模式、ed() 默认值、-C 17 项调参
    shop.rs          // 开局购物 Wo=40、技能升级
    spawn.rs         // 击杀掉落、复活 to=10±0.5
    input.rs         // PlayerInput 抽象（键盘/AI/将来网络）
  ui/
    hud.rs           // 血/蓝/CD/等级、热键槽 G/F/D/E/R/T/Y/C/P
    render.rs        // 用 ggez draw 画圆/线/文字
data/
  port_spec_098b.json   // 从 ce_old98b/build_port_spec.py 生成
```

**输入抽象（为联机留口）**：
```rust
trait PlayerInput { fn poll(&self, world: &World) -> Intent; }
// KeyboardInput / AIInput / (将来) NetInput 都实现它
// step() 只消费 Intent，不碰具体设备
```

---

## 8. 分阶段里程碑

**M1 — 引擎骨架 + tick 无关性验证**
- ggez 窗口、固定步长循环、插值渲染一个移动的圆（英雄）。
- 验证：不同渲染帧率下，圆在固定时间内位移完全一致（写个断言/回放测试）。
- 交付：`engine/`、`sim/world.rs`、`hero.rs` 最小版。

**M2 — 碰撞 + 投射物管线（数据驱动）**
- 反序列化 `port_spec_098b.json`；实现 `cast_projectile` + 命中分派。
- 先用 S000 火球 跑通：生成→飞行→命中→伤害→点燃 AoE。
- 静态柱子碰撞（英雄/投射物撞柱反弹或阻挡）。
- 交付：火球/陨石/弹跳弹 3 个走通用管线的技能可玩。

**M3 — 全技能 + 控制/位移**
- 按 `abilities_*.md` 逐个实现 28 技能（投射物类复用 M2；控制/位移/ buff 写 `control.rs`）。
- 升级形态 S032–S036（读 `abilities_tooltips_098b.md` 的形态名与分支）。
- 交付：全部技能数值对齐 spec 的本地对战。

**M4 — 游戏循环 / 模式 / 商店 / 复活**
- `ed()` 默认常量、`-C#` 17 项调参、`En1–5` 模式。
- 开局购物 `Wo=40`、技能升级、击杀掉落、复活 `to=10±0.5`。
- 交付：一局完整对战（多英雄、AI 或热座）。

**M5 — 场地 / 岩浆 / 抛光**
- 岩浆带（持续伤害场）、柱子布局（对齐 098b 场地）。
- HUD（血/蓝/CD/热键槽）、胜负判定、基础音效（可选 rodio）。
- 交付：可玩 demo。

---

## 9. 验证方法

1. **tick 确定性测试**：固定输入序列 + 固定种子，跑 N tick，断言最终状态哈希一致（30/60/144 渲染帧下皆同）。
2. **数值对账**：每个技能实现后，对照 `port_spec_098b.json` 的 `speed/radius/life/cooldown_s/aoe_radius_obj` 与 `abilities_durations_098b.md` 的持续时间，写单测断言关键值。
3. **机制对账**：对照 `abilities_consolidated_098b.md` / `abilities_control_098b.md` 的"真实机制"描述，人工/录制验证行为（如反射盾反弹、弹跳弹 -20%/跳、冲撞定身 0.5s）。
4. **手感旁证**：`Hr=1300*0.03=39` 与 CE 的 `ThrustVel` 一致 → 冲撞/冲刺手感已与经典版对齐，可作为"没跑偏"的信号。

---

## 10. 风险与未知项

| 项 | 状态 | 处理 |
|----|------|------|
| DoT settle 时机（火球 `xc`/陨石 `nB`） | **未确认** | 统一按 `DPS×TICK`，等价"每秒总伤害"，与帧率无关 |
| `do/Do/fo/go/ar/nr/la` 等常量语义 | 未逐一核实 | 仅影响次要手感，先占位常量，M3 逐技能核对 |
| 岩浆/柱子精确布局 | 未在 spec 中 | 从 `mechanics_098b.md` 场地描述重建，M5 微调 |
| 联机 | ggez 无网络 | 本期不做；`input.rs` 抽象已留口，将来接 `renet` |
| 升级形态 S032–S036 的 JASS 细节 | 仅 tooltip 名 | 形态名已知，机制需回查 `jass_deobf.md` impact 表 |
| 复活 `10±0.5s` 抖动分布 | 未确认分布 | 用确定性 PRNG 在 [9.5,10.5] 取，M4 再校准 |
| `jn[player]` 缩放 | 默认 1，可由 -C 改 | 所有持续时间/buff 乘 `jn`，存档到 hero 上 |

---

## 11. 数据来源索引（改数值时回哪查）

- **施法/投射物/伤害数值** → `port_spec_098b.json`（机器可读，优先）
- **技能真实机制说明** → `abilities_consolidated_098b.md`、`abilities_control_098b.md`
- **持续时间公式** → `abilities_durations_098b.md`
- **tooltip 文案/形态名** → `abilities_tooltips_098b.md`
- **tooltip vs 真实校准** → `abilities_calibration_098b.md`（CD≈L1，反射盾≈L2）
- **全局模式/启动常量/引擎常量** → `mechanics_098b.md`（§引擎常量表、`ed()`）
- **与 CE 差异（仅参考）** → `compare_ce_098b.md`、`compare_ce_098b_objects.md`

> 再生成/刷新 spec：`python3 ce_old98b/build_port_spec.py`（输出 `port_spec_098b.json`）。
