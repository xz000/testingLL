# 后续工作总索引 WORK_BACKLOG

> 创建 2026-08-25。把之前散落在 `NEXT_STEPS.md` / `PLAN.md` / `ROADMAP.md` / `UI_MENUS.md` /
> `LATENCY_MASKING.md` 里的**所有未完成工作**汇总成一张可执行清单，作为下次接续的统一入口。
> 阅读顺序：`NEXT_STEPS.md`（交接）→ 本文件（后续工作索引）→ 各专项文档（`STEAM_MULTIPLAYER_PLAN.md` 等）。

---

## 0. 当前状态快照（2026-08-29 会话末）
- **Steam 联机主线三阶段全部完成并真机验证**：掉线处理+重连 ✅ / 快照广播 ✅ / **主机迁移（连续迁移）✅**。
- **Steamworks 第一批（好友邀请 + Rich Presence）已落**（2026-08-29，待真机双账号复验）：
  房间界面按 I 展开「邀请好友」面板（定向邀请 / Steam 邀请窗口）；
  Rich Presence 按阶段上报（房间/配置/对局）+ `connect` 串 → 好友可一键加入；
  被邀请方支持「游戏在跑 → 回调自动进房」与「游戏未跑 → `+connect_lobby <id>` 冷启动进房」。
  详见 `NEXT_STEPS.md`「Steamworks 第一批」节。
- 基线：workspace **127 测试全绿**（+2 connect 串解析），steam feature 下 client 6 全绿（+1 命令行解析），
  build/test/clippy（默认 + steam）全绿，HEAD=`3b6bbd5` + 未提交改动。
- 架构定案：**Steam 为中心，纯玩家 P2P + 不租服务器 + 帧同步 host 权威**（见 `STEAM_MULTIPLAYER_PLAN.md`）。
- 字体：`assets/fonts/cjk.ttf` 已换为全量 **SourceHanSansCN-VF**（17.7MB，字形齐全；旧 168KB 子集留作 `cjk-168k.ttf`）；
  冒烟确认新字体能加载、窗口正常。

---

## 1. Steamworks 增强（用户确认的优先级顺序）
详见 `STEAM_MULTIPLAYER_PLAN.md` §3 盘点。

| 批次 | 内容 | 价值 | 说明 |
|---|---|---|---|
| **第一批** | **好友邀请 + Rich Presence** | 高 | ✅ **已落（2026-08-29）**，待真机双账号复验（见 `NEXT_STEPS.md`） |
| **第二批** | 成就 / 排行榜 / 头像 / Ping | 中 | 击杀/名次/胜场、头像显示、到 host 的延迟 |
| **最后** | 云存档 | 中 | 技能树绑定/成长/金币 meta 存云端（数据现成，延后） |

> 已排除：专用服务器（不租）、商店/支付/DRM。
>
> 能力边界（已核实 steamworks 0.13）：**没有** `InviteUserToLobby`，定向邀请只能走
> `Friend::invite_user_to_game(connect 串)`；Rich Presence 用 `status` + `connect` 两个键（未做 `steam_display` 本地化）。

---

## 2. 单机调试辅助（ROADMAP M1，未做）
`UI_MENUS.md` §3 定义：单机试验场加调试工具，作为数值/手感调优的"安全试验场"。
- 工具栏：玩家位置 / HP / 各技能冷却 / 当前施法阶段。
- 快捷键：重置冷却、回满血（无敌开关）、重置位置、"假人目标"测伤害。

---

## 3. 局域网体验补完（ROADMAP M2，部分做了）
局域网是**离线备胎**（Steam 为主），只做必要维护，不投入 Steam 专属增强。
- 玩家名 / 开局房间界面 / 对局结束回主菜单（已做 Q 回主菜单）。
- **局域网房间列表 / 自动发现**：默认**不做**（方向性决定：Steam 大厅天然提供，局域网手动 `--host`/`--join` 即可）。

---

## 4. 游戏手感 / 系统调优（PLAN 阶段 2 遗留）
| 项 | 状态 | 入口 |
|---|---|---|
| 技能成长数值手感调优 | 待做 | `PLAN.md` 阶段 2 |
| **击退模型 Impulse**（瞬时初速度+逐帧减速，替代定速推） | 待做，**不影响协议** | `PLAN.md`「待议/搁置」 |
| shift 施法瞄准线起点（当前位置 vs 最终位置） | 暂缓 | `PLAN.md`「待议/搁置」 |
| 升级流程交互（数字键=选技能绑定 vs 升级）是否困惑 | 待真机确认 | `PLAN.md`「待议/搁置」 |
| shift 冲刺 / 力场手感 | 未定级 | `PLAN.md`「待议/搁置」 |
| 冷却 HUD 换图标贴图 | 阶段 4 美术 | `PLAN.md` 阶段 2 |

---

## 5. 表现层美术（阶段 4）
- Cell-Graph-Risk 扁平几何 / 节点细胞风（目标美术）。
- 粒子、音效、菜单打磨。

---

## 6. 延迟掩盖：完整回滚重放（LATENCY_MASKING 阶段二 / 4.7）
- 决策门：先真机感受乐观预测，跳变明显再做。
- 前提：局域网/Steam 已"严格 lockstep + 快照 + 迁移"，完整回滚需在此基础上加本地预测+回滚。

---

## 7. UI 打磨
- **暂停 / 退出菜单**（联网暂停=本地暂停交互、不暂停时间；退出按离场处理）。
- **中文房间名输入（IME）**：当前文本输入框只支持 ascii（ggez 逻辑键无法捕获中文 IME）。
- 主菜单 / 各阶段 UI"清爽化"：S1–S5 已做大部分（卡片式、左右分栏），后续按需。

---

## 8. 已知边界 / 未尽（联机）
| 项 | 说明 |
|---|---|
| Steam 迁移：多个 client 同时掉线 | 阶段 3 目前只处理"原 host 掉线"；其他 client 掉线时新 host 会等其输入而卡（可扩展对掉线 client 也 auto_drop） |
| Steam 迁移：不满员对局 | 阶段 3 假设满员，未特别处理不满员迁移 |
| 重连手感真机验证 | 局域网多窗口手动验证重连（host 掉线快照给候补/回大厅） |

---

## 9. 下次建议起点（按价值/依赖排序）
1. **真机复验 Steamworks 第一批**（好友邀请 + Rich Presence，清单见 `NEXT_STEPS.md`「真机待复验」4 条）；
   复验通过后第二批：**成就 / 排行榜 / 头像 / Ping**。
2. 之后按需：**单机调试辅助**（加速数值/手感调优）→ **游戏手感调优**（击退 Impulse 等）。
3. 表现层美术 / UI 打磨 / 延迟回滚 作为长期目标。
4. 若暂不便真机，可先做不依赖 Steam 的部分：单机调试辅助 / 手感调优；
   也可顺带把被替换成 ASCII 的 UI 符号（新字体字形齐全了）恢复成 `✓`/`→` 等。

---

## 10. 常用命令（续接用）
```
cargo test --workspace                  # 回归（127 全绿基线）
cargo clippy --workspace -- -D warnings
cargo clippy -p client --features client/steam -- -D warnings
powershell -File run-steam.ps1 -Mode menu   # Steam 版主菜单（按 3 进大厅，H 建厅 / J 房间列表）
powershell -File check.ps1                  # 一键 build+test+clippy
```
