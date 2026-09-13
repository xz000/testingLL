# 音效与本地设置规划（2026-09-13）

> 目标：给表现层 P2（音效）一份**可施工**清单，并规划承载「音量/静音」的**主菜单设置界面**。
> **本清单直接来自 098c JASS 实证**（`war3map_pretty.j` 的 `CreateSound`/`StartSound`），是权威来源。
> 关联：`PRESENTATION_PLAN.md`（P1–P6）· `HANDOVER.md` · `JASS_AUDIT_098c.md`。
> 纪律：音效**纯客户端本地**，不进 `World`/快照，不改 `PROTOCOL_VERSION`；同帧同类去重；缺素材静默降级。

---

## 0. 关键结论（先看这个）

1. **098c 自定义音效几乎全是「播报语音」**：首杀 / 连杀 / 多重击杀 / Hattrick/Vampire/Denied/Burnout/ Pancake/Silencer/Last-Second-Save / 胜负面板。
2. **普通战斗音（命中、爆炸、施法、武器）098c 没有写进 JASS** —— 它们由 **War3 引擎**按技能/武器自动播放。
   → 这部分我们要**自己制作/找免版权素材**（用户录制或 CC0）。
3. 098c 的播报语音素材是 `war3mapImported\*.mp3`（版权归原项目）。我们应**重新录制/替换**成自己的版本，
   但**事件、时机、屏幕文本**按 098c 对齐（这才是手感）。

---

## 1. 098c 实测音效清单（JASS 权威，`war3map_pretty.j`）

> 「播放变量」= 实际 `StartSound(...)` 用到的变量；「CreateSound 行」≈ 26138–26392。
> 可见性：全局=所有玩家；本队/本机=仅相关玩家（`GetLocalPlayer` 守卫）。

### 1.1 击杀事件播报（Kill feed / announcer）
| 播放变量 | 098c 素材文件 | 事件 | 屏幕文本 | 可见性 |
|---|---|---|---|---|
| `ux` | `CFirstBlood.mp3` | 全场首杀 | `First Blood` | 全局 |
| `Qx` | `BDoubleKill2.mp3` | 短时间内第 2 杀 | `Double Kill` | 全局 |
| `tx` | `BMultiKill3.mp3` | 第 3 杀 | `Multi Kill` | 全局 |
| `sx` | `BMegaKill4.mp3` | 第 4 杀 | `Mega Kill` | 全局 |
| `Tx` | `BUltraKill5.mp3` | 第 5 杀 | `Ultra Kill` | 全局 |
| `Sx` | `BMonsterKill6.mp3` | 第 6 杀 | `Monster Kill` | 全局 |
| `Io` | `CLudicrousKill.mp3` | 击杀「0 死」的对手 | `Ludicrous Kill!` | 全局 |

### 1.2 连杀（Killing spree，按击杀数递增）
| 播放变量 | 098c 素材文件 | 击杀数 | 文本标签（`Mn[]`） |
|---|---|---|---|
| `Yx` (`mn[3]`) | `AAKillingSpree3.mp3` | 3 | `Killing Spree (3)` |
| `zx` (`mn[4]`) | `AADominating4.mp3` | 4 | `Dominating (4)` |
| `Zx` (`mn[5]`) | `AAOwnage5.mp3` | 5 | `Ownage (5)` |
| `vo` (`mn[6]`) | `AARampage6.mp3` | 6 | `Rampage (6)` |
| `eo` (`mn[7]`) | `AAUnstoppable7.mp3` | 7 | `Unstoppable (7)` |
| `xo` (`mn[8]`) | `AAWhickedSick8.mp3` | 8 | `Wicked Sick (8)` |
| `oo` (`mn[9]`) | `AAGodLike9.mp3` | 9 | `Godlike (9)` |
| `ro` (`mn[10]`) | `AAHolyShit10.mp3` | 10 | `Holy Shit (10)` |
| `io` | `AAHoly.mp3` | >10 | `Holy Shit !!! (n)` |

> 实现细节：098c 用 `DI` 暂存本帧要播的连杀音（`DI=ux` / `DI=Qx` / `DI=mn[n]` / `DI=io`），
> 在击杀结算处 `StartSound(DI)`（`war3map_pretty.j:6885`）。

