# 音效脚本（占位音清单 + 台词）

> 用途：把 `client/assets/audio/` 下的**全部 59 个占位音**逐个说明用途；需要台词的（播报类）给出**中文/English 台词**，
> 方便录制或找素材。素材**同名替换**即可，代码无需改动（`client/src/audio.rs` 的 `AudioCue::file()` 为准）。
> 关联：`AUDIO_PLAN.md`（098c 来源与接法）。

- 命名规则：`<类别>_<名字>.wav`。类别：`combat_`（战斗）、`flow_`（流程）、`ui_`（界面）、`shop_`（商店）、`ann_`（播报）。
- 音量/静音由主菜单「设置」与 `F10` 控制（不受素材影响）。
- 标注 **「音效」** = 纯音效，无台词；标注 **「台词」** = 人声/播报，建议按中英台词录制。
- `ann_*` 中文台词为**建议译文**；English 台词基本沿用 098c 原文（见 `AUDIO_PLAN.md §1`）。

---

## A. 战斗核心（自制品，无 098c 对应，纯音效）
| 文件 | 用途 / 触发时机 | 台词 |
|---|---|---|
| `combat_cast.wav` | 技能起手（前摇开始） | 音效（无台词） |
| `combat_release.wav` | 技能释放 / 弹体生成 | 音效 |
| `combat_hit.wav` | 命中敌人（造成伤害；60ms 限流） | 音效 |
| `combat_explode.wav` | AoE 爆炸（新星/陨石/弹体爆炸） | 音效 |
| `combat_bounce.wav` | 弹体撞墙/柱子反弹 | 音效 |
| `combat_shield.wav` | 护盾吸收伤害 | 音效 |
| `combat_reflect.wav` | 反弹（盾反弹/弹射） | 音效 |
| `combat_heal.wav` | 治疗 / 吸血生效 | 音效 |
| `combat_kill.wav` | 击杀确认 | 音效 |
| `combat_death.wav` | 玩家死亡 / 淘汰 | 音效 |
| `combat_respawn.wav` | 复活 | 音效 |
| `combat_pillar_break.wav` | 柱子被摧毁 | 音效 |
| `combat_lava.wav` | 岩浆 / 出界灼烧（节流播放） | 音效 |

## B. 回合 / 对局流程（自制品）
> 注：098c 自身**没有**回合/倒计时/缩圈/出界音（见 `AUDIO_PLAN.md`）；这些是为我们流程补的，可纯音效。
| 文件 | 用途 / 触发时机 | 台词 |
|---|---|---|
| `flow_round_start.wav` | 回合开始 | 音效 |
| `flow_round_end.wav` | 回合结束 | 音效 |
| `flow_config_phase.wav` | 进入配置 / 学习阶段（商店开启） | 音效 |
| `flow_countdown.wav` | 开局倒计时滴答（最后 N 秒，每秒） | 音效 |
| `flow_oob_warn.wav` | 出界警告 | 音效 |
| `flow_shrink_warn.wav` | 缩圈预警 | 音效 |

## C. 大厅 / 房间 UI（自制品）
| 文件 | 用途 / 触发时机 | 台词 |
|---|---|---|
| `ui_move.wav` | 选择行上下移动 | 音效 |
| `ui_confirm.wav` | 确认 / 进入 | 音效 |
| `ui_cancel.wav` | 取消 / 返回 / 关闭 | 音效 |
| `ui_error.wav` | 无效操作 / 金币不足 / 房间满 | 音效 |
| `ui_ready.wav` | 就绪 / 取消就绪 | 音效 |
| `ui_all_ready.wav` | 全员就绪（倒计时开始） | 音效 |
| `ui_join.wav` | 有玩家加入房间 | 音效 |
| `ui_leave.wav` | 有玩家离开房间 | 音效 |
| `ui_host_left.wav` | 房主离开 | 音效 |
| `ui_invite.wav` | 邀请 / 好友通知 | 音效 |

## D. 商店 / 成长（自制品）
| 文件 | 用途 / 触发时机 | 台词 |
|---|---|---|
| `shop_buy.wav` | 购买物品 | 音效 |
| `shop_upgrade.wav` | 升级技能 / 精通 | 音效 |
| `shop_sell.wav` | 卖出物品 | 音效 |

## E. 播报（Announcer）—— 部分为台词
> 对照 098c 原声（`AUDIO_PLAN.md §1`）：**「人声」= 098c 原声即为语音台词**；
> **「器乐」= 098c 原声是器乐/提示音（无人声）**，我们若录人声属于**新增**。
> 证据：098c 原文件名（`war3mapImported\A*/B*/C*/Denied/Pancake.mp3`）+ 与之同步的**屏幕文本**（如 `Hattrick`、`Burn out`）。
> 中文台词为建议译文；English 台词基本沿用 098c 原文。

