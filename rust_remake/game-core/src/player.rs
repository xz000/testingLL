//! 玩家圆球 —— 确定性移动 / 受力 / buff 统一模型。
//!
//! 原版 `MoveScript` 采用「自走速度 + VelotoAdd 附加速度」的模型：玩家朝目标点以
//! `movespeed` 直走，同时场景里的力场（引力场 / 回拉线等）会往 `VelotoAdd` 里逐帧累加
//! 一个向量，二者相加即为本帧速度。受击（`RBScript::GetPushed`）时玩家失去自走控制，
//! 以给定矢量被推一段时间。
//!
//! 本模块把这三块统一为确定性的结构，供所有把受技能复用的地基：
//! - [`Control`]：强制速度（击退 / 冲锋 / 冲刺斩），带剩余时长。
//! - `pull`：本帧累计的附加速度（引力场 / 回拉线等持续力）。
//! - [`Buff`]：统一的自buff/减益（加速、护盾、隐身、束缚……），可加减、计时、到期回收。

use crate::fix::{Fix64, Vec2};
use crate::skill::{Caster, SkillId};

/// 玩家常量（与原版数值相符的量级）。
pub const BASE_SPEED: f64 = 3.2;
pub const DEFAULT_RADIUS: f64 = 1.0;
pub const MAX_HP: f64 = 100.0;
/// 自走起步的加速度（速度/秒²）。决定起步多快到达满速。
pub const ACCEL: f64 = 20.0;
/// 自走刹停的减速度（速度/秒²）。决定松手/到达后多快停下。
pub const DECEL: f64 = 40.0;
/// 每位玩家可同时叠加的 buff 数量上限。
pub const MAX_BUFFS: usize = 16;

/// 每个玩家拥有的技能槽数量（= 全技能上限，用于等级数组宽度）。
const SKILL_SLOTS: usize = crate::MAX_SKILL_SLOTS;

/// 一次"踢击/撞击"窗口：原版 `ColliderScript::StartKick`。
/// 携带者的碰撞在有 Kick 起效期间会把撞到的人推开/造成伤害，然后消耗。
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct Kick {
    pub push_power: Fix64,
    pub push_time: Fix64,
    pub push_damage: Fix64,
    /// 剩余生效时间
    pub remaining: Fix64,
}

/// 强制位移状态（原版 `MoveScript.controllable=false + Givenvelocity`）。
///
/// 玩家被 `push(vel, time)` 后，这段时间内不计自走，只按 `vel` 移动；
/// 到期后恢复自走控制。
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct Control {
    /// 强制速度（单位 / 秒）
    pub vel: Vec2,
    /// 剩余时长
    pub remaining: Fix64,
}

/// 一个带计时器/强度的自效果。
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct Buff {
    pub kind: BuffKind,
    /// 剩余时长（秒）
    pub remaining: Fix64,
}

/// 各类 buff 的类型。
#[derive(Copy, Clone, Debug, PartialEq)]
pub enum BuffKind {
    /// 移速倍率（1 = 无增益；可叠加时取最大）。
    Speed(f64),
    /// 反弹护盾（C2）：有效期内把撞上的弹体/玩家镜向反射（无吸收）。
    Reflect,
    /// 隐身（视觉隐藏；不影响碰撞判定）。
    Stealth,
    /// 束缚：期间不能施法（原版 `DoSkill::GetTied`）。
    Tied,
    /// 疾跑/生命偷取（C1）：受击时返还一半伤害作回血，移速随累积回血量成长。
    Boost,
}

impl Buff {
    pub fn new(kind: BuffKind, remaining: f64) -> Self {
        Buff {
            kind,
            remaining: Fix64::from_num(remaining),
        }
    }
}

impl BuffKind {
    fn same_variant(&self, other: &BuffKind) -> bool {
        // 仅按"种类"比对，不比较携带的数值（Speed / Shield 用值但不代表不同类）。
        std::mem::discriminant(self) == std::mem::discriminant(other)
    }
}

/// T2 扇扫连射的发射状态（施法者上的持久发射器）。
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct SweepState {
    pub dir: Vec2,
    pub bullet_speed: Fix64,
    pub damage: Fix64,
    /// 剩余要发射的弹数
    pub remaining: u32,
    /// 每发间隔（秒）
    pub cadence: f64,
    /// 每发角度步进（弧度）
    pub turn_step: f64,
    /// 距上次发射累计时间
    pub elapsed: f64,
    /// 上次发射的 id（用于归零判定，当前仅判断是否还有剩余）
    pub id: u32,
}

