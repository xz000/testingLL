# 主机固定节拍 + 输入延迟 —— 设计草案（2026-09-13）

> 状态：**草案，未实施**。对应 `FRAME_SYNC_ANALYSIS.md` 第 2 项（B）。
> 目标读者：下次动手前的自己。评审通过后再改代码。

---

## 0. 目标 / 非目标

**目标**
1. host **产帧恒定 60Hz**，不再被“最慢那端输入到达”牵着走（消除抖动源头 B）。
2. 用**固定输入延迟 D 帧**吸收网络抖动；某帧输入没到时用**默认输入占位**，**绝不整房停摆**。
3. 全端仍严格回放 host 的权威 `Frame`，**逐位确定性不变**。

**非目标**
- 不做客户端预测 / rollback（那是另一套大工程）。
- 不解决画面观感（帧到达抖动 → 画面跳）——那是**渲染插值**，与本项互补、独立。
- 不改 `TICK`（仍 `1/60`；见 `PORT_098B_DECISIONS.md` D1）。

---

## 1. 现状（代码位置）

- `TICK = 1.0/60`（`client/src/main.rs:47`）。
- host 主循环（Steam/LAN 两处）：`while accumulator >= TICK { set_local_input; poll; if try_emit() {...} else break }`
  （`main.rs:4975` / `:5231`）。
- `HostLockstep::try_emit`（`net/src/lockstep.rs:725`）：
  - 要求**每个参与端本帧输入都已到位**（`latest_input[c].is_some()`），否则返回 `None`；
  - 主循环 `else break` → **host 停摆**，直到那份输入到达；
  - 用完后把 `latest_input` 清空（掉线段改填 `default_input_bytes()`）。
- 输入是**单槽“最新覆盖”**（`poll` 里 `latest_input[c] = Some(bytes)`），**不带帧号**。
- 承载方式：
  - Steam client：`ClientLockstep::send_room_state(...)` → `Packet::RoomState { index, ready, build_done, input_bytes }`；
  - LAN client：`ClientLockstep::send_input(...)` → `Packet::Input { index, bytes }`；
  - 两者 host 端都在 `HostLockstep::poll` 里落到 `latest_input[c]`。
- `FrameData = Vec<(u8, Vec<u8>)>`（玩家 new-index + 裸输入字节）；`Packet::Frame { seq, entries }`。
- 掉线：`auto_drop_idle(HOST_DROP_TICKS=180)` 只处理**长期**空闲；对**短抖动**没有任何缓冲（这就是 B 的缺口）。

---

## 2. 核心思想

把“**产帧**”和“**收输入**”解耦：

1. **固定节拍**：host 每个 `TICK` 到点就产一帧，**不检查输入是否到齐**。
2. **输入帧号**：client 给每条输入打上“**这条是给第 N 帧的**”。
3. **固定延迟 D**：client 产出的是“**给第 `expect_seq + D` 帧**”的输入；host 在第 N 帧到点时取 `tag == N` 的那条。
   - D 帧的时间（`D*TICK`）用来覆盖“输入产生 → 到达 host”的网络时延（RTT/2 + 抖动余量）。
4. **缺输入 → 默认占位**：该端这一帧按 `PlayerInput::default()` 处理（操作丢一帧），**其余端与 host 继续 60Hz**。
5. **host 组帧即权威**：所有端回放同一个 `Frame`，所以“丢谁一帧/用默认”是 host 决定、全端一致，**确定性不变**。

**时间线示意**（D=3）：
```
host tick:   ... N-1        N          N+1   ...          ← 固定 60Hz
client at:        E=N-D     ...                            ← client 边打边发
client 发送 tag=E+D=N 的输入  ──RTT/2──►  到达 host（需 < D*TICK）
host 第 N 帧到点：取 tag==N 的输入（到了就用，没到用 default）→ 广播 Frame(N)
```

**D 的估算**：`D * TICK ≥ RTT/2 + jitter_margin`。
- 局域网 `RTT < 5ms` → `D = 1~2`；
- Steam relay `RTT 60~150ms` → `D = 4~8`（这是**固定**增加的操作延迟，需实测手感后定）。

---

## 3. 协议改动（两个方案）

### 方案 A（推荐）：给输入包加显式 `seq`
- `Packet::Input { index, seq, bytes }`、`Packet::RoomState { index, ready, build_done, seq, input_bytes }`
  （房间阶段 `seq = 0`）。