### 1.3 战斗事件播报（事件触发 + 目标头顶漂字）
| 播放变量 | 098c 素材文件 | 触发条件（JASS） | 屏幕文本 | 可见性 |
|---|---|---|---|---|
| `Xo` | `CBurnout.mp3` | 命中**队友**且自身处于（燃烧/反噬）状态 | `Burn out` | 相关双方 |
| `Eo` | `CSilencer.mp3` | 一次 Silence 命中 **>2** 个目标（`uI>2`） | `Silencer` | 全局 |
| `ao` | `Pancake.mp3` | 被砸扁（受害方 `Gn *= 0.1`，地震法球陨石） | `Pancake` | 相关双方 |
| `Wx` | `Denied.mp3` | 击杀/致残被**阻止**（target 逃过一劫） | `Denied` | 相关双方 |
| `Oo` | `CVampire.mp3` | 单次命中 **≥3** 且**佩戴面具(Mask)** | `Vampire` | 全局 |
| `Ux` | `CHattrick.mp3` | 单次命中 **≥3** 且**未佩戴面具** | `Hattrick` | 全局 |
| `wx` | `CLastSecondSave.mp3` | 在危险地带（岩浆）**极限逃生**（`Fn<=6*To+0.5`） | `Last Second Save` | 仅本队 |

### 1.4 流程 / 界面
| 播放变量 | 098c 素材文件 | 触发条件 | 可见性 |
|---|---|---|---|
| `Ro` | `Sound\Dialogue\GenericWarnings\GenericWarningResearchComplete1.mp3` | **学习/升级完成**（科技研究） | 仅本机 |
| `no` | `EpicVictory.mp3` | 整场**胜利**（`yD and not YD`） | 全局 |
| `Vo` | `Sound\Interface\GameFound.wav` | **游戏开始**（`OR==2`）；平局**加赛** | 全局 |
| `yx` | `Sound\Interface\Rescue.wav` | 终局**过场**（白屏淡出） | 全局 |

### 1.5 定义但**未播放**（遗留，供参考）
| 变量 | 素材文件 | 说明 |
|---|---|---|
| `qx` | `SpellStealMissile.wav` | 定义了 `SetSoundParamsFromLabel(...,"SpellStealMissileLaunch")`，但无 `StartSound` |

> 环境音/音乐：`SetAmbientDaySound("BlackCitadelDay")`、`SetMapMusic("Music",...)` 是 War3 默认，**无自定义 BGM**。

### 1.6 接法对照（我们的事件 → 098c 来源 → 接/不接）

> 原则：**只接 098c 有音效来源的事件**（JASS 显式，或 War3 引擎在 098c 里实际提供的反馈）；
> 098c 没有的一律**不接**（不发明）。

| 我们的事件 | 098c 来源 | 处理 |
|---|---|---|
| 伤害飘字 | War3 引擎（攻击/命中音） | ✅ 接 `combat_hit`（自制，最小间隔限流） |
| 治疗飘字 | War3 引擎（治疗/吸血） | ✅ 接 `combat_heal` |
| 玩家死亡 | War3 引擎（死亡音） | ✅ 接 `combat_death` |
| 击杀 | 098c 击杀播报（`DI`/`mn`） | ✅ 接 `combat_kill` |
| 首杀 | `ux` CFirstBlood | ✅ 接 `ann_first_blood` |
| 连杀 3..10 / >10 | `mn[3..10]` / `io` | ✅ 接 `ann_spree3..10` / `ann_spree_holy`（对齐断点） |
| 终局胜负 | `no` EpicVictory | ✅ 接 `ann_victory`（Finished 一次性） |
| 多重击杀 Double/Multi/Mega/Ultra/Monster | `Qx/tx/sx/Tx/Sx` | ✅ 接（9s 窗口 `Wn`，阈值 2/3/4/5/6，`multikill_cue()`） |
| Ludicrous Kill | `Io` | ✅ 接：击杀**本轮 0 伤害**的对手（`Rn[NI]==0`） |
| Hattrick / Vampire | `Ux` / `Oo` | ⬜ 待战斗信号（单次命中 ≥3） |
| Silencer / Pancake / Burnout / Denied / Last Second Save | `Eo`/`ao`/`Xo`/`Wx`/`wx` | ⬜ 待战斗信号（P1 扩展共用） |
| 学习 / 升级完成 | `Ro` ResearchComplete | ✅ 接 `ann_research`（仅本机，仅技能购买/升级成功） |
| 开局 / 平局加赛 | `Vo` GameFound | ⬜ 待接 |
| 终局过场 | `yx` Rescue | ⬜ 待接 |
| **缩圈 / 出界 / 倒计时 / 回合开始结束 / 配置阶段** | **098c 无** | ⛔ **不接**（098c 确实没这些音效） |
| 商店买 / 卖 | 098c 无（只有“研究完成”音） | ⛔ 不接买卖；升级/学习→Research |
| 施法 / 爆炸 / 反弹 / 护盾 / 柱子破碎 | War3 引擎（JASS 无显式数据） | ⏸ 暂缓（可可靠映射后再接） |

