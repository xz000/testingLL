# S012B 凤凰冲刺 重写计划（按 098c）

> 状态：**已实施**（2026-10-17，commit `651b208`，协议 **67**）。
> 目标：把当前“直线 dash 近似”改成 098c 的 **固定时长、可点地转向、结束时保留速度** 的凤凰冲刺。
> 会改模拟与输入协议；已从 66 升到 **67**。

## 1. 098c 真值（`war3map.j`）

### 1.1 开始：`WB`

```jass
if not kr[ri] then
    call bO(ii, 20, TX, TY)          // 初速度：+20/tick ≈ 667/s，方向 = 点击点
    call IssueImmediateOrderById(...)
    call SetUnitFacing(...)
    call JO(true, ii, true, 7331, false, 0)
    set kr[ri] = true
    set Jr[ri] = CreateTrigger()     // EVENT_UNIT_ISSUED_POINT_ORDER -> UB
    // 3.1 * jn[ri] 秒后 -> PB
endif
```

- 初速度 **20/tick ≈ 667/s**；
- 持续时间 **`3.1 × jn` 秒**（`jn` 是持续时间倍率，不是时间精通）；
- 期间**每次点地**触发 `UB`。

### 1.2 转向：`UB`（每次新点地）

```jass
set wB = SquareRoot(Q*Q + S*S)       // 当前每 tick 速度
if Fr[gX] then                      // 风步状态
    call tB(gX, orderX, orderY)     // 生成凤凰弹
    call JO(...7331...)
else
    call bO(gX, 18*Pow(.5, wB/20), orderX, orderY)
endif
```

- 普通状态：**追加**速度  
  `18 × 0.5^(当前每 tick 速度 / 20)`；
- **不是覆盖**；速度越快追加越少；
- 只在**新的点地指令**触发，不是每帧持续触发；
- 目标点只决定方向，**没有最大施法距离/clamp**。

### 1.3 风步分支：`tB`

```jass
set ev[Nb] = 1.4*(1+.1*ei[id])       // 生命 1.4*(1+.1×时间精通)
speed = 1000；radius = 44
Xv = Wr + wr
// 伤害 handler ea/xa；当前 Rust 侧按 4+0.5*(R+E 等级) 近似
```

- 只有 `Fr[gX]`（风步）时才生成；
- 普通凤凰状态**没有** `tB` 弹体。

### 1.4 结束：`PB`

```jass
call DestroyEffect(oe)
call DestroyTrigger(Jr[id])
set kr[id] = false
```

- **只结束状态**，不清 `Q/S`；
- 所以结束后角色保留当前速度继续移动/滑行，不会急停；
- 没有“20% 余速”这类二次缩放（那是 S012A 的 `CR`）。

### 1.5 伤害结论

- `CA` 的伤害分支只检查 `Hr`（S012A）；
- S012B 在普通状态**没有直接接触伤害**；
- 伤害来自风步状态下的 `tB` 弹体。
- 当前 Rust 给 Phoenix 设置的 `p.kick`（接触伤害/击退）需要移除或确认（按 098c 应移除）。

## 2. 当前 Rust 实现与差距

当前 `world.rs` 的 `W098bUtilKind::Phoenix`：

```rust
let dist = stats.max_distance * ei_mult(false);
let dur_s = (dist / speed).max(dur);
p.push(dir * speed, dur_s);
p.kick = Some(Kick { ... });
p.phoenix_remaining = dur;
```

差距：

| # | 当前 | 098c |
|---|---|---|
| 1 | 持续时间固定 `3.1` | `3.1 × jn` |
| 2 | 转向**替换** `control.vel`，且 `set_target` 每帧都执行 | 每次**新点地**才**追加**速度 |
| 3 | `cast_range_limit` 对 B 做距离 clamp | 无距离限制 |
| 4 | `cast_max_distance` 对 B 乘时间精通 | 主凤凰不吃 `ei` |
| 5 | 结束 `control` 到期后 `cur_vel=0`（burning=false） | 保留当前 `Q/S` 继续走 |
| 6 | `p.kick` 接触伤害/击退 | 普通状态无直接伤害；仅风步 `tB` |
| 7 | `phoenix_shots` 已有雏形 | 与 `tB` 对齐（生命/伤害/半径/速度） |

## 3. 设计草案

### 3.1 状态

复用现有字段，尽量不新增快照字段：

- `Player.phoenix_remaining: Fix64`（已有）：剩余持续时间；
- `Player.control: Option<Control>`（已有）：凤凰期间的速度载体；
  - `remaining = 3.1×jn`；
  - `decay: false`；
- `PlayerInput.phoenix_order: bool`（**新增，运行时输入字段**）：本 tick 是否有新的点地指令。

不改 `Control` 结构，避免快照格式变化；只加输入字段。

### 3.2 开始（cast effect）

