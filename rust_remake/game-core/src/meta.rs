//! 多局对抗（meta）数据模型 —— 经济 / 升级 / 结算 / 周期。
//!
//! 原版游戏里这块并未完成（等级系统只到"禁用/启用"两级、金币只扣不发）。
//! 本模块按"术士之战"式目标补齐：
//! - 每轮开局发放固定金币（参与奖）
//! - 击杀奖励金币
//! - 每轮结束按名次发放额外金币
//! - 技能等级随回合成长（`SkillGrowth`）
//!
//! 本模块只处理纯逻辑，不涉及渲染。`World`（单场战斗）通过调用方驱动，
//! 本模块负责记录每轮排名与金币，并在"学习阶段"提供购买/升级接口。

use crate::skill::{CastKey, SkillId};

/// 一场完整对抗的总小局数与时长配置。
#[derive(Clone, Debug, PartialEq)]
pub struct MatchConfig {
    /// 总小局数
    pub total_rounds: u32,
    /// 学习阶段时长（秒）；0 用 0 表示"无学习阶段，自动进入下一局"
    pub learn_time_secs: f64,
    /// 每轮为每位玩家固定发放的金币（参与奖）
    pub gold_per_round: i32,
    /// 每一个击杀奖励的金币
    pub gold_per_kill: i32,
    /// 每轮结束时按名次的额外奖励（索引 = 名次-1，0=冠军；超过数组长度的名次不额外奖励）
    pub place_rewards: Vec<i32>,
    /// 开局（第一小局开始前）为每位玩家一次性发放的初始金币；与每轮参与奖 `gold_per_round` 相互独立、叠加。
    /// 房主可设置；098b 默认 So=20（D6）。
    pub starting_gold: i32,
    /// 击杀得分（098b lo，默认 1；D6 分数体系）。
    pub score_per_kill: u32,
    /// 助攻得分（098b Lo，默认 1）。
    pub score_per_assist: u32,
    /// 轮胜利得分（098b po，默认 1；胜利 = 每轮最后存活者）。
    pub score_per_round_win: u32,
    /// 对局模式（098b En：1=回合制打满 N 轮按总分排名，2=死亡竞赛先到胜利分；
    /// En3-5 化身/弑君/生存 TODO）。默认 1。
    pub game_mode: u8,
    /// 死亡竞赛（En2）的胜利得分（098b `-+胜利得分` 开局设置）。
    pub win_score: u32,
    /// 开局购物时长（098b Wo=40；独立于每轮 wo=30 的 `learn_time_secs`）。
    /// 开局购物阶段与单机/Steam 进局流程的耦合见 M4 TODO。
    pub shopping_time_secs: f64,
}

impl Default for MatchConfig {
    fn default() -> Self {
        // 经济默认值对齐 098b ed()（PORT_098B_DECISIONS.md D6）：
        // 开局 So=20、每轮 so=10、击杀金 mo=0（默认只给分不给钱）、名次奖励无此机制（默认空）。
        MatchConfig {
            total_rounds: 3,
            learn_time_secs: 30.0, // 098b wo=30
            gold_per_round: 10,
            gold_per_kill: 0,
            place_rewards: Vec::new(),
            starting_gold: 20,
            // 098c 计分（D9 批次3）：胜利 2 分、击杀 2 分、助攻 1 分
            score_per_kill: 2,
            score_per_assist: 1,
            score_per_round_win: 2,
            game_mode: 1,
            win_score: 10,
            shopping_time_secs: 40.0,
        }
    }
}

