# 玩法问题修复计划（2026-09-13）

> 状态：**Bug A / Bug B 已修复**（提交见文末）。仍在的待办：联机抖动（见第 0 节，待讨论）。
> 关联：`FRAME_SYNC_ANALYSIS.md`（抖动）、`JASS_AUDIT_098c.md`、`PORT_098B_DECISIONS.md`。

---

## 0. 日志结论：抖动仍在，来源已明确（非快照）

更新后的两份日志（`client日志（部分）.txt` / `host日志（部分）.txt`）：

- **快照洗清**：`recv Snapshot` 每 **恰好 ~150 帧**（我们 `SNAPSHOT_BROADCAST_EVERY` 生效）、约 **3.1KB**；
  卡顿点与快照到达**不相关**。
- **host 帧间隔仍抖**：`[jit] host sim gap` 分布大致为 `33/34ms ×~260`、`49/50ms ×~100`、`66ms ×11`、`83ms ×5`、`100ms ×7`；
  并伴随 **72 次** `[steam-host] trying to emit but waiting for client input`。
- 即：**host 产帧被“输入到达”牵着走**（分析中的 B）。已回退的「每 update 只发一条输入」不是唯一原因；
  网络抖动本身仍会让 `try_emit` 偶尔等一帧 → 2~3 tick 的微顿。

**结论**：在不引入固定 D（用户已裁定）的前提下，剩下的平滑手段是 **渲染插值**（不增加输入延迟），
或接受这个量级的偶发微顿。见第四节（待规划）。

---

## 1. Bug A：被击退后原有移动目标被清除（应修）

### 现象
给出移动指令（右键）后角色朝目标走；中途被**击退**，击退结束后**不再继续**走向原目标。

### 已核实（世界层不背锅）
临时单测（已删除）验证：设置 `move_target` → `push_knockback` → 推进 180 帧，
`move_target` **从未变为 None**，玩家最终仍在朝目标移动。

也就是说：**世界/`step_velocity`/`push_knockback` 都不会因击退清掉 `move_target`。**

### 真正可疑处（客户端启发式）
`client/src/main.rs::note_self_cast()` 里的「到达清除」（引入于 `f1d369e`，改用于 `093f6a5`）：

```rust
if let Some(t) = self.player_target {
    if let Some(p) = self.world.players.get(me as usize) {
        if p.move_target.is_some() {
            self.player_target_accepted = true;
        } else if self.player_target_accepted || p.pos == t {   // ← 这里
            self.player_target = None;                            // 停止每帧重发
            self.player_target_accepted = false;
        }
    }
}
```
它把「世界 `move_target` 变 None」**一律**当成“到达”，于是停止重发 `player_target`。
但世界清 `move_target` 的原因不止“到达”：还有 `stop_move`、`tied()`（定身）、`caster.is_windup()`（施法）、
以及**某些位移/技能路径**。**它无法区分「到达」与「被位移」**。一旦误判，客户端就不再重发，角色永久停住。

> 注：当前世界实现里击退**不清** `move_target`，所以这条启发式理论上不该被击退触发；
> 但它是唯一会把「移动指令」永久清掉的客户端路径，且已被证实会在到达/定身/施法时清除。
> 若用户实测仍复现，很可能是某个**技能位移**路径让 `move_target` 短暂为 None。

### 修复方案
### 已实施（Bug A）
已改为纯函数 `Game::should_clear_player_target(accepted, in_control, dashing, pos, target)`：
- **强制位移/冲刺期间（`control.is_some()` / `dash_active`）绝不清**；
- 仅在“已接受过该目标 + 未位移 + 距目标 ≤ `PLAYER_TARGET_ARRIVE_EPS`(12 世界单位)”时清。
- 单测 `player_target_clear_requires_arrival_and_no_displacement`（含击退中/冲刺中不清这两个回归断言）。

（以下为原设计方案，保留供参考）
在客户端把「清除」条件收紧为**只在真正到达时**：

1. **强制位移期间不清**：`p.control.is_none() && !p.dash_active` 才允许清（直接排除击退/冲刺/冲锋）。
2. **到达需临近目标**：`dist(p.pos, t) <= ARRIVE_EPS`（`ARRIVE_EPS` 取“一帧最大位移”级别，
   如 `base_speed * TICK * 2`，兼顾冰面不吸附的滑行落点）。
