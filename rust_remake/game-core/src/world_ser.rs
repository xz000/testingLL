//! 确定性 World 序列化（重连快照 / 存档用）。
//!
//! 纯手写、大端、长度前缀，逐字段覆盖 World（含 Player / Caster / Projectile / Obstacle 等）。
//! 保证 `to_bytes` ↔ `from_bytes` 后逐位一致，供重连端重建整场 World 后继续 lockstep。

use crate::fix::{Fix64, Vec2};
use crate::player::{Buff, BuffKind, Cmd, Control, Kick, Player, SweepState, MAX_CMDS};
use crate::skill::{CastPhase, Caster, SkillId};
use crate::world::{Obstacle, Projectile, ProjectileKind, ScatterKind, World};

fn wf64(o: &mut Vec<u8>, v: f64) {
    o.extend_from_slice(&v.to_bits().to_be_bytes());
}
fn wu64(o: &mut Vec<u8>, v: u64) {
    o.extend_from_slice(&v.to_be_bytes());
}
fn wu32(o: &mut Vec<u8>, v: u32) {
    o.extend_from_slice(&v.to_be_bytes());
}
fn wu8(o: &mut Vec<u8>, v: u8) {
    o.push(v);
}
fn wfix(o: &mut Vec<u8>, v: Fix64) {
    wu64(o, v.to_bits() as u64);
}
fn wvec(o: &mut Vec<u8>, v: Vec2) {
    wfix(o, v.x);
    wfix(o, v.y);
}
fn u64at(b: &[u8], p: &mut usize) -> Option<u64> {
    let s = b.get(*p..*p + 8)?;
    *p += 8;
    Some(u64::from_be_bytes(s.try_into().ok()?))
}
fn u32at(b: &[u8], p: &mut usize) -> Option<u32> {
    let s = b.get(*p..*p + 4)?;
    *p += 4;
    Some(u32::from_be_bytes(s.try_into().ok()?))
}
fn u8at(b: &[u8], p: &mut usize) -> Option<u8> {
    let v = *b.get(*p)?;
    *p += 1;
    Some(v)
}
fn fixat(b: &[u8], p: &mut usize) -> Option<Fix64> {
    Some(Fix64::from_bits(u64at(b, p)? as i64))
}
fn vecat(b: &[u8], p: &mut usize) -> Option<Vec2> {
    Some(Vec2::new(fixat(b, p)?, fixat(b, p)?))
}

// ===== 不可信输入的上界防护（RISK_ANALYSIS.md P4 / D2） =====
//
// 快照字节来自网络（重连 / 主机迁移），长度前缀是对方完全可控的 u32。
// 若在 `Vec::with_capacity` 里直接使用，一个 `count = 0xFFFFFFFF` 的**小包**
// 就能让进程申请数十 GB 后 abort（OOM / DoS）。上界取远大于任何合法对局的宽松值，
// 超限即视为快照非法并返回 `None`，由调用方按「快照无效」处理。

const MAX_DECODE_PLAYERS: usize = 64;
const MAX_DECODE_OBSTACLES: usize = 256;
const MAX_DECODE_PROJECTILES: usize = 4096;
const MAX_DECODE_ELIMINATED: usize = 64;
const MAX_DECODE_KILLS: usize = 4096;

/// 读取一个长度前缀并校验上界；超限返回 `None` 拒绝该快照。
fn count_at(b: &[u8], p: &mut usize, max: usize) -> Option<usize> {
    let n = u32at(b, p)? as usize;
    if n > max {
        None
    } else {
        Some(n)
    }
}

/// 校验指令环形缓冲的下标范围。
///
/// `cmd_buf` 只有 `MAX_CMDS` 个槽（game-core/src/player.rs:29），损坏/恶意快照给出的
/// 越界 `cmd_head` 会让 `Player` 的 `cmd_buf[cmd_head]`（player.rs:586）越界 panic。
fn cmd_indices_valid(head: usize, len: usize) -> bool {
    head < MAX_CMDS && len <= MAX_CMDS
}

fn wopt_vec(o: &mut Vec<u8>, v: Option<Vec2>) {
    match v {
        Some(t) => {
            wu8(o, 1);
            wvec(o, t);
        }
        None => wu8(o, 0),
    }
}
fn opt_vec(b: &[u8], p: &mut usize) -> Option<Option<Vec2>> {
    Some(if u8at(b, p)? != 0 { Some(vecat(b, p)?) } else { None })
}

fn encode_cast_phase(o: &mut Vec<u8>, ph: CastPhase) {
    match ph {
        CastPhase::Idle => wu8(o, 0),
        CastPhase::Windup { id, target, remaining } => {
            wu8(o, 1);
            wu32(o, id.as_u32());
            wopt_vec(o, target);
            wfix(o, remaining);
        }
        CastPhase::Recovery { id, remaining } => {
            wu8(o, 2);
            wu32(o, id.as_u32());
            wfix(o, remaining);
        }
    }
}
fn decode_cast_phase(b: &[u8], p: &mut usize) -> Option<CastPhase> {
    match u8at(b, p)? {
        0 => Some(CastPhase::Idle),
        1 => {
            let id = SkillId::from_u32(u32at(b, p)?);
            let target = opt_vec(b, p)?;
            let remaining = fixat(b, p)?;
            Some(CastPhase::Windup { id, target, remaining })
        }
        2 => {
            let id = SkillId::from_u32(u32at(b, p)?);
            let remaining = fixat(b, p)?;
            Some(CastPhase::Recovery { id, remaining })
        }
        _ => None,
    }
}

fn encode_caster(o: &mut Vec<u8>, c: &Caster) {
    let (phase, cd) = c.raw_snapshot();
    encode_cast_phase(o, phase);
    for cc in cd {
        wfix(o, cc);
    }
}
fn decode_caster(o: &mut Caster, b: &[u8], p: &mut usize) -> Option<()> {
    let phase = decode_cast_phase(b, p)?;
    let mut cd = [Fix64::ZERO; crate::MAX_SKILL_SLOTS];
    for c in cd.iter_mut() {
        *c = fixat(b, p)?;
    }
    o.raw_restore(phase, cd);
    Some(())
}

fn encode_buff(o: &mut Vec<u8>, b: &Buff) {
    match b.kind {
        BuffKind::Speed(v) => {
            wu8(o, 0);
            wf64(o, v);
        }
        BuffKind::Reflect => wu8(o, 1),
        BuffKind::Stealth => wu8(o, 2),
        BuffKind::Tied => wu8(o, 3),
        BuffKind::Boost => wu8(o, 4),
        BuffKind::Scorched => wu8(o, 5),
        BuffKind::LavaShield => wu8(o, 6),
        BuffKind::Aegis => wu8(o, 7),
        BuffKind::Pancake => wu8(o, 8),
        BuffKind::Slow(v) => {
            wu8(o, 9);
            wu64(o, v.to_bits());
        }
        BuffKind::Weakened => wu8(o, 10),
        BuffKind::Silenced => wu8(o, 11),
        BuffKind::Mirror => wu8(o, 13),
        BuffKind::Haste => wu8(o, 14),
    }
    wfix(o, b.remaining);
}
fn decode_buff(b: &[u8], p: &mut usize) -> Option<Buff> {
    let kind = match u8at(b, p)? {
        0 => BuffKind::Speed(f64::from_bits(u64at(b, p)?)),
        1 => BuffKind::Reflect,
        2 => BuffKind::Stealth,
        3 => BuffKind::Tied,
        4 => BuffKind::Boost,
        5 => BuffKind::Scorched,
        6 => BuffKind::LavaShield,
        7 => BuffKind::Aegis,
        8 => BuffKind::Pancake,
        9 => BuffKind::Slow(f64::from_bits(u64at(b, p)?)),
        10 => BuffKind::Weakened,
        11 => BuffKind::Silenced,
        13 => BuffKind::Mirror,
        14 => BuffKind::Haste,
        _ => return None,
    };
    let remaining = fixat(b, p)?;
    Some(Buff { kind, remaining })
}

