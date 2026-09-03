# Rust 重制版潜在问题分析报告

> 创建 2026-09-02。对 `rust_remake/`（lockstep 帧同步 + host 权威 + Steam P2P 的 Warlock Brawl 重制版）
> 做的只读代码审查。每条问题都给出 `文件:行号` 证据与触发条件；`[已核实]` = 已读源码确认，
> `[待验证]` = 需真机/双账号/压力测试才能定论。
>
> 严重度约定：**严重**（可崩溃 / 静默 desync / 远程可触发）/ **中** / **轻**。

---

## TL;DR（最该先修的几条）

1. **网络包解码两处 off-by-one → 局域网/Steam 任意端可发恶意包让 host 崩溃**（远程 DoS）。`proto.rs:252`、`proto.rs:357`。
2. **击杀记账整体丢失**：被弹体/爆炸击杀的玩家永不进 `eliminated_order` / `kills_this_round` → 击杀金币全不发、名次奖励发错人。`world.rs:437-456` + `world.rs:751/1406`。
3. **Steam 快照入站缓冲只有 8192 字节**，中后期对局快照必然超限被静默丢弃 → 重连/主机迁移永远拿不到基线、永久卡死。`transport_steam.rs:255`。
4. **主机迁移无收敛机制**：原 host 复活（脑裂）时两个 host 同时产同 seq 不同帧，协议无校验和 → 静默分叉。`lockstep.rs:640` + `lockstep.rs:1011`。
5. **`check.ps1` / pre-commit 完全不编 steam 分支**：`client/src/steam.rs`(539 行) + `net-steam/` + 所有 `#[cfg(feature="steam")]` 块只在 `publish.ps1` 发布时才第一次编译 → 提交门禁形同虚设。

---

## 一、网络协议与崩溃风险（net / game-core 序列化）

### 严重

**P1 · `TAG_ACK` 长度守卫 off-by-one → 远端可触发 panic `[已核实]`**
- `net/src/proto.rs:252-255`：`TAG_ACK if buf.len() >= 10`，却 `buf[3..11]`（需 11 字节）。恰好 10 字节的包在切片时直接 panic。
- 入口可达：host 解码 `lockstep.rs:556/431`，client `lockstep.rs:1009/762`。同大厅任意成员都能让 host 崩溃。

**P2 · `TAG_ROOM_STATE` 长度守卫 off-by-one → 远端可触发 panic `[已核实]`**
- `net/src/proto.rs:353-357`：`TAG_ROOM_STATE if buf.len() >= 5`，却读 `[buf[4], buf[5]]`（需 6 字节）。恰好 5 字节的包 `buf[5]` 越界 panic。

**P3 · 击杀记账整体丢失 `[已核实]`**
- `world.rs:437-449` 死亡结算循环开头 `if !p.alive { continue; }`。
- 但 `damage_player`(`world.rs:751`) 与 `explode_at`(`world.rs:1406`) 在 `step_projectiles`(step 6，早于 step 7) 里**已把 `alive` 置 false**。
- 后果：被弹体/爆炸击杀者永不进 `eliminated_order`、击杀者永不进 `kills_this_round` → `take_kills()` 为空 → 击杀金币全不发(`meta.rs`)，`placement()` 只剩存活者再补 `u32::MAX` → 名次奖励发错人。游戏中**绝大多数击杀**走这条路径（技能/子弹/边界外圈出的死亡才被 step 7 记录，二者不一致）。

**P4 · 快照反序列化长度无上界 → 巨量内存分配/OOM `[已核实]`**
- `world_ser.rs:527/531/536/541/546/551`：`u32at(b)? as usize` 直接 `Vec::with_capacity`（`players`/`obstacles`/`projectiles`/`eliminated_order`/`kills_this_round`），无任何上限。
- 恶意或 flip-bit 损坏的快照填 `0xFFFFFFFF` 可直接申请数十 GB 后 `abort`。

**P5 · `ReconnectReq` 无鉴权、无限流 → 放大式 DoS `[已核实]`**
- `lockstep.rs:607-638`：任意来源 17 字节请求，host 即回整份 `World` 快照并**向全体广播 Resync**。可循环触发，并把无关端基线卷进来。