/// 一位玩家在整场对抗中的累计档案。
#[derive(Clone, Debug, PartialEq)]
pub struct PlayerProfile {
    pub player_id: u32,
    pub gold: i32,
    pub total_kills: u32,
    /// 存活过的小局数
    pub rounds_survived: u32,
    /// 累计得分（击杀/助攻/轮胜，098b lo/Lo/po；En1 按总分定胜负，D6）。
    pub score: u32,
    /// 当前连杀数（死亡清零；>=3 触发连杀播报，098b Mn[3..10]）。
    pub current_streak: u32,
    /// 本场已出 First Blood（098c：全场第一杀播报）。
    pub first_blood_taken: bool,
    /// 本场最佳名次（1 = 冠军；0 = 未结束任何局）
    pub best_placement: u32,
    /// 各技能当前等级（索引 = SkillId::as_u32）
    pub skill_levels: Vec<u32>,
    /// 每个键位绑定的技能（索引 = CastKey::as_u32）
    pub key_slots: [Option<SkillId>; 8],
    /// 持有的物品（098b 6 格；升级链同家族替换，M3）。
    pub items: Vec<crate::item::ItemId>,
    /// 累计在技能升级上花费的金币（用于洗点退款）
    pub gold_spent: i32,
    /// 战斗属性（4.6b）：Hp/移速等成长值，跨局/跨端确定性同步。
    pub attributes: crate::attribute::Attributes,
    /// 成长点（4.6b）：用于购买属性。每局/阶段固定发放，也可用金币兑换。
    pub growth_points: u32,
}

impl PlayerProfile {
    pub fn new(player_id: u32, skill_count: usize) -> Self {
        // 等级数组统一覆盖全部技能槽，避免越界（调用方传的 skill_count 可能 < 全槽数）
        let n = skill_count.max(crate::MAX_SKILL_SLOTS);
        PlayerProfile {
            player_id,
            gold: 0,
            total_kills: 0,
            score: 0,
            current_streak: 0,
            first_blood_taken: false,
            rounds_survived: 0,
            best_placement: 0,
            skill_levels: vec![1; n],
            key_slots: [None; 8],
            items: Vec::new(),
            gold_spent: 0,
            attributes: crate::attribute::Attributes::default(),
            growth_points: 0,
        }
    }

    /// 该玩家某技能的当前等级。
    pub fn skill_level(&self, skill: SkillId) -> u32 {
        self.skill_levels[skill.as_u32() as usize]
    }

    /// 该键当前绑定哪个技能。
    pub fn bound_skill(&self, key: CastKey) -> Option<SkillId> {
        self.key_slots[key.as_u32() as usize]
    }

    /// 把一个技能绑定到某个键。转换技能会保留各自已有等级，仅改变键指向。
    pub fn bind_skill(&mut self, key: CastKey, skill: SkillId) {
        self.key_slots[key.as_u32() as usize] = Some(skill);
    }

    /// 解除某键的绑定。
    pub fn unbind_skill(&mut self, key: CastKey) {
        self.key_slots[key.as_u32() as usize] = None;
    }

    /// 购买/升级物品（M3）：金币不足失败；同家族持有低档则替换（098b 升级链语义）。
    pub fn buy_item(&mut self, id: crate::item::ItemId) -> bool {
        let cost = id.def().cost;
        if self.gold < cost {
            return false;
        }
        self.gold -= cost;
        let family = id.def().family;
        self.items.retain(|&it| it.def().family != family);
        self.items.push(id);
        true
    }

    /// 购买/升级某技能一级。返回是否成功（金币不足则失败）；成功计入洗点累计花费。
    ///
    /// `cost(当前等级) -> 升级到 当前等级+1 的价格`。调用方负责提供价格表。
    pub fn upgrade_skill(&mut self, skill: SkillId, cost: i32) -> bool {
        if self.gold < cost {
            return false;
        }
        self.gold -= cost;
        self.gold_spent += cost;
        self.skill_levels[skill.as_u32() as usize] += 1;
        true
    }

    /// 洗点：按 `refund_ratio`（0..=1）返还升级花费的金币，清空所有键位绑定并把技能等级重置为 1。
    ///
    /// `refund_ratio` 由配置决定（原版全额退；也可设比例）。
    pub fn respec(&mut self, refund_ratio: f64) {
        let ratio = refund_ratio.clamp(0.0, 1.0);
        let refund = (self.gold_spent as f64 * ratio).round() as i32;
        self.gold += refund;
        self.gold_spent = 0;
        self.key_slots = [None; 8];
        for lv in self.skill_levels.iter_mut() {
            *lv = 1;
        }
    }

