# 界面 / 菜单规划（三功能各自入口与流程）UI_MENUS.md

> 创建 2026-08-15，配合 ROADMAP.md（三功能）使用。
> 现状：无主菜单，靠命令行参数（--host/--join/-solo 等）直接进模式，update/draw 按 MatchPhase 分支。
> 目标：加程序内【主菜单 → 模式 →（模式内流程）】的顶层状态机，让三类玩家各有清晰入口。

## 1. 顶层状态机（改造 Game）
当前 `Game::new(ctx, NetMode)` 直接定模式。改为：
```
AppState = MainMenu | Solo | LanHost | LanJoin | Steam
```
`update` / `draw` 顶层先按 `AppState` 分支，再进 `MatchPhase`（Fighting/Learning/Finished）。
命令行参数仍可作为"直通"入口（--headless 测/自动化 | 快速调试），菜单则是默认交互入口。

## 2. 主菜单（MainMenu）
三个入口（鼠标点击 / 键盘 1/2/3）：
1. 单机技能试验场（Solo）
2. 局域网对战（Lan：开房间 / 加入）
3. Steam 对战（占位：显示"需 Steam，敬请期待"）

## 3. 单机技能试验场（Solo）
- 无 AI：只一个玩家，World 只含自己。
- 默认进入 Fighting，玩家自由试技能/移动。
- **调试辅助（Solo 专属，便于测玩法/数值）**：
  - 工具栏显示：玩家位置、HP、各技能冷却、当前施法阶段。
  - 快捷键：重置冷却、回满血（无敌开关）、重置位置、可能放一个"假人目标"测伤害。
- 功能：验证技能机制 / 连招 / 伤害数值 / 手感（是 4.6b 属性、手感调优的"安全试验场"）。

## 4. 局域网对战（Lan）
子流程：
- **开房间 LanHost**：设端口/人数 → 等待玩家加入（可看到已加入 N / 期望 M）→ （可选"开始"按钮/自动开始）→ Fighting。
- **加入 LanJoin**：输入 host:port → 连接握手 → 等待开局 → Fighting。
- 战斗中：HUD（hp/技能冷却/击杀）已在 draw_meta_overlay。
- 局间学习（Learning 已有）+ 结算/下一局（已有）。
- 可复用当前 `--host`/`--join` 的 lockstep（ROADMAP M2 只补 UI/流程，逻辑已通）。

## 5. Steam 对战（Steam，占位）
- 入口占位。真做时：Steam 大厅房间 → 同意加入 → SteamTransport 跑 lockstep。
- 界面：大厅列表/创建房间/邀请，接 Steam 后另详（ROADMAP M3）。

## 6. 菜单触发方式
- 默认 GUI 菜单（鼠标/方向键+回车）。
- 命令行仍可直通（`--host`/`--join`/`--solo`）用于自动化/快速调试/无头，两者并存。

## 7. 数据/状态
- `AppState` 决定进入哪个模式；`NetMode` 可在进入具体模式时构造（复用 Game::new 现有逻辑或抽成 fn）。
- 结算/学习界面（MatchPhase）与模式耦合：Solo 也用同一套但无网络；Lan 用网络；Steam 占位。

## 8. 实施顺序（建议）
1. 加 `AppState` + 主菜单（三个按钮）→ 能从菜单进 Solo。
2. Solo：去 AI、加调试工具栏（无敌/重置CD/伤害数字/假人）。
3. Lan 子菜单（开/加入）+ 等待界面 → 复用现有 lockstep。
4. Steam 入口占位（灰显 + 提示）。
5. 命令行直通参数保留（与菜单并存）。

## 9. 验收
- 纯键盘/鼠标可完成：主菜单 → 进入各自模式 → 正常游玩/退出。
- Solo 能自由测技能与数值（调试辅助可用）。
- Lan 能开房/加入打多局（复用已验证 lockstep）。
- Steam 入口给出占位提示，不崩。