### 中

**P6 · Snapshot 长度前缀是 u16，接收缓冲过小 → 静默丢包 `[已核实]`**
- `proto.rs:208` `world_bytes.len() as u16`：>65535 静默截断，解码端 `end` 错位、seq 变垃圾。
- 更现实：`transport_steam.rs:255-259` `data.len() > buf.len()` 直接**静默丢弃并出队**；调用方缓冲仅 8192/4096(`main.rs:2577/2798/3010`)。见 P-Steam1。

**P7 · 无 session/magic/version，跨局旧帧污染 `[已核实]`**
- `Packet` 枚举(`proto.rs:52-98`)无任何局标识；client `expect_seq` 每局从 0 起(`lockstep.rs:744`)，`step_frame` 对 `seq >= expect_seq` 一律入队(`lockstep.rs:1012`)。只有 `drain_cfg`(`lockstep.rs:449`) 清旧 `PlayerCfg`，**没有清 Frame 的等价物** → 上一局在途旧帧会造出无法填补的巨大缺口。

**P8 · 玩家索引不校验来源 → 顶号/重定向作弊 `[已核实]`**
- `lockstep.rs:559/568/579/592`：`Input`/`RoomState`/`PlayerReady`/`PlayerCfg` 一律只按包里的 `index` 定位槽位并覆写 `self.client_peers[c] = Some(from)`，从不比对注册 peer。恶意端可顶替他人槽位输入，并把他人帧流重定向到自己。

**P9 · 掉线语义被 `poll` 自我撤销 `[已核实]`**
- `mark_dropped` 置 `client_peers[c] = None`(`lockstep.rs:395`)，但 `poll` 收到该槽任意 `Input` 就重新登记(`lockstep.rs:561`)，`dropped` 仍为 true → 默认占位与真实输入交替入帧，抖动且「不再向它广播」失效。

**P10 · 无输入队列，同窗口多包只留最后一个 `[已核实]`**
- `lockstep.rs:563/572` `latest_input[c] = Some(bytes)` 直接覆盖，前面到达的输入被静默丢弃（按键丢失）。

**P11 · `Periodic` 撒弹 `while` 无迭代上限 `[已核实-当前不触发]`**
- `world.rs:821-825`：`while *elapsed >= *interval { *elapsed -= *interval; spawn.push(...) }`。`interval` 为 0 则死循环挂死。
- **当前不可触发**：`ScatterPeriodic` 不继承 `DEF_ZERO`（全部字段显式填写，`skill.rs:1005-1012` 写死 `interval: 0.2`），所以现有技能表不会给 0。属**潜伏风险**——新增技能若漏填 `interval` 即中招，建议加 `debug_assert!(interval > 0)` 防御。

**P12 · 补发窗口仅 1 秒，缺口超 60 帧后客户端永久卡死 `[已核实]`**
- `lockstep.rs:602` 只从 `frame_buf` 找，`frame_buf_capacity = 60`(`lockstep.rs:104`)；缺失帧早于 60 帧即被 `pop_front`(`lockstep.rs:692`) 丢弃 → 缺口永远填不上，只能靠 180 tick 后的重连兜底。

### 轻

- `lockstep.rs:339` `orig_to_new.unwrap()`：当前不可达，一旦 `participants_orig` 与 `client_indices` 不同步即 panic。
- `lockstep.rs:1058-1082`：`try_advance` 的 `while` 首轮必 `return`，「连续消费多帧」注释与代码不符。
- `handshake.rs:70/82` `players: total as u8` / `my_index: idx as u8`：>255 人静默截断。
- `lockstep.rs:973-1000` `pump_frames` 只入不出、无上界无去重（当前仅测试调用）。

---

## 二、确定性风险（game-core）

