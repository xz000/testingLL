//! 战绩 → Steam 统计 / 成就 / 排行榜 的**纯规则层**（不依赖 steamworks，默认也能编译与单测）。
//!
//! 为什么要分两层：
//! - 本文件只决定「一场打成什么样 → 该记什么统计、该解锁哪些成就、排行榜算多少分」，
//!   是纯函数、确定性、可无 Steam 单测（锁死门槛，防止以后调数值时悄悄改坏）。
//! - 真正写 Steam 的部分在 `session.rs`（`record_match_result` 等），它会**失败即忽略**：
//!   统计/成就/排行榜都需要在 Steamworks 后台先定义对应的 key，没配置时 `set_stat/set_achievement`
//!   会失败，游戏不该因此报错，只打一条日志。
//!
//! ⚠ 后台需配置的 key（与本文件常量一致）：
//!   统计：`STAT_MATCHES` / `STAT_WINS` / `STAT_KILLS`
//!   成就：`ACH_FIRST_WIN` / `ACH_KILL_5` / `ACH_KILL_10` / `ACH_FLAWLESS`
//!   排行榜：`arena_best_score`

/// 一整场（多小局）打完后的战绩摘要。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MatchSummary {
    /// 本场总击杀。
    pub kills: u32,
    /// 本场最佳名次（1 = 冠军；0 = 未产生名次，例如直接退出）。
    pub best_placement: u32,
    /// 本场参与人数（含自己）。
    pub players: u32,
    /// 本场打了几个小局。
    pub rounds: u32,
    /// 本场存活过的小局数（一次没死=全场存活）。
    pub rounds_survived: u32,
}

/// 统计键：累计场次。
pub const STAT_MATCHES: &str = "STAT_MATCHES";
/// 统计键：累计胜场（拿到过第 1 名）。
pub const STAT_WINS: &str = "STAT_WINS";
/// 统计键：累计击杀。
pub const STAT_KILLS: &str = "STAT_KILLS";

/// 成就键：首胜（多人对局中拿到一次第 1 名）。
pub const ACH_FIRST_WIN: &str = "ACH_FIRST_WIN";
/// 成就键：单场 5 杀。
pub const ACH_KILL_5: &str = "ACH_KILL_5";
/// 成就键：单场 10 杀。
pub const ACH_KILL_10: &str = "ACH_KILL_10";
/// 成就键：全场无死亡（打满且每局都活下来）。
pub const ACH_FLAWLESS: &str = "ACH_FLAWLESS";

/// 排行榜名称（后台需按此名建榜；计分用 [`leaderboard_score`]）。
pub const LEADERBOARD: &str = "arena_best_score";

impl MatchSummary {
    /// 是否算“赢了一场”：多人对局（≥2 人）且拿到过第 1 名。
    /// 单人试验场不算胜场（否则刷试验场就能刷成就）。
    pub fn won(&self) -> bool {
        self.players >= 2 && self.best_placement == 1
    }

    /// 是否“全场无死亡”：至少打了一局，且每局都活到最后。
    pub fn flawless(&self) -> bool {
        self.rounds >= 1 && self.rounds_survived >= self.rounds
    }
}

/// 本场应解锁的成就键（按固定顺序，便于测试与展示）。
pub fn achievements_for(s: MatchSummary) -> Vec<&'static str> {
    let mut out = Vec::new();
    if s.won() {
        out.push(ACH_FIRST_WIN);
    }
    if s.kills >= 5 {
        out.push(ACH_KILL_5);
    }
    if s.kills >= 10 {
        out.push(ACH_KILL_10);
    }
    if s.flawless() {
        out.push(ACH_FLAWLESS);
    }
    out
}

/// 成就的中文展示名（后台没配本地化时用它；也避免界面显示原始 key 这种英文串）。
pub fn achievement_label(key: &str) -> &'static str {
    match key {
        ACH_FIRST_WIN => "首次夺冠",
        ACH_KILL_5 => "单场 5 杀",
        ACH_KILL_10 => "单场 10 杀",
        ACH_FLAWLESS => "全场无死亡",
        _ => "新成就",
    }
}

/// 排行榜计分：击杀为主（每杀 100 分），名次为辅（名次越高加成越大，冠军额外 +500）。
/// 纯函数、两端一致；同分时先上榜者在前（Steam 侧 KeepBest 语义）。
pub fn leaderboard_score(s: MatchSummary) -> i32 {
    let placement_bonus = match s.best_placement {
        1 => 500,
        2 => 250,
        3 => 120,
        4 => 60,
        _ => 0,
    };
    s.kills as i32 * 100 + placement_bonus
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 默认「打了 3 局、只活下来 1 局」→ 不满足无死亡成就，其它用例不受干扰。
    fn sum(kills: u32, best_placement: u32, players: u32) -> MatchSummary {
        MatchSummary { kills, best_placement, players, rounds: 3, rounds_survived: 1 }
    }

    #[test]
    fn win_requires_at_least_two_players() {
        // 单人试验场拿第一不算胜场（否则刷试验场就能刷成就）。
        assert!(!sum(9, 1, 1).won());
        assert!(sum(9, 1, 2).won());
        assert!(!sum(9, 2, 4).won(), "第二名不算赢");
    }

    #[test]
    fn achievements_unlock_at_thresholds() {
        // 4 杀：还不够 5 杀门槛，且是第二名 → 一个都不解锁。
        assert_eq!(achievements_for(sum(4, 2, 4)), Vec::<&str>::new());
        // 5 杀第二 → 只有 5 杀。
        assert_eq!(achievements_for(sum(5, 2, 4)), vec![ACH_KILL_5]);
        // 10 杀 + 夺冠 → 首胜 + 5 杀 + 10 杀（顺序固定，便于展示）。
        assert_eq!(
            achievements_for(sum(10, 1, 4)),
            vec![ACH_FIRST_WIN, ACH_KILL_5, ACH_KILL_10]
        );
    }

    #[test]
    fn flawless_needs_every_round_survived() {
        let mut s = sum(0, 1, 2);
        s.rounds = 3;
        s.rounds_survived = 3;
        assert!(s.flawless());
        s.rounds_survived = 2; // 死过一局 → 不算
        assert!(!s.flawless());
        s.rounds = 0; // 没打过也不算（避免空对局刷成就）
        s.rounds_survived = 0;
        s.best_placement = 0; // 同时不该算胜场
        assert!(!s.flawless());
        assert_eq!(achievements_for(s), Vec::<&str>::new());
    }

    #[test]
    fn leaderboard_score_rewards_kills_and_placement() {
        assert_eq!(leaderboard_score(sum(0, 1, 4)), 500, "冠军加成");
        assert_eq!(leaderboard_score(sum(3, 1, 4)), 800, "3 杀 + 冠军");
        assert_eq!(leaderboard_score(sum(3, 2, 4)), 550, "3 杀 + 亚军");
        assert_eq!(leaderboard_score(sum(0, 9, 4)), 0, "没名次没击杀 = 0 分");
    }

    #[test]
    fn achievement_label_falls_back_for_unknown_key() {
        assert_eq!(achievement_label(ACH_FIRST_WIN), "首次夺冠");
        assert_eq!(achievement_label("ACH_SOMETHING_ELSE"), "新成就");
    }
}