/// 单个玩家的确定性状态。
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct Player {
    pub id: u32,
    pub pos: Vec2,
    pub radius: Fix64,
    pub hp: Fix64,
    pub max_hp: Fix64,
    /// 当前移动目标点；`None` 表示本帧没有移动命令（停下来）。
    pub move_target: Option<Vec2>,
    /// 施法状态机（前摇 / 后摇 / 冷却 / 打断）
    pub caster: Caster,
    /// 各技能等级（索引 = SkillId::as_u32）
    pub skill_levels: [u32; SKILL_SLOTS],
    /// 最近一次伤害来源（用于击杀结算；None = 环境伤害）
    pub last_hit_by: Option<u32>,
    /// 强制位移（击退 / 冲锋 / 冲刺斩）。`None` = 处于自走控制中。
    pub control: Option<Control>,
    /// 本帧由场效应（引力场 / 回拉线 / 束缚等）累计的附加速度；每帧清零后由世界累加。
    pub pull: Vec2,
    /// 当前自走运动速度（单位 / 秒）。用于起步/刹停的加速度/减速度积分（平滑手感）。
    pub cur_vel: Vec2,
    /// 统一 buff 槽（加速 / 护盾 / 隐身 / 束缚 / 疾跑等）。
    pub buffs: [Buff; MAX_BUFFS],
    /// 影身（C3）锚点：存在时再次施法影身可传送回该点（持久的特殊状态，非计时 buff）。
    pub shadow_anchor: Option<Vec2>,
    /// 影身记号的有效倒计时（C3 `maxshadowtime`）：到期自动传回锚点并清记号。
    pub shadow_window: Fix64,
    /// 正在生效的踢击/撞击窗口（冲锋 / 潜行踢 / 冲刺斩……）。
    pub kick: Option<Kick>,
    /// 疾跑/生命偷取累积量（C1）。boost 期间受击返还一半回血并把待返还量暂存于此，
    /// 结束后一次性把移速加成回落（原版 `boostnow`）。
    pub boost_soaked: Fix64,
    /// 幻象（C4）「待幻」状态：存在时表示已施放但尚未在点击处留下假身；
    /// 记录已过时间用于计算剩余假身时长。`None` = 未处于待幻。
    pub fake_active: Option<Fix64>,
    /// 二段闪（R1b）可用窗口：第一次闪后其内可再免冷却短闪一次。`None` = 无窗口。
    pub blink2_window: Option<Fix64>,
    /// 冲刺斩（R2b）激活中：无限时长 + 隐身直冲，直到玩家给出新的移动命令才解除（原版 `IdoDSWL`）。
    pub dash_active: bool,
    /// 冲刺斩的位移速度（单位 / 秒），`dash_active` 时按此直线移动。
    pub dash_vel: Vec2,
    /// 潜行踢·连推（E2b）：撞障碍后需延迟重新踢击的时间；`None` = 无待重踢。
    pub ricochet_pending: Option<Fix64>,
    /// 潜行踢·连推：碰撞障碍时重放的踢击参数。
    pub ricochet_kick: Option<Kick>,
    /// 潜行踢·连推：可重踢的总窗口剩余时间（撞障碍后递减）。
    pub ricochet_window: Fix64,
    /// 扇扫连射（T2）：正在进行的依次发射状态。`None` = 未在发射。
    pub sweep: Option<SweepState>,
    /// 蓄力跳弹（T3b）累计的额外伤害（每命中 +0.3，miss 归零）。
    pub damageplus: f64,
    pub alive: bool,
}