> 两个是非题：**世界模拟用了 f64 浮点吗？——是**（位置/速度/伤害系数多为 `f64`/`Fix64` 混用），但所有三角/开方走 `cordic` + `fixed` 整数实现（`fix.rs`），全仓 `powf/sin/cos/f64::sqrt` **零命中**，Rust 默认不启用 FMA/fast-math，故 f64 `+-*/` 在同 target 上 IEEE 确定，**本身不导致 desync**。**有 HashMap 驱动模拟吗？——否**（`game-core` 仅 `#[cfg(test)]` 用 2 处 `HashSet`；net 层 HashMap 仅做键值查找，帧合成后 `sort_by_key` 确定排序）。`dt` 是固定常量 `1/60`(`main.rs:35`)，无可变 dt。**未发现** seq 回绕（`u64`）。

### 严重

**D1 · `Fix64::from_num(f64)` 溢出：dev panic / release 静默回绕 `[已核实]`**
- `fixed 1.28`：`debug_assert!(!overflow)` 仅 dev 生效，release 返回 wrapped 值；NaN/Inf 则无条件 panic。
- 仓库 `Cargo.toml` 无 `[profile.release]`（`fix.rs` 用的 fixed crate 默认 release `debug-assertions` 关闭）→ 若某端用 dev、某端用 release 构建即**静默分叉**。触发点：`player.rs:262/364`、`world.rs:2053`（`Fix64::from_num(p.damageplus)` 在属性叠加后极易溢出）、`world.rs:740/1369`。
- 下溢（`f64::MIN`，幅值远低于 I32F32 下界约 -2.1e9）由 fixed 1.28 同一 `debug_assert!(!overflow)` 覆盖，`[profile.release] debug-assertions=true` 下同样 panic。已补对称回归测试 `from_num_underflow_panics_instead_of_wrapping` / `from_num_overflow_panics_instead_of_wrapping`（`fix.rs`）。

### 中

**D2 · `cmd_head`/`cmd_len` 解码无范围校验 → 越界 panic `[已核实]`**
- `world_ser.rs:359-360` 解码为 usize 无范围检查；`player.rs:586` `self.cmd_buf[self.cmd_head]` 直接下标。快照里 `id != index` 必崩（`player.rs:734` `self.players[id as usize]` 同样无校验，风格不一致）。
- 上界（`cmd_head`/`cmd_len` 越界）已修（`005e6e2`）；**下界**（`decode_player` 的 `id`、`eliminated_order`/`kills_this_round` 的 id 須 `< np`，否则 `players[id as usize]` OOB panic）已补：`decode_player` 收 `np` 参数校验 `id < np`，`world_from_bytes` 校验 `eliminated_order`/`kills_this_round` 的 id。回归测试 `world_from_bytes_rejects_out_of_range_player_id` / `_eliminated_id` / `_kill_id`。

**D3 · 配置解码失败静默跳过 → desync `[已核实]`**
- `main.rs:1082` 只 `eprintln!` 后继续；`progress.rs:120` 版本不符即返回 `None` → `CONFIG_VERSION` 一 bump 就全部拒收，各端保留旧技能等级/金币 → 世界状态分叉。

**D4 · 输入解码失败静默替换为空输入 `[已核实]`**
- `main.rs:2603/2718/2816` `decode_player_input(&bytes).unwrap_or_default()`，丢包/坏包被当成「什么都不做」。

### 轻

- **D5 · RNG 返回值区间与注释不符 `[已核实]`**：`rng.rs:37` `next_fix` 取高 32 位但 `as i32 as i64` 符号扩展 → 实际返回 **[-0.5, 0.5)** 而非文档声明的 `[0,1)`。确定性无损，但所有调用点数值偏离设计：`world.rs:1512` jitter 恒为负、`world.rs:1514` 柱子半径落在 [0.85,1.35) 等。建议修正或改注释。
- **D6 · `progress.rs` 实为网络快照编解码器，非本地存档 `[已核实]`**：全仓 `std::fs`/`File::` 零命中，没有真正的本地存档读写——故「存档损坏/原子写入/路径」风险不适用；等价风险即 D3（解码失败=静默丢进度，无重试）。
- **D7 · i64→i32 静默截断 `[已核实]`**：`progress.rs:78-79` `p.gold = self.gold as i32`；`meta.rs:109-117` `upgrade_skill` 无等级上限、无 `cost <= 0` 校验 → 负 cost 可刷金币；`upgrade_cost = (level*5+5) as i32` 极高等级 u32 溢出后可转负。
- **D8 · `debug_assert_eq!(input.len(), self.players.len())`(`world.rs:348`) release 失效**，长度不符时 `zip` 静默截断。
- **D9 · `projectiles` 无数量上限**：E3b 沿途撒弹 + 快照体积随弹数膨胀（与 P6 叠加）。
- **D10 · `round_seed.wrapping_add(1)` + `next_fix` 取高 32 位（2⁶⁴→2³² 折叠）**，长赛程柱子布局碰撞概率待验。