3. 提取纯函数便于单测：
   ```rust
   fn should_clear_player_target(accepted: bool, world_has_target: bool,
                                 in_control: bool, dashing: bool,
                                 pos: Vec2, target: Vec2, eps: f64) -> bool
   ```
   测试覆盖：击退中（in_control）→ 不清；冲刺中 → 不清；远离目标且 target 消失 → 不清；
   临近目标且 target 消失 → 清。

### 风险 / 验证
- 低风险、纯客户端。唯一副作用：若某处确实依赖“目标被外部清掉即停”，会改成继续走向目标——正是 098c 期望。
- 双机实测：右键移动 → 被击退 → 应继续走向原目标。

---

## 2. Bug B：冲撞撞柱子的表现与 098c 不一致（已修复）

### 当前实现
`game-core/src/world.rs::resolve_obstacles`：把玩家推出障碍后，只要 `hit_wall && control.is_some()` 就
**`control = None`（强制位移立即截断、整体停下）**。对应测试 `s012_dash_truncates_at_obstacle`
断言「撞墙截断、不沿墙滑行」。

### JASS 实证（`../098c/out/war3map_pretty.j`）
移动结算循环（约 `8640-8730`）是**逐轴（分轴）**处理，不是“整体清零”：

```
set px = K[gX]+Q[gX]        // 下一帧 X
set py = L[gX]+S[gX]        // 下一帧 Y
if RA(px, L[gX]) == 6 then          // 6 = 该点不可通行（RA=地形/寻路格查询，见下）
    if nv[gX]==1 or xv[gX] > 0 then
        set Q[gX] = -Q[gX]*xv[gX]   // 反射 X 分量（撞墙弹回）
    else
        set Q[gX] = 0; call IA(gX)  // 否则只清 X 分量
endif
if RA(K[gX], py) == 6 then          // Y 轴同理
    ...
endif
// 场地边界：夹到边界并 set Q/S = -Q/S*0.5（反弹一半）
```
- `RA(x,y) = kx[vO(x-Kx)+65*vO(y-lx)]`（`7693`）——预计算的**寻路/地形网格**查询，`6` = 不可通行。
- `xv` = **反弹系数**（弹性）：普通单位 `xv = -1`（`3620`/`3781`）；冲刺/弹体类 `xv = 1`（如 `12332`）→ 满反射。

**要点**：
1. 098c 撞障碍是**分轴**响应：斜撞墙时只清/反射“撞进去的那个轴”，**切向分量保留** → 表现为**沿墙滑行/偏折**，
   而不是整体停死。
2. `xv>0` 的位移（多数冲刺/弹体）会**反弹**；否则该轴清零。
3. 场地边界：夹边 + 速度**×0.5 反弹**。

我们的「任何接触即 `control=None`」相比之下确实**过强**——这正是“手感不一样”的来源。

### 待确认（决定修复细节）
- ~~**S012 冲撞的“施法者”本身** `nv`/`xv` 取值~~ **已定案**：英雄 `nv==1`、`xv=0.5`（`FR`）→ 撞柱半速反弹。
  `AB`（`11916`）蓄力设 `Q/S/U/w/ev`，不碰 `xv`。
- 是否需要区分“撞**柱子**（圆形障碍）”与“撞**场地边界**”：098c 是同一网格 `RA`，边界另走夹取+0.5 反弹。

### 修复方案（推荐）
把 `resolve_obstacles` 从“接触即清 control”改为**分轴响应**：

1. 用障碍法线 `n = normalize(p.pos - o.pos)` 求碰撞法向。
2. 把 `control.vel` 分解为法向 `vn = dot(v, n)` 与切向 `vt = v - vn*n`：
   - 若该 mover 标记为“可反弹”（对应 `xv>0`）：`v' = vt - vn*n`（反射法向，保留切向）；
   - 否则：`v' = vt`（切向保留，法向清零）——对应 098c `Q=0` 只清一轴的整体效果。
3. 位置推出障碍（已有）。
4. **不再**因一般碰撞清 `control`；只有冲刺“命中敌人急停”（`stop_on_hit`，已单独实现）才停。
5. ~~场地边界：夹取 + 速度×0.5 反向~~ → **不做**：`Nr/Br/br/cr = GetRectMinX(bj_mapInitialPlayableArea)`
   （War3 地图可玩矩形，`26640`），属**引擎限制**；我们的边界是**岩浆区**（出界灼烧）。