impl Player {
    pub fn new(id: u32, pos: Vec2, radius: Fix64) -> Self {
        let max_hp = Fix64::from_num(MAX_HP);
        Player {
            id,
            pos,
            radius,
            hp: max_hp,
            max_hp,
            move_target: None,
            caster: Caster::new(),
            skill_levels: [1; SKILL_SLOTS],
            last_hit_by: None,
            control: None,
            pull: Vec2::ZERO,
            cur_vel: Vec2::ZERO,
            buffs: [Buff::new(BuffKind::Speed(1.0), 0.0); MAX_BUFFS],
            shadow_anchor: None,
            shadow_window: Fix64::ZERO,
            kick: None,
            boost_soaked: Fix64::ZERO,
            fake_active: None,
            blink2_window: None,
            dash_active: false,
            dash_vel: Vec2::ZERO,
            ricochet_pending: None,
            ricochet_kick: None,
            ricochet_window: Fix64::ZERO,
            sweep: None,
            damageplus: 0.0,
            alive: true,
        }
    }

    /// 该玩家某技能的等级。
    pub fn skill_level(&self, id: SkillId) -> u32 {
        self.skill_levels[id.as_u32() as usize]
    }

    /// 设置某技能等级（用于学习阶段购买升级）。
    pub fn set_skill_level(&mut self, id: SkillId, level: u32) {
        self.skill_levels[id.as_u32() as usize] = level;
    }

    // ---- 强制位移（受力） ----

    /// 立即进入强制位移：以 `vel` 移动 `time` 秒（覆盖旧状态；原版 `GetPushed`）。
    /// `inf` 用于无限时长（冲刺斩等），用 `true` 表示不设到期。
    pub fn push(&mut self, vel: Vec2, time: f64) {
        self.control = Some(Control {
            vel,
            remaining: Fix64::from_num(time),
        });
    }

    /// 是否正处于强制位移（不受自走控制）。
    pub fn in_control(&self) -> bool {
        self.control.is_some()
    }

    // ---- Buff 工具 ----

    /// 加一个 buff（同种刷新 / 取更久者，覆盖到一个空闲槽；无空槽则忽略）。
    pub fn add_buff(&mut self, kind: BuffKind, remaining: f64) {
        self.add_buff_fix(kind, Fix64::from_num(remaining));
    }

    fn add_buff_fix(&mut self, kind: BuffKind, remaining: Fix64) {
        // 若已有同类，取更长的剩余时间刷新。
        for slot in self.buffs.iter_mut() {
            if slot.remaining > Fix64::ZERO && slot.kind.same_variant(&kind) {
                if remaining > slot.remaining {
                    slot.remaining = remaining;
                }
                // 刷新时同时更新数值（如新的护盾量/移速）。
                slot.kind = kind;
                return;
            }
        }
        // 覆盖第一个空闲槽。
        for slot in self.buffs.iter_mut() {
            if slot.remaining <= Fix64::ZERO {
                *slot = Buff { kind, remaining };
                return;
            }
        }
    }

    /// 清空全部 buff。
    pub fn clear_buffs(&mut self) {
        for slot in self.buffs.iter_mut() {
            *slot = Buff::new(BuffKind::Speed(1.0), 0.0);
        }
    }

    /// 移除某种 buff（幂等）。
    pub fn remove_buff(&mut self, kind: BuffKind) {
        for slot in self.buffs.iter_mut() {
            if slot.remaining > Fix64::ZERO && slot.kind.same_variant(&kind) {
                slot.remaining = Fix64::ZERO;
            }
        }
    }

    /// 是否存在某种 buff。
    pub fn has_buff(&self, kind: BuffKind) -> bool {
        self.buffs
            .iter()
            .any(|b| b.remaining > Fix64::ZERO && b.kind.same_variant(&kind))
    }

    /// 取某种 buff 的第一个（用于读强度，如护盾剩余量）。
    pub fn buff_value(&self, kind: BuffKind) -> f64 {
        self.buffs
            .iter()
            .find(|b| b.remaining > Fix64::ZERO && b.kind.same_variant(&kind))
            .map(|b| match b.kind {
                BuffKind::Speed(v) => v,
                _ => 0.0,
            })
            .unwrap_or(0.0)
    }

    /// 是否隐身。
    pub fn stealth(&self) -> bool {
        self.has_buff(BuffKind::Stealth)
    }

    /// 是否被束缚（不能施法）。
    pub fn tied(&self) -> bool {
        self.has_buff(BuffKind::Tied)
    }

    /// 反弹护盾是否激活。
    pub fn shield(&self) -> bool {
        self.has_buff(BuffKind::Reflect)
    }