fn encode_cmd(o: &mut Vec<u8>, c: &Cmd) {
    match c {
        Cmd::Move(t) => {
            wu8(o, 0);
            wvec(o, *t);
        }
        Cmd::Cast(id, t) => {
            wu8(o, 1);
            wu32(o, id.as_u32());
            wopt_vec(o, *t);
        }
        Cmd::Stop => wu8(o, 2),
    }
}
fn decode_cmd(b: &[u8], p: &mut usize) -> Option<Cmd> {
    match u8at(b, p)? {
        0 => Some(Cmd::Move(vecat(b, p)?)),
        1 => {
            let id = SkillId::from_u32(u32at(b, p)?);
            let t = opt_vec(b, p)?;
            Some(Cmd::Cast(id, t))
        }
        2 => Some(Cmd::Stop),
        _ => None,
    }
}

fn encode_player(o: &mut Vec<u8>, p: &Player) {
    wu32(o, p.id);
    wvec(o, p.pos);
    wfix(o, p.radius);
    wfix(o, p.hp);
    wfix(o, p.max_hp);
    // rewind（S006 时光回溯）：开关 + (pos, hp, remaining)
    match p.rewind {
        Some((pos, hp, rem)) => {
            wu8(o, 1);
            wvec(o, pos);
            wfix(o, hp);
            wfix(o, rem);
        }
        None => wu8(o, 0),
    }
    // catastrophe_stage（S020 灾变三级递进）
    wu8(o, p.catastrophe_stage);
    // 熔岩靴激活 CD（M5）
    wfix(o, p.lava_boot_cd);
    // 魔法张力 + 伤害成长（098c，D9 批次1）
    wf64(o, p.mana);
    wf64(o, p.growth);
    // 守护之盾充能（098c Ha）
    wu8(o, p.aegis_charged as u8);
    // 精通战斗快照（B1）：生命/远程/时间各 1 字节
    for m in &p.mastery {
        wu8(o, *m);
    }
    // 形态位（B4）：MAX_SKILL_SLOTS 字节（按 SkillId 索引）
    for f in &p.forms {
        wu8(o, *f as u8);
    }
    // 凤凰冲刺剩余（B4-R）
    wfix(o, p.phoenix_remaining);
    // 队伍号（B2，098c cn[]）
    wu8(o, p.team);
    // items（M3）：u8 数量 + u32 id（解码后重算 item_fx）
    wu8(o, p.items.len() as u8);
    for it in &p.items {
        wu32(o, it.as_u32());
    }
    wopt_vec(o, p.move_target);
    encode_caster(o, &p.caster);
    for lv in &p.skill_levels {
        wu32(o, *lv);
    }
    match p.last_hit_by {
        Some(h) => {
            wu8(o, 1);
            wu32(o, h);
        }
        None => wu8(o, 0),
    }
    match p.control {
        Some(c) => {
            wu8(o, 1);
            wvec(o, c.vel);
            wfix(o, c.remaining);
            wu8(o, c.decay as u8);
        }
        None => wu8(o, 0),
    }
    wvec(o, p.pull);
    wvec(o, p.cur_vel);
    for bf in &p.buffs {
        encode_buff(o, bf);
    }
    wopt_vec(o, p.shadow_anchor);
    wfix(o, p.shadow_window);
    match p.kick {
        Some(k) => {
            wu8(o, 1);
            wfix(o, k.push_power);
            wfix(o, k.push_time);
            wfix(o, k.push_damage);
            wfix(o, k.remaining);
            wu8(o, k.stop_on_hit as u8);
        }
        None => wu8(o, 0),
    }
    wfix(o, p.boost_soaked);
    wfix(o, p.s007_absorb);
    wfix(o, p.s007_bonus);
    match p.fake_active {
        Some(v) => {
            wu8(o, 1);
            wfix(o, v);
        }
        None => wu8(o, 0),
    }
    match p.blink2_window {
        Some(v) => {
            wu8(o, 1);
            wfix(o, v);
        }
        None => wu8(o, 0),
    }
    wu8(o, p.dash_active as u8);
    wvec(o, p.dash_vel);
    wu8(o, p.burning as u8);
    wu8(o, p.charging as u8);
    wu8(o, p.parry_ready as u8);
    wfix(o, p.parry_cd);
    match p.ricochet_pending {
        Some(v) => {
            wu8(o, 1);
            wfix(o, v);
        }
        None => wu8(o, 0),
    }
    match p.ricochet_kick {
        Some(k) => {
            wu8(o, 1);
            wfix(o, k.push_power);
            wfix(o, k.push_time);
            wfix(o, k.push_damage);
            wfix(o, k.remaining);
            wu8(o, k.stop_on_hit as u8);
        }
        None => wu8(o, 0),
    }
    wfix(o, p.ricochet_window);
    match p.sweep {
        Some(s) => {
            wu8(o, 1);
            wvec(o, s.dir);
            wfix(o, s.bullet_speed);
            wfix(o, s.damage);
            wu32(o, s.remaining);
            wf64(o, s.cadence);
            wf64(o, s.turn_step);
            wf64(o, s.elapsed);
            wu32(o, s.id);
        }
        None => wu8(o, 0),
    }
    wf64(o, p.damageplus);
    for c in &p.cmd_buf {
        encode_cmd(o, c);
    }
    wu32(o, p.cmd_head as u32);
    wu32(o, p.cmd_len as u32);
    wu8(o, p.alive as u8);
}