### E1. 击杀 / 连杀 / 多重击杀（098c 均为**人声**）
| 文件 | 触发 | 098c 原声 | English 台词 | 中文台词（建议） |
|---|---|---|---|---|
| `ann_first_blood.wav` | 全场首个击杀 | 人声（`CFirstBlood.mp3`） | First Blood | 首杀！ |
| `ann_double_kill.wav` | 9s 内第 2 杀 | 人声（`BDoubleKill2.mp3`） | Double Kill | 双杀！ |
| `ann_multi_kill.wav` | 第 3 杀 | 人声（`BMultiKill3.mp3`） | Multi Kill | 三杀！ |
| `ann_mega_kill.wav` | 第 4 杀 | 人声（`BMegaKill4.mp3`） | Mega Kill | 四杀！ |
| `ann_ultra_kill.wav` | 第 5 杀 | 人声（`BUltraKill5.mp3`） | Ultra Kill | 五杀！ |
| `ann_monster_kill.wav` | 第 6 杀及以后 | 人声（`BMonsterKill6.mp3`） | Monster Kill | 疯狂杀戮！ |
| `ann_ludicrous_kill.wav` | 击杀「本轮 0 伤害」的对手 | 人声（`CLudicrousKill.mp3`） | Ludicrous Kill! | 荒谬击杀！ |
| `ann_spree3.wav` | 连杀 3 | 人声（`AAKillingSpree3.mp3`） | Killing Spree | 杀戮开始！ |
| `ann_spree4.wav` | 连杀 4 | 人声（`AADominating4.mp3`） | Dominating | 主宰比赛！ |
| `ann_spree5.wav` | 连杀 5 | 人声（`AAOwnage5.mp3`） | Ownage | 无人能挡！ |
| `ann_spree6.wav` | 连杀 6 | 人声（`AARampage6.mp3`） | Rampage | 暴走！ |
| `ann_spree7.wav` | 连杀 7 | 人声（`AAUnstoppable7.mp3`） | Unstoppable | 势不可挡！ |
| `ann_spree8.wav` | 连杀 8 | 人声（`AAWhickedSick8.mp3`） | Wicked Sick | 骇人听闻！ |
| `ann_spree9.wav` | 连杀 9 | 人声（`AAGodLike9.mp3`） | Godlike | 如同神明！ |
| `ann_spree10.wav` | 连杀 10 | 人声（`AAHolyShit10.mp3`） | Holy Shit | 天哪！ |
| `ann_spree_holy.wav` | 连杀 >10 | 人声（`AAHoly.mp3`） | Holy Shit!!! | 神迹降临！ |

### E2. 战斗事件播报
| 文件 | 触发 | 098c 原声 | English 台词 | 中文台词（建议） |
|---|---|---|---|---|
| `ann_hattrick.wav` | 一次 AoE 命中 ≥3 敌人（无面具） | 人声（`CHattrick.mp3`） | Hattrick | 帽子戏法！ |
| `ann_vampire.wav` | 同上但戴死亡面具 | 人声（`CVampire.mp3`） | Vampire | 吸血鬼！ |
| `ann_denied.wav` | 天罚打断「被链接 + 特殊状态」目标 | 人声（`Denied.mp3`） | Denied | 拒绝！ |
| `ann_burnout.wav` | 燃烧冲刺撞到队友熄火 | 人声（`CBurnout.mp3`） | Burn out | 燃尽！ |
| `ann_silencer.wav` | 一次沉默 ≥3 目标 | 人声（`CSilencer.mp3`） | Silencer | 沉默！ |
| `ann_pancake.wav` | 岩浆滚石拍扁敌人 | 人声?（`Pancake.mp3`，也可能为音效） | Pancake | 肉饼！ |
| `ann_last_second_save.wav` | 出界→回场且残血极低 | 人声（`CLastSecondSave.mp3`） | Last Second Save | 极限逃生！ |

### E3. 流程 / 其它播报（098c 多为**器乐**）
| 文件 | 触发 | 098c 原声 | English 台词 | 中文台词（建议） |
|---|---|---|---|---|
| `ann_victory.wav` | 整场结束 | **器乐**（`EpicVictory.mp3`） | Victory | 胜利！ |
| `ann_game_start.wav` | 首局开局 / **平局加赛**（`Vo`） | **器乐**（`GameFound.wav`） | The battle begins / Draw! One more round | 战斗开始！/ 平局！再战一轮！ |
| `ann_finish.wav` | 终局过场（离开结算画面） | **器乐**（`Rescue.wav`） | 战斗结束 | 战斗结束 |
| `ann_research.wav` | 学习 / 升级完成（仅本机） | **人声**（`ResearchCompleteGeneric.mp3`，War3 顾问） | Research complete | 升级完成！ |

> 对照小结：098c 自有播报里**人声台词**为 `A*/B*/C*` 系列 + `Denied` + `Pancake`（共 ~23 条）；
> `EpicVictory` / `GameFound` / `Rescue` 是**器乐/提示音**（无人声）；`ResearchCompleteGeneric` 是 **War3 顾问语音**。
> 因此 E1/E2 基本应录**人声**；E3 的 victory/game_start/finish 录**器乐**更贴原版（人声属新增）。

---

## F. 统计
| 类别 | 数量 | 是否需台词 |
|---|---|---|
| A 战斗核心 | 13 | 否 |
| B 流程 | 6 | 否 |
| C UI | 10 | 否 |
| D 商店 | 3 | 否 |
| E 播报 | 27 | **大部分需台词**：098c 人声 ~23–24（E1/E2 + research）；器乐 3（victory/game_start/finish，098c 原声为器乐） |
| **合计** | **59** | 战斗/流程/UI/商店 32 条为音效；播报 27 条中 **23–24 条录人声**、3 条器乐 |

> 录制建议：播报单条 0.4–1.2s；命中/UI 短促 0.05–0.2s；爆炸/胜负 0.3–1.0s。
> 采样率 44.1kHz、单声道或立体声 `.wav` 均可（`ggez::audio`/rodio 解码 WAV）。