    /// 反弹护盾剩余时长。
    pub fn shield_remaining(&self) -> f64 {
        self.buffs
            .iter()
            .find(|b| b.remaining > Fix64::ZERO && b.kind.same_variant(&BuffKind::Reflect))
            .map(|b| b.remaining.to_num::<f64>())
            .unwrap_or(0.0)
    }

    // ---- 每帧推进 ----

    /// 本帧移动速度（自走速度 × 移速 buff 倍率）。
    #[inline]
    fn base_speed(&self) -> Fix64 {
        let mult = self.buff_value(BuffKind::Speed(1.0)).max(1.0);
        Fix64::from_num(BASE_SPEED) * Fix64::from_num(mult)
    }

    /// 依据"朝目标直线前进 + 加速度/减速度 + 附加速度"推进一帧的位移（整个速度合成模型）。
    ///
    /// 自走不是瞬到满速，而是用 [`Self::cur_vel`] 做**渐加速起步 / 渐减速刹停**（平滑手感）：
    /// - 有移动目标 → `cur_vel` 朝目标方向的期望速度以 `ACCEL` 逼近。
    /// - 无移动目标（或前摇期间被清目标）→ `cur_vel` 以 `DECEL` 逐渐减速到 0。
    /// - 一帧内可到达目标 → 直接落点并清零速度（自然成为刹停）。
    ///
    /// 强制位移（`control`）期间不走自走，只按 `control.vel` 定速并重置自走惯性。
    /// `pull`（力场附加）恒叠加到位移。
    pub fn step_velocity(&mut self, dt: Fix64) {
        if !self.alive {
            return;
        }
        // 0) 冲刺斩：无限时长直线冲刺，优先级最高（直到新移动命令解除）。
        if self.dash_active {
            self.cur_vel = Vec2::ZERO;
            self.pos += self.dash_vel * dt;
            self.pos += self.pull * dt;
            return;
        }
        // 1) 强制位移：定格速推进，重置自走惯性。
        if let Some(c) = &self.control {
            self.cur_vel = Vec2::ZERO;
            self.pos += c.vel * dt;
            self.pos += self.pull * dt;
            return;
        }
        // 2) 自走（前摇期间禁止：清目标使其自然减速停下）。
        if self.caster.is_windup() {
            self.move_target = None;
        }
        let target_vel = self.base_speed();
        match self.move_target {
            Some(target) => {
                let to_target = target - self.pos;
                // 若一帧的当前速度足够到达目标 → 落点并刹停（自然结束）。
                let step = self.cur_vel.length() * dt;
                if to_target.length_squared() <= step * step {
                    self.pos = target;
                    self.move_target = None;
                    self.cur_vel = Vec2::ZERO;
                } else {
                    let dir = to_target.normalized();
                    let desired = dir * target_vel;
                    self.cur_vel = move_toward(self.cur_vel, desired, Fix64::from_num(ACCEL) * dt);
                }
            }
            None => {
                // 无目标：渐减速到 0。
                self.cur_vel = brake(self.cur_vel, Fix64::from_num(DECEL) * dt);
            }
        }
        self.pos += self.cur_vel * dt;
        // 附加速度（力场）永远叠加。
        self.pos += self.pull * dt;
    }