---

## 2. 098c 没有、需要我们自制的音效

因为 098c 依赖 War3 引擎自动播放，所以我们**从零定义**下列原生音效（用户录制 / CC0 / 占位）。事件源用客户端状态对比。

### A. 战斗核心（P0，自制）
| 编号 | 占位名 | 触发时机 |
|---|---|---|
| A01 | `combat_cast.wav` | 技能起手（前摇开始） |
| A02 | `combat_release.wav` | 技能释放 / 弹体生成 |
| A03 | `combat_hit.wav` | 命中敌人（造成伤害） |
| A04 | `combat_explode.wav` | AoE 爆炸（新星/陨石/弹体爆炸） |
| A05 | `combat_bounce.wav` | 弹体撞墙 / 柱子反弹 |
| A06 | `combat_shield.wav` | 护盾吸收伤害 |
| A07 | `combat_reflect.wav` | 反弹（盾反弹 / 弹射） |
| A08 | `combat_heal.wav` | 治疗 / 吸血生效 |
| A09 | `combat_kill.wav` | 击杀确认（配合播报） |
| A10 | `combat_death.wav` | 玩家死亡 / 淘汰 |
| A11 | `combat_respawn.wav` | 复活 |
| A12 | `combat_pillar_break.wav` | 柱子被摧毁 |
| A13 | `combat_lava.wav` | 岩浆 / 出界灼烧（节流） |

### B. 回合 / 对局流程（P0，自制）
| 编号 | 占位名 | 触发时机 |
|---|---|---|
| B01 | `flow_round_start.wav` | 回合开始 |
| B02 | `flow_round_end.wav` | 回合结束 |
| B03 | `flow_config_phase.wav` | 进入配置 / 学习阶段（商店开启） |
| B04 | `flow_countdown.wav` | 开局倒计时滴答（最后 N 秒） |
| B05 | `flow_oob_warn.wav` | 出界警告 |
| B06 | `flow_shrink_warn.wav` | 缩圈预警 |

> 胜负 / 游戏开始 用 §1.4 的 `no`/`Vo`/`yx` 三个（重新录制的自有版本），不另造。

### C. 大厅 / 房间 UI（P1，自制）
| 编号 | 占位名 | 触发时机 |
|---|---|---|
| C01 | `ui_move.wav` | 选择行上下移动 |
| C02 | `ui_confirm.wav` | 确认 / 进入 |
| C03 | `ui_cancel.wav` | 取消 / 返回 / 关闭 |
| C04 | `ui_error.wav` | 无效操作 / 金币不足 / 房间满 |
| C05 | `ui_ready.wav` | 就绪 / 取消就绪 |
| C06 | `ui_all_ready.wav` | 全员就绪（倒计时开始） |
| C07 | `ui_join.wav` | 有玩家加入房间 |
| C08 | `ui_leave.wav` | 有玩家离开房间 |
| C09 | `ui_host_left.wav` | 房主离开 |
| C10 | `ui_invite.wav` | 邀请 / 好友通知 |