---

## 三、Steam 联机（net-steam / client/steam.rs）

### 严重

**S1 · 快照入站缓冲仅 8192 字节 → 重连/迁移永远拿不到基线 `[已核实]`**
- `transport_steam.rs:255-258` `data.len() > buf.len()` 直接丢弃并继续循环；接收缓冲全是 8192(`main.rs:2577/2653/2688`、`steam.rs:132`)。
- `world_ser.rs` 每玩家 ≥761 字节(`caster 36×8 + skill_levels 36×4 + buffs 16槽 + cmd_buf 8×22`)，7–8 人 + 场上弹幕即 >8KB → `recv_snapshot` 永不返回 → `poll_steam_reconnect` 无限等待(`steam.rs:159`)，迁移接管也无快照基线。**需实测一次中后期对局打印 `world_to_bytes().len()` 确认是否真超 8192**。

**S2 · 单端掉线被误当 host 掉线 → client 自行 takeover → 双 host `[已核实]`**
- `main.rs:2734-2739`：180 帧无权威帧就直接 `steam_migrating=true`，**不区分「我断」还是「host 断」**，也没用 `lobby_owner`/`is_established` 交叉验证；随后 `steam.rs:65-86` 若自己 ID 最小就接管。自己网络闪断或被 host 判掉后反手当 host。

**S3 · 原 host 复活（脑裂）无收敛机制 `[已核实]`**
- `lockstep.rs:640` poll 对 `Takeover` 走 `_ => {}`；`lockstep.rs:1011` client 收 `Frame` **不校验来源是不是 host**；接管方给旧 host 的 peer 是 `None`(`steam.rs:202`)→根本不发 Takeover。两个 host 同时产同 seq 不同内容帧，协议无校验和，静默分叉。

**S4 · 迁移阶段 B 无超时/无重选/无 UI → 永久卡死 `[已核实]`**
- `steam.rs:88-108`：收不到 Takeover 就一直 `Ok(Some(cli))` 返回，世界冻结。当选出的新 host 自己也没了、或候选集为空致 `new_host_id=0`(`steam.rs:73` `unwrap_or(0)`) 时必死。全仓库无迁移状态覆盖层。

**S5 · 重连每帧重发 Req，host 每帧向全员广播整份快照 `[已核实]`**
- `steam.rs:127` 每帧发 `ReconnectReq` 无退避；host 每收一次就 `send_to(Snapshot)` + 向**所有** peer 广播 `Resync`(`lockstep.rs:626-638`)→ 60 次/秒 × N 人 × 多 KB 放大。且 Steam 端**从不置 `conn_dropped=true`**（仅局域网 `main.rs:2880` 置位）。

**S6 · 二次 `Client::init_app` `[待验证-高可疑]`**
- `enter_steam_mode` 用 `self.steam_sess.take()`(`main.rs:3902`)；退房/失败后 `steam_session_tried=false`(`main.rs:3104/4041`) → 主菜单再次 `SteamSession::init`(`steam.rs:503`)。steamworks 0.13.1 文档明示「每进程只应有一个 Client」，且 `Client` 无 `Drop` 关停 → 二次 init 会建一个空回调表的新 `Inner`，旧 `session_request_callback` 用 `mem::forget` 注册在旧 `Inner` 上。可能导致重进房间后大厅/P2P 回调失效。**需真机验证。**

### 中

