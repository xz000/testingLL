# 帧同步 / 联机卡顿 分析（2026-09-13）

> 起因：用户问「client 每帧读房间信息是否有必要 / 网络波动会不会收不到」，并反馈上一次联机
> 「偶尔一卡一卡」。本文记录从代码得出的结论与修复建议。
> 结论先行：**房间信息读取不是问题；卡顿主因高度指向「所有包共用一个可靠有序 Steam 频道」，
> 周期性 64KiB+ 快照与 60Hz 帧包互相队头阻塞。** 其次是 host 产帧节奏与 client 追赶方式。

---

## 一、client 每帧读房间信息：可靠，但非必需

- `steam_sync_room_meta`（`client/src/steam.rs`）调用 `matchmaking().lobby_data(...)` / `lobby_member_limit(...)`。
  这些是 Steam **本地大厅缓存**的读取（`GetLobbyData`），**不发起网络请求**；数据由 `LobbyDataUpdate_t`
  回调在本地更新。
- 回调是否被泵出：房间阶段每帧都在 `cli.send_room_state(...)` / `cli.recv_room_inbox(...)`，
  底层 `SteamTransport::send_to/recv_from` 会 `run_callbacks()`（`net-steam/src/transport_steam.rs:85` 等）。
  所以缓存会**持续更新**，不存在“某帧没读到就永久丢失”。
- 房间设置串 `room_cfg` 的变更检测（`steam_lobby_update` 里 client 分支）用的**正是同一条** `lobby_data`
  通路 —— 也就是说：“房主改设置 → 各端取消准备”与“client 回读房名/备注”依赖同一机制，可靠级别相同。
- 所以：**每帧读不是必要的**（改到每 ~15 帧一次也完全可以），但**没有可靠性问题**；它是“最终一致”，
  不是“每包必达”。网络波动不会让数据丢失，只会让更新晚几十毫秒到（取决于 Steam 同步）。

**可选小优化**：给 `steam_sync_room_meta` 加一个 ~15 帧的节流计数器，省一点无谓的字符串分配；非必需。

---

## 二、联机「偶尔一卡一卡」的成因（按怀疑度排序）

### A. 所有包共用一个**可靠有序**频道 → 周期性快照队头阻塞（设计隐患，影响取决于链路）

- `SteamTransport::send_reliable` 固定 `send_message_to_user(identity, flags, data, /*channel=*/0)`，
  flags = `RELIABLE_NO_NAGLE | AUTO_RESTART_BROKEN_SESSION`（`net-steam/src/transport_steam.rs:95`）。
  接收端也固定 `receive_messages_on_channel(0, batch)`。同一 channel 上 RELIABLE = **有序恰好一次**。
- **每 30 帧（约 0.5s）**host 会 `broadcast_snapshot(...)` 把**整个 World** 序列化后发给所有 client
  （`client/src/main.rs` 两处 `host_frame_count % SNAPSHOT_EVERY == 0`，`SNAPSHOT_EVERY = 30`）。
- **实测快照大小**（临时测试 `world_to_bytes`，已验证后删除）：
  2 人 ≈ **3.0KB**、4 人 ≈ **6.1KB**、8 人 ≈ **12.5KB**（还带若干弹体/柱时略大）。
  ⚠️ 修正：`net/src/proto.rs` 的 `snapshot_over_64kib_roundtrips` 是**合成负载**测试，
   **不代表**真实快照 >64KiB（之前初稿误引了它）。
- 即便如此，大快照与 60Hz 小 `Frame` 包走**同一可靠有序频道**：可靠有序 = **前一条完整送达前后面不交付**。
  正常情况下 12KB ≈ 8 个满包，队头窗口只有几～十几 ms（影响小）；但**一旦其中一个包丢失**，
  重传要一个 RTT（几十～上百 ms），期间后面的帧全被挡 → 偶发明显卡顿。
  另外会话暂不可发时 `flush_pending` 的 FIFO 补发队列（`send_to` 里“有 pending 就追加队尾”）
  会让帧包排在快照后面，也放大突发。
- 同类还有 `StateHash`（每 30 帧，很小）。

> 结论：A 是**设计隐患 + 偶发大卡**的主因（快照不大但共用可靠有序频道）；
> 日常的“持续小抖”更可能来自 B（host 产帧被输入到达牵着走）与 C（client 追赶快进）。

### B. host 产帧节奏被“输入到达”牵着走

- `HostLockstep::try_emit`（`net/src/lockstep.rs`）：**收齐所有参与端本帧输入**才产帧；缺任一端输入就 `None`。
- host 主循环 `while accumulator >= TICK { poll(); if try_emit() {...} else break }`
  （`client/src/main.rs` 非 Steam host 与 Steam host 两处）。
- 结果：帧间隔 = 「最慢那端输入到达」的间隔，**把网络抖动直接转成帧率抖动**。没有固定节拍产出/输入延迟缓冲。
- 另外输入是「latest wins」：`poll` 用新输入覆盖 `latest_input`，如果某端一个 tick 内发来两条，前一条被覆盖丢弃
  （离散施法在抖动下可能被吞）。这更多影响“操作响应”，不直接造成卡顿，但会放大体感。

