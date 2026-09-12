//! 帧同步圆球竞技场 — 逻辑核心 crate（确定性、可回放、无引擎依赖）。
//!
//! 该 crate 只包含游戏规则与确定性模拟：定点数、玩家、场地、碰撞、技能。
//! 渲染与输入在 `client` crate 中处理；联网在阶段 3 接入。

pub mod balance;
pub mod fix;
pub mod meta;
pub mod netcode;
pub mod player;
pub mod progress;
pub mod rng;
pub mod item;
pub mod skill;
pub mod world;
pub mod world_ser;

/// 技能总数上限（用于 `skill_levels` / `cooldowns` 数组宽、档案长度）。
/// 需 >= 所有 `SkillId::as_u32` 的最大值 + 1。
/// 098b 名册扩充：Unity 版 36 个 + 098b 41 技能 + 物品技能余量（M1 起为 64）。
pub const MAX_SKILL_SLOTS: usize = 73;

/// 逻辑核心导出预置。阶段 1 已加入 Player / World / Rng。
pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

/// **联机兼容版本**：任何会改变「线上协议」或「确定性模拟结果」的改动都必须 +1。
/// 大厅建房时写入该值，加入者比对；不一致即拒绝加入，避免改前/改后构建联机导致
/// desync / 握手失败 / 行为分叉（这类问题极难从现象定位）。
///
/// 何时需要 +1（举例）：
/// - 改动 `net::proto` 的任何包格式 / tag；
/// - 改动 `game_core::netcode` 的输入编码；
/// - 改动 `game_core::world_ser` 的快照格式或**任何会影响模拟的字段**；
/// - 改动技能/物品数值、世界模拟逻辑（会改变同一输入下的世界演化）。
///
/// v7（2026-09-12）：删除属性购买系统（`Player` 的 speed_mult/armor/spell/kb 因子从快照移除）。
/// v8（2026-09-12）：陨石改为 `ProjectileKind::DelayedBlast`（落点定时爆炸，无飞行弹体）。
/// v9（2026-09-12）：S013A 换位/S013B 搬运改为弹体（`SwapTarget`/`CarrySelf`）；凤凰弹规格对齐 098c。
pub const PROTOCOL_VERSION: u32 = 13;