- **S7 · 无缓存快照时不广播 Takeover `[已核实]`**：`steam.rs:222-225`/`main.rs:2584-2588` 要求 `current_snapshot()` 有值。`SNAPSHOT_EVERY=30`(`main.rs:45`)，开局 0.5 秒内 host 掉线 → **零 host**。
- **S8 · 选举集不随 host 自动掉线更新 `[已核实]`**：`main.rs:2590` 只 `mark_dropped`，不改 `steam_online`；下次迁移可能选到早已掉线的端(`steam.rs:72`)。
- **S9 · 迁移后大厅侧与 lockstep 侧槽位不一致 `[已核实]`**：`main.rs:3290-3294` 用 `t.steam_id()` 当 host 重建表，而 lockstep 里新 host 仍是原 player index(`steam.rs:182/211`)；HUD 仍取 `steam_participants.first()` 当 host 查 ping(`main.rs:1954`)。
- **S10 · `send_to` 恒返回 Ok `[已核实]`**：`transport_steam.rs:217/232` 入队也算成功；`PENDING_MAX=1024` 满时丢最老一条(`transport_steam.rs:132-138`)，破坏 RELIABLE 有序语义、只打 10 条日志后静默。
- **S11 · 掉线判定与接管竞态 `[已核实]`**：`CLIENT_STALE_TICKS == HOST_DROP_TICKS == 180`(`main.rs:40/47`)，迁移期 client 不再上行 RoomState → 双方几乎同时判定对方已死。
- **S12 · 大厅操作在主线程 sleep 忙等 `[已核实]`**：`session.rs:459-469/666-672/707-713/773-779`，`join_lobby_by_id(id, 240)`(`main.rs:3954`) 最长阻塞 12 秒。

### 轻

- `join_requests` 队列无上界(`session.rs:376/405/418`)。
- 回调里 `.lock().unwrap()`(`session.rs:285/336/405/418`、`steam.rs:454`、`main.rs:2148`) + `main.rs:2544` `.expect("all_cfgs 已确保收齐")`：持锁端 panic 会连锁 panic。
- **未发现** `unsafe` / `static mut`（net-steam 与 client 全量 grep 零命中）；成就/统计**未发现**每帧上报（`steam.rs:431-434` 有一次性闸门，presence 3s 节流、ping/头像 30 帧节流，失败只打日志不影响逻辑）。

### 需真机/双账号验证

1. S1 快照真实字节数 vs 8192（最优先，在 `host_frame_count % 30` 处打一条 `world_to_bytes().len()` 即可判定）。
2. S6 二次 `Client::init_app` 后回调是否仍工作。
3. `steam_online` 是否逐位一致（`main.rs:2659` 只在收到 `PlayerCfgAll` 后试收一次 Participants，未收到则永久为空 → 选举必得 0）。
4. host 掉线后 Steam 何时改 `lobby_owner`、与代码选出的新 host 是否重合。

---

## 四、客户端（client/src/main.rs 4833 行）

### 严重

**C1 · 联网客户端在对局中无任何退出路径（死状态）`[已核实-逻辑]`**
- `main.rs:2471-2476` Esc 被 `solo_no_net` 门控；Q 只在 `MatchPhase::Finished` 生效(`main.rs:2368`)。`LanJoin` 下 `net_link.is_some()`、Steam join 下 `steam_cli_ls.is_some()` → Esc 永久失效，加入方只能打完整场或强杀窗口。

**C2 · `solo_no_net` 漏检 `net_host_ls`，host/client 退出能力不对称 `[待验证]`**
- `main.rs:2466`（非 steam）：`net_link.is_none() && net_host.is_none()`。握手收齐后 `net_host` 已移交 `net_host_ls`(`main.rs:3020`)？→ 若 `net_host` 在 Fighting 期间为 None，则 host 反而 `solo_no_net=true` 能 Esc 弃局，client 不能，能力倒置。**需核对 LAN host 在 Fighting 时 `net_host`/`net_host_ls` 的实际取值。**

### 中

