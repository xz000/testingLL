# 击退系数对齐（098c `mI` 的 `lI`）—— 改动记录

> 日期：2026-09-24 ｜ commit：`b3a0a97` ｜ 协议：26 → **27**
>
> 依据：`P0_RECHECK.md` §B（复审发现 5 处击退系数缺口 + S001 距离衰减）。
> 真值来源：`../098c/out/war3map_pretty.j`。
> ⚠ **行号定位方法**：本仓库的 `.j` 文件用 `findstr` 计数会错乱（与 `Select-String` 差数千行）；
> 本文件所有行号均由 **`Select-String -Path ... -Pattern ...`** 取得。

---

## 0. 结论速览

| 技能 | 098c 表达式（函数/行） | 旧值 | 新值 |
|---|---|---|---|
| S000 火球 | `mI(nr,Vr,6.3+.7*Xv,1)`（`ib` 10287） | 1.0 | **1.0**（不变 ✓） |
| S002 闪电 | `mI(cb,db,cX,.95)`（`Bb` 10821） | 1.0 | **0.95** |
| S003 追踪 | 自撞 `TI(...,1,...)`（`lb` 10975）；同主非 Hr `tI(...,1.3,...)`（`lb` 11031）；敌 `mI(...,.95)`（`lb` 11060） | 1.0 | **AoE 1.3 / 自撞 1.0**（直伤 .95 见 §3 遗留） |
| S004 回旋镖 | `local real PI=.95`（`Sb` 11352）→ `mI(nr,Vr,6.4+.8*Xv,PI)`（11368） | 1.0 | **0.95** |
| S008A 陨石 | `mI(nr,gX,Zb(...),.75)`（`oB` 11581） | 0.8 | **0.75** |
| S008B 岩浆 | `tI(nr,cX,.6,...)`（`EB` 11798） | 0.8 | **0.6** |
| S001 天罚 | `mI(ii,gX,cX,1.-cO/$3E8)`（`mC` 15528） | 固定 | **距离衰减 `1-d/1000`** |
| S020 灾变 | `mI(ii,gX,cX-cO/60,1)`（`qC` 15737/15748/15758） | 半径衰减 | **固定 1** |
| S021 虔诚 | `mI(ii,gX,cX,1-cO/$3E8)`（`QC` 15938） | 固定 | **距离衰减 `1-d/1000`** |
| S009A 父弹 | `mI(nr,Vr,3,1.4)`（`FB` 12220） | 1.4 | **1.4**（不变 ✓） |
| S014A / S014B / S015 | `.2` / `.6` / `.65`（`vc` 13009 / `oc` 13132 / `BB` 11992） | 同 | **不变 ✓** |
| S016 | `1.15`（`gc` 13685 / `Gi` 13842/13846） | 同 | **不变 ✓** |

**修正了 `P0_RECHECK.md §B` 的两处误判**：
- S021 虔诚**不是**固定 1 —— 实为 `1-d/1000`（`QC` 15938）。
- S020 灾变是固定 1（`qC` 三条 `mI` 的 `lI=1`），但其**伤害**是加法衰减（`cX-cO/60` 或 `cX-cO/40`）。

---

## 1. 背景：`mI` 的击退公式不含距离项

`mI(HI,JR,HX,lI)`（`mI` 7051）：`HI`=伤害来源、`JR`=受击者，
`LI = ('d'+gn[JI])*HX*Gn[攻]*hn[受]*.03*Hn[受]*lI`，再 `Q/S[JR] += LI*dir`。
**`mI` 本身没有距离因子**——击退是否随距离衰减，完全取决于调用方传入的 `lI`：

- 绝大多数技能传**常数** `lI`（1 / .95 / .75 / .6 / …）。
- 只有 **S001 天罚 / S021 虔诚** 传 `lI = 1 - d/1000`（`1.-cO/$3E8`）。
- S020 灾变传 `lI = 1`（距离只影响伤害）。

