# Steam Demo（试玩版）方案

> 目标：同一份源码出**两个产物**——正式版（AppID `908660`）与 demo（AppID `1042120`，Depot `1042121`）。
> demo 玩法与正式版**一致**（训练场 + PvP + Steam 大厅），但**不提供创意工坊 / 成就 / 天梯**。
> 两个版本用**同一个 commit / 同一 `PROTOCOL_VERSION`** 出包。

---

## 0. 已拍板的决定

| # | 问题 | 决定 |
|---|------|------|
| Q1 | demo 的 PvP 范围 | **也开 Steam 大厅**（demo 玩家之间对战；**与正式版天然不互通**） |
| Q2 | 工坊/成就/天梯入口 | **保留入口 + 点击给出"demo 不支持"提示**（不隐藏；值列显示 `demo 不支持`） |
| Q3 | 发布节奏 | demo 与正式版**同一 commit / 协议**出包 |
| Q4 | AppID 形式 | **硬编码在代码里按 feature 决定**（不依赖文件夹里的 `steam_appid.txt`） |
| Q5 | 结算画面 | 比"不画"更简单：在**生产端**（`steam_record_match_result`）早退，数据不产生 → 绘制自然为空 |

---

## 1. 实现方式

### 1.1 能力开关：`client/src/appid.rs`（新增）

```rust
pub const APP_ID_FULL: u32 = 908_660;
pub const APP_ID_DEMO: u32 = 1_042_120;
pub const IS_DEMO: bool = cfg!(feature = "demo");
pub const APP_ID: u32 = if IS_DEMO { APP_ID_DEMO } else { APP_ID_FULL };
pub fn app_id_str() -> String;      // 工坊内容目录名
pub fn workshop_url() -> String;    // 指向本 app 的工坊页
pub fn workshop_enabled() -> bool;  // demo = false
pub fn stats_enabled() -> bool;     // demo = false（成就/统计/天梯）
```

`client/Cargo.toml` 新增：`demo = ["steam"]`（demo 自动带 steam）。

### 1.2 之前写死 AppID 的地方（已全部改为引用 `appid`）

| 位置 | 原值 | 现状 |
|------|------|------|
| `client/src/main.rs` | `const APP_ID: u32 = 908660;` | `use appid::APP_ID;` |
| `client/src/audio_pack.rs` | `pub const APP_ID: &str = "908660";` | `app_id_str()`（另提供 `workshop_content_root()` 内部使用） |
| `client/src/steam.rs` | `WORKSHOP_URL = .../app/908660/workshop/` | `appid::workshop_url()` |
| 测试 | 断言含 `"908660"` | 断言含 `app_id_str()`（两个 feature 都过） |

### 1.3 demo 的能力降级（"给提示"而不是隐藏）

- **设置页**（`SETTINGS_ROWS` 第 6–10 行：创意工坊物品 / 要发布的包 / 发布本地包 / 发布时复用 id / 发布可见性）：
  - `settings_adjust()` 开头判断 `!appid::workshop_enabled()` → `settings_flash("demo 版不支持创意工坊（正式版可用）")` 并 `return`；
  - `settings_value_text()` 对应行返回 `demo 不支持`；
  - 新增 `settings_msg` + `settings_msg_until`（按**帧号**计时 ≈3s），画在设置页底部原本放包详情/快捷键提示的那一行。
  - **为什么不用"隐藏行"**：`SETTINGS_ROWS` 是**定长数组**，行号被 `settings_row`、`settings_adjust`/`settings_value_text` 的 `match`、`layout::row_in(..., SETTINGS_ROWS.len())`、键位表与布局测试引用；动态隐藏要把定长数组改成运行时 `Vec` 并逐处检查索引 → 改动面大、易漏。给提示只动"动作处"。
- **结算画面**（Steam 统计行 + 天梯 TOP5 + 成就 toast）：
  - `steam.rs::steam_record_match_result()` 开头 `if !appid::stats_enabled() { return; }`；
  - `steam.rs::steam_ensure_leaderboard()` 同样早退（省掉无谓的异步请求）；
  - 效果：`steam_stats_snapshot` 保持 `None` → 结算画面那块 `if let Some(s) = ...` **自然不画**，成就 toast 不弹，也不会出现"暂无数据（需在 Steamworks 后台创建）"这类脏文案。**绘制代码一行未改**（这正是"比不画更简单"的做法）。

### 1.4 脚本

