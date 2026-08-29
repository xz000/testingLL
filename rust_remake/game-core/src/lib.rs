//! 帧同步圆球竞技场 — 逻辑核心 crate（确定性、可回放、无引擎依赖）。
//!
//! 该 crate 只包含游戏规则与确定性模拟：定点数、玩家、场地、碰撞、技能。
//! 渲染与输入在 `client` crate 中处理；联网在阶段 3 接入。

pub mod attribute;
pub mod balance;
pub mod fix;
pub mod meta;
pub mod netcode;
pub mod player;
pub mod progress;
pub mod rng;
pub mod skill;
pub mod world;
pub mod world_ser;

/// 技能总数上限（用于 `skill_levels` / `cooldowns` 数组宽、档案长度）。
/// 需 >= 所有 `SkillId::as_u32` 的最大值 + 1。
pub const MAX_SKILL_SLOTS: usize = 36;

/// 逻辑核心导出预置。阶段 1 已加入 Player / World / Rng。
pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}