我方的旧实现有一个**自造的半径衰减** `(1 - d/radius).max(0.2)`（`explode_at` 内），
它与 098c 对不上：S001 应衰减到 `1-d/1000`、S020 应完全不衰减、S000/S008A/S008B 应传固定系数。

---

## 2. 代码改动

### 2.1 `skill.rs`：五处系数

| 技能 | 位置 | 改动 |
|---|---|---|
| S003 | `warlock098b_def(S003).effect.kb_ji` | `ONE` → `1.3`（AoE 系数） |
| S004 | `warlock098b_def(S004).effect.kb_ji` | `ONE` → `0.95` |
| S002 | `warlock098b_def(S002).effect = W098bBolt { kb_ji }` | `ONE` → `0.95` |
| S008A | `warlock098b_def(S008).effect.kb_ji` | `0.8` → `0.75` |
| S008B | `warlock098b_def_alt(S008).effect.kb_ji` | `0.8` → `0.6` |

> S003 的 `kb_ji` 只被 **AoE 爆炸**（`expiry_blasts`）消费——我们的 `Homing` 命中统一走 AoE，
> 直伤 push 分支对 `homing` 是跳过的，故 `kb_ji` 即 AoE 系数（见 §3）。

### 2.2 `world.rs`：`explode_at` 新增击退衰减模式

原来只有 `dmg_falloff: DmgFalloff`，击退固定套用 `(1 - d/radius).max(0.2)`。
现新增独立参数 `kb_attn: KbAttn`：

```rust
enum KbAttn {
    Radius,        // 旧近似 (1-d/radius).max(0.2)：非名册（石头/导弹）保留
    Fixed,         // 固定系数（lI 与距离无关）
    Mul(Fix64),    // lI = 1 - d/k（S001/S021 的 k=1000）
}
```

各调用点：

| 调用点 | `KbAttn` | 依据 |
|---|---|---|
| 石头/导弹爆炸（`explode`） | `Radius` | 非 098c 名册，保留旧行为 |
| `expiry_blasts`（S000/S008B 等 `FloorMul`） | `Fixed` | `pI`/`tI` 的 `lI` 是固定系数 |
| `DelayedBlast`（S008A 陨石） | `Fixed` | `oB`：`mI(...,.75)` 固定 |
| S001 天罚 | `Mul(1000)` | `mC`：`lI=1-d/1000` |
| S020 灾变 | `Fixed` | `qC`：`lI=1` |
| S021 虔诚 | `Mul(1000)` | `QC`：`lI=1-d/1000` |

`explode_at` 内击退分支改为：
```rust
let falloff = match kb_attn {
    KbAttn::Radius => (ONE - dist / radius).max(0.2),
    KbAttn::Fixed  => ONE,
    KbAttn::Mul(k) => (ONE - dist / k).max(ZERO),
};
```

### 2.3 `world.rs`：S003 自撞 AoE 系数覆盖

```rust
// 098c lb：自撞（Hr 分支）用 TI(...,1,...) → AoE 击退系数 1.0；其余用 *kb_ji(=1.3)。
let aoe_kb = if homing_self { Fix64::ONE } else { *kb_ji };
expiry_blasts.push((pr.owner, pr.pos, r, dmg, aoe_kb, *blast_floor, hit_dmg));
```

### 2.4 协议

`PROTOCOL_VERSION 26 → 27`（`lib.rs`）。本次**没有新增序列化字段**（`KbAttn` 是运行期
参数，不入快照），但改动了会改变同一输入下世界演化的**数值/逻辑**，按 `lib.rs` 的版本约定
（"改动技能/物品数值、世界模拟逻辑"）需 +1。`world_ser` 布局不变。

---

## 3. 遗留 / 未建模（继续对齐时注意）

