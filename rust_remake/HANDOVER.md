# 交接说明（HANDOVER）—— 下次对话从这里开始

> 本文件是**唯一的"下一步"入口**：状态、命令、待办、坑，都在这。
> 细节看各自的专题文档（见文末索引）。

## 一、怎么跑 / 怎么验

```bat
cargo test --workspace                                  :: 基线：308 项
cargo clippy --workspace -- -D warnings                 :: 必须干净
cargo clippy --workspace --features client/steam -- -D warnings
cargo build -p client                                   :: debug
cargo build --release -p client --features client/steam  :: release（联机用这个）
```
- 联机复验走 `powershell -ExecutionPolicy Bypass -File run-steam.ps1`
- **改完必看 exe 时间戳**；或在 exe 里搜新增字符串确认是新版本（详见 `WORKFLOW_NOTES.md`）

## 二、当前状态（已实现且已验证）

- **设置系统（统一）**：`H`（建房）与 `O`（房内）打开**同一个**设置编辑器；分组 `[A]房间 [Z]经济 [X]玩法 [C]地图 [V]模式`；
  `=`/回车=确认，`T`/`Shift+回车`=编辑当前行，`Esc`/回车=保存关闭（**关闭即生效**）；非房主**只读**；
  房主改设置后**各端自动取消准备**（走大厅设置串 `room_cfg`，无需额外协议）。
- **金币时序（已按 098c 校准）**：首次进配置/商店时发 `初始金+首轮参与奖`；**每轮参与奖在回合结算时发**（进商店前到账）；
  两者都**幂等**；对局未开始时金币为 0。
- **商店（一行两动作）**：
  - `=`/回车 = 购买或升级该家族；**`退格`/`Delete` = 卖出**当前持有的该家族物品；
  - 行标签**同时标出两个动作**：`[= 操作 · 退格 卖出 +NG]`；满级家族行为 `[= 或 退格 卖出]`；
  - **未选中任何行时，退格也能卖**（回退到第一件可卖物）并打日志 —— 不会"按了没反应"；
  - 已无冗余的"已持有（可卖）"独立区（同一物品不再占两行）。
  - 逻辑核心是纯函数 `Game::shop_sell_target`，有单测 `shop_sell_target_resolution` 钉住三种情形。
- **大厅 UI**：四带版面骨架（`layout.rs`）、统一调色板、覆盖层统一入口、鼠标可点（建房/设置）、
  子界面**清屏**、房间面板显示设置信息块（所有端可见）、房间列表显示「自定义 N 项」。
- **E 键已退休**（房间名/备注并入 `[A]房间`）。

## 三、关键常量与版本

| 项 | 值 |
|---|---|
| `PROTOCOL_VERSION` | 13 |
| `CONFIG_VERSION` | 15 |
| UI 设计分辨率 | `UI_W=1280 / UI_H=720`（`ui::design_rect` 自适应） |
| 房间设置串 | `MatchConfig::to_meta_string()`，单键 `room_cfg`（`ROOM_SETTINGS_KEY`） |
| 测试基线 | 308 项（client 30 / game-core 233 / net 36 / net-steam 9） |

## 四、待办（按建议优先级）

1. **表现层 P1**（`PRESENTATION_PLAN.md`）：伤害/治疗**飘字** + 击杀/首杀/连杀**横幅** —— 玩家感知最强的未做项
2. **输入路由 `InputMode`** + **中文 IME**（`UI_MASTER_PLAN.md` 第十一节）：
   文本态不触发快捷键、IME 提交写入当前缓冲
3. **死代码清理**（旧建房界面 `draw_steam_create_lobby`、`CreateAction`、`create_hitboxes`、`steam_create_*` 缓冲）
   - 现在有 `#[allow(dead_code)]`，**不影响运行**；属整洁性
   - **做法**：一次只删**一个**符号 → `cargo check` → 通过再删下一个（上次一次删一批，改坏过 `draw_menu`，已回滚）
4. **房间设置与 098c 设置对话框的剩余对齐**：`-league` / `-no reward` 模式开关
5. `R017` 的小遗漏：`I004` 持有者击退减免按 +3 级计（`JASS_AUDIT_098c.md`）

## 五、文档索引（读哪个）

| 文档 | 内容 |
|---|---|
| `HANDOVER.md` | ← 本文件：状态 / 命令 / 待办 |
| `WORKFLOW_NOTES.md` | **工程坑**（脚本锚点只用 ASCII、探针落文件、验二进制） |
| `JASS_AUDIT_098c.md` | **098c 数值真值台账**（唯一权威，含 JASS 行号证据） |
| `UI_MASTER_PLAN.md` | UI 总规划 + 迁移步骤 + IME 待办 + "开局前发钱"修正记录 |
| `ROOM_SETTINGS_PLAN.md` | 房间设置 17 项与 098c 对照、档位/自定义设计 |
| `LOBBY_UI_PLAN.md` | 大厅重构动机（已被 UI_MASTER_PLAN 取代，保留来龙去脉） |
| `PRESENTATION_PLAN.md` | 表现层 P1–P6 |
| `UI_AUDIT.md` | 更早的 UI 审视结论 |
| `tools/README.md` + `tools/parse_w3a.py` / `parse_w3q.py` / `parse_objects.py` | 098c 物体数据解析工具 |

## 六、最近提交（新→旧）

`565728e` 商店退格卖出可用化 ← `7270497` HANDOVER ← `961182d` 商店一行两动作
← `065c4ff` 源码扫描测试抗重构 ← `74edf9d` 金币时序 ← `12f1946` 工程坑文档
← `31bf70b` 人数/提示 ← `30e6ea5` 非房主只读 ← `5ce9ee1` 开局发钱（初版）
← `7ebacee` 客户端同步 meta.config ← `e75579d` 房间信息界面职责拆分
← `fb62684` 统一设置（双数据源）← `254b044` H 走统一编辑器 ← `aa7899e` 建房界面收敛