| 脚本 | 变更 |
|------|------|
| `publish.ps1` | 新增 `-Target full\|demo`（默认 `full`）；按目标切换 `AppId / DepotId / 编译 feature / staging 目录 / VDF`。demo：`1042120 / 1042121 / client/demo,client/gui / target/steam-pipe-demo` |
| `run-steam.ps1` | 新增 `-Target full\|demo`；编译对应 feature，并**按目标写入** `target/debug/steam_appid.txt`（不再从仓库根拷贝，避免 demo 跑到正式版 appid 上）；日志名带 target |

### 1.5 关于 `steam_appid.txt`

- 代码走 `Client::init_app(appid)`，AppID **显式传给 API**，不依赖该文件；
- `steam_appid.txt` 只在**开发时脱离 Steam 直跑 exe** 有用 → 由 `run-steam.ps1` 按 target 生成；
- **发布包不带它**（`publish.ps1` 不收集该文件）；
- 已知风险：直跑 exe 时 **Steam 覆盖层可能不挂**（覆盖层靠 Steam 判定"当前跑的是哪个 app"）；由 Steam 启动则正常。→ 待真机验证（§4）。

---

## 2. Steamworks 后台清单（非代码）

1. demo app **1042120** 下**建 Depot**：本方案用 **1042121**。
2. 在 demo app 后台与正式版建立 **"Demo of app" 关联**到 `908660`（否则商店页不会出现"下载 Demo"）。
3. steamcmd 使用的账号对 **1042120** 有发布权限。
4. **不需要**为 demo 配置：Workshop、Stats/Achievements、Leaderboards（我们按 `IS_DEMO` 关闭入口）。
5. Steam 大厅（matchmaking）**无需额外配置**，默认可用；**lobby 属于各自 app → demo 与正式版天然互不可见**（正是 Q1 要的效果，零代码）。

---

## 3. 命令速查

```powershell
# 开发（本地跑）
powershell -ExecutionPolicy Bypass -File run-steam.ps1 -Mode menu                 # 正式版
powershell -ExecutionPolicy Bypass -File run-steam.ps1 -Mode menu -Target demo    # demo

# 出包（编译 + 收集产物，不上传）
powershell -ExecutionPolicy Bypass -File publish.ps1 -BuildOnly                  # 正式版
powershell -ExecutionPolicy Bypass -File publish.ps1 -BuildOnly -Target demo      # demo

# 自动发布到 Steamworks
powershell -ExecutionPolicy Bypass -File publish.ps1 -SteamUser <账号>                  # 正式版
powershell -ExecutionPolicy Bypass -File publish.ps1 -SteamUser <账号> -Target demo     # demo
# 需要设为分支上线时再加 -SetLive <branch>（default 分支会被 steamcmd 拒，需在后台网页设置）
```

---

## 4. 已知限制 / 待真机验证

| 项 | 说明 |
|----|------|
| **Steam Cloud 不互通** | 云按 AppID 存 → demo 与正式版的键位/设置是两套；demo 未配置云时自动退化为纯本地（代码无需改） |
| **匹配池隔离** | demo 只能匹配 demo（Steam 行为）。demo 池冷启动可能约不到人 → 可考虑"demo 提供直连/LAN 作为备选"（`--host/--join` 已支持） |
| **跨版本直连** | `--join <ip>` 理论上可跨版本，但需两端 `PROTOCOL_VERSION` 与构建一致；不一致会被现有协议校验挡住 |
| **覆盖层直跑** | 直跑 exe 时覆盖层可能不挂（见 §1.5）；发布包由 Steam 启动无此问题 |
| 待验证 1 | `run-steam.ps1 -Target demo` 能起来，日志出现 `[steam] session ready`，且 appid 是 1042120（`t.app_id()`） |
| 待验证 2 | demo 里设置页 5 行显示 `demo 不支持`，点击给出提示；对局结束后结算画面**没有**统计/天梯块 |
| 待验证 3 | demo 能进 Steam 大厅建厅/加入；正式版客户端**看不到** demo 大厅（反向亦然） |
| 待验证 4 | `publish.ps1 -BuildOnly -Target demo` 的 staging 目录内容正确（exe / dll / 字体 / LICENSE，**无** appid.txt） |

---

## 5. 未做（明确不在本次范围）

- 自建匹配/中继实现"demo 与正式版同池"（Steam 原生不支持，属大工程）；
- demo 内的"购买正式版"引导入口（可后续在设置页加一行链接到商店页）；
- demo 专属成就/天梯（按决定不提供）。