1. **S003 直伤 `.95` 无独立路径**：098c `lb` 对**敌人**（异主异队）走 `mI(nr,Vr,kb(nr),.95)`
   —— **直伤、无 AoE、无 +50 移速**；只有**同主**（自撞）才走 `TI/tI` AoE。
   我方 `Homing` 命中**统一**做 AoE（且非自撞还会给目标 +50 移速）。
   → 我方目前用 `kb_ji=1.3` 覆盖 AoE 情形；**敌人直伤 `.95` 这一支仍未建模**（属结构性偏差，
   非纯系数问题，另行处理）。`s003_can_hit_teammate_for_speed_buff` 的语义也源自此处，
   待结构性对齐时一并复核。
2. **S008B 撞柱击退 `.8`**：`Na`(11784) `mI(nr,Vr,$A+2*Xv,.8)`；我方撞柱只扣柱血、不给玩家击退。
3. **S002 的第二支 AoE**：`Bb` 命中友方弹体分支 `pI(db, …, $E6, .6)`（10832）——我方闪电是瞬时光束，未建模。
4. **S009B 区域父弹**：`kb_ji` 仍为 `1.4`（本次未核到其独立 `mI`；`FB` 的 1.4 是 A 形态父弹）。
5. 旧 `(1-d/radius).max(0.2)` 仍用于非名册石头/导弹，是否也要对齐待定。

---

## 4. 测试

- `skill::tests::s002_lightning_matches_spec`：`kb_ji == 0.95`。
- `skill::tests::s003_homing_matches_spec`：`kb_ji == 1.3`。
- `skill::tests::s004_boomerang_matches_spec`：`kb_ji == 0.95`。
- `skill::tests::s008_meteor_matches_spec`：`kb_ji == 0.75`。
- `skill::tests::s008b_magma_kb_matches_spec`（新增）：S008B `kb_ji == 0.6`。
- `world::tests::smite_knockback_uses_1_minus_d_over_1000`（新增）：S001 击退按 `1-d/1000`
  衰减（100→0.9、200→0.8，比值≈0.89；可区分半径衰减的 ≈0.33）。

回归：`check.ps1` 全绿（client 100 / game-core 277 / net 39 / net-steam 9 / steam+gui 107）。

---

## 5. 参考：本次核到的 `mI`/`tI`/`TI` 系数全表（`Select-String` 行号）

| 行 | 调用 | 归属性 |
|---|---|---|
| 10287 / 10365 / 10416 / 10441 | `mI(nr,Vr,5+.5*Xv,1)` 等 | S000 火球/火焰喷射 → 1 |
| 10821 | `mI(cb,db,cX,.95)` | S002 闪电直伤 → .95 |
| 10975 | `TI(nr,kb(nr),1,$C8*Xr)` | S003 自撞 AoE → 1 |
| 11031 | `tI(nr,kb(nr),1.3,$C8*Xr)` | S003 同主非 Hr AoE → 1.3 |
| 11060 | `mI(nr,Vr,kb(nr),.95)` | S003 敌人直伤 → .95 |
| 11364 / 11368 | `SI(...,.5*PI)` / `mI(...,PI)`（`PI=.95`） | S004 回旋镖 → .95 |
| 11581 | `mI(nr,gX,Zb(...),.75)` | S008A 陨石 → .75 |
| 11784 / 11798 | `mI(...,.8)` / `tI(nr,cX,.6,...)` | S008B 撞柱 / 爆炸 → .8 / .6 |
| 11992 | `mI(nr,Vr,2.5+.5*Xv,.65)` | S015 → .65 |
| 12220 | `mI(nr,Vr,3,1.4)` | S009A 父弹 → 1.4 |
| 13009 / 13132 | `mI(nr,Vr,ZO,.2)` / `...,.6)` | S014A / S014B → .2 / .6 |
| 13685 / 13842 / 13846 | `mI(...,1.15)` | S016 → 1.15 |
| 15528 | `mI(ii,gX,cX,1.-cO/$3E8)` | S001 → 1-d/1000 |
| 15737 / 15748 / 15758 | `mI(ii,gX,cX-cO/60|40,1)` | S020 → 1 |
| 15938 | `mI(ii,gX,cX,1-cO/$3E8)` | S021 → 1-d/1000 |