    /// 推进统一 buff 计时（到期回收），并推进强制位移/踢击的剩余时长。
    pub fn tick_buffs(&mut self, dt: Fix64) {
        let eps = Fix64::from_num(1.0 / 65536.0);
        for b in self.buffs.iter_mut() {
            if b.remaining > Fix64::ZERO {
                b.remaining = (b.remaining - dt).max(Fix64::ZERO);
                // 定点残差防护：小于极小阈值视为已到期（避免 0 附近残留一个极小正值永不归零）。
                if b.remaining < eps {
                    b.remaining = Fix64::ZERO;
                }
            }
        }
        // 强制位移计时
        if let Some(c) = &mut self.control {
            c.remaining = (c.remaining - dt).max(Fix64::ZERO);
            if c.remaining < eps {
                self.control = None;
            }
        }
        // 踢击窗口计时
        if let Some(k) = &mut self.kick {
            k.remaining = (k.remaining - dt).max(Fix64::ZERO);
            if k.remaining < eps {
                self.kick = None;
            }
        }
        // 影身记号倒计时：到期自动回归锚点（原版 `BackToShadow`）
        if let Some(anchor) = self.shadow_anchor {
            self.shadow_window = (self.shadow_window - dt).max(Fix64::ZERO);
            if self.shadow_window < eps {
                // 自动回归：传回记号点，清记号与清移动目标
                self.pos = anchor;
                self.shadow_anchor = None;
                self.shadow_window = Fix64::ZERO;
                self.move_target = None;
            }
        }
        // 幻象「待幻」超时失效
        if let Some(t) = &mut self.fake_active {
            *t = (*t - dt).max(Fix64::ZERO);
            if *t < eps {
                self.fake_active = None;
            }
        }
        // 二段闪窗口计时（到期失效）
        if let Some(t) = &mut self.blink2_window {
            *t = (*t - dt).max(Fix64::ZERO);
            if *t < eps {
                self.blink2_window = None;
            }
        }
        // 潜行踢·连推（E2b）：总窗口倒计时；撞障碍后 delay 结束则重新踢击。
        if self.ricochet_window > Fix64::ZERO {
            self.ricochet_window = (self.ricochet_window - dt).max(Fix64::ZERO);
            if let Some(t) = &mut self.ricochet_pending {
                *t = (*t - dt).max(Fix64::ZERO);
                if *t < eps {
                    let pending_kick = self.ricochet_kick; // Copy
                    if let Some(k) = pending_kick {
                        self.kick = Some(k);
                        self.add_buff(BuffKind::Stealth, self.ricochet_window.to_num::<f64>().max(0.1));
                    }
                    self.ricochet_pending = None;
                }
            }
            if self.ricochet_window < eps {
                self.ricochet_window = Fix64::ZERO;
                self.ricochet_pending = None;
                self.ricochet_kick = None;
            }
        }
        // 注：冲刺斩不靠计时到期，而是由「玩家给出新的移动命令」触发解除（见 world.step）。
    }

    /// 清空本帧的附加速度（世界在每帧移动前清零）。
    pub fn reset_pull(&mut self) {
        self.pull = Vec2::ZERO;
    }

    /// 新一轮开始时重置回合相关状态（保留 id / pos / 技能等级 / 半径）。
    pub fn reset_state(&mut self) {
        self.hp = self.max_hp;
        self.alive = true;
        self.last_hit_by = None;
        self.control = None;
        self.pull = Vec2::ZERO;
        self.cur_vel = Vec2::ZERO;
        self.clear_buffs();
        self.kick = None;
        self.shadow_anchor = None;
        self.shadow_window = Fix64::ZERO;
        self.boost_soaked = Fix64::ZERO;
        self.fake_active = None;
        self.blink2_window = None;
        self.dash_active = false;
        self.dash_vel = Vec2::ZERO;
        self.ricochet_pending = None;
        self.ricochet_kick = None;
        self.ricochet_window = Fix64::ZERO;
        self.sweep = None;
        self.damageplus = 0.0;
        self.caster = Caster::new();
    }

    /// C1 疾跑：受击时若在 Boost buff 内，返回**实际应扣到 HP 上的净伤害**（返回一半作为回血，
    /// 并把待结算的移速成长量累进 [`Self::boost_soaked`]）。原版：`hp -= damage; hp += boostvalue`。
    pub fn soak_boost(&mut self, damage: Fix64) -> Fix64 {
        if !self.has_buff(BuffKind::Boost) {
            return damage;
        }
        let refund = damage / Fix64::from_num(2);
        self.boost_soaked += refund;
        damage - refund
    }
}

/// 把 `cur` 朝 `desired` 移动，但单步最多移动 `max_delta` 的矢量变化量。
fn move_toward(cur: Vec2, desired: Vec2, max_delta: Fix64) -> Vec2 {
    let diff = desired - cur;
    let len = diff.length();
    if len <= max_delta {
        desired
    } else {
        cur + diff.normalized() * max_delta
    }
}