6. 保留 E2b 潜行踢·连推的 `ricochet` 分支（撞墙排重踢），它与上述独立。

> **状态（2026-09-13）**：英雄冲刺逐轴反弹**已做**；**弹体撞柱**仍多为挡下/消失 →
> `xv>0` 的 **mover（弹体类）通用反弹未做**（098c 对所有 mover 统一 `Q=-Q*xv`）。待定是否按简化保留。

### 已实施（Bug B，第一次，后经 JASS 复核更正）
`resolve_obstacles` 改为**逐轴**响应：对 X/Y 各自判断速度是否指向障碍（`vel.axis * dir.axis < 0`），
是则只清该轴速度、保留另一轴 → 斜撞沿墙滑行；不再因接触清 `control`。同样处理 `dash_active` 的 `dash_vel`。

### 更正（Bug B，2026-09-13）：英雄撞柱是**半速反弹**，不是清零
复核 JASS 后确认之前把 `nv` 认反了：
- 英雄由 `FR` → `OO(1,..)` 创建（`war3map_pretty.j:4953`）→ **`nv==1`**，且 `set xv[i]=.5`（`4979`）。
- 撞柱分支（`8656/8674`）：`if nv[gX]==1 or xv[gX]>0 then set Q[gX]=-Q[gX]*xv[gX]`。
  → 英雄 `nv==1,xv=.5` 走这分支：该轴速度 **反向 ×0.5（半速反弹）**，切向保留。
- `else`（`Q=0`）只适用于 `nv!=1 且 xv<=0` 的 mover；`xv=1`（全反射）是多数投射物。
- `AB`（`11916`）蓄力设 `Q=Gr*dir`、`U=hr*dir`（`Gr=1300*.03`、`hr=Gr*.04`），道具不碰 `xv` → 英雄始终 0.5。

**已实施（最终）**：`resolve_obstacles` 对玩家 `control.vel`/`dash_vel` 逐轴 `v_axis = -v_axis * 0.5`（常量
`PLAYER_OBS_RESTITUTION=0.5`）。测试 `s012_dash_bounces_off_obstacle_head_on`（正面撞柱速度反向、
明显退回）、`s012_dash_slides_along_obstacle_at_angle`（斜撞法向反弹、切向保留）。
遗留：场地**矩形边界** 098c 是 `Q=-Q*.5` 反弹（`8707/8728`），我们目前只做出界掉血（未实现边界反弹，与本 bug 独立）。

### 测试改动
- **替换** `s012_dash_truncates_at_obstacle`（它钉住的“整体截断”与 JASS 冲突）：
  - 正面撞柱：若该 mover 可反弹 → 断言 `vel` 反向（或至少不再朝原方向）；若不可 → 断言法向分量清零、切向保留。
  - 斜角撞柱：断言切向速度保留（会沿墙滑行），而不是 `control=None`。
- 新增“撞墙后仍保有 control（持续时间未被打断）”断言。

### 风险
- 行为改动影响所有“强制位移撞墙”（冲撞/击退/跳弹/凤凰等），需回归 `control`/`kick` 相关全部测试。
- 需先确定各 mover 的“可反弹”标记从哪来（098c `xv`）；可先用一个 `Player` 字段或按技能类型判断。

---

## 3. 建议执行顺序

- ✅ **Bug A**（客户端清除条件收紧 + 单测）。
- ✅ **Bug B**（`resolve_obstacles` 逐轴响应；替换/新增测试）。
- ⬜ **抖动**：如仍不可接受，另开「渲染插值」规划（不增加输入延迟）；否则暂接受。

---

## 4. 附：渲染插值（抖动备选，仅要点）

- 目标：**不改网络时序**，只让画面平滑——把“2~3 tick 的帧到达抖动”在视觉上抹平。
- 做法：world 每次 `step` 前保存“上一帧玩家/弹体位置”，绘制时按 `alpha = accumulator / TICK`
  在 `prev → cur` 之间 lerp。
- 代价：画面最多滞后 <1 帧（~16ms），**不增加操作延迟**（与用户拒绝的“固定 D”不同）。
- 风险：需覆盖玩家、弹体、投射体；不影响确定性（纯渲染）。
