//! 导出我方技能数值表（TSV），用于与 098c 的 w3a tooltip/`acdn` 做系统性交叉校验。
//!
//! 用法：`cargo run -q -p game-core --example dump_defs > ours.tsv`
//!
//! 注意：`SkillId::as_u32()` 返回的是 **enum 判别值**（与数字后缀无关），
//! 故这里显式列出 `(数字后缀, from_u32 可接受值)` 表。

use game_core::skill::{DefTable, SkillId};

/// (数字后缀, as_u32 判别值)
const IDS: &[(u32, u32)] = &[
    (0, 36), (1, 55), (2, 39), (3, 37), (4, 38), (5, 45), (6, 46), (7, 47),
    (8, 40), (9, 41), (10, 48), (11, 49), (12, 50), (13, 51), (14, 42), (15, 43),
    (16, 44), (17, 52), (18, 53), (19, 54), (20, 56), (21, 57), (22, 69), (23, 70),
    (24, 58), (25, 59), (26, 60), (27, 61), (30, 62), (31, 63), (32, 64), (33, 65),
    (34, 66), (35, 67), (36, 68),
];

fn main() {
    println!("skill\tform\tlevel\tcooldown\tdamage\trange\tmax_distance\tduration\tradius\tspeed\tpush_damage");
    for &(suffix, disc) in IDS {
        let id = SkillId::from_u32(disc);
        for alt in [false, true] {
            let d = DefTable::def_for(id, alt);
            for lv in 1..=10u32 {
                let s = d.stats_at(lv);
                println!(
                    "S{:03}\t{}\t{}\t{:.3}\t{:.3}\t{:.3}\t{:.3}\t{:.3}\t{:.3}\t{:.3}\t{:.3}",
                    suffix,
                    if alt { "B" } else { "A" },
                    lv,
                    s.cooldown.to_num::<f64>(),
                    s.damage.to_num::<f64>(),
                    s.range.to_num::<f64>(),
                    s.max_distance.to_num::<f64>(),
                    s.duration.to_num::<f64>(),
                    s.radius.to_num::<f64>(),
                    s.speed.to_num::<f64>(),
                    s.push_damage.to_num::<f64>(),
                );
            }
        }
    }
}