fn decode_player(b: &[u8], p: &mut usize, np: usize) -> Option<Player> {
    let id = u32at(b, p)?;
    // 下界防护（RISK_ANALYSIS.md D2）：玩家 id 直接作为 `players[id as usize]` 下标，
    // 在 step / record_death 等路径里被广泛使用（world.rs:738/746/1104/1199…）。
    // 损坏/恶意快照给出越界 id 会在后续逻辑里 OOB panic。合法快照的 id 一定 < np。
    if (id as usize) >= np {
        return None;
    }
    let pos = vecat(b, p)?;
    let radius = fixat(b, p)?;
    let hp = fixat(b, p)?;
    let max_hp = fixat(b, p)?;
    let rewind = if u8at(b, p)? != 0 {
        Some((vecat(b, p)?, fixat(b, p)?, fixat(b, p)?))
    } else {
        None
    };
    let catastrophe_stage = u8at(b, p)?;
    let lava_boot_cd = fixat(b, p)?;
    let mana = f64::from_bits(u64at(b, p)?);
    let growth = f64::from_bits(u64at(b, p)?);
    let aegis_charged = u8at(b, p)? != 0;
    let mastery = [u8at(b, p)?, u8at(b, p)?, u8at(b, p)?];
    let mut forms = [false; crate::MAX_SKILL_SLOTS];
    for f in forms.iter_mut() {
        *f = u8at(b, p)? != 0;
    }
    let phoenix_remaining = fixat(b, p)?;
    let team = u8at(b, p)?;
    let n_items = u8at(b, p)? as usize;
    let mut items = Vec::with_capacity(n_items);
    for _ in 0..n_items {
        items.push(crate::item::ItemId::from_u32(u32at(b, p)?)?);
    }
    let move_target = opt_vec(b, p)?;
    let mut caster = Caster::new();
    decode_caster(&mut caster, b, p)?;
    let mut skill_levels = [0u32; crate::MAX_SKILL_SLOTS];
    for lv in skill_levels.iter_mut() {
        *lv = u32at(b, p)?;
    }
    let last_hit_by = if u8at(b, p)? != 0 {
        let k = u32at(b, p)?;
        // 下界防护（同 id）：越界 killer id 会被 record_death 当作玩家下标/比较使用，
        // 且在下一帧被各端解码器判非法 → 重连/迁移失败或分叉。合法快照 k < np。
        if (k as usize) >= np {
            return None;
        }
        Some(k)
    } else {
        None
    };
    let control = if u8at(b, p)? != 0 {
        let vel = vecat(b, p)?;
        let remaining = fixat(b, p)?;
        let decay = u8at(b, p)? != 0;
        Some(Control { vel, remaining, decay })
    } else {
        None
    };
    let pull = vecat(b, p)?;
    let cur_vel = vecat(b, p)?;
    let mut buffs = [Buff::new(BuffKind::Speed(1.0), 0.0); crate::player::MAX_BUFFS];
    for bf in buffs.iter_mut() {
        *bf = decode_buff(b, p)?;
    }
    let shadow_anchor = opt_vec(b, p)?;
    let shadow_window = fixat(b, p)?;
    let kick = if u8at(b, p)? != 0 {
        Some(Kick {
            push_power: fixat(b, p)?,
            push_time: fixat(b, p)?,
            push_damage: fixat(b, p)?,
            remaining: fixat(b, p)?,
            stop_on_hit: u8at(b, p)? != 0,
        })
    } else {
        None
    };
    let boost_soaked = fixat(b, p)?;
    let s007_absorb = fixat(b, p)?;
    let s007_bonus = fixat(b, p)?;
    let fake_active = if u8at(b, p)? != 0 { Some(fixat(b, p)?) } else { None };
    let blink2_window = if u8at(b, p)? != 0 { Some(fixat(b, p)?) } else { None };
    let dash_active = u8at(b, p)? != 0;
    let dash_vel = vecat(b, p)?;
    let burning = u8at(b, p)? != 0;
    let charging = u8at(b, p)? != 0;
    let parry_ready = u8at(b, p)? != 0;
    let parry_cd = fixat(b, p)?;
    let ricochet_pending = if u8at(b, p)? != 0 { Some(fixat(b, p)?) } else { None };
    let ricochet_kick = if u8at(b, p)? != 0 {
        Some(Kick {
            push_power: fixat(b, p)?,
            push_time: fixat(b, p)?,
            push_damage: fixat(b, p)?,
            remaining: fixat(b, p)?,
            stop_on_hit: u8at(b, p)? != 0,
        })
    } else {
        None
    };
    let ricochet_window = fixat(b, p)?;
    let sweep = if u8at(b, p)? != 0 {
        Some(SweepState {
            dir: vecat(b, p)?,
            bullet_speed: fixat(b, p)?,
            damage: fixat(b, p)?,
            remaining: u32at(b, p)?,
            cadence: f64::from_bits(u64at(b, p)?),
            turn_step: f64::from_bits(u64at(b, p)?),
            elapsed: f64::from_bits(u64at(b, p)?),
            id: u32at(b, p)?,
        })
    } else {
        None
    };
    let damageplus = f64::from_bits(u64at(b, p)?);
    let mut cmd_buf = [Cmd::Stop; crate::player::MAX_CMDS];
    for c in cmd_buf.iter_mut() {
        *c = decode_cmd(b, p)?;
    }
    let cmd_head = u32at(b, p)? as usize;
    let cmd_len = u32at(b, p)? as usize;
    // 越界防护（RISK_ANALYSIS.md D2）：cmd_buf 只有 MAX_CMDS 个槽，
    // 损坏/恶意快照的越界 head/len 会让 Player::peek_cmd 的 cmd_buf[cmd_head] 越界 panic。
    if !cmd_indices_valid(cmd_head, cmd_len) {
        return None;
    }
    let alive = u8at(b, p)? != 0;

    let mut pl = Player::new(id, Vec2::ZERO, Fix64::ONE);
    pl.id = id;
    pl.pos = pos;
    pl.radius = radius;
    pl.hp = hp;
    pl.max_hp = max_hp;
    pl.rewind = rewind;
    pl.catastrophe_stage = catastrophe_stage;
    pl.lava_boot_cd = lava_boot_cd;
    pl.mana = mana;
    pl.growth = growth;
    pl.aegis_charged = aegis_charged;
    pl.mastery = mastery;
    pl.forms = forms;
    pl.phoenix_remaining = phoenix_remaining;
    pl.team = team;
    pl.items = items;
    pl.recompute_item_fx();
    pl.move_target = move_target;
    pl.caster = caster;
    pl.skill_levels = skill_levels;
    pl.last_hit_by = last_hit_by;
    pl.control = control;
    pl.pull = pull;
    pl.cur_vel = cur_vel;
    pl.buffs = buffs;
    pl.shadow_anchor = shadow_anchor;
    pl.shadow_window = shadow_window;
    pl.kick = kick;
    pl.boost_soaked = boost_soaked;
    pl.s007_absorb = s007_absorb;
    pl.s007_bonus = s007_bonus;
    pl.fake_active = fake_active;
    pl.blink2_window = blink2_window;
    pl.dash_active = dash_active;
    pl.dash_vel = dash_vel;
    pl.burning = burning;
    pl.charging = charging;
    pl.parry_ready = parry_ready;
    pl.parry_cd = parry_cd;
    pl.contact_by_enemy = None;
    pl.ricochet_pending = ricochet_pending;
    pl.ricochet_kick = ricochet_kick;
    pl.ricochet_window = ricochet_window;
    pl.sweep = sweep;
    pl.damageplus = damageplus;
    pl.cmd_buf = cmd_buf;
    pl.cmd_head = cmd_head;
    pl.cmd_len = cmd_len;
    pl.alive = alive;
    Some(pl)
}

fn encode_scatter(o: &mut Vec<u8>, s: &ScatterKind) {
    match s {
        ScatterKind::Burst { count, step_rad, bullet_speed } => {
            wu8(o, 0);
            wu32(o, *count);
            wfix(o, *step_rad);
            wfix(o, *bullet_speed);
        }
        ScatterKind::Periodic { count, interval, elapsed, bullet_speed, turn_rad } => {
            wu8(o, 1);
            wu32(o, *count);
            wfix(o, *interval);
            wfix(o, *elapsed);
            wfix(o, *bullet_speed);
            wfix(o, *turn_rad);
        }
    }
}
fn decode_scatter(b: &[u8], p: &mut usize) -> Option<ScatterKind> {
    match u8at(b, p)? {
        0 => Some(ScatterKind::Burst {
            count: u32at(b, p)?,
            step_rad: fixat(b, p)?,
            bullet_speed: fixat(b, p)?,
        }),
        1 => Some(ScatterKind::Periodic {
            count: u32at(b, p)?,
            interval: fixat(b, p)?,
            elapsed: fixat(b, p)?,
            bullet_speed: fixat(b, p)?,
            turn_rad: fixat(b, p)?,
        }),
        _ => None,
    }
}