- **C3 · `check.ps1` / pre-commit 完全不覆盖 steam feature `[已核实]`**：`check.ps1:27-29` 只跑 `--workspace`；`client/Cargo.toml:11` 的 `steam` feature 默认关闭 → `client/src/steam.rs`(539 行)+`net-steam/`+ 所有 `#[cfg(feature="steam")]` 块**零编译零 clippy**，`publish.ps1:79` 是唯一编译它们的地方。**提交门禁形同虚设**，Steam 分支的编译错误要等到发布才发现。
- **C4 · 每帧重建 `Text` + `format!` + 全量 `measure()` `[已核实]`**：`main.rs:4489-4493` `draw_text` 每调用 `.font("cjk".to_string())` 堆分配 + `measure()` 重做字形布局；`draw_pre_game` 30 处 / `draw_meta_overlay` 20 处 / `draw_menu` 12 处均由每帧 `draw()` 触发。性能隐患（低端机/高分辩率掉帧）。
- **C5 · `draw_scene` 每帧新建约 40 个 Mesh 无缓存 `[已核实]`**：`main.rs:1377-1728` 粒子/子弹/圆环逐条 `Mesh::new_*`，无批处理/复用。
- **C6 · 17MB 字体 `include_bytes!` 内联 `[已核实]`**：`main.rs:524` 直接 +17.7MB 进二进制；`assets/fonts/cjk-168k.ttf`(168KB) 已入库但代码无任何引用（死资源）。
- **C7 · 联网热路径上的 `expect` `[已核实]`**：`main.rs:2544/2776` `host.collect_cfgs().expect("all_cfgs 已确保收齐")`；一旦竞态使两者不一致直接 panic 退出进程。
- **C8 · 文本输入两条独立写入路径，有重复插入风险 `[已修复-已验证]`**：`Ime::Commit`→`on_text_input`(`main.rs:2964`) 直接 `buf.push`，与每帧遍历 81 字符白名单 `just(c)`(`main.rs:3236/3665`) 写同一 buffer，二者互不感知。已由 `c6db353` 用去重守卫修复，并**真机验证通过**（winit 0.30 下 IME 组合键走 `Named(Process)`，不双发 `Character`，无重复插入；IME 后直接 ASCII 输入正常）。
- **C9 · IME 声称支持 `ReceivedCharacter`，实际未实现 `[已核实]`**：注释(`main.rs:4569/2961`)说接入两者，但事件循环只有 `WindowEvent::Ime(Ime::Commit)`(`main.rs:4621`) 一个分支；`key_down_event`(`main.rs:2942`) 只 `eprintln!`，`text_input_event` 全文件不存在。

### 轻

- `main.rs:2998/3236/3665` 用 `buf.len() < 80` 按**字节**限长，中文 3 字节 → 实际约 26 汉字，与 UI 语义不符。
- `main.rs:2900` 附近 `key_down_event` 收到 `repeat` 参数直接丢弃(`main.rs:2942`)：Backspace 长按能否连续删除未验证。
- **未发现**：`netlink.rs` 生产代码仅 `netlink.rs:94` 一处被守卫的 `unwrap()`；按玩家索引取 HUD 越界未发现实际越界点（均有 `min()/.min(n)/(idx as usize)<n` 守卫）。

### 工程化 / 脚本 / 仓库卫生

