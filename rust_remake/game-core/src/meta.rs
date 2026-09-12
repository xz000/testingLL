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

/// 乔丹之石价格（098c `bD` h004：戒指 5 金；卖掉**不退**）。
/// 我方把它做成**一次性解锁**（不占物品栏）：首次突破某槽上限时扣，之后各槽免费。
pub const JORDAN_PRICE: i32 = 5;

/// 一场完整对抗的总小局数与时长配置。
#[derive(Clone, Debug, PartialEq)]
pub struct MatchConfig {
    /// 基础生命恢复（HP/s）。098c 主机常量 `-C9`（`In`，默认 `In=.05`/0.1s = **0.5/s**）。
    pub base_regen: f64,
    /// 总小局数
    pub total_rounds: u32,
    /// 学习阶段时长（秒）；0 用 0 表示"无学习阶段，自动进入下一局"
    pub learn_time_secs: f64,
    /// 每轮为每位玩家固定发放的金币（参与奖）
    pub gold_per_round: i32,
    /// 击杀金币（098c `lo`，全局默认 **1** —— `war3map_pretty.j` 209）。
    /// 助攻金币（098c `Lo`，默认 **1**；6060 `register_assists`）。
    pub gold_per_assist: i32,
    /// 胜利金币（098c `Mo`，默认 **2**）。
    pub gold_per_round_win: i32,
    /// 每一个击杀奖励的金币
    pub gold_per_kill: i32,
    /// **伤害金**（098c 设置 16 `po`，全局默认 1）：回合结束时发给**本回合伤害最高**的玩家
    /// （并列者都发）。实证 `war3map_pretty.j` 5364-5379：`if Rn[i] >= ZR then ... + po`，
    /// 并播报 "X has dealt the most damage in this round (N)."。**与伤害量无关**，是"最高者独占"奖。
    pub gold_per_most_damage: i32,
    /// 每轮结束时按名次的额外奖励（索引 = 名次-1，0=冠军；超过数组长度的名次不额外奖励）
    pub place_rewards: Vec<i32>,
    /// 开局（第一小局开始前）为每位玩家一次性发放的初始金币；与每轮参与奖 `gold_per_round` 相互独立、叠加。
    /// 房主可设置；098c 全局 `Qo=20`。
    pub starting_gold: i32,

    // ───────── 房间设置：玩法项（对应 098c 设置对话框 1-6/9，`war3map_pretty.j` 18768-18813） ─────────
    /// **伤害倍率**（098c 设置 2 `Gn`）。档位 0.75/1.0/1.25/1.5，也允许自定义。
    pub damage_mult: f64,
    /// **击退倍率**（098c 设置 3 `Hn`）。档位同上。
    pub knockback_mult: f64,
    /// **岩浆伤害倍率**（098c 设置 1 `To`）。`0.0` = 关闭岩浆伤害
    /// （098c 该项**无下限校验**，输入 0 即关闭 —— 我们同样允许，UI 标注"不推荐"）。
    pub lava_damage_mult: f64,
    /// **第一轮配置期秒数**（098c 设置 5 `Uo` = 40，「Shop Time initial」）。
    pub first_round_time_secs: f64,
    /// **局间配置期秒数**（098c 设置 4 `uo` = 30，「Shop Time」）。
    pub between_rounds_time_secs: f64,
    /// **收缩延迟秒数**：开局静止期，之后开始连续收缩。
    pub shrink_delay_secs: f64,
    /// **每环收缩时长**（098c 设置 6 `wo`，默认 10）：越过一环所需秒数。
    /// 实际速率 = `环宽 / (本值 × √存活人数)`，且**开局延迟**同样为 `本值 × √存活人数`
    /// —— 与 098c `TimerStart(Sa, wo*SquareRoot(sn), ...)` 同构（`sn` = 本轮存活人数）。
    pub shrink_ring_secs: f64,
    /// **柱子**：0=关闭 1=随机 2=每局必有（098c 设置 8 `Po`，0=随机）。
    pub pillar_mode: u8,
    /// **冰面**：0=关闭 1=随机 2=每局必有（098c 把"关冰"绑在 `-league` 里，我们独立出来）。
    pub ice_mode: u8,
    /// **地图形状**：0=圆形（当前仅支持；后续可扩正方形/六边形）。
    pub arena_shape: u8,
    /// **金币奖励总开关**：`false` = 关闭全部金币奖励（等价 098c `-no reward`／`-league` 的奖励部分），
    /// 点数奖励不受影响。
    pub gold_rewards_enabled: bool,
    /// 击杀得分（098b lo，默认 1；D6 分数体系）。
    pub score_per_kill: u32,
    /// 助攻得分（098b Lo，默认 1）。
    pub score_per_assist: u32,
    /// 轮胜利得分（098b po，默认 1；胜利 = 每轮最后存活者）。
    pub score_per_round_win: u32,
    /// 对局模式（098c nn，B3）：1=轮次（默认）/2=死亡竞赛/3=化身/4=国王/5=最后生还。
    /// 选择走房间属性（D13 #1），不再用聊天命令。
    pub game_mode: u8,
    /// 队伍数（098c -mgl/-teams，B2）：1=FFA（各为一队，默认）；2=按序号对半分两队。
    pub team_count: u8,
    /// 死亡竞赛（En2）的胜利得分（098b `-+胜利得分` 开局设置）。
    pub win_score: u32,
    /// 开局购物时长（098b Wo=40；独立于每轮 wo=30 的 `learn_time_secs`）。
    /// 进局耦合已通过 `begin_first_round_config` / `enter_first_round` 实现（倒计时归零进入第一局，
    /// 不重复发参与奖、round 保持 1）。
    pub shopping_time_secs: f64,
}

/// 房间设置串（用于大厅元数据/同步）的**模式版本**：字段顺序或语义变更时必须递增，
/// 否则不同版本的端会按各自的顺序解析同一串。
pub const ROOM_SETTINGS_SCHEMA: u32 = 1;

impl MatchConfig {
    /// 序列化为**紧凑单行**（大厅元数据用；`|` 分隔、`;` 分隔列表）。
    ///
    /// 为什么不逐项开 `host_set_*`：设置项已达 20+，逐项接口样板过重，
    /// 且"整体替换"天然满足"任何改动都要重新同步 + 取消准备"的需求。
    pub fn to_meta_string(&self) -> String {
        let f = |v: f64| format!("{v}");
        let parts: Vec<String> = vec![
            ROOM_SETTINGS_SCHEMA.to_string(),
            self.total_rounds.to_string(),
            f(self.learn_time_secs),
            f(self.shopping_time_secs),
            f(self.first_round_time_secs),
            f(self.between_rounds_time_secs),
            self.starting_gold.to_string(),
            self.gold_per_round.to_string(),
            self.gold_per_kill.to_string(),
            self.gold_per_assist.to_string(),
            self.gold_per_round_win.to_string(),
            self.gold_per_most_damage.to_string(),
            self.score_per_kill.to_string(),
            self.score_per_assist.to_string(),
            self.score_per_round_win.to_string(),
            self.place_rewards
                .iter()
                .map(|v| v.to_string())
                .collect::<Vec<_>>()
                .join(";"),
            f(self.damage_mult),
            f(self.knockback_mult),
            f(self.lava_damage_mult),
            f(self.shrink_delay_secs),
            f(self.shrink_ring_secs),
            self.pillar_mode.to_string(),
            self.ice_mode.to_string(),
            self.arena_shape.to_string(),
            if self.gold_rewards_enabled { "1" } else { "0" }.to_string(),
            f(self.base_regen),
            self.game_mode.to_string(),
            self.team_count.to_string(),
            self.win_score.to_string(),
        ];
        parts.join("|")
    }