use ProjectileKind as PK;
fn encode_projectile(o: &mut Vec<u8>, pr: &Projectile) {
    wu32(o, pr.owner);
    wu8(o, pr.alive as u8);
    wvec(o, pr.pos);
    match &pr.kind {
        PK::Rock { fuse, radius, damage, bomb_force } => { wu8(o, 0); wfix(o, *fuse); wfix(o, *radius); wfix(o, *damage); wfix(o, *bomb_force); }
        PK::Decoy { radius, lifetime } => { wu8(o, 1); wfix(o, *radius); wfix(o, *lifetime); }
        PK::Bullet { dir, speed, damage, radius, remaining } => { wu8(o, 2); wvec(o, *dir); wfix(o, *speed); wfix(o, *damage); wfix(o, *radius); wfix(o, *remaining); }
        PK::Missile { dir, speed, damage, radius, push_power, push_time, remaining } => { wu8(o, 3); wvec(o, *dir); wfix(o, *speed); wfix(o, *damage); wfix(o, *radius); wfix(o, *push_power); wfix(o, *push_time); wfix(o, *remaining); }
        PK::Boomerang { vel, accelerate, damage, radius, push_power, push_time, life, owner_pos } => { wu8(o, 4); wvec(o, *vel); wfix(o, *accelerate); wfix(o, *damage); wfix(o, *radius); wfix(o, *push_power); wfix(o, *push_time); wfix(o, *life); wvec(o, *owner_pos); }
        PK::Banana { dir, speed, turn, damage, radius, push_power, push_time, life } => { wu8(o, 5); wvec(o, *dir); wfix(o, *speed); wfix(o, *turn); wfix(o, *damage); wfix(o, *radius); wfix(o, *push_power); wfix(o, *push_time); wfix(o, *life); }
        PK::Rolling { dir, speed, damage_per_sec, radius, remaining } => { wu8(o, 6); wvec(o, *dir); wfix(o, *speed); wfix(o, *damage_per_sec); wfix(o, *radius); wfix(o, *remaining); }
        PK::ScatterLine { dir, speed, remaining, scatter } => { wu8(o, 7); wvec(o, *dir); wfix(o, *speed); wfix(o, *remaining); encode_scatter(o, scatter); }
        PK::Beam { dir, length, width, damage_per_sec, remaining } => { wu8(o, 8); wvec(o, *dir); wfix(o, *length); wfix(o, *width); wfix(o, *damage_per_sec); wfix(o, *remaining); }
        PK::Chain { dir, speed, damage, heal, ratio, ratio_decay, life, last_target, owner, max_chain, hit_count, turn_delay } => { wu8(o, 9); wvec(o, *dir); wfix(o, *speed); wfix(o, *damage); wfix(o, *heal); wfix(o, *ratio); wfix(o, *ratio_decay); wfix(o, *life); wu32(o, *last_target); wu32(o, *owner); wu32(o, *max_chain); wu32(o, *hit_count); wfix(o, *turn_delay); }
        PK::BonusBomb { dir, speed, damage, radius, push_power, push_time, remaining, owner } => { wu8(o, 10); wvec(o, *dir); wfix(o, *speed); wfix(o, *damage); wfix(o, *radius); wfix(o, *push_power); wfix(o, *push_time); wfix(o, *remaining); wu32(o, *owner); }
        PK::Returner { dir, speed, damage, radius, push_power, push_time, owner } => { wu8(o, 11); wvec(o, *dir); wfix(o, *speed); wfix(o, *damage); wfix(o, *radius); wfix(o, *push_power); wfix(o, *push_time); wu32(o, *owner); }
        PK::Tether { owner, target, damage_per_sec, pull_speed, remaining, beam } => { wu8(o, 12); wu32(o, *owner); wu32(o, *target); wfix(o, *damage_per_sec); wfix(o, *pull_speed); wfix(o, *remaining); wu8(o, *beam as u8); }
        PK::Gravity { dir, speed, radius, pull_speed, damage_per_sec, remaining } => { wu8(o, 13); wvec(o, *dir); wfix(o, *speed); wfix(o, *radius); wfix(o, *pull_speed); wfix(o, *damage_per_sec); wfix(o, *remaining); }
        PK::Star { owner, radius, damage_per_sec, heal_per_sec, remaining, heal_team } => { wu8(o, 14); wu32(o, *owner); wfix(o, *radius); wfix(o, *damage_per_sec); wfix(o, *heal_per_sec); wfix(o, *remaining); wu8(o, *heal_team as u8); }
        PK::BindLine { dir, speed, count, fired, bind_time, from, end } => { wu8(o, 15); wvec(o, *dir); wfix(o, *speed); wu32(o, *count); wu32(o, *fired); wfix(o, *bind_time); wvec(o, *from); wvec(o, *end); }
        PK::PushBullet { dir, speed, damage, radius, push_power, push_time, remaining } => { wu8(o, 16); wvec(o, *dir); wfix(o, *speed); wfix(o, *damage); wfix(o, *radius); wfix(o, *push_power); wfix(o, *push_time); wfix(o, *remaining); }
        PK::W098b { proj, vel, speed, radius, remaining, life, gx, kb_ji, ignite, blast, target, returning, on_hit, debuff_dur, lateral, forward_dir, out_dist, burst, emit_cooldown, emit_angle, pillar_bounce, pillar_rest, lightning_dmg } => {
            wu8(o, 17);
            wu8(o, match proj { crate::skill::W098bProjKind::Straight => 0, crate::skill::W098bProjKind::Homing => 1, crate::skill::W098bProjKind::Boomerang => 2, crate::skill::W098bProjKind::Bounce => 3, crate::skill::W098bProjKind::Magma => 4 });
            wvec(o, *vel); wfix(o, *speed); wfix(o, *radius); wfix(o, *remaining); wfix(o, *life); wfix(o, *gx); wfix(o, *kb_ji);
            wu8(o, ignite.is_some() as u8);
            if let Some(v) = ignite { wfix(o, *v); }
            wu8(o, blast.is_some() as u8);
            if let Some(v) = blast { wfix(o, *v); }
            wu32(o, target.unwrap_or(u32::MAX));
            wu8(o, *returning as u8);
            wfix(o, *lateral);
            wvec(o, *forward_dir);
            wfix(o, *out_dist);
            wu8(o, match on_hit { crate::skill::W098bOnHit::Ki => 0, crate::skill::W098bOnHit::Cripple => 1, crate::skill::W098bOnHit::ChainPull => 2, crate::skill::W098bOnHit::Scorched => 3, crate::skill::W098bOnHit::DrainSlow => 4, crate::skill::W098bOnHit::Weaken => 5, crate::skill::W098bOnHit::Recharge => 6, crate::skill::W098bOnHit::RedChain => 7, crate::skill::W098bOnHit::Silence => 8, crate::skill::W098bOnHit::SwapTarget => 9, crate::skill::W098bOnHit::CarrySelf => 10 });
            wfix(o, *debuff_dur);
            wu8(o, *burst);
            wfix(o, *emit_cooldown);
            wu64(o, emit_angle.to_bits());
            wu8(o, *pillar_bounce as u8);
            wfix(o, *pillar_rest);
            wfix(o, *lightning_dmg);
        }
        PK::Clone { owner, offset, fire_timer, fire_cd, fire_dmg, remaining } => {
            wu8(o, 18);
            wu32(o, *owner);
            wvec(o, *offset);
            wfix(o, *fire_timer);
            wfix(o, *fire_cd);
            wfix(o, *fire_dmg);
            wfix(o, *remaining);
        }
        PK::DelayedBlast { radius, damage, kb_ji, falloff_denom, remaining } => {
            wu8(o, 19);
            wfix(o, *radius);
            wfix(o, *damage);
            wfix(o, *kb_ji);
            wfix(o, *falloff_denom);
            wfix(o, *remaining);
        }
    }
}