    // ---- 4.6b 成长点 / 属性购买 ----

    /// 发放成长点（每局/阶段调用）。
    pub fn add_growth_points(&mut self, n: u32) {
        self.growth_points = self.growth_points.saturating_add(n);
    }

    /// 用金币兑换成长点：扣 `cost_gold` 金币、得 1 成长点。返回是否成功。
    pub fn buy_growth_with_gold(&mut self, cost_gold: i32) -> bool {
        if self.gold < cost_gold {
            return false;
        }
        self.gold -= cost_gold;
        self.growth_points = self.growth_points.saturating_add(1);
        true
    }

    /// 用成长点购买 1 点属性（`which` 决定买哪个），成本 `cost_points`。成功则 `attributes[which] += 1`。
    pub fn buy_attribute(&mut self, which: crate::attribute::GrowthAttr, cost_points: u32) -> bool {
        if self.growth_points < cost_points {
            return false;
        }
        self.growth_points -= cost_points;
        self.attributes.add_point(which);
        true
    }

    /// 该玩家当前实际可用的（已绑定到某个键的）技能列表。
    pub fn bound_skills(&self) -> impl Iterator<Item = SkillId> + '_ {
        self.key_slots.iter().flatten().copied()
    }
}

/// 当前处于哪个阶段。
#[derive(Copy, Clone, Debug, PartialEq)]
pub enum MatchPhase {
    /// 对局进行中
    Fighting,
    /// 学习/购买阶段（购买技能）；结束后进入下一局
    Learning,
    /// 整场对抗结束
    Finished,
}

/// 一场对抗的多局进度。
#[derive(Clone, Debug)]
pub struct MatchState {
    pub config: MatchConfig,
    /// 当前/即将进行的小局号（1 开始）
    pub round: u32,
    pub phase: MatchPhase,
    /// 学习阶段剩余时间
    pub learn_remaining: f64,
    pub profiles: Vec<PlayerProfile>,
    /// 每局：玩家名次（round 结束后填充，索引 = 名次-1 位置是玩家id）。len=回合数。
    pub round_placements: Vec<Vec<u32>>,
    /// 首局配置学习标志：仅 `begin_first_round_config()` 置 true；学习倒计时归零时据此走
    /// `enter_first_round`（round 保持 1、不重复发参与奖），区别于局间的 `advance_round`（round+1）。
    pending_first_round: bool,
    /// 本场是否已出 First Blood（098c：全场第一杀播报，D9 批次3）。
    pub first_blood_taken: bool,
}

impl MatchState {
    pub fn new(config: MatchConfig, player_ids: &[u32], skill_count: usize) -> Self {
        let mut m = MatchState {
            round: 1,
            phase: MatchPhase::Fighting,
            learn_remaining: 0.0,
            profiles: player_ids
                .iter()
                .map(|&id| PlayerProfile::new(id, skill_count))
                .collect(),
            round_placements: Vec::new(),
            config,
            pending_first_round: false,
            first_blood_taken: false,
        };
        m.give_starting_gold();
        m.give_round_gold();
        m
    }

    /// 开局发放初始金币（第一局开始前一次性发放，独立于每轮参与奖）。
    fn give_starting_gold(&mut self) {
        for p in self.profiles.iter_mut() {
            p.gold += self.config.starting_gold;
        }
    }

    fn give_round_gold(&mut self) {
        for p in self.profiles.iter_mut() {
            p.gold += self.config.gold_per_round;
        }
    }