    /// 从 [`Self::to_meta_string`] 还原；缺字段/格式不符返回 `None`（由调用方回退默认值）。
    pub fn from_meta_string(s: &str) -> Option<Self> {
        let p: Vec<&str> = s.trim().split('|').collect();
        // schema + 27 个字段
        if p.len() < 28 {
            return None;
        }
        if p[0].parse::<u32>().ok()? != ROOM_SETTINGS_SCHEMA {
            return None;
        }
        let num = |i: usize| -> Option<f64> { p.get(i)?.parse::<f64>().ok() };
        let int = |i: usize| -> Option<i32> { p.get(i)?.parse::<i32>().ok() };
        let uint = |i: usize| -> Option<u32> { p.get(i)?.parse::<u32>().ok() };
        let byte = |i: usize| -> Option<u8> { p.get(i)?.parse::<u8>().ok() };
        let place: Vec<i32> = if p[15].is_empty() {
            Vec::new()
        } else {
            p[15].split(';').filter_map(|v| v.parse::<i32>().ok()).collect()
        };
        Some(MatchConfig {
            total_rounds: uint(1)?,
            learn_time_secs: num(2)?,
            shopping_time_secs: num(3)?,
            first_round_time_secs: num(4)?,
            between_rounds_time_secs: num(5)?,
            starting_gold: int(6)?,
            gold_per_round: int(7)?,
            gold_per_kill: int(8)?,
            gold_per_assist: int(9)?,
            gold_per_round_win: int(10)?,
            gold_per_most_damage: int(11)?,
            score_per_kill: uint(12)?,
            score_per_assist: uint(13)?,
            score_per_round_win: uint(14)?,
            place_rewards: place,
            damage_mult: num(16)?,
            knockback_mult: num(17)?,
            lava_damage_mult: num(18)?,
            shrink_delay_secs: num(19)?,
            shrink_ring_secs: num(20)?,
            pillar_mode: byte(21)?,
            ice_mode: byte(22)?,
            arena_shape: byte(23)?,
            gold_rewards_enabled: p[24] == "1",
            base_regen: num(25)?,
            game_mode: byte(26)?,
            team_count: byte(27)?,
            win_score: uint(28)?,
        })
    }

    /// 设置的**稳定哈希**（FNV-1a 64）：用于"配置是否变更"的比较（第 5 步：变更即取消全员准备）。
    /// 直接哈希紧凑串，避免逐字段比较遗漏。
    pub fn settings_hash(&self) -> u64 {
        let mut h: u64 = 0xcbf2_9ce4_8422_2325;
        for b in self.to_meta_string().as_bytes() {
            h ^= *b as u64;
            h = h.wrapping_mul(0x100_0000_01b3);
        }
        h
    }
}

impl Default for MatchConfig {
    fn default() -> Self {
        // 经济默认值对齐 **098c 全局声明 + 设置对话框**（`war3map_pretty.j` 205-224 / 18799-18813）：
        //   设置 10 ko=1 / 11 Ko=1 / 14 mo=2        ← 点数
        //   设置 12 lo=1 / 13 Lo=1 / 15 Mo=2        ← 金币（击杀/助攻/胜利）
        //   设置 16 po=1（Damage Gold Reward，回合结算时另加）
        //   设置 17 qo=10（**Gold per round** = 每轮基础金币）← 注意是 `qo` 不是 `po`
        //   Qo=20（初始金币）
        MatchConfig {
            total_rounds: 3,
            learn_time_secs: 30.0, // 098b wo=30
            gold_per_round: 10, // 设置 17 `qo`（此前误按 `po=1` 改成 1，已改回）
            gold_per_assist: 1,
            gold_per_round_win: 2,
            gold_per_kill: 1,
            gold_per_most_damage: 1, // 098c 设置 16 `po`
            place_rewards: Vec::new(),
            starting_gold: 20,
            // 098c 计分（JASS 实证；globals ko=1/Ko=1/mo=2）：胜利 2 分、击杀 1 分、助攻 1 分。
            // 注：MECHANICS.md §5「击杀 2 分」是笔误，实际 ko=1（war3map_pretty.j:2 / :9028 / :10292）。
            score_per_kill: 1,
            score_per_assist: 1,
            score_per_round_win: 2,
            game_mode: 1,
            base_regen: 0.5,
            team_count: 1,
            win_score: 10,
            shopping_time_secs: 40.0,
            // ── 玩法项默认值（098c 设置对话框/全局声明实证） ──
            damage_mult: 1.0,               // 设置 2
            knockback_mult: 1.0,            // 设置 3
            lava_damage_mult: 1.0,          // 设置 1（倍率语义：1.0 = 原版 `To=.9` 的"标准"档）
            first_round_time_secs: 40.0,    // 设置 5 `Uo`
            between_rounds_time_secs: 30.0, // 设置 4 `uo`
            shrink_delay_secs: 10.0,        // 设置 6 `wo`
            shrink_ring_secs: 10.0,         // 设置 6 `wo`（原版 10s/环，且延迟同为 wo×√存活）
            pillar_mode: 1,                 // 设置 8 `Po=0` → 随机
            ice_mode: 1,                    // 默认随机
            arena_shape: 0,                 // 圆形
            gold_rewards_enabled: true,
        }
    }
}

/// 098c 精通研究（D12.3，kf handler 实证）：学习期购买、不涨价、跨回合永久保留。
/// **上限（2026-09-12 w3q 实证）**：R00D/R00I/R00Y 各 **6 级**（tooltip 名字「… Mastery 1..6」+ `glvl=6`）；
/// **价格（w3q `gglb` 实证）**：生命 6 / 范围 7 / 射程 5 / 背包 3。
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub struct Mastery {
    /// R00D **Life steal Mastery**（生命精通）：伤害吸血 +8%/级。
    pub life: u8,
    /// R00I **Area of Effect mastery**（范围精通）：火球爆炸范围/力度 +12%/级（xi>0 火球才有落点爆炸）。
    pub range: u8,
    /// R00Y **Range Mastery**（射程精通）：法术持续/射程 +10%/级（火球系 +15%/级）。
    pub time: u8,
    /// R000 背包研究：升级 S128，容量 = (L²+L)/2（L = 1 + 购买数）：1→3→6→10→…。
    /// 098c `alev=3`（上限 6 格）；本作放开到 3 次（L4 = 10 格，突破 war3 限制）。
    pub backpack: u8,
}

impl Mastery {
    /// 购买价（**w3q `gglb` 实证**：生命 R00D=6 / 范围 R00I=7 / 射程 R00Y=5 / 背包 R000=3）。
    pub const COSTS: [i32; 4] = [6, 7, 5, 3];
    /// 级数上限：三精通各 **6**（w3q tooltip「Life steal Mastery 1..6」+ `glvl=6` 实证；
    /// R017 合成科技 glvl=20 → 6+6+6=18 ≤ 20 亦相符）；背包 **3**（098c `alev=3` 为 2 次，
    /// 本作放开 1 次到 L4 = 10 格，突破 war3 的 6 格上限）。
    pub const CAPS: [u8; 4] = [6, 6, 6, 3];