/// 把速度 `v` 朝零减速：单步最多减少 `max_delta` 的速率（保留方向）。
fn brake(v: Vec2, max_delta: Fix64) -> Vec2 {
    let len = v.length();
    if len <= max_delta {
        Vec2::ZERO
    } else {
        v * ((len - max_delta) / len)
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    fn near(a: Fix64, b: f64, tol: f64) -> bool {
        (a.to_num::<f64>() - b).abs() < tol
    }

    #[test]
    fn push_overrides_and_expires() {
        let mut p = Player::new(0, Vec2::ZERO, Fix64::ONE);
        p.push(Vec2::new(Fix64::from_num(10.0), Fix64::ZERO), 1.0);
        assert!(p.in_control());
        let dt = Fix64::from_num(0.1);
        for _ in 0..20 {
            p.step_velocity(dt);
            p.tick_buffs(dt);
        }
        // 强制位移 1s：10 单位/秒 × 1s = 10 单位；到期后无自走目标则停。
        assert!(!p.in_control());
        assert!(near(p.pos.x, 10.0, 0.5));
    }

    #[test]
    fn buff_stack_refresh_and_expire() {
        let mut p = Player::new(0, Vec2::ZERO, Fix64::ONE);
        p.add_buff(BuffKind::Speed(2.0), 1.0);
        p.add_buff(BuffKind::Speed(3.0), 2.0); // 同种刷新为更长
        assert!((p.buff_value(BuffKind::Speed(0.0)) - 3.0).abs() < 1e-6);
        // 等 2s 后到期
        let dt = Fix64::from_num(0.2);
        for _ in 0..10 {
            p.tick_buffs(dt);
        }
        assert!(!p.has_buff(BuffKind::Speed(0.0)));
    }

    #[test]
    fn stealth_and_tied_buff() {
        let mut p = Player::new(0, Vec2::ZERO, Fix64::ONE);
        p.add_buff(BuffKind::Stealth, 1.0);
        assert!(p.stealth());
        p.remove_buff(BuffKind::Stealth);
        assert!(!p.stealth());
        p.add_buff(BuffKind::Tied, 1.0);
        assert!(p.tied());
    }

    #[test]
    fn pull_adds_to_movement() {
        let mut p = Player::new(0, Vec2::ZERO, Fix64::ONE);
        let dt = Fix64::from_num(0.1);
        // 每帧往 pull 方向推（模拟引力场）
        p.pull = Vec2::new(Fix64::ONE, Fix64::ZERO);
        p.step_velocity(dt);
        assert!(near(p.pos.x, 0.1, 1e-3));
    }

    #[test]
    fn self_walk_accelerates_gradually() {
        let mut p = Player::new(0, Vec2::ZERO, Fix64::ONE);
        let dt = Fix64::from_num(1.0 / 60.0);
        let target = Vec2::new(Fix64::from_num(100.0), Fix64::ZERO); // 足够远
        p.move_target = Some(target);
        // 第 1 帧：起步速度 = ACCEL * dt ≈ 0.33，而非瞬时满速 3.2
        p.step_velocity(dt);
        let first_speed = p.cur_vel.length().to_num::<f64>();
        assert!(first_speed > 0.0 && first_speed < 1.0, "起步应渐加速，第一帧速度约 {}, 不应瞬满", first_speed);
        // 跑几秒应接近满速 3.2（但略低于，因尚未完全到达满速）
        for _ in 0..120 {
            p.step_velocity(dt);
        }
        let speed = p.cur_vel.length().to_num::<f64>();
        assert!(speed > 3.0 && speed <= 3.2 + 0.01, "渐加速后应接近满速 3.2，实际 {}", speed);
    }

    #[test]
    fn self_walk_decelerates_to_stop() {
        let mut p = Player::new(0, Vec2::ZERO, Fix64::ONE);
        let dt = Fix64::from_num(1.0 / 60.0);
        let target = Vec2::new(Fix64::from_num(3.0), Fix64::ZERO); // 短途
        p.move_target = Some(target);
        let mut prev_speed = 0.0;
        let mut peaked = false;
        let mut stopped_frame = None;
        for frame in 0..120 {
            p.step_velocity(dt);
            let speed = p.cur_vel.length().to_num::<f64>();
            if speed > prev_speed + 1e-6 {
                peaked = true;
            }
            if p.move_target.is_none() && speed < 1e-4 {
                stopped_frame = Some(frame);
                break;
            }
            prev_speed = speed;
        }
        assert!(peaked, "应经历先加速的过程");
        assert!(stopped_frame.is_some(), "到达后应渐减速并在某个时间点完全停住");
        assert!(near(p.pos.x, 3.0, 0.05), "应停在目标点附近");
    }
}