fn decode_projectile(b: &[u8], p: &mut usize) -> Option<Projectile> {
    let owner = u32at(b, p)?;
    let alive = u8at(b, p)? != 0;
    let pos = vecat(b, p)?;
    let kind = match u8at(b, p)? {
        0 => PK::Rock { fuse: fixat(b, p)?, radius: fixat(b, p)?, damage: fixat(b, p)?, bomb_force: fixat(b, p)? },
        1 => PK::Decoy { radius: fixat(b, p)?, lifetime: fixat(b, p)? },
        2 => PK::Bullet { dir: vecat(b, p)?, speed: fixat(b, p)?, damage: fixat(b, p)?, radius: fixat(b, p)?, remaining: fixat(b, p)? },
        3 => PK::Missile { dir: vecat(b, p)?, speed: fixat(b, p)?, damage: fixat(b, p)?, radius: fixat(b, p)?, push_power: fixat(b, p)?, push_time: fixat(b, p)?, remaining: fixat(b, p)? },
        4 => PK::Boomerang { vel: vecat(b, p)?, accelerate: fixat(b, p)?, damage: fixat(b, p)?, radius: fixat(b, p)?, push_power: fixat(b, p)?, push_time: fixat(b, p)?, life: fixat(b, p)?, owner_pos: vecat(b, p)? },
        5 => PK::Banana { dir: vecat(b, p)?, speed: fixat(b, p)?, turn: fixat(b, p)?, damage: fixat(b, p)?, radius: fixat(b, p)?, push_power: fixat(b, p)?, push_time: fixat(b, p)?, life: fixat(b, p)? },
        6 => PK::Rolling { dir: vecat(b, p)?, speed: fixat(b, p)?, damage_per_sec: fixat(b, p)?, radius: fixat(b, p)?, remaining: fixat(b, p)? },
        7 => PK::ScatterLine { dir: vecat(b, p)?, speed: fixat(b, p)?, remaining: fixat(b, p)?, scatter: decode_scatter(b, p)? },
        8 => PK::Beam { dir: vecat(b, p)?, length: fixat(b, p)?, width: fixat(b, p)?, damage_per_sec: fixat(b, p)?, remaining: fixat(b, p)? },
        9 => PK::Chain { dir: vecat(b, p)?, speed: fixat(b, p)?, damage: fixat(b, p)?, heal: fixat(b, p)?, ratio: fixat(b, p)?, ratio_decay: fixat(b, p)?, life: fixat(b, p)?, last_target: u32at(b, p)?, owner: u32at(b, p)?, max_chain: u32at(b, p)?, hit_count: u32at(b, p)?, turn_delay: fixat(b, p)? },
        10 => PK::BonusBomb { dir: vecat(b, p)?, speed: fixat(b, p)?, damage: fixat(b, p)?, radius: fixat(b, p)?, push_power: fixat(b, p)?, push_time: fixat(b, p)?, remaining: fixat(b, p)?, owner: u32at(b, p)? },
        11 => PK::Returner { dir: vecat(b, p)?, speed: fixat(b, p)?, damage: fixat(b, p)?, radius: fixat(b, p)?, push_power: fixat(b, p)?, push_time: fixat(b, p)?, owner: u32at(b, p)? },
        12 => PK::Tether { owner: u32at(b, p)?, target: u32at(b, p)?, damage_per_sec: fixat(b, p)?, pull_speed: fixat(b, p)?, remaining: fixat(b, p)?, beam: u8at(b, p)? != 0 },
        13 => PK::Gravity { dir: vecat(b, p)?, speed: fixat(b, p)?, radius: fixat(b, p)?, pull_speed: fixat(b, p)?, damage_per_sec: fixat(b, p)?, remaining: fixat(b, p)? },
        14 => PK::Star { owner: u32at(b, p)?, radius: fixat(b, p)?, damage_per_sec: fixat(b, p)?, heal_per_sec: fixat(b, p)?, remaining: fixat(b, p)?, heal_team: u8at(b, p)? != 0 },
        15 => PK::BindLine { dir: vecat(b, p)?, speed: fixat(b, p)?, count: u32at(b, p)?, fired: u32at(b, p)?, bind_time: fixat(b, p)?, from: vecat(b, p)?, end: vecat(b, p)? },
        16 => PK::PushBullet { dir: vecat(b, p)?, speed: fixat(b, p)?, damage: fixat(b, p)?, radius: fixat(b, p)?, push_power: fixat(b, p)?, push_time: fixat(b, p)?, remaining: fixat(b, p)? },
        17 => {
            let proj = match u8at(b, p)? {
                0 => crate::skill::W098bProjKind::Straight,
                1 => crate::skill::W098bProjKind::Homing,
                2 => crate::skill::W098bProjKind::Boomerang,
                3 => crate::skill::W098bProjKind::Bounce,
                4 => crate::skill::W098bProjKind::Magma,
                _ => return None,
            };
            let vel = vecat(b, p)?;
            let speed = fixat(b, p)?;
            let radius = fixat(b, p)?;
            let remaining = fixat(b, p)?;
            let life = fixat(b, p)?;
            let gx = fixat(b, p)?;
            let kb_ji = fixat(b, p)?;
            let ignite = if u8at(b, p)? != 0 { Some(fixat(b, p)?) } else { None };
            let blast = if u8at(b, p)? != 0 { Some(fixat(b, p)?) } else { None };
            let tid = u32at(b, p)?;
            let target = if tid == u32::MAX { None } else { Some(tid) };
            let returning = u8at(b, p)? != 0;
            let lateral = fixat(b, p)?;
            let forward_dir = vecat(b, p)?;
            let out_dist = fixat(b, p)?;
            let on_hit = match u8at(b, p)? {
                0 => crate::skill::W098bOnHit::Ki,
                1 => crate::skill::W098bOnHit::Cripple,
                2 => crate::skill::W098bOnHit::ChainPull,
                3 => crate::skill::W098bOnHit::Scorched,
                4 => crate::skill::W098bOnHit::DrainSlow,
                5 => crate::skill::W098bOnHit::Weaken,
                6 => crate::skill::W098bOnHit::Recharge,
                7 => crate::skill::W098bOnHit::RedChain,
                8 => crate::skill::W098bOnHit::Silence,
        9 => crate::skill::W098bOnHit::SwapTarget,
        10 => crate::skill::W098bOnHit::CarrySelf,
                _ => return None,
            };
            let debuff_dur = fixat(b, p)?;
            let burst = u8at(b, p)?;
            let emit_cooldown = fixat(b, p)?;
            let emit_angle = f64::from_bits(u64at(b, p)?);
            let pillar_bounce = u8at(b, p)? != 0;
            let pillar_rest = fixat(b, p)?;
            let lightning_dmg = fixat(b, p)?;
            PK::W098b { proj, vel, speed, radius, remaining, life, gx, kb_ji, ignite, blast, target, returning, on_hit, debuff_dur, lateral, forward_dir, out_dist, burst, emit_cooldown, emit_angle, pillar_bounce, pillar_rest, lightning_dmg }
        }
        18 => PK::Clone {
            owner: u32at(b, p)?,
            offset: vecat(b, p)?,
            fire_timer: fixat(b, p)?,
            fire_cd: fixat(b, p)?,
            fire_dmg: fixat(b, p)?,
            remaining: fixat(b, p)?,
        },
        19 => PK::DelayedBlast { radius: fixat(b, p)?, damage: fixat(b, p)?, kb_ji: fixat(b, p)?, falloff_denom: fixat(b, p)?, remaining: fixat(b, p)? },
        _ => return None,
    };
    Some(Projectile { owner, kind, pos, alive })
}