    /// 本局结束时结算：传入本局名次（`placement[i]` = 名次为 i+1 的玩家 id）。
    /// 发放存活/名次奖励，记录存活局数与最优名次，退回学习阶段。
    pub fn finish_round(&mut self, placement: Vec<u32>) {
        self.round_placements.push(placement.clone());
        for (rank_idx, &player_id) in placement.iter().enumerate() {
            let rank = (rank_idx + 1) as u32;
            if let Some(p) = self
                .profiles
                .iter_mut()
                .find(|pr| pr.player_id == player_id)
            {
                // 名次奖励 & 最优名次
                if let Some(&reward) = self.config.place_rewards.get(rank_idx) {
                    p.gold += reward;
                }
                p.best_placement = if p.best_placement == 0 {
                    rank
                } else {
                    p.best_placement.min(rank)
                };
                if rank == 1 {
                    p.rounds_survived += 1; // 冠军视为存活（保留存活局数语义）
                }
            }
        }
        // 进入学习阶段，或整场结束
        // En2 死亡竞赛：有人达到胜利分 → 提前终局（D6/En 批）。
        let early_win = self.config.game_mode == 2
            && self.profiles.iter().any(|pr| pr.score >= self.config.win_score);
        if early_win || self.round >= self.config.total_rounds {
            self.phase = MatchPhase::Finished;
        } else {
            self.phase = MatchPhase::Learning;
            self.learn_remaining = self.config.learn_time_secs;
        }
    }

    /// 终局排名（En 批）：按分数降序、最优名次升序；返回 (player_id, score)。
    /// En1（打满轮数）与 En2（先到胜利分）共用。
    pub fn final_ranking(&self) -> Vec<(u32, u32)> {
        let mut v: Vec<(u32, u32, u32)> = self
            .profiles
            .iter()
            .map(|pr| (pr.player_id, pr.score, pr.best_placement))
            .collect();
        v.sort_by(|a, b| b.1.cmp(&a.1).then(a.2.cmp(&b.2)));
        v.into_iter().map(|(id, score, _)| (id, score)).collect()
    }

    /// 记录击杀（由 World 报告或由外部按规则上报），给击杀者发金币。
    /// 返回：是否本场 First Blood（供播报；D9 批次3）。
    pub fn register_kill(&mut self, killer_id: u32) -> bool {
        let first = !self.first_blood_taken;
        self.first_blood_taken = true;
        if let Some(p) = self
            .profiles
            .iter_mut()
            .find(|pr| pr.player_id == killer_id)
        {
            p.total_kills += 1;
            p.gold += self.config.gold_per_kill;
            p.score += self.config.score_per_kill;
            p.current_streak += 1; // 连杀计数（死亡清零在 register_death）
        }
        first
    }

    /// 死亡结算（D6）：受害者连杀清零（连杀播报由调用方在清零前读取）。
    pub fn register_death(&mut self, victim_id: u32) -> u32 {
        let mut streak = 0;
        if let Some(p) = self
            .profiles
            .iter_mut()
            .find(|pr| pr.player_id == victim_id)
        {
            streak = p.current_streak;
            p.current_streak = 0;
        }
        streak
    }

    /// 助攻结算（D6）：对本局伤害过死者（但非击杀者）的玩家发分/金。
    pub fn register_assists(&mut self, victim_id: u32, killer_id: u32, assists: &[u32]) {
        for &a in assists {
            if a == killer_id || a == victim_id {
                continue;
            }
            if let Some(p) = self.profiles.iter_mut().find(|pr| pr.player_id == a) {
                p.score += self.config.score_per_assist;
                p.gold += 0; // 098b Mo=0（金钱-助攻默认 0，可调）
            }
        }
    }

    /// 轮胜利结算（D6）：每轮最后存活者 +分（组队模式待 M4 队伍系统）。
    pub fn register_round_win(&mut self, winner_id: u32) {
        if let Some(p) = self
            .profiles
            .iter_mut()
            .find(|pr| pr.player_id == winner_id)
        {
            p.score += self.config.score_per_round_win;
        }
    }