- client 发送处填 `seq = expect_seq() + D`；host `poll` 按 `seq` 归档。
- `PROTOCOL_VERSION` +1（已有大厅版本校验会拒绝旧端，`main.rs` 房间列表/加入处）。
- 优点：显式、可测、与现有“包结构即协议”的风格一致。
- 成本：`net/src/proto.rs` 两个包的编解码 + 若干测试。

### 方案 B（备选）：把 seq 作为 8 字节前缀塞进 `bytes`
- `bytes = seq.to_be_bytes() || encode_player_input(input)`；host `poll` 读前 8 字节当 tag，余下存为裸输入。
- 不改 `Packet`/`PROTOCOL_VERSION`；但：
  - 语义隐式（`bytes` 对 net 层变“半透膜”，房间阶段 presence 信号也带前缀）；
  - `default_input_bytes()` 仍须是**裸字节**，主/备两套字节格式易混淆。
- 不推荐，除非想避免协议版本 bump。

> 建议：**方案 A**。lockstep 本来就是同版本才联机，bump 成本低、清晰最重要。

---

## 4. `HostLockstep` 数据结构与伪代码

新增/改动：
```
// 每端一个“按帧号归档”的输入队列（去重；只留窗口内）。
input_queue: Vec<VecDeque<(u64 /*seq*/, Vec<u8>)>>,   // 或 BTreeMap<u64, Vec<u8>>
local_seq_input: Option<(u64, Vec<u8>)>,              // host 自身输入（若要统一走队列）
next_seq: u64,                                        // 不变，就是“下一个要产的帧号”
```
`poll`（收到 `Input`/`RoomState`）：
```
if let Some(seq) = pkt.seq {
    let q = &mut input_queue[c];
    if !q.iter().any(|(s, _)| *s == seq) {            // 去重：同 tag 只留先到
        q.push_back((seq, input_bytes));
    }
    // 驱逐：只保留 seq >= next_seq - D - 1（更老的没用了）
    while q.front().is_some_and(|(s, _)| *s + D as u64 + 1 < next_seq) { q.pop_front(); }
}
```
`try_emit`（改为固定节拍语义，可改名 `emit_tick`）：
```
// 不再有“必须收齐”的早退。
let mut entries = FrameData::new();
for c in 0..expected {
    if !is_active(c) { continue; }
    let orig = client_indices[c];
    let new = orig_to_new(orig);
    let bytes = if dropped[c] {
        default_input_bytes()
    } else if let Some(b) = take_tagged_input(c, next_seq) {
        b
    } else {
        default_input_bytes()      // 缺帧占位（确定性；不重复上一帧的离散施法）
    };
    entries.push((new, bytes));
}
// host 自身同理：取 local input for next_seq，缺则 default。
let seq = next_seq; next_seq += 1;
broadcast Frame{seq, entries}; frame_buf.push_back(...); // 供补发
```

关键点：
- **绝不 break**：`try_emit` 总返回 `Some`（除 `superseded`）。
- **默认占位而非复用上一条输入**：避免离散施法（cast）被重复触发。
- **去重**：client 在等帧时会重复发同一 tag；host 同 tag 只应用一次。
- `local_base>0` 时 host 自身输入同样按 `next_seq` 取；host 本地可直接用当前输入（它自己产生，无网络延迟），也可统一排队（更简单一致）。

---

## 5. 客户端改动

发送点（`main.rs` Steam client `:5123`、LAN client `:5300` 一带）：
- 现在：每次 update 至多一条，`send_room_state(..., &enc)` / `upload(&enc)`。
- 改为：`seq = cli.expect_seq() + INPUT_DELAY_FRAMES`，把 seq 编进包（方案 A）。
- 发送频率：维持“每 update 至多一条”。若某 update 没发（accumulator 未跨 tick），host 那一帧对应该端就是 default —— 可接受。
- 注意：client **等帧时 `expect_seq` 不前进**，会重复发同一 tag；host 去重即可。

常量：`const INPUT_DELAY_FRAMES: u64 = 4;`（LAN/Steam 可分开，或按 RTT 自适应——见开放问题）。

---

## 6. 确定性与边界

