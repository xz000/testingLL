# Circle Brawl · 圆圈之战

> 确定性帧同步（lockstep）圆球竞技场对战游戏 —— Rust 重制版。
>
> A deterministic, frame-synchronized (lockstep) circular-arena brawler, rewritten in Rust.

《圆圈之战》是一款多人圆球竞技场对战游戏：玩家在持续缩小的场地里互掷法术、争夺生存。
本仓库是该游戏的 **Rust 重制版**，核心逻辑为保证联机一致性的**确定性帧同步**实现。

---

## 特性

- **确定性逻辑核心**：`game-core` 无引擎依赖，使用定点数（`fixed`）、确定性三角函数与整数 RNG；
  dev 与 release 强制一致的溢出/断言行为，保证各端逐帧计算结果完全一致。
- **帧同步网络**：`net` 提供与传输无关的 lockstep（主机权威采集 + 客户端按序号对齐步进），
  支持房间就绪、快照、掉线重连与主机迁移。
- **多平台联机**：局域网 UDP（`net`）与 Steam 大厅 / P2P（`net-steam`，可选 feature）。
- **完整游戏流程**：主菜单 → 建房/进房 → 配置学习与商店 → 回合对战 → 结算 → 下一轮。
- **国际化**：中文 / English（由 Steam 语言驱动，亦可手动固定）。
- **跨平台渲染**：`ggez` 0.10 客户端（1280×720 设计分辨率自适应）。

---

## 仓库结构

Rust 工程位于 `rust_remake/`（Cargo workspace）：

| 路径 | 说明 |
| --- | --- |
| `rust_remake/game-core/` | 确定性纯逻辑核心：世界、技能、物品、缩圈、计分、RNG（无引擎依赖） |
| `rust_remake/client/` | `ggez` 客户端：渲染、输入、UI、设置、音频 |
| `rust_remake/net/` | 传输无关的帧同步网络层 + 局域网 UDP 实现 |
| `rust_remake/net-steam/` | Steam 传输适配（`SteamNetworkingSockets` / 大厅），复用 `net` 的 lockstep |
| `rust_remake/tools/` | 离线数据 / 资源生成脚本（Python） |
| `rust_remake/assets/` | 运行期资源（字体等） |

---

## 环境要求

- **Rust** 稳定版（edition 2021；Steam 相关依赖建议 1.80+）。
- **Windows 10/11**（Steam 联机功能与发布版 GUI 子系统仅针对 Windows）。
- Steam 联机需要本机 Steam 客户端已登录，并具备对应 AppID。

---

## 构建与运行

所有命令都在 `rust_remake/` 目录下执行：

```bat
cd rust_remake

cargo build -p client          :: 调试构建
cargo run   -p client          :: 启动 → 主菜单
```

### 单机试验场

```bat
cargo run -p client -- --solo
```

### 局域网对战

```bat
:: 主机
cargo run -p client -- --host 9001 --players 4
:: 客户端
cargo run -p client -- --join 127.0.0.1:9001
```

### Steam 联机

需开启 `client/steam` feature（会编译 Steam 传输与大厅接入）：

```bat
cargo build --release -p client --features client/steam
cargo run   -p client --features client/steam -- --steam-host
cargo run   -p client --features client/steam -- --steam-join
```

本机双开联调可用：

```bat
powershell -ExecutionPolicy Bypass -File run-steam.ps1
```

### 命令行参数

| 参数 | 说明 |
| --- | --- |
| `--solo` | 直接进入单机技能试验场 |
| `--host <port> [--players N]` | 创建局域网房间 |
| `--join <host:port>` | 加入局域网房间 |
| `--steam-host` | 创建 Steam 大厅（需 `--features client/steam`） |
| `--steam-join [lobby_id]` | 加入 Steam 大厅（可省略 ID 自动匹配） |
| `+connect_lobby <id>` | 由 Steam 邀请链接直接进入大厅 |
| `--players <N>` | 覆盖房间人数 |
| `--lang <auto\|zh\|en>` | 本次启动的语言覆盖（不写盘） |

---

## 测试与代码门禁

```bat
cd rust_remake
cargo test --workspace
cargo clippy --workspace -- -D warnings
cargo clippy --workspace --features client/steam -- -D warnings
```

一键本地回归（build + test + clippy）：

```bat
powershell -ExecutionPolicy Bypass -File check.ps1
```

启用提交前门禁（把 `.githooks` 挂到 `core.hooksPath`）：

```bat
powershell -ExecutionPolicy Bypass -File install-hooks.ps1
```

---

## 文档

`rust_remake/HANDOVER.md` 是**状态与下一步的总入口**；以下为专题文档：

- `FRAME_SYNC_ANALYSIS.md` / `FRAME_SYNC_INPUT_DELAY_DESIGN.md` / `FRAME_SYNC_OPTIONS_COMPARE.md` —— 帧同步设计与取舍
- `RECONNECT.md` —— 掉线与重连设计
- `STEAM_MULTIPLAYER_PLAN.md` / `STEAM_UI_REDESIGN.md` —— Steam 联机与大厅
- `RISK_ANALYSIS.md` —— 健壮性/正确性风险复核
- `UI_MASTER_PLAN.md` / `UI_AUDIT.md` / `LOBBY_UI_PLAN.md` —— 界面
- `I18N.md` —— 多语言
- `AUDIO_PLAN.md` —— 音效
- `WORKFLOW_NOTES.md` —— 开发约定与验证纪律

---

## 发布

```bat
cd rust_remake
powershell -ExecutionPolicy Bypass -File publish.ps1 -BuildOnly        :: 仅编译产物
powershell -ExecutionPolicy Bypass -File publish.ps1 -SteamUser <账号>  :: 上传构建
```

> 密码不写入命令行；依赖 `steamcmd` 已缓存的登录态。

---

## 许可证

本项目以 **Apache License 2.0** 授权，详见 [`LICENSE`](LICENSE)。
Copyright 2018-2026 Xv Zan。

### 第三方组件

- 随附字体 **LXGW WenKai（霞鹜文楷）** 采用 SIL Open Font License 1.1，
  许可全文见 `rust_remake/assets/fonts/FONT_LICENSE.md`。
- Rust 依赖及其许可证见 `rust_remake/Cargo.lock` 与各自 crate 的 `LICENSE`。