### C. client 用墙钟 accumulator，且“无帧时不扣” → 追赶式快进

- client：`self.accumulator += dt.min(0.25)`，然后
  `while accumulator >= TICK { 发输入; if step_frame() Some { step; accumulator -= TICK } else break }`
  （`client/src/main.rs` Steam client / LAN client 两处）。
- `else break` **不扣 accumulator**（注释写“避免时间凭空流逝导致分叉”）。但推进本身是**按收到帧**进行的，
  少扣 accumulator 不会分叉，只会让任何一次卡顿期间 accumulator 越涨越多；恢复后在一个渲染帧内
  **连续 step 多帧**（快进），于是“卡一下 → 追一下”。而且发输入也在追赶循环里，一追就发一串输入。
- 没有渲染插值：帧率/网络任何抖动都直接变成画面位置抖动。

---

## 三、修复建议（优先级从高到低）

1. **不再每 30 帧广播快照；本地快照 + 低频广播**（**已实施**，见第四节）
   - 重连用：host 本地每 30 帧 `set_snapshot`（不发网络）；收到 `ReconnectReq` 时按需单发。
   - 接管用：广播快照降频到每 150 帧（`SNAPSHOT_BROADCAST_EVERY`，约 2.5s），且接管时取
     「缓存快照 vs 新 host 自己的 World」中 seq 更新的一份（`newer_snapshot`），故低频不会多回滚。
   - 这比「分频道」简单得多——局域网本就不广播快照，已证明可行。
2. **host 固定节拍产帧 + 输入延迟缓冲**（消除 B，**已搜置：用户裁定走 RTS 式、不引入 D**，2026-09-13）
   - 用户先实测手感；若不可接受，优先级：渲染插值 → 降 tick → 固定 D → 预测/回滚。
   - **详细设计草案（备选方案库）见 `FRAME_SYNC_INPUT_DELAY_DESIGN.md`**。
3. **client 收敛追赶**（**已实施**，见第四节）
   - `accumulator` 夹到 `MAX_CATCHUP_STEPS=4` 步，避免一帧快进 10+ 步；
   - ~~输入改为每次 `update` 只发一条~~ **已回退**：实测会导致 host 缺输入、sim 掉到 ~20–30Hz
     （见第四节）；改为**每模拟 tick 一条**（与 host 每帧消耗 1 条一一对应）。
   - 进阶：加 ~1 帧渲染插值（上一帧与当前帧位置 lerp）——**未做**，是消除抖动观感最有效的手段。
4. **快照瘦身**：delta 快照 / 压缩。在完成第 1 项后优先级下降（周期大包已基本消失）。
   注意：重连靠 `frame_buf` 补齐，`SNAPSHOT_EVERY=30` 与容量 60 是配套的，不能只改频率。

## 四、已实施 / 现状

**已实施（2026-09-13）**，对应第 1/2/3 项（快照相关）：
1. Steam host 每 30 帧只 **本地** `set_snapshot`（重连用）；**广播**快照降为每 150 帧（`SNAPSHOT_BROADCAST_EVERY`）。
   局域网 host 本就只 `set_snapshot`（不广播）。
2. 同一帧的 hash 与 snapshot **复用一份 `world_to_bytes`**：新增 `world_ser::state_hash_bytes(&[u8])`，
   `state_hash(w)` 改为委托它（单测钉住两者一致）。
3. 主机迁移取基线改为 **取 seq 更新者**：新增 `net::lockstep::newer_snapshot(a, b)`，
   `HostLockstep::takeover` 不再无条件优先缓存快照；`steam_do_takeover` 也据此选基线。
   这样低频广播不会让接管无谓回滚（有单测 `takeover_prefers_newer_baseline_over_stale_cache`）。

**已实施（2026-09-13，分析第 3 项 / C）**：
- 新增纯函数 `accumulate_tick(acc, dt)`：累加器夹到 `MAX_CATCHUP_STEPS = 4` 步（单测）。
- client 输入**每模拟 tick 上行一条**（与 host 每帧消耗一一对应）；
  ~~曾改为“每次 update 只发一条”~~ → **已回退**。实测日志（双机）：client 渲染<60fps 时发送率低于
  host 产帧率，host `try_emit` 因缺输入而停摆，`[jit] host sim gap` 出现大量 **33/50/66/83ms** 间隔
  （sim 掉到 ~20–30Hz 且抖）。**结论：输入发送率必须 ≥ host 产帧率，即“每 sim tick 一条”。**
- （渲染插值仍未做。）

**未实施（需联机验证）**：B（host 固定节拍 + 输入延迟）、渲染插值。
下一步建议：安排两台 Steam 实测；实测时打开现有诊断日志
（`send_stats`、`steam-cli`/`steam-host` 的 `emit seq`、`frame -> seq`）确认帧到达间隔。