### D. 商店 / 成长（P1，自制）
| 编号 | 占位名 | 触发时机 |
|---|---|---|
| D01 | `shop_buy.wav` | 购买物品 |
| D02 | `shop_upgrade.wav` | 升级技能 / 精通（**也可复用 098c 的 ResearchComplete 语义**） |
| D03 | `shop_sell.wav` | 卖出物品 |

### E. 098c 播报语音（**重新录制/替换**，事件对齐 §1）
| 编号 | 占位名 | 对应 098c |
|---|---|---|
| V01 | `ann_first_blood.wav` | CFirstBlood |
| V02 | `ann_double_kill.wav` | BDoubleKill2 |
| V03 | `ann_multi_kill.wav` | BMultiKill3 |
| V04 | `ann_mega_kill.wav` | BMegaKill4 |
| V05 | `ann_ultra_kill.wav` | BUltraKill5 |
| V06 | `ann_monster_kill.wav` | BMonsterKill6 |
| V07 | `ann_ludicrous_kill.wav` | CLudicrousKill |
| V08 | `ann_spree3.wav` | AAKillingSpree3 |
| V09 | `ann_spree4.wav` | AADominating4 |
| V10 | `ann_spree5.wav` | AAOwnage5 |
| V11 | `ann_spree6.wav` | AARampage6 |
| V12 | `ann_spree7.wav` | AAUnstoppable7 |
| V13 | `ann_spree8.wav` | AAWhickedSick8 |
| V14 | `ann_spree9.wav` | AAGodLike9 |
| V15 | `ann_spree10.wav` | AAHolyShit10 |
| V16 | `ann_spree_holy.wav` | AAHoly（>10） |
| V17 | `ann_hattrick.wav` | CHattrick |
| V18 | `ann_vampire.wav` | CVampire |
| V19 | `ann_denied.wav` | Denied |
| V20 | `ann_burnout.wav` | CBurnout |
| V21 | `ann_silencer.wav` | CSilencer |
| V22 | `ann_pancake.wav` | Pancake |
| V23 | `ann_last_second_save.wav` | CLastSecondSave |
| V24 | `ann_victory.wav` | EpicVictory |
| V25 | `ann_game_start.wav` | GameFound |
| V26 | `ann_finish.wav` | Rescue |
| V27 | `ann_research.wav` | ResearchCompleteGeneric |

> 若暂时不录播报，可先**只做 A/B/C/D**，播报用 §1 的文本横幅 + 占位音。

---

## 3. 后端与架构（已定）

- **播放后端**：`ggez::audio`（底层 rodio；用现成 `Context`，不另引依赖）。
- **本地偏好**（音量/静音）**不进房间设置串**（那是 `MatchConfig`，会同步全房）。
  归属「本机设置」：新增 `client/src/local_settings.rs`，**手写 key=value 文本**持久化（不给客户端引 serde），
  存 ggez 用户数据目录（`ctx.fs.user_config_dir()`）。
- **事件源**：客户端比较上帧状态推导（与 P1 飘字/横幅同源）。
- **触发**：每帧收集一组 `AudioCue`，去重 + 最小间隔节流后播放；缺失静默。
- **素材目录**：`client/assets/audio/<cue>.wav`（先占位合成，后续同名替换）；命名即 §1/§2 的「占位名」。

### 占位音策略
用 `tools/` 脚本合成短 `.wav`（正弦/方波 + 包络），按编号命名，先跑通链路；素材到位后同名替换，**不改代码**。

---

## 4. 主菜单「设置」界面（本地设置承载）

现状：主菜单只有 3 张卡片（单机 / 局域网 / Steam），无本地设置入口。计划新增第 4 项 **「设置」**，
打开**本地设置界面**（与「房间设置编辑器」同风格的行列表，但作用于本机、不进同步）。

### 4.1 界面项（先做音频，后续可扩）
| 行 | 类型 | 说明 |
|---|---|---|
| 主音量 | 0–100（步进 5） | 总音量 |
| 音效音量 | 0–100 | 战斗/UI/播报 |
| 音乐音量 | 0–100 | 仅 BGM 启用后有意义（可先置灰） |
| 静音 | 开关 | 总静音（等同 `M`） |