/// 世界状态哈希（周期性帧同步分歧检测用）：对序列化字节做 FNV-1a 64。
/// 两端在相同输入下必须得到同一哈希；不一致即判定 desync。
/// 纯整数运算，跨平台确定（不含浮点/平台库）。
pub fn state_hash(w: &World) -> u64 {
    state_hash_bytes(&world_to_bytes(w))
}

/// 对**已序列化**的世界字节做 FNV-1a 哈希。
///
/// 供「同一帧既要发 hash 又要存/发快照」时复用一次 `world_to_bytes`（避免每 30 帧序列化两遍）。
/// `state_hash(w)` 即 `state_hash_bytes(&world_to_bytes(w))`（两者必须一致，有单测钉住）。
pub fn state_hash_bytes(bytes: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for &b in bytes {
        h ^= b as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}

/// World 序列化 / 反序列化。
pub fn world_to_bytes(w: &World) -> Vec<u8> {
    let mut o = Vec::new();
    wfix(&mut o, w.arena_radius);
    wu8(&mut o, w.sandbox as u8);
    wu64(&mut o, w.round_seed);
    wfix(&mut o, w.time);
    wu32(&mut o, w.players.len() as u32);
    for p in &w.players {
        encode_player(&mut o, p);
    }
    // 轮数（岩浆成长，D9）
    wu32(&mut o, w.round_number);
    // 注意：`lightning_visual`（闪电特效显示剩余时间）是纯渲染瞬态，仅 client 读、
    // 不参与确定性模拟，故**不写入**序列化字节——否则 state_hash 会把视觉瞬态也算进去，
    // 两端瞬态若有任一帧差异（哪怕模拟完全同步）就会误报 desync。
    // 冰面（U4 圆圈化）：u8 数量 + 每个（圆心+半径）
    wu8(&mut o, w.ice.len() as u8);
    for (c, r) in &w.ice {
        wvec(&mut o, *c);
        wfix(&mut o, *r);
    }
    // 模式/角色（B3）：mode + avatar + kings + f_override + round_forced
    wu8(&mut o, w.mode);
    match w.avatar {
        Some(id) => {
            wu8(&mut o, 1);
            wu32(&mut o, id);
        }
        None => wu8(&mut o, 0),
    }
    wu8(&mut o, w.kings.len() as u8);
    for k in &w.kings {
        wu32(&mut o, *k);
    }
    for f in &w.f_override {
        match f {
            Some(id) => {
                wu8(&mut o, 1);
                wu32(&mut o, id.as_u32());
            }
            None => wu8(&mut o, 0),
        }
    }
    wu8(&mut o, w.round_forced as u8);
    // 伤害矩阵（D6）：n×n Fix64（行主序）
    let n = w.players.len();
    for row in &w.damage_matrix {
        for v in row.iter().take(n) {
            wfix(&mut o, *v);
        }
    }
    // 化身累计伤害积分（模式 3）：n 个 Fix64
    for v in w.avatar_score.iter().take(n) {
        wfix(&mut o, *v);
    }
    wu32(&mut o, w.obstacles.len() as u32);
    for ob in &w.obstacles {
        wvec(&mut o, ob.pos);
        wfix(&mut o, ob.radius);
        wu32(&mut o, ob.hp);
    }
    wu32(&mut o, w.projectiles.len() as u32);
    for pr in &w.projectiles {
        encode_projectile(&mut o, pr);
    }
    wu32(&mut o, w.eliminated_order.len() as u32);
    for e in &w.eliminated_order {
        wu32(&mut o, *e);
    }
    wu32(&mut o, w.kills_this_round.len() as u32);
    for (k, v) in &w.kills_this_round {
        wu32(&mut o, *k);
        wu32(&mut o, *v);
    }
    // 缩圈倒计时 + 下轮角色（U5 同步修复）：这三个字段此前**从未编码**，解码端只能填
    // 硬编码默认值（10.0 / None / 空），于是每次快照 round-trip 后本端的缩圈节奏与
    // 化身/国王分配都被重置 → 与另一端永久分叉（Steam 双机不同步的根因）。
    wfix(&mut o, w.shrink_timer);
    match w.pending_avatar {
        Some(id) => {
            wu8(&mut o, 1);
            wu32(&mut o, id);
        }
        None => wu8(&mut o, 0),
    }
    wu8(&mut o, w.pending_kings.len() as u8);
    for k in &w.pending_kings {
        wu32(&mut o, *k);
    }
    o
}

pub fn world_from_bytes(b: &[u8]) -> Option<World> {
    let mut p = 0usize;
    let arena_radius = fixat(b, &mut p)?;
    let sandbox = u8at(b, &mut p)? != 0;
    let round_seed = u64at(b, &mut p)?;
    let time = fixat(b, &mut p)?;
    let np = count_at(b, &mut p, MAX_DECODE_PLAYERS)?;
    let mut players = Vec::with_capacity(np);
    for _ in 0..np {
        players.push(decode_player(b, &mut p, np)?);
    }
    // 轮数（岩浆成长，D9）
    let round_number = u32at(b, &mut p)?;
    // 闪电视觉段不参与序列化（见 world_to_bytes），解码端恒为空。
    let lightning_visual = Vec::new();
    let n_ice = u8at(b, &mut p)? as usize;
    let mut ice = Vec::with_capacity(n_ice);
    for _ in 0..n_ice {
        let c = vecat(b, &mut p)?;
        let r = fixat(b, &mut p)?;
        ice.push((c, r));
    }
    let mode = u8at(b, &mut p)?;
    let avatar = if u8at(b, &mut p)? == 1 { Some(u32at(b, &mut p)?) } else { None };
    let n_kings = u8at(b, &mut p)? as usize;
    let mut kings = Vec::with_capacity(n_kings);
    for _ in 0..n_kings {
        kings.push(u32at(b, &mut p)?);
    }
    let mut f_override = Vec::with_capacity(np);
    for _ in 0..np {
        let f = if u8at(b, &mut p)? == 1 {
            Some(SkillId::from_u32(u32at(b, &mut p)?))
        } else {
            None
        };
        f_override.push(f);
    }
    let round_forced = u8at(b, &mut p)? != 0;
    // 伤害矩阵（D6）：n×n Fix64，紧跟玩家（与编码顺序一致）
    let mut damage_matrix = vec![vec![Fix64::ZERO; np]; np];
    for row in damage_matrix.iter_mut() {
        for v in row.iter_mut() {
            *v = fixat(b, &mut p)?;
        }
    }
    // 化身累计伤害积分（模式 3）：n 个 Fix64
    let mut avatar_score = vec![Fix64::ZERO; np];
    for v in avatar_score.iter_mut() {
        *v = fixat(b, &mut p)?;
    }
    let no = count_at(b, &mut p, MAX_DECODE_OBSTACLES)?;
    let mut obstacles = Vec::with_capacity(no);
    for _ in 0..no {
        obstacles.push(Obstacle { pos: vecat(b, &mut p)?, radius: fixat(b, &mut p)?, hp: u32at(b, &mut p)? });
    }
    let npr = count_at(b, &mut p, MAX_DECODE_PROJECTILES)?;
    let mut projectiles = Vec::with_capacity(npr);
    for _ in 0..npr {
        projectiles.push(decode_projectile(b, &mut p)?);
    }
    // 下界防护（RISK_ANALYSIS.md D2）：eliminated_order / kills_this_round 里的 id
    // 同样作为 `players[id as usize]` 下标（world.rs:738-740 record_death）。
    // 损坏/恶意快照给出越界 id 会在结算名次/击杀赏金时 OOB panic，故需 < np。
    let ne = count_at(b, &mut p, MAX_DECODE_ELIMINATED)?;
    let mut eliminated_order = Vec::with_capacity(ne);
    for _ in 0..ne {
        let e = u32at(b, &mut p)?;
        if (e as usize) >= np {
            return None;
        }
        eliminated_order.push(e);
    }
    let nk = count_at(b, &mut p, MAX_DECODE_KILLS)?;
    let mut kills_this_round = Vec::with_capacity(nk);
    for _ in 0..nk {
        let k = u32at(b, &mut p)?;
        let v = u32at(b, &mut p)?;
        if (k as usize) >= np || (v as usize) >= np {
            return None;
        }
        kills_this_round.push((k, v));
    }
    // 缩圈倒计时 + 下轮角色（U5 同步修复；顺序必须与 `world_to_bytes` 一致）。
    // 化身/国王 id 与 eliminated/kills 同样作为玩家下标使用，越界一律拒绝（D2 下界防护）。
    let shrink_timer = fixat(b, &mut p)?;
    let pending_avatar = if u8at(b, &mut p)? == 1 {
        let id = u32at(b, &mut p)?;
        if (id as usize) >= np {
            return None;
        }
        Some(id)
    } else {
        None
    };
    let n_pk = u8at(b, &mut p)? as usize;
    let mut pending_kings = Vec::with_capacity(n_pk);
    for _ in 0..n_pk {
        let k = u32at(b, &mut p)?;
        if (k as usize) >= np {
            return None;
        }
        pending_kings.push(k);
    }
    // 倍率（房间设置）不是世界状态，快照不含；重建后用 `configure_mults` 重新对齐（默认 1.0）。
    Some(World { players, arena_radius, base_regen: crate::balance::Balance::default().hp_regen, shrink_delay_secs: Fix64::from_num(crate::balance::Balance::default().shrink_delay_secs), shrink_total_secs: Fix64::from_num(crate::world::DEFAULT_SHRINK_TOTAL_SECS), shrink_ref_radius: arena_radius, damage_mult: Fix64::ONE, knockback_mult: Fix64::ONE, lava_damage_mult: Fix64::ONE, pillar_mode: 1, ice_mode: 1, sandbox, round_seed, obstacles, projectiles, eliminated_order, kills_this_round, round_number, damage_matrix, avatar_score, time, lightning_visual, mode, avatar, kings, f_override, round_forced, pending_avatar, pending_kings, shrink_timer, ice, combat_events: Vec::new() })
}

/// 序列化用的便捷接口：`World::to_bytes` / `from_bytes`（依赖本模块）。
pub struct SerializeWorld;
impl SerializeWorld {
    pub fn encode(w: &World) -> Vec<u8> {
        world_to_bytes(w)
    }
    pub fn decode(b: &[u8]) -> Option<World> {
        world_from_bytes(b)
    }
}

#[cfg(test)]
mod tests {
    use crate::fix::{Fix64, Vec2};
    use crate::world::World;
    use super::*;

    #[test]
    fn world_roundtrip_preserves_characters() {
        let mut w = World::new(3, 99);
        let dt = Fix64::from_num(1.0 / 60.0);
        let none = vec![crate::world::PlayerInput::default(); 3];
        // 璺戝嚑甯у埗閫犱竴浜涚姸鎬?
        for _ in 0..20 {
            w.step(none.clone(), dt);
        }
        w.players[0].dash_active = true;
        w.players[0].shadow_anchor = Some(Vec2::new(Fix64::ONE, Fix64::from_num(2.0)));
        // U5：把此前「漏编码」或「已编码但漏断言」的字段全部置为非默认值，
        // 确保 round-trip 真的保真（填默认值会让断言失去意义）。
        w.mode = 3;
        w.avatar = Some(1);
        w.kings = vec![0, 2];
        w.f_override = vec![None, Some(crate::skill::SkillId::S020), None];
        w.round_forced = true;
        w.round_number = 4;
        w.pending_avatar = Some(2);
        w.pending_kings = vec![1];
        w.shrink_timer = Fix64::from_num(7.5);
        let bytes = world_to_bytes(&w);
        let back = world_from_bytes(&bytes).expect("decode");

        assert_eq!(w.arena_radius, back.arena_radius);
        assert_eq!(w.sandbox, back.sandbox);
        assert_eq!(w.round_seed, back.round_seed, "round_seed equal");
        assert_eq!(w.time, back.time);
        assert_eq!(w.round_number, back.round_number, "轮数（岩浆成长）equal");
        assert_eq!(w.players, back.players, "players equal");
        assert_eq!(w.obstacles, back.obstacles, "obstacles equal");
        assert_eq!(w.projectiles, back.projectiles, "projectiles equal");
        assert_eq!(w.eliminated_order, back.eliminated_order);
        assert_eq!(w.kills_this_round, back.kills_this_round);
        assert_eq!(w.damage_matrix, back.damage_matrix, "伤害矩阵 equal");
        // lightning_visual 是瞬态渲染痕迹，不参与序列化（见 lightning_visual_not_serialized 测试）。
        assert_eq!(w.ice, back.ice, "冰面 equal");
        // 模式/角色（B3）
        assert_eq!(w.mode, back.mode);
        assert_eq!(w.avatar, back.avatar);
        assert_eq!(w.kings, back.kings);
        assert_eq!(w.f_override, back.f_override, "F 槽替换 equal");
        assert_eq!(w.round_forced, back.round_forced);
        // U5 同步修复：这三个此前从未进入字节流，解码端只能填硬编码默认值。
        assert_eq!(w.shrink_timer, back.shrink_timer, "缩圈倒计时必须随快照同步");
        assert_eq!(w.pending_avatar, back.pending_avatar, "下轮化身必须随快照同步");
        assert_eq!(w.pending_kings, back.pending_kings, "下轮国王必须随快照同步");
    }

    /// 闪电视觉段（lightning_visual）是纯渲染瞬态：不参与序列化/快照/state_hash。
    /// 回归：它若被写进 world_to_bytes，state_hash 会把「显示剩余时间」这种视觉瞬态也算进去，
    /// 两端瞬态任一帧不一致（哪怕模拟完全同步）就会误报 desync。
    #[test]
    fn lightning_visual_not_serialized() {
        let mut w = World::new(2, 7);
        w.lightning_visual.push((
            Vec2::ZERO,
            Vec2::new(Fix64::ONE, Fix64::ONE),
            Fix64::from_num(0.05),
        ));
        let bytes = world_to_bytes(&w);
        let back = world_from_bytes(&bytes).expect("decode");
        assert!(back.lightning_visual.is_empty(), "闪电视觉是瞬态，不应进入快照/哈希");

        // 相同模拟状态下，带/不带瞬态视觉的 state_hash 必须一致。
        let h_with = state_hash(&w);
        let mut w_clean = back;
        w_clean.lightning_visual.clear();
        assert_eq!(state_hash(&w_clean), h_with, "瞬态视觉不得影响 state_hash");
    }

    /// 回归 U5：快照 round-trip 后，本端与「对端」继续跑相同帧，**缩圈进度必须仍一致**。
    ///
    /// 修复前 `shrink_timer` 解码时被硬编码成 10.0，导致对端缩圈倒计时被续命、永远不缩
    /// （而本端正常缩）→ 双机「一方没缩圈」。本测试直接复现该分叉。
    #[test]
    fn roundtrip_preserves_shrink_progress_across_peers() {
        let mut a = World::new(3, 7);
        let dt = Fix64::from_num(1.0 / 60.0);
        let none = vec![crate::world::PlayerInput::default(); 3];
        for _ in 0..10 {
            a.step(none.clone(), dt);
        }
        // 让缩圈在 0.5 秒后开始（否则默认约 17 秒，测试要跑上千帧）。
        a.shrink_timer = Fix64::from_num(0.5);
        let radius_before = a.arena_radius;
        // 模拟对端：仅通过快照重建世界（重连 / host 迁移 / 接管走的就是这条路）。
        let mut b = world_from_bytes(&world_to_bytes(&a)).expect("decode");
        // 两端各跑 2 秒（120 帧），期间缩圈应已启动。
        for _ in 0..120 {
            a.step(none.clone(), dt);
            b.step(none.clone(), dt);
        }
        assert!(a.arena_radius < radius_before, "前置：本端 2 秒内应已开始缩圈");
        assert_eq!(a.arena_radius, b.arena_radius, "快照重建端缩圈进度必须与本端一致");
        assert_eq!(a.shrink_timer, b.shrink_timer, "两端缩圈倒计时必须一致");
    }

    #[test]
    fn decode_rejects_absurd_counts_instead_of_huge_allocation() {
        // 回归 P4：count=0xFFFFFFFF 的小包若直接喂给 Vec::with_capacity，
        // 会申请数十 GB 后 abort（OOM / 远程 DoS），必须被拒绝。
        let huge = 0xFFFF_FFFFu32.to_be_bytes();
        let mut p = 0;
        assert_eq!(count_at(&huge, &mut p, MAX_DECODE_PLAYERS), None);
        let ok = 3u32.to_be_bytes();
        let mut p2 = 0;
        assert_eq!(count_at(&ok, &mut p2, MAX_DECODE_PLAYERS), Some(3));
    }

    #[test]
    fn decode_rejects_out_of_range_cmd_indices() {
        // 回归 D2：cmd_buf 只有 MAX_CMDS 个槽，越界 head/len 会让 cmd_buf[cmd_head] panic。
        assert!(cmd_indices_valid(0, 0));
        assert!(cmd_indices_valid(MAX_CMDS - 1, MAX_CMDS));
        assert!(!cmd_indices_valid(MAX_CMDS, 0), "head 越界应拒绝");
        assert!(!cmd_indices_valid(0, MAX_CMDS + 1), "len 越界应拒绝");
    }

    /// 状态哈希（分歧检测）：相同世界 → 相同哈希；不同世界 → 不同哈希。
    #[test]
    fn state_hash_is_deterministic_and_sensitive() {
        let a = World::new(3, 99);
        let b = World::new(3, 99);
        assert_eq!(state_hash(&a), state_hash(&b), "相同世界必须同哈希（两端可据此判定一致）");
        let c = World::new(3, 1234);
        assert_ne!(state_hash(&a), state_hash(&c), "不同世界应得不同哈希");
        // `state_hash` 与「先序列化再 state_hash_bytes」必须一致（host 每帧复用一份字节）。
        assert_eq!(state_hash(&a), state_hash_bytes(&world_to_bytes(&a)));
    }

    #[test]
    fn world_from_bytes_rejects_absurd_player_count() {
        // 端到端：把合法快照里的玩家数改成 0xFFFFFFFF，必须被拒绝而非巨额分配。
        let w = World::new(3, 99);
        let mut bytes = world_to_bytes(&w);
        // 头部：arena_radius(8) + sandbox(1) + round_seed(8) + time(8) = 25，紧接玩家数 u32
        bytes[25..29].copy_from_slice(&0xFFFF_FFFFu32.to_be_bytes());
        assert!(world_from_bytes(&bytes).is_none(), "超大玩家数必须被拒绝");
    }

    #[test]
    fn world_from_bytes_rejects_out_of_range_player_id() {
        // 回归 D2（下界）：玩家 id 直接作 `players[id as usize]` 下标，
        // 越界 id 应在解码期被拒绝，而非在 step 里 OOB panic。
        let w = World::new(3, 99);
        let mut bytes = world_to_bytes(&w);
        // 头部(25) + 玩家数 u32(4) = 29，紧接第一个玩家 id。改成 0xFFFFFFFF（>= np=3）。
        bytes[29..33].copy_from_slice(&0xFFFF_FFFFu32.to_be_bytes());
        assert!(world_from_bytes(&bytes).is_none(), "越界玩家 id 必须被拒绝");
    }

    #[test]
    fn world_from_bytes_rejects_out_of_range_eliminated_id() {
        // 回归 D2（下界）：eliminated_order 里的 id 也作 players[id] 下标，必须 < np。
        let mut w = World::new(2, 99);
        w.eliminated_order.push(0xFFFF_FFFF); // >= np=2，非法
        let bytes = world_to_bytes(&w);
        assert!(world_from_bytes(&bytes).is_none(), "越界 eliminated_order id 必须被拒绝");
    }

    #[test]
    fn world_from_bytes_rejects_out_of_range_kill_id() {
        // 回归 D2（下界）：kills_this_round 的 (击杀者, 被击杀者) 都作 players[id] 下标，必须 < np。
        let mut w = World::new(2, 99);
        w.kills_this_round.push((0xFFFF_FFFF, 1)); // 击杀者越界
        let bytes = world_to_bytes(&w);
        assert!(world_from_bytes(&bytes).is_none(), "越界击杀者 id 必须被拒绝");

        let mut w2 = World::new(2, 99);
        w2.kills_this_round.push((0, 0xFFFF_FFFF)); // 被击杀者越界
        let bytes2 = world_to_bytes(&w2);
        assert!(world_from_bytes(&bytes2).is_none(), "越界被击杀者 id 必须被拒绝");
    }
}