- **C10 · `publish.ps1` 把 Steam 密码还原成明文拼进命令行 `[已核实]`**：`publish.ps1:199-221` `SecureStringToBSTR`→`PtrToStringUni` 转明文后拼入 `$steamArgs` 传给 steamcmd → 同机任意进程可枚举命令行读到密码。
- **C11 · `install-hooks.ps1` 钩子作用域过大 `[已核实]`**：`git config` 把**整个 testingLL/** 的 hooksPath 指向 `rust_remake/.githooks`，仓库内其它子目录提交也会跑本项目的 build+test+clippy。
- **C12 · pre-commit 易绕过/静默放行 `[已核实]`**：`pre-commit:10-13` 一行 `SKIP_HOOKS=1` 即可绕过；`:20-29` 找不到 `check.ps1`/`powershell` 时静默 `exit 0` 放行。
- **C13 · 缺 `[profile.release]` `[已核实]`**：`Cargo.toml` 只有 `[profile.dev]`，叠加 17MB 字体产物更大、无 LTO/strip；且与 D1 的 `debug-assertions` 行为相关。
- **C14 · `steam_appid.txt` 既未跟踪也未忽略 `[已核实]`**：`rust_remake/steam_appid.txt` 工作区长期脏状态（仓库根的同名文件反而已入库）。
- **C15 · 仓库卫生「未发现」**：`netlogs/` 已被 `.gitignore:7` 忽略（未跟踪）；`target/` 已忽略；`Cargo.lock` 已提交（373 包全 `registry+crates.io`，无 git 依赖 → 可复现构建 OK）。

---

## 五、修复优先级建议（供排期）

| 优先级 | 条目 | 理由 |
|---|---|---|
| **P0** | P1 / P2 解码 off-by-one | 远程可崩，几行 `>=` 改 `>=` 即可修，零成本 |
| **P0** | P3 击杀记账丢失 | 影响经济/名次核心玩法，逻辑修复 |
| **P0** | C3 pre-commit 覆盖 steam | 提门禁可信度，避免发布才爆编译错误 |
| **P1** | S1 快照缓冲 8192 | 重连/迁移基础，需先测真实字节数再定方案（增缓冲/分片） |
| **P1** | S2 / S3 / S4 迁移收敛 | 多 account 真机必踩，当前脑裂/双 host/卡死无解 |
| **P1** | P4 / D2 解码长度/范围上界 | 防 OOM/越界 panic，尤其接收不可信数据 |
| **P1** | D1 `from_num` 溢出 release 分叉 | 加 `overflow-checks` 或 clamp，或确认所有端同 profile |
| **P2** | P5 / P8 / P9 / P10 / P12 网络健壮性 | 掉线/作弊/卡死边界 |
| **P2** | C1 / C2 联网退出路径 | UX 死状态 |
| **P2** | C10 密码明文 | 改走 stdin/env，不进命令行 |
| **P3** | C4 / C5 渲染性能 | 低端机掉帧 |
| **P3** | D5 RNG 区间、D7 负金币、S7/S8/S9/S11 迁移细节 | 数值/一致性打磨 |
| **待定** | P6 / P7 / D3 / D4 / S5 / S6 / C6-C9 / C11-C14 | 视真机验证结果定 |

---

## 修复进度（2026-09-03，P0 全部完成）

| 项 | 提交 | 说明 |
|---|---|---|
| P1 / P2 协议解码越界（DoS） | `9f80eba` | `proto.rs` 长度守卫 `>=10→>=11` / `>=5→>=6`；补边界回归测试 `decode_length_guard_exact_boundary_does_not_panic` |
| P3 击杀记账丢失 | `37d2f7e` | `damage_player`/`explode_at` 致死瞬间记账（每玩家一次）；补测试 `projectile_kill_is_recorded_in_kills_and_eliminated_order` |
| C3 门禁覆盖 steam | `eeec84b` | `check.ps1` 增 `-p client --features client/steam` 的 build+test+clippy（已验证干净：7 测试通过、无 clippy 警告） |
| D1 `from_num` 溢出 release 静默分叉 | `46ddbbc` | `[profile.release]` 开 `debug-assertions` + `overflow-checks`；`from_num` 溢出在 release 与 dev 一致地 panic。验证：release 全量 152 测试通过无误触发，回归测试在 dev/release 均 panic |
| P4 / D2 解码上界 | `005e6e2` | `world_ser` 的 count 经 `count_at()` 上界校验（64 玩家/256 柱/4096 弹体/4096 击杀）防 OOM；`cmd_head`/`cmd_len` 越界校验防 `cmd_buf` OOB panic |
| D1 `from_num` 下溢对称回归 | `6b5740b` | `fix.rs` 增 `from_num_underflow_panics_instead_of_wrapping`：`f64::MIN` 在 `[profile.release] debug-assertions=true` 下 panic（与溢出同一 `debug_assert!`，行为锁定） |
| D2 解码下界（id 范围） | `6b5740b` | `decode_player` 校验 `id < np`；`world_from_bytes` 校验 `eliminated_order`/`kills_this_round` 的 id `< np`，越界快照解码期返回 `None` 而非 step 期 OOB panic；增 3 个回归测试 |
| S1 快照缓冲 8192→256KiB | `c6db353` | 所有 Steam 入站缓冲提到 256KiB（快照不再被 transport_steam 静默丢弃）；transport_steam 的「超界包丢弃」改可见告警；host 接管广播快照处打字节数日志；重连 stall-abort（~10s 无快照放弃，玩家可 Esc） |
| S2 旧 host 接管通知 | `c6db353` | 新 host 接管时单发 `Takeover` 给旧 host（`HostLockstep::notify_old_host_takeover`）；旧 host 的 `HostLockstep` 收到 `Takeover` 标记 `superseded` |
| S3 迁移阶段 B 超时 | `c6db353` | `poll_steam_migration` 阶段 B 超 `MIGRATE_BAIL_TICKS`(600) 退回主菜单，避免永久冻结在「已冻结世界」 |
| S4 脑裂/双 host 收敛 | `c6db353` | `HostLockstep.superseded` 置位后 `try_emit` 不再产权帧；`main.rs` 的 steam host 分支检测 `is_superseded()` 退回主菜单 |
| S5 重连每帧重发刷屏 | `c6db353` | host 对每客户端重连应答限速 `RECONNECT_RESP_INTERVAL`(30)；新增回归测试 `reconnect_resp_is_rate_limited_per_client` |
| C1/C2 联网退出路径 | `c6db353` | 对局中 Esc 直接 `reset_to_main_menu`（联网亦可；host 离开触发既有 drop→迁移路径），消除死状态 |
| C6 17MB 字体内联 | `c6db353` | 运行时从磁盘加载完整 `cjk.ttf`（不再 `include_bytes!` 内联 17.7MB），找不到回退内联 168k 子集；`publish.ps1` 随 exe 分发 `cjk.ttf` |
| C7 热路径 `expect` | `c6db353` | `collect_cfgs` 竞态由 `.expect` 改为本轮重试（不 panic），下一帧再同步 |
| C8 IME 双路径重复插入 | `c6db353` | `frame`/`last_ime_commit_frame` 去重；`just(c)` ASCII 白名单在 IME 提交帧跳过。**已真机验证通过**：winit 0.30 在 `set_ime_allowed(true)` 下，IME 组合键走 `Named(Process)` 而非独立 `Character` 键，不双发；`sadf`/`大幅度` 各精确插入一次，IME 后直接 ASCII 输入不被误抑制。补充无头单测 `c8_*` 覆盖同帧抑制/无 IME 正常插入/跨帧残留风险 |
| C9 注释错误 | `c6db353` | 修正：winit 0.30 已移除 `ReceivedCharacter`，文本只走 `Ime::Commit` |
| C10 密码明文 | `c6db353` | `publish.ps1` 不再把 Steam 密码还原明文拼命令行，要求 `steamcmd` 已登录缓存（`loginusers.vdf`） |

> 剩余未修项：S6-S12 其余 Steam 迁移/健壮性细节（S10 `send_to` 恒返回 Ok、S11 掉线判定与接管竞态、`CLIENT_STALE_TICKS==HOST_DROP_TICKS`、S12 大厅 sleep 忙等）、
> C4/C5 渲染性能（按规划暂缓）、C11-C14 工程化（钩子作用域过大/易绕过/`steam_appid.txt` 未跟踪/缺 `[profile.release]`）、
> D3/D4/D5/D7 数值与解码边界、P5/P8-P12 网络健壮性——多为需真机/双账号验证或涉及设计取舍的改动。

---

## 附：审查过程与可信度

- 由 4 个并行只读 Agent 分别审查 网络同步层 / Steam 联机层 / 世界确定性 / 客户端与工程化，再由人工逐条复核关键证据（`proto.rs`、`world.rs` 死亡记账路径、`rng.rs`、`world_ser.rs`、`transport_steam.rs`、`main.rs` 退出判定、`publish.ps1`、`Cargo.toml`）。
- 标注 `[已核实]` 的条目均已读源码确认；`[待验证]` 为需动态环境（真机/双账号/压力测试）才能定论者，已在各节末尾汇总。
- 与原 `WORK_BACKLOG.md` / `resume.md` 的关系：本文件聚焦**代码层潜在缺陷**（含此前未记录的新问题），业务待办仍看 `WORK_BACKLOG.md`。