1. 方向 = 指向点击点；
2. `p.control = Some(Control { vel: dir*667, remaining: 3.1*jn, decay:false })`；
3. `p.phoenix_remaining = 3.1*jn`；
4. **不**设置 `p.kick`、**不**设置 `burning`；
5. 清 `move_target`；不 clamp 目标距离。

### 3.3 转向（每 tick 输入阶段）

在现有 `phoenix_remaining > 0 && control.is_some()` 分支中：

- **只有 `pi.phoenix_order == true` 才执行**（新点地）；
- 普通状态：
  ```text
  v_tick = |control.vel| * 0.03
  imp = 18 * 0.5^(v_tick / 20)
  control.vel += dir * (imp / 0.03)     // 追加，不是覆盖
  ```
- 风步状态（`windwalk_state > 0 && !charging`）：生成 `phoenix_shots`（tB），不追加位移；
- 分支结束 `continue`，避免本次 `set_target` 再覆盖普通 `move_target`。

### 3.4 结束

在 `tick_buffs` 的 `control` 到期处：

- 若 `phoenix_remaining > 0`（Phoenix 状态）：
  - `cur_vel = control.vel`（**保留速度**）；
  - 不要走 S012A 的 20% 余速逻辑（`burning=false`）；
- `control=None`；`phoenix_remaining` 随后归零；
- 之后由普通移动/冰面减速接管。

### 3.5 范围与指示线

- `cast_range_limit(S012, alt=true, ...) -> None`（B 无距离 clamp）；
- `cast_max_distance(S012, alt=true, ...) -> None`（指示线跟随鼠标/只表示方向）；
- S012A 保持现状（700×(1+.1时间精通)，clamp）。
- 客户端 `skill_aim_hint` 对 B 返回 0 时，现有绘制自然不截断。

### 3.6 输入边沿

- `PlayerInput` 新增 `phoenix_order: bool`；
- 客户端右键时设置 `pending_phoenix_order = true`；
- `local_player_input()` 取走并写入 `phoenix_order`（与 `pending_cast` 同模式）；
- `netcode::encode/decode_player_input` 增加 1 字节；
- 协议 **67**（运行期输入格式变化，不是快照字段）。

> 为什么不靠 `set_target` 变化判断：锁步输入每帧持续发送同一个 `set_target`；098c 只在 `EVENT_UNIT_ISSUED_POINT_ORDER` 时追加，必须显式边沿。

## 4. 实施步骤（每步一个 commit）

1. **输入边沿**：`PlayerInput.phoenix_order` + netcode + client pending flag + 单测往返。
2. **B 范围/指示线**：`cast_range_limit`/`cast_max_distance` 对 S012 alt 返回 None；不影响 A。
3. **状态机**：Phoenix 开始/转向/结束按 §3 改；移除 `p.kick`；保留速度；`jn` 时长。
4. **风步 tB 核对**：生命/伤害/半径/速度与 JASS 一致；必要时修正。
5. **单测**：
   - `phoenix_duration_scales_with_jn`；
   - `phoenix_new_order_adds_velocity_once`（同一 tick 多个 set_target 不追加，只有 phoenix_order 才追加）；
   - `phoenix_end_preserves_velocity`（结束不归零、无 20%）；
   - `phoenix_b_has_no_range_clamp`；
   - `phoenix_windwalk_spawns_tb_projectile`；
   - netcode `phoenix_order` 往返。
6. **协议 67** + 文档（台账/审计/HANDOFF）。

## 5. 风险

| 风险 | 说明 | 对策 |
|---|---|---|
| 输入边沿丢失 | 锁步下右键频率/网络缓冲 | `PlayerInput` 显式 flag；每帧发送；单测 |
| `control` 与普通移动冲突 | 凤凰期间 `set_target` 仍会写 `move_target` | 转向分支 `continue`，跳过普通移动 |
| 结束速度与冰面/刹车模型 | 098c `Q/S` 保留；我们转 `cur_vel` 后由现有模型衰减 | 单测地面/冰面各一条 |
| `jn` 来源 | 冲撞/凤凰时长倍率 | 用 `p.jn()`；确认 buff/物品生效 |
| 风步 `tB` 伤害 | handler 未完全反编译 | 先按现有 `4+0.5*(R+E)` 近似，标注待核 |
| 协议兼容 | 输入格式 +1 字节 | 升 67，两端同时更新 |
| S012B 指示线 | 无距离限制，目标只表方向 | `max_dist=0`，画到鼠标；不再显示截断/红圈 |

## 6. 需要拍板的细节

1. **普通凤凰是否真的没有直接接触伤害**？（按 `CA` 只有 `Hr` 分支，判断为没有；需你确认是否按此重写。）
2. 结束保留速度时，地面/冰面都保留（098c 是）；是否接受“结束后还会滑一段”？
3. 指示线是否接受“B 不显示最大距离，只画方向线”？
4. 协议 67 是否现在就要升（联机两端同步更新）。