### 4.2 交互
- 主菜单：↑/↓ 选到「设置」→ 回车/点击进入；数字键 `4` 直选。
- 设置界面：`↑/↓` 选择行、`←/→` 调值、`回车`/点击 调整（音量 +5%，满则回 0；静音行切换）；`Esc`/`Q`/「返回」关闭（**自动保存**）。
- 鼠标：沿用 `HitRegistry`（点行选中并调整、可点返回）+ 底部操作条 + 按键提示。
- 快捷：`F10` 一键静音（**任何界面全局生效**；`M` 已被学习期商店 `B/N/M` 分类键占用，故不用 `M`）。

### 4.3 持久化
- 字段 `master_volume / sfx_volume / music_volume / muted`；启动读、失败默认（100/100/100/否）。
- 关闭设置界面 / `M` 时写回；格式 `key=value\n`；位置 `user_config_dir/settings.txt`（失败静默）。

---

## 5. 分步实施（每步可编译/提交/过门禁）

- [x] **P2-0 占位素材**：`tools/gen_placeholder_audio.py` 合成 `client/assets/audio/*.wav`（共 59 个，约 550KB）。
- [x] **P2-1 音频基础设施**：`local_settings.rs`（读写+默认+单测）；`audio.rs` `AudioBank`（加载/播放/静音/降级）。
- [x] **P2-2 主菜单「设置」界面**：第 4 卡片 + 设置界面（行列表 + 滑条 + 键鼠 + 提示）+ 持久化 + `F10` 全局静音。
- [~] **P2-3 战斗音效**：已接 命中/治疗/死亡/击杀（引擎反馈）；施法/爆炸/反弹等暂缓（无可可靠映射）。
- [ ] **P2-4 流程音效**：**098c 无回合/倒计时/缩圈/出界音** → 不接；仅剩 开局 `Vo` / 终局过场 `yx` 待接。
- [~] **P2-5 播报语音**：已接 首杀 / 连杀 3..10/>10 / 多重击杀 2..6 / Ludicrous / 胜利；Hattrick·Vampire·Denied 等需先补信号。
- [~] **P2-6 UI/商店音效**：已接 主菜单/设置导航 + 学习/升级完成（`Ro`，仅本机）；买/卖 098c 无 → 不接。
- [ ] **P2-7（可选）BGM**。

> 建议顺序：P2-1 → P2-2 → P2-3/4 → P2-6 → P2-5（播报依赖新的战斗信号）。

---

## 6. 风险 / 注意
- **性能**：别每帧 `load`；缓存到 `AudioBank`；播放不阻塞主线程。
- **去重/节流**：命中/灼烧等高频事件必须节流（最小间隔 + 同帧同类一次）。
- **降级**：CI/无声卡环境**不 panic**（全部 `Option`，缺失静默）。
- **协议**：音频/设置改动**不得**触碰 `PROTOCOL_VERSION`/快照。
- **版权**：播报语音优先**自制/CC0**；临时用 098c 素材仅本地测试、不入仓库/不发布。

---

## 7. 记录
- 2026-09-13：初版（通用清单 + 架构 + 设置界面）。
- 2026-09-13：**按 098c JASS 实证重写** —— 查明 098c 自定义音效**几乎全是播报语音**（§1），
  普通战斗音依赖 War3 引擎（§2 需自制）；补 V01–V27 播报对齐清单与文件原名。
- 2026-09-13：**P2-0/P2-1/P2-2 完成** —— 占位素材脚本；`local_settings` + `AudioBank`；
  主菜单第 4 项「设置」界面（滑条 + 键鼠）；`F10` 全局静音（`M` 已被商店分类键占用，故不用）；自动持久化。
- 2026-09-13：**按 098c 接法接入第一批**（§1.6）—— 命中/治疗/死亡/击杀 + 首杀/连杀/胜利；
  **明确不接** 缩圈/出界/倒计时/回合流程/商店买卖（098c 无）。
- 2026-09-13：**多重击杀窗口**（098c `Wn=9s` + `dn[VI+$C]`）：2/3/4/5/6 → Double..Monster（音效 + 横幅）。
- 2026-09-13：**Ludicrous Kill**（`Io`，击杀本轮 0 伤害对手）+ **学习/升级完成**（`Ro` ResearchComplete，仅本机）。