    /// 连杀播报（098b Mn[3..10]）：大杀特杀(3)…超越神了(10)；None = 无播报。
    pub fn streak_label(streak: u32) -> Option<&'static str> {
        match streak {
            3 => Some("大杀特杀"),
            4 => Some("主宰比赛"),
            5 => Some("迈向胜利"),
            6 => Some("狂暴了"),
            7 => Some("无法阻挡"),
            8 => Some("变态了"),
            9 => Some("接近神了"),
            10..=u32::MAX => Some("超越神了"),
            _ => None,
        }
    }

    /// 学习阶段推进；时间用完则进入下一局（回到 Fighting）。
    /// 返回单位：是否需要进入下一局。
    pub fn tick_learning(&mut self, dt: f64) -> bool {
        if self.phase != MatchPhase::Learning {
            return false;
        }
        self.learn_remaining -= dt;
        if self.learn_remaining <= 0.0 {
            if self.pending_first_round {
                // 首局配置：归零 → 进入第一局（round 保持 1、不重复发参与奖，构造时已发）。
                self.pending_first_round = false;
                self.enter_first_round();
            } else {
                self.advance_round();
            }
            true
        } else {
            false
        }
    }

    /// 手动结束学习阶段（例如玩家点了"开始"，且设置学习时长为必点）。
    pub fn start_next_round(&mut self) {
        if self.phase == MatchPhase::Learning {
            self.advance_round();
        }
    }

    /// 开局前的配置阶段结束 → 进入第一局（不 +round，参与奖已在构造时发放）。
    pub fn enter_first_round(&mut self) {
        self.phase = MatchPhase::Fighting;
    }

    /// 首局进入配置学习（联机用倒计时自动开始）：进入 Learning，倒计时 =
    /// `shopping_time_secs`（098b Wo=40 开局购物，D6/M4；区别于每轮 wo=30 的 learn_time_secs）。
    /// 倒计时归零（`tick_learning`）走 `enter_first_round`（round 保持 1、不重复发参与奖）。
    pub fn begin_first_round_config(&mut self) {
        self.phase = MatchPhase::Learning;
        self.learn_remaining = self.config.shopping_time_secs;
        self.pending_first_round = true;
    }

    /// 手动结束首局配置（单机试验场用）：直接进入第一局（不 +round、不重复发参与奖）。
    pub fn finish_first_round_config(&mut self) {
        self.pending_first_round = false;
        self.enter_first_round();
    }

    /// 当前是否处于「首局配置学习」（未开过战）。client 用来区分单机首局的手动开始。
    pub fn is_first_config(&self) -> bool {
        self.pending_first_round
    }

    fn advance_round(&mut self) {
        self.round += 1;
        self.phase = MatchPhase::Fighting;
        self.give_round_gold(); // 新的参与奖
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 机制验证用的旧口径配置（每轮 20/击杀 15/名次 30/20/10——非 098b 默认，D6 默认值另有对账测试）。
    fn sample() -> MatchState {
        MatchState::new(
            MatchConfig {
                gold_per_round: 20,
                gold_per_kill: 15,
                place_rewards: vec![30, 20, 10],
                ..MatchConfig::default()
            },
            &[0, 1, 2],
            8,
        )
    }

    #[test]
    fn round_start_gives_participation_gold() {
        let m = sample();
        assert_eq!(m.profiles[0].gold, 40, "So20 + 首轮参与奖 20");
        assert_eq!(m.profiles[1].gold, 40);
    }

    #[test]
    fn starting_gold_granted_once_at_creation_plus_round_participation() {
        let config = MatchConfig {
            starting_gold: 50,
            ..Default::default()
        };
        let m = MatchState::new(config, &[0, 1], 8);
        // 第一局 = 初始金币 50 + 参与奖（默认 so=10）= 60
        assert_eq!(m.profiles[0].gold, 50 + 10);
        assert_eq!(m.profiles[1].gold, 50 + 10);
    }

    #[test]
    fn kill_gives_gold() {
        let mut m = sample();
        m.register_kill(0);
        assert_eq!(m.profiles[0].gold, 40 + 15, "基础 40 + 击杀 15");
        assert_eq!(m.profiles[0].total_kills, 1);
    }

    #[test]
    fn finish_round_rewards_placement_and_gold() {
        let mut m = sample();
        // 名次：0=冠军（+30+存活），1=第二（+20），2=第三（+10）
        m.finish_round(vec![0, 1, 2]);
        assert_eq!(m.profiles[0].gold, 40 + 30);
        assert_eq!(m.profiles[1].gold, 40 + 20);
        assert_eq!(m.profiles[2].gold, 40 + 10);
        assert_eq!(m.profiles[0].best_placement, 1);
        assert_eq!(m.profiles[1].best_placement, 2);
        assert_eq!(m.round_placements.len(), 1);
        assert_eq!(m.phase, MatchPhase::Learning, "未到总局数应进入学习阶段");
    }

    #[test]
    fn learning_then_advance_gives_round_gold_again() {
        let mut m = sample();
        m.finish_round(vec![0, 1, 2]); // → Learning
        assert_eq!(m.phase, MatchPhase::Learning);
        let advanced = m.tick_learning(30.01); // 学习超时（098b wo=30）
        assert!(advanced);
        assert_eq!(m.round, 2);
        assert_eq!(m.phase, MatchPhase::Fighting);
        // 第二局参与奖已发放
        assert_eq!(m.profiles[0].gold, 40 + 30 + 20);
    }

    #[test]
    fn en2_deathmatch_early_win_and_ranking() {
        // En2：先到胜利分提前终局；final_ranking 按分数排序。
        let mut m = MatchState::new(
            MatchConfig { game_mode: 2, win_score: 3, ..Default::default() },
            &[0, 1],
            34,
        );
        // 打两轮（每轮胜者 0 得 1 分 + 击杀分）
        for _ in 0..2 {
            m.register_kill(0);
            m.register_round_win(0);
            m.finish_round(vec![0, 1]);
        }
        // score = 2 轮 × (杀 2 + 胜 2) = 8 ≥ 3 → 已提前终局
        assert_eq!(m.phase, MatchPhase::Finished, "En2 达到胜利分应提前终局");
        let ranking = m.final_ranking();
        assert_eq!(ranking[0], (0, 8), "按分数降序，实际 {ranking:?}");
        // En1 同分数时按 best_placement 升序
        let mut m1 = MatchState::new(MatchConfig::default(), &[0, 1], 34);
        m1.finish_round(vec![0, 1]);
        m1.profiles[1].score = m1.profiles[0].score;
        let r = m1.final_ranking();
        assert_eq!(r[0].0, 0, "同分按最优名次排（0 是冠军）");
    }

    #[test]
    fn en1_default_runs_full_rounds_without_early_win() {
        // En1（默认）：分数不影响轮数——打满 3 轮才终局。
        let mut m = MatchState::new(MatchConfig::default(), &[0, 1], 34);
        for round in 1..=3 {
            for _ in 0..5 {
                m.register_kill(0); // 击杀分累计但不触发 En2（mode=1）
                m.register_round_win(0);
            }
            m.finish_round(vec![0, 1]);
            if round < 3 {
                assert_eq!(m.phase, MatchPhase::Learning, "En1 打满轮数前不应终局");
                m.tick_learning(30.01);
            }
        }
        assert_eq!(m.phase, MatchPhase::Finished);
    }

    #[test]
    fn d6_economy_defaults_match_098b() {
        // PORT_098B_DECISIONS.md D6：So=20 / so=10 / 击杀金 0（只给分）/ 名次奖默认空。
        let config = MatchConfig::default();
        assert_eq!(config.starting_gold, 20);
        assert_eq!(config.gold_per_round, 10);
        assert_eq!(config.gold_per_kill, 0);
        assert!(config.place_rewards.is_empty());
        // 098c 计分（D9 批次3）：胜 2 / 杀 2 / 助 1
        assert_eq!((config.score_per_kill, config.score_per_assist, config.score_per_round_win), (2, 1, 2));
        // 开局购物 Wo=40 / 每轮 wo=30（D6/M4 En 批）
        assert_eq!(config.shopping_time_secs, 40.0);
        assert_eq!(config.learn_time_secs, 30.0);
        assert_eq!(config.game_mode, 1);
        let mut m = MatchState::new(config, &[0, 1], 8);
        assert_eq!(m.profiles[0].gold, 20 + 10, "开局 20 + 首轮 10");
        // 击杀只给分
        m.register_kill(0);
        assert_eq!(m.profiles[0].gold, 30, "击杀金默认 0");
        assert_eq!(m.profiles[0].score, 2, "098c 击杀 2 分");
        assert_eq!(m.profiles[0].current_streak, 1);
        // 助攻
        m.register_assists(1, 0, &[0, 1]);
        assert_eq!(m.profiles[0].score, 2, "击杀者不算助攻");
        // 轮胜分
        m.register_round_win(0);
        assert_eq!(m.profiles[0].score, 4, "098c 轮胜 2 分");
        // 连杀标签
        assert_eq!(MatchState::streak_label(2), None);
        assert_eq!(MatchState::streak_label(3), Some("大杀特杀"));
        assert_eq!(MatchState::streak_label(12), Some("超越神了"));
    }

    #[test]
    fn buy_item_spends_gold_and_upgrades_chain() {
        let mut ms = MatchState::new(MatchConfig::default(), &[0, 1], 34);
        let pr = &mut ms.profiles[0];
        pr.gold = 20;
        // 买头盔 1（10 金）
        assert!(pr.buy_item(crate::item::ItemId::Helm1));
        assert_eq!(pr.gold, 10);
        assert_eq!(pr.items, vec![crate::item::ItemId::Helm1]);
        // 升级头盔 2（20 金）——不足失败；同家族替换语义
        assert!(!pr.buy_item(crate::item::ItemId::Helm2), "金币不足应失败");
        pr.gold = 30;
        assert!(pr.buy_item(crate::item::ItemId::Helm2));
        assert_eq!(pr.gold, 10);
        assert_eq!(pr.items, vec![crate::item::ItemId::Helm2], "同家族应替换为高档");
        // 不同家族共存
        assert!(pr.buy_item(crate::item::ItemId::Boots1));
        assert_eq!(pr.items.len(), 2);
    }

    #[test]
    fn upgrade_skill_spends_gold_and_fails_when_poor() {
        let mut m = sample();
        // 升一级 Rock(id=6) 花费 10（sample 基础 40）
        assert!(m.profiles[0].upgrade_skill(SkillId::Rock, 10));
        assert_eq!(m.profiles[0].gold, 30);
        assert_eq!(m.profiles[0].skill_level(SkillId::Rock), 2);
        // 再花 40 不够（剩 30）→ 失败
        assert!(!m.profiles[0].upgrade_skill(SkillId::Rock, 40));
        assert_eq!(m.profiles[0].skill_level(SkillId::Rock), 2);
    }

    #[test]
    fn after_last_round_becomes_finished() {
        let config = MatchConfig {
            total_rounds: 1,
            ..Default::default()
        };
        let mut m = MatchState::new(config, &[0, 1], 8);
        m.finish_round(vec![1, 0]);
        assert_eq!(m.phase, MatchPhase::Finished);
    }

    #[test]
    fn bind_and_respec_full_refund() {
        let mut p = PlayerProfile::new(0, 8);
        // 绑定 C 键到 Rock，E 键到 Blink
        p.bind_skill(CastKey::C, SkillId::Rock);
        p.bind_skill(CastKey::E, SkillId::Blink);
        assert_eq!(p.bound_skill(CastKey::C), Some(SkillId::Rock));
        assert_eq!(p.bound_skill(CastKey::E), Some(SkillId::Blink));

        // 升级 Rock 两级（花费 10 + 15）
        p.gold = 100;
        assert!(p.upgrade_skill(SkillId::Rock, 10));
        assert!(p.upgrade_skill(SkillId::Rock, 15));
        assert_eq!(p.skill_level(SkillId::Rock), 3);
        assert_eq!(p.gold_spent, 25);

        // 全额洗点：返还所有金币、清绑定、重置等级
        p.respec(1.0);
        assert_eq!(p.gold, 100); // 100 - 25 + 25 = 100
        assert_eq!(p.bound_skill(CastKey::C), None);
        assert_eq!(p.skill_level(SkillId::Rock), 1);
        assert_eq!(p.gold_spent, 0);
    }

    #[test]
    fn respec_partial_refund() {
        let mut p = PlayerProfile::new(1, 8);
        p.gold = 100;
        assert!(p.upgrade_skill(SkillId::Rock, 30));
        assert!(p.upgrade_skill(SkillId::Fake, 20));
        assert_eq!(p.gold_spent, 50);
        // 50% 退还
        p.respec(0.5);
        assert_eq!(p.gold, 100 - 50 + 25); // = 75
        assert_eq!(p.skill_level(SkillId::Rock), 1);
    }

    #[test]
    fn enter_first_round_keeps_round_one() {
        let mut m = sample();
        assert_eq!(m.round, 1);
        assert_eq!(m.phase, MatchPhase::Fighting);
        // 模拟开局配置：先进 Learning，再 enter_first_round 回 Fighting 且 round 不变。
        m.phase = MatchPhase::Learning;
        m.enter_first_round();
        assert_eq!(m.phase, MatchPhase::Fighting);
        assert_eq!(m.round, 1, "开局配置结束不应 +round");
        // 与局间 start_next_round（会 +round）区分。
    }

    #[test]
    fn first_round_config_countdown_enters_round_one_without_extra_gold() {
        // 首局配置学习：begin -> Learning + 倒计时；归零 -> Fighting 且 round 保持 1、不重复发参与奖。
        let mut m = sample();
        let gold_before = m.profiles[0].gold; // 构造时已发 starting_gold + 第一轮参与奖
        m.begin_first_round_config();
        assert_eq!(m.phase, MatchPhase::Learning);
        assert_eq!(m.round, 1);
        // 首局购物时长用 Wo=40（shopping_time_secs），不是每轮 wo=30
        assert_eq!(m.learn_remaining, 40.0, "首局购物应为 Wo=40");
        // 时间到：应走 enter_first_round（round 不变、金币不加）而非 advance_round（+round、发参与奖）。
        let advanced = m.tick_learning(m.learn_remaining + 0.1);
        assert!(advanced);
        assert_eq!(m.phase, MatchPhase::Fighting);
        assert_eq!(m.round, 1, "首局配置归零不应 +round");
        assert_eq!(m.profiles[0].gold, gold_before, "首局配置归零不应重复发参与奖");
        // 之后再进局间学习：归零应 advance_round（+round + 参与奖）。
        m.finish_round(vec![0, 1, 2]);
        assert_eq!(m.phase, MatchPhase::Learning);
        assert_eq!(m.round, 1);
        let gold_before2 = m.profiles[0].gold;
        m.tick_learning(m.learn_remaining + 0.1);
        assert_eq!(m.round, 2, "局间学习归零应 +round");
        assert!(m.profiles[0].gold > gold_before2, "局间学习归零应发参与奖");
    }

    /// 4.6b 成长点：发放、金币兑成长点、成长点买属性，均向 `growth_points`/`attributes` 正确记账。
    #[test]
    fn growth_points_for_attributes() {
        let mut m = sample();
        let p = &mut m.profiles[0];
        p.gold = 100;
        p.add_growth_points(3);
        assert_eq!(p.growth_points, 3);
        assert!(p.buy_growth_with_gold(20));
        assert_eq!(p.growth_points, 4);
        assert_eq!(p.gold, 80);
        assert!(!p.buy_growth_with_gold(999), "金币不足应失败");
        assert!(p.buy_attribute(crate::attribute::GrowthAttr::Hp, 1));
        assert_eq!(p.attributes.hp_bonus, 1);
        assert_eq!(p.growth_points, 3);
        assert!(!p.buy_attribute(crate::attribute::GrowthAttr::Hp, 99), "成长点不足应失败");
    }
}