| 场景 | 处理 |
|---|---|
| 普通帧 | host 组帧权威；全端回放同一 Frame → 逐位一致 |
| 缺输入 | host 用 `default_input_bytes()`；全端一致 |
| 重复 tag | host 去重只应用一次 |
| 首局/首帧 | seq 从 0 起；房间期 `RoomState.seq=0` 不参与归档 |
| 开局配置 | 不进对局不产帧；`StartConfig`/`Go` 语义不变 |
| 掉线 | `auto_drop_idle` 仍生效；被 drop 后一直 default |
| 主机迁移 takeover | 新 host 继承 `next_seq`；各端输入队列可清空（重新填充前用 default） |
| 重连 | client 重建后 `expect_seq = 快照 seq`，继续发 `expect_seq + D` |
| 混版本 | `PROTOCOL_VERSION` bump → 旧端被大厅版本校验拒绝 |

**关键不变量**：产帧内容由 host 决定，client 只负责“尽早把自己的 tag=N 输入送到”。任何“没送到”都退化为 default，**不会分叉**。

---

## 7. 与现有机制交互

- `accumulate_tick` 的 clamp（已实施）：host 固定节拍后仍适用（限制单次追赶步数）。
- 快照/`StateHash`：seq 语义不变，无需改。
- `RECONNECT_RESP_INTERVAL`/`HOST_DROP_TICKS`/`CLIENT_STALE_TICKS`：按帧计数不变。
- `latest wins` 的“持续重发施法”机制：`local_player_input` 仍是电平量；只要该端每帧都能送 tag，就能持续到被 host 采纳；若某帧 default，则该帧不施法（下一帧继续重发）→ 不影响“最终能施法”。
- 现在 `try_emit` 的"清空 `latest_input`"逻辑被 `input_queue` + 驱逐取代。

---

## 8. 测试计划（`net/src/lockstep.rs` 的 `FakeTransport`）

1. **帧间隔稳定**：注入输入 0~3 帧抖动延迟，跑 N 帧，断言 host 每 tick 都产帧、`seq` 连续、无停摆。
2. **丢输入**：故意丢弃某些 tag → host 用 default 继续；两端世界逐位一致。
3. **不重复施法**：丢一帧后，某端 `cast` 不得在相邻两帧各触发一次。
4. **重复 tag**：同一 tag 发两次 → 只应用一次。
5. **确定性**：host + 2 client 跑 N 帧后 `state_hash` 一致。
6. **迁移/重连**：`takeover` 后从 `next_seq` 续打、慢端重新填充前用 default，仍逐位一致。
7. 回归：现有 38 项 net 测试全绿（含 `try_emit` 语义变化的用例需同步）。

真实联机验证（必须）：两台 Steam 实机，观察 `emit seq` 日志是否变为**恒定节奏**、体感延迟是否可接受，据此调 `INPUT_DELAY_FRAMES`。

---

## 9. 风险与回滚

| 风险 | 缓解 |
|---|---|
| 协议 bump → 只能同版本联机 | 大厅已有版本校验；发版同步 |
| D 帧操作延迟手感变差 | D 可调；先 LAN=1~2、Steam=4 起步，实测调 |
| `try_emit` 语义变化引入确定性 bug | 大量 `FakeTransport` 单测 + 双机 `state_hash` 比对 |
| 队列内存/CPU | 窗口内小队列（D 个），逐个驱逐，开销可控 |
| 迁移/重连边界 | 队列清空 + default 过渡；复用现有迁移测试 |

回滚：改动集中在 `net/src/lockstep.rs`（poll/try_emit/字段）+ `net/src/proto.rs`（seq 字段）+ client 两处发送 + 一个常量；回退单个提交即可，无数据结构迁移。

---

## 10. 开放问题（待定）

1. **D 固定 vs 自适应**：先用编译期常量（全体一致）；后续可按 host 测得的 RTT 自适应，但“全端对 D 的理解必须一致”，需 host 广播 D。
2. **同 tag 去重策略**：先到 vs 最新？建议**先到**（稳定、可预测）。
3. **host 自身输入**：本地直接取（零延迟）还是统一排队？建议统一排队，代码更简单，host 自己的手感几乎无差别。
4. **Steam 战斗期是否继续用 `RoomState` 承载输入**：可继续（可靠通道），只加 `seq` 字段；`ready/build_done` 战斗期无意义（置 false）。
5. 是否顺带把输入改为**每端独立队列**（已按此设计）以支持未来 rollback——暂不需要。