    /// 三精通总级数（击退减免用；背包不计——098c lf=vi+ei+xi）。
    pub fn levels(&self) -> u8 {
        self.life + self.range + self.time
    }

    fn at(&self, kind: usize) -> u8 {
        [self.life, self.range, self.time, self.backpack][kind]
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
    /// 累计伤害（098c `Rn[12+i]`：跨轮累计的造成伤害，记分板/排名用；仅统计展示）。
    pub total_damage: f64,
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
    /// 累计在技能购买/升级上花费的金币
    pub gold_spent: i32,
    // 战斗属性 / 成长点（4.6b）已删除（2026-09-12）：098c 无点数购买属性机制；
    // 成长轴 = 精通（mastery）+ 物品。
    /// 精通研究等级（098c，D12.3）：学习期购买、跨回合永久保留。
    pub mastery: Mastery,
    /// 队伍号（098c cn[]，B2）：默认 = 玩家 id（FFA）；分队由开局配置覆写。
    pub team: u8,
    /// 形态位（B4，按 SkillId 索引）：true=B 形态；学习界面 B 键切换。
    pub forms: Vec<bool>,
    /// 乔丹之石（098c `Hf`）：**按槽**记录各槽累计突破次数（索引 = `CastKey::as_u32()`）。
    /// 每买一颗戒指（5G）只能给**一个**槽 +2，用掉后戒指即被消耗（"can only be applied once"）；
    /// 但**可反复购买**，故次数不设上限（`cap_bonus = 2 × 次数`）。
    pub jordan_breaks: [u8; 8],
    /// 已购买的法术数（098c JASS `oi[id]`；`war3map_pretty.j` 25849 自增、20376 初始化）。
    /// 买下第 3/4/5 个法术时各触发一次 `Jf`，把全部升级科技的已研究等级 +1 →
    /// **此后每次技能升级都贵一个 `glvl`**（见 [`Self::upgrade_cost_escalated`]）。
    pub spell_buys: u8,
    /// 本回合造成的伤害（098c `Rn[i]`）：回合结算时用于判定"伤害最高者"（设置 16 `po` 奖励），
    /// 结算后清零。与 `score` 无关（`score` 是累计分）。
    pub damage_this_round: f64,
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
            total_damage: 0.0,
            current_streak: 0,
            first_blood_taken: false,
            rounds_survived: 0,
            best_placement: 0,
            skill_levels: vec![1; n],
            key_slots: [None; 8],
            items: Vec::new(),
            gold_spent: 0,
            mastery: Mastery::default(),
            team: player_id as u8,
            forms: vec![false; skill_count.max(crate::MAX_SKILL_SLOTS)],
            jordan_breaks: [0; 8],
            spell_buys: 0,
            damage_this_round: 0.0,
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

    /// 已购买的技能数量（键位已占用的数量）。用于 UI 显示「已购 N 个」。
    ///
    /// 注：098c 实测每个法术科技在 `war3map.w3q` 只有**单级金币成本**（10~15），
    /// 购买后科技即被 `SetPlayerTechMaxAllowed(...,0)` 锁死，`Jf` 抬级不生效——
    /// 故 098c **无**「买越多越贵」的功能性涨价（"Purchase cost..." 为遗留提示）。
    /// 各技能价格即 `SkillId::learn_cost`，不随已购数量变化。
    pub fn purchased_spell_count(&self) -> usize {
        self.key_slots.iter().filter(|s| s.is_some()).count()
    }

    /// 花钱购买某键（树）下的一个技能：扣金币、置 1 级、锁定该树其余技能。
    ///
    /// 对标 098c `kf`：技能经 WC3 科技树购买扣金（此处直接扣 `gold`），
    /// 每个技能有固定单价（`SkillId::learn_cost`，10~15，单级、不随数量涨价），
    /// 同树（槽）内 3 选 1 互斥、整场锁定不可改（无洗点/解绑）。
    ///
    /// 返回是否购买成功：
    /// - 技能不属于该键对应的树 → 失败；
    /// - 该键已被占用（整场锁定）→ 失败；
    /// - 金币不足 → 失败（不绑定、不扣金）。
    pub fn purchase_skill(&mut self, key: CastKey, skill: SkillId) -> bool {
        if !key.tree().skills_in_tree().contains(&skill) {
            return false;
        }
        let idx = key.as_u32() as usize;
        if self.key_slots[idx].is_some() {
            return false;
        }
        let cost = skill.learn_cost();
        if self.gold < cost {
            return false;
        }
        self.gold -= cost;
        self.gold_spent += cost;
        let sidx = skill.as_u32() as usize;
        if let Some(lv) = self.skill_levels.get_mut(sidx) {
            *lv = 1;
        }
        // 098c `war3map_pretty.j` 25849：买下法术即 `oi[id] = oi[id] + 1`（驱动后续涨价）。
        self.spell_buys = self.spell_buys.saturating_add(1);
        self.key_slots[idx] = Some(skill);
        true
    }

    /// 某键（树）是否已锁定（已购买技能、整场不可改）。用于 UI 判定同树其余技能是否可购。
    pub fn slot_locked(&self, key: CastKey) -> bool {
        self.key_slots[key.as_u32() as usize].is_some()
    }

    /// 购买/升级物品（M3）：金币不足失败；**同家族只能按档位逐级进化**
    /// （098c `bD`：无→1档；持1→买2；持2→买3；持3→退款/拒绝）。每步同价。
    /// 独立物品（Standalone：死亡面具/法杖等）各自独立，可共存、不可重复持有。
    pub fn buy_item(&mut self, id: crate::item::ItemId) -> bool {
        let def = id.def();
        let cost = def.cost;
        if self.gold < cost {
            return false;
        }
        if def.family == crate::item::ItemFamily::Standalone {
            if self.items.contains(&id) {
                return false;
            }
            if self.items.len() >= self.inventory_slots() {
                return false;
            }
            self.gold -= cost;
            self.items.push(id);
            return true;
        }
        // 链式物品：只接受「下一档」（无持有=最低档；已满级=拒绝）。
        let family = def.family;
        let owned = self.items.iter().copied().find(|it| it.def().family == family);
        let expected = match owned {
            None => match crate::item::ItemDef::chain(family).first() {
                Some(first) => first.id,
                None => return false,
            },
            Some(cur) => match cur.next_tier() {
                Some(next) => next,
                None => return false, // 已满级
            },
        };
        if id != expected {
            return false; // 不能跳档购买
        }
        // 首次入包受容量限制；升级是替换，不受限。
        if owned.is_none() && self.items.len() >= self.inventory_slots() {
            return false;
        }
        self.gold -= cost;
        if owned.is_some() {
            self.items.retain(|it| it.def().family != family);
        }
        self.items.push(id);
        true
    }

    /// 卖出物品（098c `-sell #` 原生化为界面操作）：按 `ItemDef.sell` 返还金币并移除。
    /// 影响物品集合的聚合效果（调用方需 `world` 侧重算 `item_fx`）。
    pub fn sell_item(&mut self, id: crate::item::ItemId) -> bool {
        let Some(pos) = self.items.iter().position(|&it| it == id) else {
            return false;
        };
        let price = id.def().sell;
        self.items.remove(pos);
        self.gold += price;
        true
    }

    /// 购买 1 级精通（kind：0=生命 1=远程 2=时间 3=背包）。098c kf：不涨价、永久保留。
    pub fn buy_mastery(&mut self, kind: usize) -> bool {
        let cost = Mastery::COSTS[kind];
        if self.gold < cost || self.mastery.at(kind) >= Mastery::CAPS[kind] {
            return false;
        }
        self.gold -= cost;
        match kind {
            0 => self.mastery.life += 1,
            1 => self.mastery.range += 1,
            2 => self.mastery.time += 1,
            _ => self.mastery.backpack += 1,
        }
        true
    }

    /// 物品栏可用格数：容量 = `0.5×(L²+L)`，`L = 1 + 背包研究购买数`。
    ///
    /// **与 098c 的关系（有意偏离，非 bug）**：
    /// - 098c 的 w3q `R000`(Inventory) 逐级 tooltip = 基础 1 → +2 → +3 →（Lv4 起 +1），
    ///   累进为 1 / 3 / 6 / 7 / …；
    /// - 但 **war3 引擎本身最多 6 格**，所以 098c 在 Lv3（6 格）之后无论如何加不出更多格 ——
    ///   Lv4 那行 "+1 additional item slot" 是**引擎上限下的死数据**；
    /// - 我方不受该载体限制：`CAPS[3] = 3` 允许买满 3 级，`L = 4` → **10 格**，
    ///   让"背包研究"这条线在被截断后仍然有可感知的收益（D13：只复刻功能本质，用原生载体）。
    ///
    /// 因此 `L=1/2/3/4 → 1/3/6/10` 中的**前三项与 098c 完全一致**，第四项是我方扩展。
    pub fn inventory_slots(&self) -> usize {
        let l = 1 + self.mastery.backpack as usize;
        (l * l + l) / 2
    }

    /// 购买/升级某技能一级。返回是否成功（金币不足则失败）；成功计入累计花费。
    ///
    /// `cost(当前等级) -> 升级到 当前等级+1 的价格`。调用方负责提供价格表。
    ///
    /// 注：升级是对已购技能升等级（1→2→…），不触发同树互斥，也不受涨价影响。
    pub fn upgrade_skill(&mut self, skill: SkillId, cost: i32) -> bool {
        if self.gold < cost {
            return false;
        }
        let idx = skill.as_u32() as usize;
        // 等级上限：098c 基础档数 + 上限突破（乔丹之石原生化为购买项，每档 +2）。
        let cap = crate::skill::DefTable::max_level(skill) + self.cap_bonus_for_skill(skill);
        if self.skill_levels[idx] >= cap {
            return false;
        }
        self.gold -= cost;
        self.gold_spent += cost;
        self.skill_levels[idx] += 1;
        true
    }
    /// 因「已购买法术数」造成的升级涨价档数（098c `oi[id] > 2` → 每买一个触发一次 `Jf`，最多到 `oi == 6`）。
    ///
    /// `spell_buys` 为 3/4/5 时各已触发一次（买第 6 个法术时 `oi == 6`，JASS 显式跳过不触发）。
    pub fn spell_cost_step(&self) -> i32 {
        self.spell_buys.saturating_sub(2).min(3) as i32
    }

    /// 该技能**当前**的升级价：基础升级价 + 涨价档数 × `glvl`（098c war3 升级金价公式）。
    pub fn upgrade_cost_escalated(&self, skill: SkillId) -> i32 {
        skill.upgrade_cost() + self.spell_cost_step() * crate::skill::SkillId::UPGRADE_COST_PER_LEVEL
    }

    /// 该槽的乔丹之石突破**次数**（098c `Hf`：每颗戒指只 +2 一次，但可反复购买）。
    pub fn jordan_breaks_for(&self, key: crate::skill::CastKey) -> u8 {
        self.jordan_breaks[key.as_u32() as usize]
    }

    /// 该槽因乔丹之石获得的上限加成：`2 × 突破次数`。
    pub fn cap_bonus_for(&self, key: crate::skill::CastKey) -> u32 {
        2 * self.jordan_breaks_for(key) as u32
    }

    /// 该技能所在槽的乔丹之石突破次数。
    pub fn jordan_breaks_for_skill(&self, skill: SkillId) -> u8 {
        Self::key_of_skill(skill)
            .map(|k| self.jordan_breaks_for(k))
            .unwrap_or(0)
    }

    /// 该技能所属的 `CastKey`（8 槽之一）。
    pub fn key_of_skill(skill: SkillId) -> Option<crate::skill::CastKey> {
        crate::skill::CastKey::ALL
            .iter()
            .copied()
            .find(|k| k.tree().skills_in_tree().contains(&skill))
    }

    /// 该技能的等级上限加成（乔丹之石）。
    pub fn cap_bonus_for_skill(&self, skill: SkillId) -> u32 {
        Self::key_of_skill(skill)
            .map(|k| self.cap_bonus_for(k))
            .unwrap_or(0)
    }

    /// 用一颗乔丹之石给**该技能所在槽**的上限 +2（098c `Hf`）。
    ///
    /// 实证（`war3map_pretty.j` 25343–25490）：买戒指 `h004`（5G）→ `iV[312+id]=true` 解锁石头
    /// `T000`–`T006`；使用**任意一颗**后 → 该槽科技上限 +2、**7 颗石头全部禁用**、`iV[312+id]=false`
    /// （提示 "Stone of Jordan Ring has been applied" / "can only be applied once"）。
    /// 即**一颗戒指只换一次 +2**，想再突破必须**再花 5 金**；故此处按次收费、不设次数上限。
    /// 戒指物品 `I00E` 只是 war3 载体（状态全在 `iV` flag 里），我方**不占物品栏**。
    pub fn break_cap_for(&mut self, skill: SkillId) -> bool {
        let Some(key) = Self::key_of_skill(skill) else {
            return false;
        };
        // 098c：石头只对「该槽已装备技能」可用（`kn[7*id+slot] != 0`）。
        if self.bound_skill(key) != Some(skill) {
            return false;
        }
        if self.gold < JORDAN_PRICE {
            return false;
        }
        self.gold -= JORDAN_PRICE;
        self.gold_spent += JORDAN_PRICE;
        let slot = key.as_u32() as usize;
        self.jordan_breaks[slot] = self.jordan_breaks[slot].saturating_add(1);
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
        let team_count = config.team_count.max(1);
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
        // 分队（098c kX -mgl 语义，B2）：2 队 = 按 id 序对半分；FFA = 各自一队（cn[i]=i）。
        if team_count >= 2 {
            let mut ids = m.profiles.iter().map(|p| p.player_id).collect::<Vec<_>>();
            ids.sort();
            let half = ids.len().div_ceil(2);
            for (i, &id) in ids.iter().enumerate() {
                if let Some(pr) = m.profiles.iter_mut().find(|p| p.player_id == id) {
                    pr.team = if i < half { 0 } else { 1 };
                }
            }
        }
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
        // 098c 设置 16 `po`「Damage Gold Reward」（5364-5379 实证）：
        // 回合结束时，**本回合伤害最高**的玩家（并列者都算）各得 `po` 金；随后清零本回合伤害。
        // 注：与击杀/胜利金独立，是"最高伤害独占奖"，不按伤害量比例发放。
        let most = self
            .profiles
            .iter()
            .map(|p| p.damage_this_round)
            .fold(0.0_f64, f64::max);
        if most > 0.0 {
            let reward = self.config.gold_per_most_damage;
            for p in self.profiles.iter_mut() {
                if p.damage_this_round >= most {
                    p.gold += reward;
                }
            }
        }
        for p in self.profiles.iter_mut() {
            p.damage_this_round = 0.0;
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

    /// 直接给某玩家加金（模式专属奖励用；098c 化身模式 `AI` 的 `+lo` / `+1`）。
    pub fn grant_gold(&mut self, player_id: u32, amount: i32) {
        if let Some(p) = self.profiles.iter_mut().find(|pr| pr.player_id == player_id) {
            p.gold += amount;
        }
    }

    /// 化身模式计分（098c L12055：Ln += 本轮伤害/20，B3）。
    pub fn register_damage_score(&mut self, player_id: u32, damage: f64) {
        if let Some(p) = self.profiles.iter_mut().find(|pr| pr.player_id == player_id) {
            p.score += (damage / 20.0) as u32;
            // 098c `Rn[i]`：本回合伤害累计（回合结算判"最高伤害"，见 `finish_round`）。
            p.damage_this_round += damage;
        }
    }

    /// 死亡结算（D6）：受害者连杀清零（连杀播报由调用方在清零前读取）。
    /// 死亡竞赛（模式 2，098c L2998）：死者分数 -1。
    pub fn register_death(&mut self, victim_id: u32) -> u32 {
        let mut streak = 0;
        if let Some(p) = self
            .profiles
            .iter_mut()
            .find(|pr| pr.player_id == victim_id)
        {
            streak = p.current_streak;
            p.current_streak = 0;
            if self.config.game_mode == 2 {
                p.score = p.score.saturating_sub(1);
            }
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
                // 098c `Lo`（Assist Gold Reward，全局默认 1）—— 旧值写死 0 且注释引 098b，已更正。
                p.gold += self.config.gold_per_assist;
            }
        }
    }

    /// 轮胜利结算（D6）：每轮最后存活者 +分（组队模式待 M4 队伍系统）。
    /// 模式名（房间列表/HUD 显示，D13 #1）。
    pub fn mode_name(mode: u8) -> &'static str {
        match mode {
            2 => "死亡竞赛",
            3 => "化身",
            4 => "国王",
            5 => "最后生还",
            _ => "轮次",
        }
    }

    pub fn register_round_win(&mut self, winner_id: u32) {
        if let Some(p) = self
            .profiles
            .iter_mut()
            .find(|pr| pr.player_id == winner_id)
        {
            p.score += self.config.score_per_round_win;
            // 098c `Mo`（Win Gold Reward，全局默认 2）—— 此前完全未发胜利金。
            p.gold += self.config.gold_per_round_win;
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
        // 第一局 = 初始金币 50 + 每轮金（098c 设置 17 `qo` = 10）= 60
        assert_eq!(m.profiles[0].gold, 50 + 10);
        assert_eq!(m.profiles[1].gold, 50 + 10);
    }

    /// 模式专属奖励直发（098c 化身模式 `AI` 的 `+lo` / `+1`）。
    /// 乔丹之石（098c `T000`–`T006`）：**按槽** +2、免费、每槽一次、需持戒指。
    #[test]
    fn jordan_breaks_cap_repeatable_at_5g_each() {
        use crate::skill::SkillId;
        let mut m = MatchState::new(MatchConfig::default(), &[0], 8);
        let p = &mut m.profiles[0];
        let key = PlayerProfile::key_of_skill(SkillId::S000).expect("S000 应有所属槽");
        let base = crate::skill::DefTable::max_level(SkillId::S000);
        let cost = SkillId::S000.upgrade_cost();
        // 未装备该技能 → 不能突破（098c `kn[7*id+slot] != 0`）
        assert_eq!(p.cap_bonus_for_skill(SkillId::S000), 0);
        assert!(!p.break_cap_for(SkillId::S000), "该槽未绑定该技能时不能突破");
        p.key_slots[key.as_u32() as usize] = Some(SkillId::S000);
        p.gold = 200;

        // ---- 第 1 轮：升满 → 突破（-5G）→ 上限 base+2 ----
        while p.upgrade_skill(SkillId::S000, cost) {} // 一路升到当前上限
        assert_eq!(p.skill_level(SkillId::S000), base, "应先升到基础上限");
        let g0 = p.gold;
        assert!(p.break_cap_for(SkillId::S000), "首次突破应成功");
        assert_eq!(p.gold, g0 - JORDAN_PRICE, "每次突破扣 5 金");
        assert_eq!(p.cap_bonus_for_skill(SkillId::S000), 2);
        assert!(p.items.is_empty(), "乔丹之石不进入物品栏");
        // 突破后**还能继续升级**到新上限（这就是 UI 上「升级到 Lv{n+1}」按钮）
        let mut up = 0;
        while p.upgrade_skill(SkillId::S000, cost) {
            up += 1;
        }
        assert_eq!(up, 2, "突破后应能再升 2 级");
        assert_eq!(p.skill_level(SkillId::S000), base + 2, "新上限 = base+2");

        // ---- 第 2 轮：再次满级 → **再次突破**（再 -5G）→ 上限 base+4 ----
        let g1 = p.gold;
        assert!(
            p.break_cap_for(SkillId::S000),
            "同一槽可反复突破（这正是 UI 上再次出现的「突破上限 +2」按钮）"
        );
        assert_eq!(p.gold, g1 - JORDAN_PRICE, "第二次突破也要 5 金，不是免费");
        assert_eq!(p.cap_bonus_for_skill(SkillId::S000), 4, "两次突破 = +4");
        let mut up2 = 0;
        while p.upgrade_skill(SkillId::S000, cost) {
            up2 += 1;
        }
        assert_eq!(up2, 2, "第二次突破后又能再升 2 级");
        assert_eq!(p.skill_level(SkillId::S000), base + 4, "上限随每次突破累加");
        assert!(!p.upgrade_skill(SkillId::S000, cost), "base+4 到顶");
        assert_eq!(p.jordan_breaks_for_skill(SkillId::S000), 2);

        // 金币不足 → 失败且不扣钱
        p.gold = JORDAN_PRICE - 1;
        assert!(!p.break_cap_for(SkillId::S000), "金币不足不能突破");
        assert_eq!(p.gold, JORDAN_PRICE - 1, "失败不应扣钱");
    }

    /// 乔丹之石在不同槽之间各自累计、互不影响。
    /// 技能涨价（098c `oi[id]` + `Jf`）：买第 3/4/5 个法术各触发一次，
    /// 每次让所有技能升级价 +`glvl`（w3q `glvl` 实证 = 10）；第 6 个不再触发。
    fn spell_upgrade_cost_escalates_after_third_purchase() {
        use crate::skill::SkillId;
        let per_level = SkillId::UPGRADE_COST_PER_LEVEL;
        let mut m = MatchState::new(MatchConfig::default(), &[0], 34);
        let p = &mut m.profiles[0];
        p.gold = 10_000;
        let base = SkillId::S002.upgrade_cost();

        // 档位公式（098c：`oi[id] > 2` 起每买一个触发一次 `Jf`，`oi == 6` 时 JASS 显式跳过）
        let expect = |buys: u8| (buys.saturating_sub(2).min(3)) as i32;
        for buys in 0u8..=8 {
            p.spell_buys = buys;
            assert_eq!(p.spell_cost_step(), expect(buys), "buys={buys} 的涨价档");
            assert_eq!(
                p.upgrade_cost_escalated(SkillId::S002),
                base + expect(buys) * per_level,
                "buys={buys} 的升级价"
            );
        }

        // 真实购买也要计数（098c `oi[id] = oi[id] + 1`）
        p.spell_buys = 0;
        let key = PlayerProfile::key_of_skill(SkillId::S002).expect("S002 应有所属槽");
        assert!(p.purchase_skill(key, SkillId::S002), "购买应成功");
        assert_eq!(p.spell_buys, 1, "买下法术应使计数 +1");
    }

    #[test]
    fn jordan_breaks_are_per_slot_cumulative() {
        use crate::skill::SkillId;
        let mut m = MatchState::new(MatchConfig::default(), &[0, 1], 8);
        let p = &mut m.profiles[0];
        p.gold = 30;
        let k0 = PlayerProfile::key_of_skill(SkillId::S000).unwrap();
        let k1 = PlayerProfile::key_of_skill(SkillId::S002).unwrap();
        assert_ne!(k0.as_u32(), k1.as_u32(), "两个技能应在不同槽");
        p.key_slots[k0.as_u32() as usize] = Some(SkillId::S000);
        p.key_slots[k1.as_u32() as usize] = Some(SkillId::S002);
        assert!(p.break_cap_for(SkillId::S000));
        assert_eq!(p.cap_bonus_for_skill(SkillId::S000), 2);
        assert_eq!(p.cap_bonus_for_skill(SkillId::S002), 0, "其他槽不受影响");
        assert!(p.break_cap_for(SkillId::S002));
        assert_eq!(p.cap_bonus_for_skill(SkillId::S002), 2);
        assert_eq!(p.gold, 20, "两次突破共 10 金");
        assert_eq!(p.jordan_breaks_for_skill(SkillId::S000), 1);
        assert_eq!(p.jordan_breaks_for_skill(SkillId::S002), 1);
    }

    /// 伤害金（098c 设置 16 `po`）：回合结束时**伤害最高者**得金，并列都拿，随后清零。
    #[test]
    fn most_damage_in_round_gets_po_gold() {
        let mut m = MatchState::new(MatchConfig::default(), &[0, 1, 2], 8);
        assert_eq!(m.config.gold_per_most_damage, 1, "098c `po` 默认 1");
        let before: Vec<i32> = m.profiles.iter().map(|p| p.gold).collect();
        // 玩家 0 打 50、玩家 1 打 50（并列最高）、玩家 2 打 10
        m.register_damage_score(0, 50.0);
        m.register_damage_score(1, 50.0);
        m.register_damage_score(2, 10.0);
        m.finish_round(vec![0, 1, 2]);
        assert_eq!(m.profiles[0].gold, before[0] + 1, "并列最高者 0 应得 po");
        assert_eq!(m.profiles[1].gold, before[1] + 1, "并列最高者 1 应得 po");
        assert_eq!(m.profiles[2].gold, before[2], "非最高者不得");
        for p in &m.profiles {
            assert_eq!(p.damage_this_round, 0.0, "回合伤害应清零");
        }
    }

    #[test]
    fn grant_gold_direct_reward() {
        let mut m = MatchState::new(MatchConfig { game_mode: 3, ..Default::default() }, &[0, 1], 8);
        let before = m.profiles[0].gold;
        m.grant_gold(0, 1);
        assert_eq!(m.profiles[0].gold, before + 1, "应直接加 1 金");
        m.grant_gold(0, 0);
        assert_eq!(m.profiles[0].gold, before + 1, "加 0 不应变化");
        let unknown = m.profiles[1].gold;
        m.grant_gold(99, 5); // 不存在的玩家：静默忽略
        assert_eq!(m.profiles[1].gold, unknown);
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
        // 打两轮（每轮胜者 0 得 击杀 1 分 + 轮胜 2 分）
        for _ in 0..2 {
            m.register_kill(0);
            m.register_round_win(0);
            m.finish_round(vec![0, 1]);
        }
        // score = 2 轮 × (杀 1 + 胜 2) = 6 ≥ 3 → 已提前终局
        assert_eq!(m.phase, MatchPhase::Finished, "En2 达到胜利分应提前终局");
        let ranking = m.final_ranking();
        assert_eq!(ranking[0], (0, 6), "按分数降序，实际 {ranking:?}");
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
        // 098c 全局默认 + 设置项（`war3map_pretty.j` 205-224 / 18799-18813）：
        // Qo=20 初始金、qo=10 每轮金（设置 17）、lo=1/Lo=1/Mo=2 击杀/助攻/胜利金。
        assert_eq!(config.starting_gold, 20, "初始金币 Qo");
        assert_eq!(config.gold_per_round, 10, "每轮金币 = 设置 17 `qo`");
        assert_eq!(config.gold_per_kill, 1, "击杀金 lo");
        assert_eq!(config.gold_per_assist, 1, "助攻金 Lo");
        assert_eq!(config.gold_per_round_win, 2, "胜利金 Mo");
        assert!(config.place_rewards.is_empty(), "098c 无名次金（奖励走 lo/Lo/Mo/po + ko/Ko/mo）");
        // 098c 计分（JASS 实证 globals ko=1/Ko=1/mo=2）：胜 2 / 杀 1 / 助 1
        assert_eq!((config.score_per_kill, config.score_per_assist, config.score_per_round_win), (1, 1, 2));
        // 开局购物 Wo=40 / 每轮 wo=30（D6/M4 En 批）
        assert_eq!(config.shopping_time_secs, 40.0);
        assert_eq!(config.learn_time_secs, 30.0);
        assert_eq!(config.game_mode, 1);
        let mut m = MatchState::new(config, &[0, 1], 8);
        assert_eq!(m.profiles[0].gold, 20 + 10, "开局 Qo=20 + 首轮 qo=10");
        // 击杀：发分 + 发金（098c `ko=1` / `lo=1`）
        m.register_kill(0);
        assert_eq!(m.profiles[0].gold, 30 + 1, "击杀金 lo=1（基线 30 = 20+10）");
        assert_eq!(m.profiles[0].score, 1, "098c 击杀 1 分（globals ko=1）");
        assert_eq!(m.profiles[0].current_streak, 1);
        // 助攻
        m.register_assists(1, 0, &[0, 1]);
        assert_eq!(m.profiles[0].score, 1, "击杀者不算助攻");
        // 轮胜分
        m.register_round_win(0);
        assert_eq!(m.profiles[0].score, 3, "098c 轮胜 2 分（1 击杀 + 2 轮胜）");
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
        pr.mastery.backpack = 3; // 容量 10，避免容量影响本测试
        // 买头盔 1（098c 买价 9）
        assert!(pr.buy_item(crate::item::ItemId::Helm1));
        assert_eq!(pr.gold, 11);
        assert_eq!(pr.items, vec![crate::item::ItemId::Helm1]);
        // 升级头盔 2（098c 同价 9 金）——每步同价
        assert!(pr.buy_item(crate::item::ItemId::Helm2));
        assert_eq!(pr.gold, 2);
        assert_eq!(pr.items, vec![crate::item::ItemId::Helm2], "同家族应替换为高档");
        // 不同家族共存（098c：每步同价 5 金）
        pr.gold = 10;
        assert!(pr.buy_item(crate::item::ItemId::Boots1));
        assert_eq!(pr.items.len(), 2);
    }

    #[test]
    fn buy_item_allows_distinct_standalone_and_no_dup() {
        let mut ms = MatchState::new(MatchConfig::default(), &[0, 1], 34);
        let pr = &mut ms.profiles[0];
        pr.gold = 100;
        pr.mastery.backpack = 3; // 容量 10，避免容量影响本测试
        // 独立物品（死亡面具/火球法杖）应可共存，互不替换
        // （乔丹之石已不占物品栏：它是技能页的一次性突破解锁，见 `jordan_*`）
        assert!(pr.buy_item(crate::item::ItemId::FireMask));
        assert!(pr.buy_item(crate::item::ItemId::FireStaff));
        assert_eq!(pr.items.len(), 2, "独立物品应共存而非互相删除");
        // 重复购买同一独立物品应失败（不再扣钱）
        let g = pr.gold;
        assert!(!pr.buy_item(crate::item::ItemId::FireMask), "重复持有应被拒绝");
        assert_eq!(pr.gold, g, "重复购买不应扣钱");
    }

    #[test]
    fn buy_item_requires_stepwise_tier_upgrade() {
        let mut ms = MatchState::new(MatchConfig::default(), &[0, 1], 34);
        let pr = &mut ms.profiles[0];
        pr.gold = 100;
        pr.mastery.backpack = 3; // 容量充足
        // 不能跳档：直接买三级靴应失败。
        assert!(!pr.buy_item(crate::item::ItemId::Boots3), "不能跳档直接买三级");
        assert!(!pr.buy_item(crate::item::ItemId::Boots2), "首件不能直接买二级");
        // 逐级进化：1 → 2 → 3，同族只保留一件。
        assert!(pr.buy_item(crate::item::ItemId::Boots1));
        assert_eq!(pr.items, vec![crate::item::ItemId::Boots1]);
        assert!(pr.buy_item(crate::item::ItemId::Boots2));
        assert_eq!(pr.items, vec![crate::item::ItemId::Boots2], "升级应替换低档");
        assert!(pr.buy_item(crate::item::ItemId::Boots3));
        assert_eq!(pr.items, vec![crate::item::ItemId::Boots3]);
        // 满级后再买应失败。
        assert!(!pr.buy_item(crate::item::ItemId::Boots3), "满级应拒绝");
    }

    #[test]
    fn sell_item_refunds_sell_price_and_removes() {
        let mut ms = MatchState::new(MatchConfig::default(), &[0, 1], 34);
        let pr = &mut ms.profiles[0];
        pr.gold = 100;
        pr.mastery.backpack = 3;
        assert!(pr.buy_item(crate::item::ItemId::Helm1));
        let gold_after_buy = pr.gold;
        let price = crate::item::ItemId::Helm1.def().sell;
        assert!(pr.sell_item(crate::item::ItemId::Helm1));
        assert!(pr.items.is_empty());
        assert_eq!(pr.gold, gold_after_buy + price, "卖出应返还 sell 价");
        // 未持有则卖出失败、不加钱。
        let g = pr.gold;
        assert!(!pr.sell_item(crate::item::ItemId::Helm1));
        assert_eq!(pr.gold, g);
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
    fn purchase_skill_spends_gold_locks_slot_no_escalation() {
        let mut p = PlayerProfile::new(0, 8);
        p.gold = 200;
        // 购买 D 树技能 S002（learn_cost=11，固定单价，不随已购数量涨价）
        assert!(p.purchase_skill(CastKey::D, SkillId::S002));
        assert_eq!(p.gold, 189);
        assert_eq!(p.bound_skill(CastKey::D), Some(SkillId::S002));
        assert_eq!(p.skill_level(SkillId::S002), 1);
        assert_eq!(p.gold_spent, 11);

        // 同树（槽）内互斥：D 树其它技能不可再购（整场锁定）
        assert!(!p.purchase_skill(CastKey::D, SkillId::S003));
        assert_eq!(p.bound_skill(CastKey::D), Some(SkillId::S002));

        // 技能不属于该键的树 → 失败
        assert!(!p.purchase_skill(CastKey::D, SkillId::S008));

        // 第 2、3 个技能按各自基础价（S008=14, S011=11）
        assert!(p.purchase_skill(CastKey::E, SkillId::S008));
        assert_eq!(p.gold, 175);
        assert!(p.purchase_skill(CastKey::R, SkillId::S011));
        assert_eq!(p.gold, 164);
        assert_eq!(p.purchased_spell_count(), 3);

        // 098c 无功能性涨价：第 4 个技能仍按基础价（S014=14），不叠加
        let before = p.gold;
        assert!(p.purchase_skill(CastKey::T, SkillId::S014));
        assert_eq!(p.gold, before - 14);
        assert_eq!(p.purchased_spell_count(), 4);
    }

    #[test]
    fn purchase_skill_fails_when_poor_or_occupied() {
        let mut p = PlayerProfile::new(1, 8);
        p.gold = 5;
        // 金币不足：S002 基础价 11
        assert!(!p.purchase_skill(CastKey::D, SkillId::S002));
        assert_eq!(p.bound_skill(CastKey::D), None);
        assert_eq!(p.gold, 5); // 未扣金

        // 充值后购买成功
        p.gold = 50;
        assert!(p.purchase_skill(CastKey::D, SkillId::S002));
        // 该键已锁定，重复购买同一技能也失败（整场不可改）
        assert!(!p.purchase_skill(CastKey::D, SkillId::S002));
        assert_eq!(p.bound_skill(CastKey::D), Some(SkillId::S002));
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

    // 4.6b 成长点测试已随属性系统删除（2026-09-12）。
}

    /// 精通价格/上限与 098c `war3map.w3q` 交叉校验（真值源：各 mastery 升级条目）。
    ///
    /// | 条目 | gnam | `gglb`(金价) | `glvl`(每级) | 对应 |
    /// |---|---|---|---|---|
    /// | `R00D` | Life steal Mastery 6 | 6 | 6 | COSTS[0] / CAPS[0] |
    /// | `R00I` | Area of Effect mastery 6 | 7 | 6 | COSTS[1] / CAPS[1] |
    /// | `R00Y` | Range Mastery 6 | 5 | 6 | COSTS[2] / CAPS[2] |
    /// | `R000` | Inventory | 3 | 3 | COSTS[3] / CAPS[3] |
    /// 背包格数：前三级与 098c w3q `R000` 累进一致（1/3/6），第四级是我方对
    /// "war3 6 格上限"的有意放开（10 格）—— 见 `inventory_slots` 的文档注释。
    #[test]
    fn inventory_slots_base_matches_w3q_and_extends_past_war3_cap() {
        let mut m = MatchState::new(MatchConfig::default(), &[0], 34);
        let p = &mut m.profiles[0];
        p.gold = 1000;
        assert_eq!(p.inventory_slots(), 1, "未研究：1 格（w3q Lv1 基线）");
        assert!(p.buy_mastery(3));
        assert_eq!(p.inventory_slots(), 3, "研究 1 级：3 格（w3q Lv2 的 +2）");
        assert!(p.buy_mastery(3));
        assert_eq!(p.inventory_slots(), 6, "研究 2 级：6 格（w3q Lv3 的 +3）");
        assert!(p.buy_mastery(3));
        assert_eq!(p.inventory_slots(), 10, "研究 3 级：10 格（我方放开 war3 的 6 格上限）");
        // 已达精通上限，不能再买
        assert!(!p.buy_mastery(3), "背包研究上限 = CAPS[3] = 3");
    }

    /// 房间设置默认值交叉校验：全部 = 098c 全局声明 / 设置对话框的值
    /// （`war3map_pretty.j` 205-224 全局、18768-18813 设置项）。
    /// 房间设置串往返：默认值编解码一致、哈希稳定、改动任一字段哈希变化。
    #[test]
    fn room_settings_meta_string_roundtrip() {
        let d = MatchConfig::default();
        let s = d.to_meta_string();
        let back = MatchConfig::from_meta_string(&s).expect("应能解析");
        assert_eq!(back, d, "默认配置往返应完全一致");
        assert_eq!(back.settings_hash(), d.settings_hash(), "哈希应稳定");
        // 任一字段变化 → 哈希变化（第 5 步"改设置取消准备"依赖它）
        let mut m = d.clone();
        m.gold_per_kill += 1;
        assert_ne!(m.settings_hash(), d.settings_hash(), "改金价哈希应变化");
        let mut m2 = d.clone();
        m2.pillar_mode = 2;
        assert_ne!(m2.settings_hash(), d.settings_hash(), "改柱子模式哈希应变化");
        // 畸形串/版本不符 → None（调用方回退默认值）
        assert!(MatchConfig::from_meta_string("").is_none());
        assert!(MatchConfig::from_meta_string("9|1|2").is_none());
        let wrong_schema = s.replacen('1', "999", 1);
        assert!(MatchConfig::from_meta_string(&wrong_schema).is_none(), "schema 不符应拒绝");
        // 带非空名次奖励
        let mut mp = d.clone();
        mp.place_rewards = vec![3, 2, 1];
        let sp = mp.to_meta_string();
        assert_eq!(MatchConfig::from_meta_string(&sp).unwrap(), mp, "名次奖励应往返");
    }

    #[test]
    fn room_settings_defaults_match_098c() {
        use crate::skill::SkillId as _;
        let c = MatchConfig::default();
        // 经济（设置 10-17 + 初始金）
        assert_eq!(c.starting_gold, 20, "初始金 Qo=20");
        assert_eq!(c.gold_per_round, 10, "设置 17 qo=10");
        assert_eq!(c.gold_per_kill, 1, "设置 12 lo=1");
        assert_eq!(c.gold_per_assist, 1, "设置 13 Lo=1");
        assert_eq!(c.gold_per_round_win, 2, "设置 15 Mo=2");
        assert_eq!(c.gold_per_most_damage, 1, "设置 16 po=1");
        assert_eq!(
            (c.score_per_kill, c.score_per_assist, c.score_per_round_win),
            (1, 1, 2),
            "设置 10/11/14 ko/Ko/mo=1/1/2"
        );
        assert!(c.gold_rewards_enabled, "默认开启金币奖励（等价未使用 -no reward）");
        // 玩法（设置 1-6/8/9）
        assert_eq!(c.damage_mult, 1.0, "设置 2 默认 1.0 倍");
        assert_eq!(c.knockback_mult, 1.0, "设置 3 默认 1.0 倍");
        assert_eq!(c.lava_damage_mult, 1.0, "设置 1 标准档（倍率语义 1.0）");
        assert_eq!(c.first_round_time_secs, 40.0, "设置 5 Uo=40（第一轮配置期）");
        assert_eq!(c.between_rounds_time_secs, 30.0, "设置 4 uo=30（局间配置期）");
        assert_eq!(c.shrink_delay_secs, 10.0, "设置 6 wo=10");
        assert_eq!(c.shrink_ring_secs, 10.0, "设置 6 wo=10（每环时长）");
        assert_eq!(c.base_regen, 0.5, "设置 9 In=.05/0.1s = 0.5/s");
        // 柱 / 冰 / 地图
        assert_eq!(c.pillar_mode, 1, "设置 8 Po=0 → 随机");
        assert_eq!(c.ice_mode, 1, "冰面默认随机");
        assert_eq!(c.arena_shape, 0, "当前仅圆形");
    }

    #[test]
    fn mastery_costs_and_caps_match_w3q() {
        // 顺序：0=生命汲取 1=范围 2=射程 3=背包（与 `Mastery::at` 一致）。
        assert_eq!(Mastery::COSTS, [6, 7, 5, 3], "w3q gglb：R00D/R00I/R00Y/R000");
        assert_eq!(Mastery::CAPS, [6, 6, 6, 3], "w3q glvl：R00D/R00I/R00Y/R000");
    }

    #[test]
    fn mastery_buy_costs_caps_and_backpack() {
        let mut ms = MatchState::new(MatchConfig::default(), &[0, 1], 34);
        let pr = &mut ms.profiles[0];
        pr.gold = 100;
        // 四系各买一级（生命6/范围7/射程5/背包3 = 21 金；w3q gglb 实证）
        assert!(pr.buy_mastery(0) && pr.buy_mastery(1) && pr.buy_mastery(2) && pr.buy_mastery(3));
        assert_eq!(pr.gold, 79, "精通应扣费 21 金");
        assert_eq!((pr.mastery.life, pr.mastery.range, pr.mastery.time, pr.mastery.backpack), (1, 1, 1, 1));
        // 金币不足失败
        pr.gold = 2;
        assert!(!pr.buy_mastery(0), "余 2 金买不起 6 金生命精通");
        // 上限（w3q glvl/tooltip 实证：生命/范围/射程各 6 级；背包 3）
        pr.gold = 1000;
        assert!(pr.buy_mastery(3), "背包第 2 级");
        assert!(pr.buy_mastery(3), "背包第 3 级");
        assert!(!pr.buy_mastery(3), "背包达上限 3 应失败");
        for _ in 0..5 { assert!(pr.buy_mastery(0), "生命升到 6"); }
        assert!(!pr.buy_mastery(0), "生命达上限 6 应失败");
        assert_eq!(pr.mastery.life, 6, "生命精通上限 6");
        assert_eq!(pr.mastery.backpack, 3, "背包上限 3");
        // 背包容量（098c bD：L=1+背包级 → 1/3/6/10）
        assert_eq!(pr.inventory_slots(), 10, "背包 3 级 → L4 → 10 格");
        pr.mastery.backpack = 1;
        assert_eq!(pr.inventory_slots(), 3, "背包 1 级 → L2 → 3 格");
        // 3 格容量：3 件不同家族可共存，第 4 件被拒；同家族升级不受容量限制。
        pr.items.clear();
        assert!(pr.buy_item(crate::item::ItemId::Boots1));
        assert!(pr.buy_item(crate::item::ItemId::Amulet1));
        assert!(pr.buy_item(crate::item::ItemId::Cloak1));
        assert!(!pr.buy_item(crate::item::ItemId::Helm1), "第 4 件应被容量拒绝");
        assert!(pr.buy_item(crate::item::ItemId::Boots2), "同家族升级不受容量限制");
        assert_eq!(pr.items.len(), 3);
        // levels() 只计三精通（life6 + range1 + time1）
        assert_eq!(pr.mastery.levels(), 8);
    }

    #[test]
    fn team_assignment_from_config() {
        // FFA：各为一队（cn[i]=i）
        let ms = MatchState::new(MatchConfig::default(), &[0, 1, 2, 3], 34);
        assert_eq!(ms.profiles.iter().map(|p| p.team).collect::<Vec<_>>(), vec![0, 1, 2, 3]);
        // 2 队：按 id 序对半分（098c kX）
        let cfg = MatchConfig { team_count: 2, ..Default::default() };
        let ms = MatchState::new(cfg, &[3, 0, 2, 1], 34);
        let team_of = |id: u32| ms.profiles.iter().find(|p| p.player_id == id).unwrap().team;
        assert_eq!((team_of(0), team_of(1), team_of(2), team_of(3)), (0, 0, 1, 1), "乱序传入也应按 id 排序对半分");
    }
