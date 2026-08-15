# 帧同步网络层重写（NET_REWRITE）

> 创建 2026-08-15。原因：`net/` 当前 session.rs 概念混杂、丢帧无补偿（导致 3 人偶发分叉/卡死）。
> 目标：按「正确的 lockstep」把网络层拆成分层、确定性、可注入假 transport 测试。
> 配套文档：`LOCKSTEP_FOUNDATION.md`（基座计划）、`NEXT_STEPS.md`（交接）、`PLAN.md`（总计划+测试约定）。

---

## 1. 为什么重写（背景）

- 当前 `net/src/session.rs` 把 建连 / READY-GO / 每帧收发 / 合帧 / 去重 全塞在一个结构里，职责混杂。
- **丢帧无补偿**：client 收到带 seq 帧就推进，若漏收中间帧会跳 seq → 永久分叉（3 人偶发的深层原因）。
- 等齐 N 端输入是阻塞式的，脆弱且无输入缓冲。
- 测试直接操作 session 内部收发，和 client 的 update 循环耦合松，改一处易漏。

## 2. 目标架构（三层，都在 `trait Transport` 之上）

### A. `net::proto`（字节协议）—— 包格式 + 编解码
- 定死每类包的 [长度/tag/字段]，独立编解码、可单测。
- 把现有 `frame.rs`(frame_packet/parse_frame) 与上行包提升为统一协议层。
- 包类型：
  - JOIN / ACK（client↔host 建连）
  - READY / GO（统一起始）
  - INPUT（client→host 本机输入）
  - FRAME（host→client 整帧，含 seq + 全玩家输入）
  - REQ_FRAME（client→host 请求补发缺失帧，携带缺失的 seq）

### B. `net::lockstep`（核心帧同步状态机 —— 彻底重写）
- **HostLockstep**：
  - 阶段：`WaitingPlayers → Ready → Running`。
  - `frame_buf: VecDeque<Frame>`：保留最近 K 帧，供补发。
  - `Running` 每 tick：收集全部 N 端输入 → 打 seq 帧进缓冲并广播；
    **没收齐则不推帧**（保留「等齐」正确性底线）。
  - **补发**：收到 client `REQ_FRAME(seq)` → 若帧在缓冲则重发该帧。
- **ClientLockstep**：
  - 维护 `expect_seq`（期望的下一帧）+ 收到帧的缓冲。
  - `step_frame`：收帧 →
    - `seq == expect_seq`：推进它 + 推进缓冲里后续连续帧；
    - `seq > expect_seq`（漏帧）：向 host 发 `REQ_FRAME(expect_seq)` 补发，**暂停推进等补齐**（严格按序，杜绝跳帧分叉）；
    - 没收到帧 → 返回 `None`（不盲扣时间、不推进）。
  - **严格按序推进，这是不丢帧的根。**

### C. `net::session`（建连握手 + 生命周期，简化）
- 只负责 JOIN/分配序号/READY/GO 的状态机，产出「何时可开始推进」，不再管每帧收发。
- 可靠化：client 持续重发 READY 直到 GO；host 收到全 READY 就 GO（幂等重发）。

## 3. 关键正确性设计（这次务必做对）
1. **严格按序推进**：client 只推 `seq == expect_seq`；乱序/空洞进缓冲 + 请求补发。
2. **host 帧缓冲 + 补发**：client 缺口 → host 从 `frame_buf` 补发（保留足够 K 帧）。
3. **等齐门槛保留**：host 收齐 N 端才产生第 seq 帧（逐位一致前提）。
4. **传输无关 + 假 transport 测试**：所有 lockstep 逻辑依赖 `Transport`，单测注入「丢包/乱序/重复」假 transport，验证「两端在丢帧下仍逐位一致」。

## 4. 明确不做（范围边界）
- 不做本地预测 / 乐观回滚（4.7 延迟掩盖，体验优化，后续独立分支）。
- 不改 `game-core`（确定性核心已正确）。
- 不动美术 / 玩法。

## 5. 交付物 / 接入改动
- `net/` 重写为 `proto` + `lockstep` + `session` 三层。
- `client/src/netlink.rs`、`client/src/main.rs` 联网段改为调用 `ClientLockstep` / `HostLockstep`。
- 保留 READY/GO（作为 session 一部分）。
- 全套自动化测试 + 新增「丢帧/乱序下两端仍逐位一致」确定性测试。
- 更新 `LOCKSTEP_FOUNDATION.md` / `NEXT_STEPS.md`。

## 6. 实施顺序（分步 + 每步 cargo test 锁住）
1. [x] 6.1 建 `proto` 包格式 + 编解码单测（JOIN/ACK/READY/GO/INPUT/FRAME/REQ_FRAME）。已建 `net/src/proto.rs`，单测通过。
2. [x] 6.2 建 `lockstep`：HostLockstep（等齐 + 帧缓冲 + 补发）+ 单测。
3. [x] 6.3 建 `lockstep` ClientLockstep（expect_seq + 缓冲 + REQ_FRAME）+ 单测。
4. [x] 6.4 新增「假 transport 丢包/乱序/重复」确定性测试：`lockstep::tests::client_recovers_missing_frame_via_request`（丢 seq=1 → client 请求补发 → 追平）已通过。
   - 注：`proto.rs` 含 `FrameData` 类型别名；lockstep 依赖 proto 的静态 `FrameData`（非 `session` 的，session 将重构）。
5. [ ] 6.5 简化 `session`（建连/READY/GO 状态机）。
6. [ ] 6.6 改 `client/netlink.rs`、`client/main.rs` 接入新层；跑全量。
7. [ ] 6.7 真机多开 `multi-launch.ps1 -Players 3` 手测验证不再卡死/不同步。
8. [ ] 6.8 更新文档 + 提交。

## 7. 验证基线（一直要保持）
- `cargo test --workspace` 全绿（当前 81，重写后应新增若干）。
- `cargo clippy --workspace -- -D warnings` 无警告。
- 所有联网测试带防假绿断言（见 PLAN「测试约定」）。
