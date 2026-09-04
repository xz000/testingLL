//! 纭畾鎬?World 搴忓垪鍖栵紙閲嶈繛蹇収 / 瀛樻。鐢級銆?
//!
//! 绾墜鍐欍€佸ぇ绔€侀暱搴﹀墠缂€锛岄€愬瓧娈佃鐩?World锛堝惈 Player / Caster / Projectile / Obstacle 绛夛級锛?
//! 淇濊瘉 `to_bytes` 鈫?`from_bytes` 鍚庨€愪綅涓€鑷达紝渚涢噸杩炵閲嶅缓鏁村満 World 鍚庣户缁?lockstep銆?

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
    wf64(o, p.speed_mult);
    wf64(o, p.armor_factor);
    wf64(o, p.spell_factor);
    wf64(o, p.kb_factor);
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
        }
        None => wu8(o, 0),
    }
    wfix(o, p.boost_soaked);
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
    let speed_mult = f64::from_bits(u64at(b, p)?);
    let armor_factor = f64::from_bits(u64at(b, p)?);
    let spell_factor = f64::from_bits(u64at(b, p)?);
    let kb_factor = f64::from_bits(u64at(b, p)?);
    let rewind = if u8at(b, p)? != 0 {
        Some((vecat(b, p)?, fixat(b, p)?, fixat(b, p)?))
    } else {
        None
    };
    let catastrophe_stage = u8at(b, p)?;
    let move_target = opt_vec(b, p)?;
    let mut caster = Caster::new();
    decode_caster(&mut caster, b, p)?;
    let mut skill_levels = [0u32; crate::MAX_SKILL_SLOTS];
    for lv in skill_levels.iter_mut() {
        *lv = u32at(b, p)?;
    }
    let last_hit_by = if u8at(b, p)? != 0 { Some(u32at(b, p)?) } else { None };
    let control = if u8at(b, p)? != 0 {
        let vel = vecat(b, p)?;
        let remaining = fixat(b, p)?;
        Some(Control { vel, remaining })
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
        })
    } else {
        None
    };
    let boost_soaked = fixat(b, p)?;
    let fake_active = if u8at(b, p)? != 0 { Some(fixat(b, p)?) } else { None };
    let blink2_window = if u8at(b, p)? != 0 { Some(fixat(b, p)?) } else { None };
    let dash_active = u8at(b, p)? != 0;
    let dash_vel = vecat(b, p)?;
    let ricochet_pending = if u8at(b, p)? != 0 { Some(fixat(b, p)?) } else { None };
    let ricochet_kick = if u8at(b, p)? != 0 {
        Some(Kick {
            push_power: fixat(b, p)?,
            push_time: fixat(b, p)?,
            push_damage: fixat(b, p)?,
            remaining: fixat(b, p)?,
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
    pl.speed_mult = speed_mult;
    pl.armor_factor = armor_factor;
    pl.spell_factor = spell_factor;
    pl.kb_factor = kb_factor;
    pl.rewind = rewind;
    pl.catastrophe_stage = catastrophe_stage;
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
    pl.fake_active = fake_active;
    pl.blink2_window = blink2_window;
    pl.dash_active = dash_active;
    pl.dash_vel = dash_vel;
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
        PK::Gravity { dir, speed, radius, pull_speed, remaining } => { wu8(o, 13); wvec(o, *dir); wfix(o, *speed); wfix(o, *radius); wfix(o, *pull_speed); wfix(o, *remaining); }
        PK::Star { owner, radius, damage_per_sec, heal_per_sec, remaining } => { wu8(o, 14); wu32(o, *owner); wfix(o, *radius); wfix(o, *damage_per_sec); wfix(o, *heal_per_sec); wfix(o, *remaining); }
        PK::BindLine { dir, speed, count, fired, bind_time, from, end } => { wu8(o, 15); wvec(o, *dir); wfix(o, *speed); wu32(o, *count); wu32(o, *fired); wfix(o, *bind_time); wvec(o, *from); wvec(o, *end); }
        PK::PushBullet { dir, speed, damage, radius, push_power, push_time, remaining } => { wu8(o, 16); wvec(o, *dir); wfix(o, *speed); wfix(o, *damage); wfix(o, *radius); wfix(o, *push_power); wfix(o, *push_time); wfix(o, *remaining); }
        PK::W098b { proj, vel, speed, radius, remaining, life, gx, kb_ji, ignite, blast, target, returning, on_hit, debuff_dur } => {
            wu8(o, 17);
            wu8(o, match proj { crate::skill::W098bProjKind::Straight => 0, crate::skill::W098bProjKind::Homing => 1, crate::skill::W098bProjKind::Boomerang => 2, crate::skill::W098bProjKind::Bounce => 3 });
            wvec(o, *vel); wfix(o, *speed); wfix(o, *radius); wfix(o, *remaining); wfix(o, *life); wfix(o, *gx); wfix(o, *kb_ji);
            wu8(o, ignite.is_some() as u8);
            if let Some(v) = ignite { wfix(o, *v); }
            wu8(o, blast.is_some() as u8);
            if let Some(v) = blast { wfix(o, *v); }
            wu32(o, target.unwrap_or(u32::MAX));
            wu8(o, *returning as u8);
            wu8(o, match on_hit { crate::skill::W098bOnHit::Ki => 0, crate::skill::W098bOnHit::Cripple => 1, crate::skill::W098bOnHit::ChainPull => 2 });
            wfix(o, *debuff_dur);
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
        13 => PK::Gravity { dir: vecat(b, p)?, speed: fixat(b, p)?, radius: fixat(b, p)?, pull_speed: fixat(b, p)?, remaining: fixat(b, p)? },
        14 => PK::Star { owner: u32at(b, p)?, radius: fixat(b, p)?, damage_per_sec: fixat(b, p)?, heal_per_sec: fixat(b, p)?, remaining: fixat(b, p)? },
        15 => PK::BindLine { dir: vecat(b, p)?, speed: fixat(b, p)?, count: u32at(b, p)?, fired: u32at(b, p)?, bind_time: fixat(b, p)?, from: vecat(b, p)?, end: vecat(b, p)? },
        16 => PK::PushBullet { dir: vecat(b, p)?, speed: fixat(b, p)?, damage: fixat(b, p)?, radius: fixat(b, p)?, push_power: fixat(b, p)?, push_time: fixat(b, p)?, remaining: fixat(b, p)? },
        17 => {
            let proj = match u8at(b, p)? {
                0 => crate::skill::W098bProjKind::Straight,
                1 => crate::skill::W098bProjKind::Homing,
                2 => crate::skill::W098bProjKind::Boomerang,
                3 => crate::skill::W098bProjKind::Bounce,
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
            let on_hit = match u8at(b, p)? {
                0 => crate::skill::W098bOnHit::Ki,
                1 => crate::skill::W098bOnHit::Cripple,
                2 => crate::skill::W098bOnHit::ChainPull,
                _ => return None,
            };
            let debuff_dur = fixat(b, p)?;
            PK::W098b { proj, vel, speed, radius, remaining, life, gx, kb_ji, ignite, blast, target, returning, on_hit, debuff_dur }
        }
        _ => return None,
    };
    Some(Projectile { owner, kind, pos, alive })
}

/// World 搴忓垪鍖?鍙嶅簭鍒楀寲銆?
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
    wu32(&mut o, w.obstacles.len() as u32);
    for ob in &w.obstacles {
        wvec(&mut o, ob.pos);
        wfix(&mut o, ob.radius);
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
    let no = count_at(b, &mut p, MAX_DECODE_OBSTACLES)?;
    let mut obstacles = Vec::with_capacity(no);
    for _ in 0..no {
        obstacles.push(Obstacle { pos: vecat(b, &mut p)?, radius: fixat(b, &mut p)? });
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
    Some(World { players, arena_radius, sandbox, round_seed, obstacles, projectiles, eliminated_order, kills_this_round, time, lightning_visual: None })
}

/// 搴忓垪鍖栫敤鐨勪究鎹锋帴鍙ｏ細`World::to_bytes` / `from_bytes`锛堜緷璧栨湰妯″潡锛夈€?
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
        let bytes = world_to_bytes(&w);
        let back = world_from_bytes(&bytes).expect("decode");

        assert_eq!(w.arena_radius, back.arena_radius);
        assert_eq!(w.sandbox, back.sandbox);
        assert_eq!(w.time, back.time);
        assert_eq!(w.players, back.players, "players equal");
        assert_eq!(w.obstacles, back.obstacles, "obstacles equal");
        assert_eq!(w.projectiles, back.projectiles, "projectiles equal");
        assert_eq!(w.eliminated_order, back.eliminated_order);
        assert_eq!(w.kills_this_round, back.kills_this_round);
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
