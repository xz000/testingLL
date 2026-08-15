# 4.6b 属性系统（单独立项）

> 创建 2026-08-15。在 4.6（多局配置同步）、4.7（延迟掩盖）之上。网络层已就绪（PlayerConfig 快照 +
> 通用配置收发，加属性字段网络层零改动）。

## 目标
给角色加 Dota/War3 式可成长的战斗属性，并在多局间跨端确定性同步、随 `Balance`/`PlayerConfig`
合成到实际战斗数值。**本阶段只做属性系统本身；其数值成长/价格/合成公式与具体玩法调优属后续平衡。**

## 要加的属性（候选，按当前战斗维度对齐）
| 属性 | 作用到哪 | 现状 |
|------|---------|------|
| 生命值上限(Hp) | `Player.max_hp` | 恒为 `Balance.max_hp`，加后按属性缩放 |
| 移动速度(Speed) | `base_speed()` 的倍率 | `Balance.base_speed` + buff |
| 护甲(护甲减免) | 收 `events` 伤害时按护甲减免 | 现在伤害不打折 |
| 法术抗性(SpellResist) | 技能/子弹伤害减免 | 现在不打折 |
| 击退抗性(KBResist) | `push_power`/`push_time` 缩放 | 现在全额 |
| 法术值/蓝量(Mana) | 施法消耗（新增机制） | 无 MP，纯冷却 |
| 成长点(成长分配) | 跨局累积、用于升级以上属性 | 无 |

## 架构落地
1. **`PlayerProfile` / `PlayerConfig` 加「属性快照」字段**（如 `hp_bonus/speed_bonus/armor/spell_resist/kb_resist/mana` ...）。
   快照已是版本化+长度前缀，直接加段即可，网络层不动。
2. **`Balance` 加「属性→战斗数值」的派生系数**（如每点护甲减伤比例、每点生命加成、公式）。
3. **新增派生函数**（game-core）：`PlayerConfig/Profile → 应用到 Player 的战斗数值(max_hp/移速倍率/减伤/击退缩放)`。
   在 `teardown_round_end` / `reset_round` 时用派生结果设置 `World.players[i]` 的战斗数值。
4. **伤害/施法结算改为读派生后的值**：`events` 扣血前按护甲/法抗折算；`push` 前按击退抗性缩放。
   施法若加蓝量系统，则 `try_cast` 消耗/检查蓝。

## 顺序（建议）
1. 先把「属性字段进 PlayerProfile/PlayerConfig + 版本号 bump」+ 派生函数骨架（先只接 Hp/移速两个最直观的，
   且先用「加法系数」而非复杂公式）。
2. 再接护甲/法抗/击退抗性到伤害/击退结算点。
3. 再议蓝量（它是新机制，可能改变技能平衡，最后单独评估）。

## 注意
- 战斗数值是确定性共享状态，属性派生必须也是确定性的（纯函数由 PlayerConfig/Profile 计算）。
- 不能在渲染/显示里额外随机；合成公式只在 `game-core` 一处。
- 每次改动 bump `PlayerConfig::CONFIG_VERSION`，旧存档/旧端按"版本不符"拒绝或重算。