## 7. 结论

S012B 不是“改一个时长”，而是**输入边沿 + 状态机 + 范围/指示线 + 结束速度**的组合改动。建议按 §4 的 5 个 commit 小步实施，每步 `check.ps1` + 单测，最后真机验证“点地转向、时长、结束滑行、风步弹体”。

## 8. 实施记录

- `651b208`：输入边沿 / 状态机 / 无 clamp / 结束保速 / 移除 kick 主体。
- `1469184`（手感修正）：Phoenix 的 `control.vel` 每 tick 先按地面 `0.96^(5/9)` / 冰面 `0.98^(5/9)` 衰减，再移动；初始速度改为 `cur_vel + dir×667`（追加现有 Q/S）。修掉“恒定速度 / 点击后不衰减、越点越快”的违和感。

## 9. “自然衰减”语义（重要，避免再误改）

### 9.1 098c 的移动方程

地面（`ua != 3`，未定身）：

```jass
Q = (Q - Y) * .96
S = (S - z) * .96
// 然后把 Y/z 设为朝当前移动指令的速度（gv），再加回 Q/S
```

- `Q/S` = 当前每 tick 速度；
- `Y/z` = **朝当前移动指令**的“正常走路速度”；
- 所以 `.96` 衰减的是**超出正常走路速度的那部分**（冲刺/击退/外部冲量）；
- 没有移动指令时 `Y=z=0`，速度整体每 tick ×0.96。

冰面：`Q/S × .98`（或朝实际朝向的转向式加速），没有 `Y/z` 回填。

S012A 冲刺额外有 `Hr`：

```jass
if Hr then
    Q = Q + U   // U = hr = Gr*(1-.96)
end
```

`U/w` 正好补偿 `.96` 的损失，所以 S012A 冲刺不会自然减速。

### 9.2 Phoenix 没有 `Hr`

S012B 只设置 `kr`，**不设置 `Hr`**，所以：

- 走普通 `Q/S` 分支 → 每 tick ×0.96（冰面 .98）；
- 初速度 `bO(ii,20,...)` 是 **+20/tick ≈667/s**；
- 没有 `U/w` 补偿 → 不点击时速度自然衰减；
- 每次新点地 `bO` 追加 `18×0.5^(v/20)`（v 为当前每 tick 速度，速度越快追加越少）；
- 3.1×jn 秒后 `PB` 只结束状态，**不清 Q/S**，剩余速度继续按普通移动衰减。

数值直观：

- 初速 20/tick；
- 3.1 秒 ≈103 个 0.03s tick，`20×0.96^103 ≈ 0.4/tick`（≈12/s）；
- 不点击时总位移约 `20/(1-.96) ≈ 500` 单位；
- 想维持速度/距离，必须**在窗口中继续点地**追加 `bO`。

这就是为什么严格按 098c 的 Phoenix 会显得“前段还行、后段很慢”：它是**衰减型冲刺**，不是 3.1 秒恒定速度 dash。

### 9.3 我们的近似与偏差

- 我们用 `control` 承载 Phoenix，`step_velocity` 的 control 分支会跳过普通移动的 `Y/z` 回填；
- 所以我们在 control 分支手动乘 `0.96^(5/9)` / `0.98^(5/9)`，等价于“没有走路目标时的自然衰减”；
- 初速用 `cur_vel + dir×667` 模拟 `bO` 追加；
- 差异：如果 Phoenix 期间同时存在普通走路指令/冰面转向，098c 的 `Y/z` 会参与；我们当前近似忽略这部分（影响较小，但极端情况可能手感不同）。

### 9.4 凤凰不会自动进入风步

按 098c 代码：

- `WB` **不设置** `Fr`、`gr`、`Agho`；
- `Fr` 只由 S010B（风步·隐身）的 `IB` 设置，`AA` 清除；
- `UB` 里 `if Fr[gX]` 才生成 `tB` 凤凰弹；
- 所以 Phoenix **不会自动进入风步**；只有已经处于 S010B 风步时，点地才会发 `tB`。

如果希望“Phoenix 自动进入风步”，那是**新增设计**，不是 098c 行为，需单独拍板。

### 9.5 若觉得太慢的选项

- **A（严格 098c）**：保留自然衰减；靠连续点地维持速度/距离；
- **B（手感优先，偏离 098c）**：降低衰减（如 0.98）或给一个短暂恒速段；
- **C（结构优先）**：把 Phoenix 从 `control` 改为 `cur_vel` + 专用分支，完整复用普通移动的 `Y/z`/冰面转向。

记录：**不要为了“看起来更快”直接把衰减删掉**——那会让 Phoenix 变回恒速 dash，偏离 098c；先确认要 A/B/C。
