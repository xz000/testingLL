//! Steam 联机逻辑（`feature = "steam"` 门控）。
//!
//! 这些方法原本散布在 `main.rs` 的 `impl Game` 里，现集中到本模块以便独立阅读与维护。
//! **关键**：所有 `steam_*` 字段仍属 `Game`，方法仍通过 `impl Game` 访问 `self`——
//! 因此这是**纯逻辑分组**，不改变任何行为与借用关系（局域网/单机路径不受影响）。
//!
//! **当前包含**（逻辑类已全部迁入）：
//! - 掉线/重连/迁移/接管：`poll_steam_migration`（含「先探测重连、再选举迁移」）· `steam_do_takeover` · `clear_transient_input`
//! - presence：`steam_transport` · `steam_current_room_info` · `steam_set/clear/refresh_presence`
//! - 社交/状态/工具：`steam_ping_of` · `steam_refresh_network_info` · `steam_draw_avatar` · `steam_ensure_leaderboard` · `steam_record_match_result`
//! - 好友/会话：`steam_refresh_friends` · `steam_mark_played_with`（近期一起玩过）· `steam_ensure_session` · `steam_poll_join_requests`
//!
//! **剩余仍在 `main.rs`**（多为 UI/大厅流程/渲染长方法，暂不迁移）：`steam_lobby_update` · `steam_lobby_act` ·
//! `steam_lobby_create_update` · `steam_lobby_list_update` · `enter_steam_mode` · `steam_config_update` ·
//! `steam_friend_list_update` · `steam_refresh_roster` · `steam_leave_room` ·
//! `draw_steam_ready_overlay` · `draw_steam_friend_panel` · `draw_steam_lobby_list`。
//!
//! 编译说明：默认构建（不启用 `steam` feature）时本模块所有方法都不编译，
//! `main.rs` 的调用点（如 `update` 里的 steam 分支）同样被 `#[cfg(feature = "steam")]` 门控，
//! 故默认路径完全不含 Steam 逻辑。启用 `--features client/steam` 才编译本模块。

use super::*;

/// R3'：确定性选举新 host = Steam 仲裁的当前 `lobby_owner`（`owner`），全员从后端读到同一值。
/// 仅当 owner 合法（非 0 / 非旧 host / 在本局在线参与集）时返回它；**否则返回 0（不选举）**。
///
/// 为何**不做**“最小 SteamID 回退”（2026-10-08 真机教训）：若 owner 仍是旧 host（说明 Steam 尚未把
/// ownership 移交 / 旧 host 可能还活着），回退会让自己“抢”当 host → 随后又被 R4② 自栅栏打回，
/// 造成误接管与后续混乱。无法确定合法 owner 时，应由调用方继续探测，超时则回菜单。纯函数，便于单测。
#[cfg_attr(not(feature = "steam"), allow(dead_code))]
pub(crate) fn elect_new_host(owner: u64, old_host: u64, online: &[u64]) -> u64 {
    if owner != 0 && owner != old_host && online.contains(&owner) {
        owner
    } else {
        0
    }
}

/// R4②：host 是否应自栅栏退位。
/// host 正常情况下就是 Steam 的 lobby owner；一旦 `owner` 变成别人（且是本局参与成员），
/// 说明本端已被取代（可能因 P2P 隔离而没收到 `Takeover`）→ 应退位，避免“僵尸 host”。
/// `participants` 为空（原 host 尚未登记参与集）时不作为阻塞条件。纯函数，便于单测。
#[cfg_attr(not(feature = "steam"), allow(dead_code))]
pub(crate) fn should_self_fence(owner: u64, me: u64, participants: &[u64]) -> bool {
    owner != 0 && owner != me && (participants.is_empty() || participants.contains(&owner))
}

impl Game {
    /// Steam（client）主机迁移状态机：每帧在「收不到权威帧、疑似 host 掉线」后调用。
    /// 分两阶段：
    ///  A) 探测 host 是否还在：发 `ReconnectReq` 等 Snapshot 应答；收到则（host 还在）恢复对局；超时则判定 host 掉线。
    ///  B) 判定 host 掉线后：用 `steam_participants`（排除旧 host、SteamID 最小者）确定性选举同一新 host。
    ///     - 本端是新 host → `steam_do_takeover`（ClientLockstep 转 HostLockstep，广播 Takeover+Snapshot 接管）。
    ///     - 本端不是 → 等待新 host 的 `Takeover`，收到后重定向 + 用其快照重建 + `apply_resync` 对齐续打。
    #[cfg(feature = "steam")]
    pub(crate) fn poll_steam_migration(
        &mut self,
        mut cli: net::lockstep::ClientLockstep<net_steam::SteamTransport>,
        rcv: &mut [u8],
    ) -> GameResult<Option<net::lockstep::ClientLockstep<net_steam::SteamTransport>>> {
        self.steam_migrate_ticks = self.steam_migrate_ticks.saturating_add(1);
        // —— 阶段 A：探测 host 是否还在（尚未决定新 host）。
        if self.steam_new_host_id == 0 {
            // S4：节流重发 `ReconnectReq`（每 ~0.25s），避免每帧刷可靠通道（host 侧另有回包限速）。
            if self.steam_migrate_ticks % RECONNECT_REQ_EVERY == 0 {
                let _ = cli.send_reconnect_req(self.steam_my_id);
            }
            let old_host = cli.host_peer();
            // 探测恢复：**只有收到旧 host 的 Snapshot 才算恢复**（重建 world+meta）。
            //
            // 为何不把「任意包」当恢复信号（修复 2026-10-08 真机死循环）：若本端落后超过帧缓冲
            // （frame_buf_capacity=60≈1s），仅靠锁步补帧**无法补齐**，必须靠整快照重建；
            // 若此时收到一帧/StateHash 就提前 resume，会继续卡在缺口 → 又 stale → 死循环，
            // 表现为“显示重连、操作无反应，但输入仍上行到 host（host 用 held 输入让角色继续动）”。
            // 本端在阶段 A 每 ~0.25s 重发 ReconnectReq，host 会回 Snapshot（限速≤0.5s）；
            // 且 host 还有周期快照广播——循环读包直到读到 Snapshot 或本次无更多包。
            loop {
                match cli.recv_packet(rcv) {
                    Ok(Some((from, pkt))) => {
                        if from != old_host {
                            continue; // 非旧 host（如新 host）的包：忽略。
                        }
                        if let net::Packet::Snapshot { world_bytes, seq } = pkt {
                            cli.apply_resync(rcv).ok();
                            if let Some((w, m)) = game_core::world_ser::snapshot_from_bytes(&world_bytes) {
                                self.world = w;
                                self.meta = m; // R2：快照带 meta
                                self.clear_transient_input();
                                eprintln!("[steam-client] host alive, resumed from snapshot seq={seq}");
                            }
                            self.steam_migrating = false;
                            self.steam_migrate_ticks = 0;
                            return Ok(Some(cli));
                        }
                        // 旧 host 的其它包（Frame/StateHash）：它还在，但不足以补齐缺口，继续等 Snapshot。
                    }
                    Ok(None) => break,
                    Err(_) => break,
                }
            }
            if self.steam_migrate_ticks >= MIGRATE_PROBE_TICKS {
                // 判定 host 掉线 → 选举新 host。
                // R3'：优先用 Steam 仲裁的当前 lobby owner（全员一致）；不可用则回退最小 SteamID。
                // 候选集用 `steam_online`（已排除历次掉线的 host），避免把已掉线的旧 host 再选出。
                let old_host_id = match old_host {
                    net::transport::Peer::Steam { id, .. } => id,
                    _ => 0,
                };
                let owner = self
                    .steam_lobby_id
                    .map(|lid| net_steam::session::lobby_owner(cli.transport_ref(), lid))
                    .unwrap_or(0);
                let new_host_id = elect_new_host(owner, old_host_id, &self.steam_online);
                if new_host_id != 0 {
                    self.steam_new_host_id = new_host_id;
                    // S6：进入阶段 B 重新计时，使 MIGRATE_BAIL_TICKS 的“接管窗口”名副其实。
                    self.steam_migrate_ticks = 0;
                    eprintln!(
                        "[steam-client] host gone (probe timeout), elected new host={new_host_id} (lobby_owner={owner}, I {}), online={:?}",
                        if new_host_id == self.steam_my_id { "am new host" } else { "am client" },
                        self.steam_online
                    );
                } else if self.steam_migrate_ticks >= MIGRATE_NO_OWNER_BAIL_TICKS {
                    // owner 未移交/未知（无法确定合法新 host）→ 不再猜测（不做最小 ID 回退），回菜单。
                    eprintln!(
                        "[steam-client] cannot determine a valid new host (lobby_owner={owner}) after {MIGRATE_NO_OWNER_BAIL_TICKS} ticks; returning to menu"
                    );
                    self.reset_to_main_menu();
                    self.menu_hint = i18n::t("无法确定新主机，已返回主菜单").to_string();
                    self.accumulator = 0.0;
                    return Ok(None);
                }
            }
            return Ok(Some(cli));
        }
        // —— 阶段 B：已决定新 host。我是新 host → 接管（消费 cli）；否则等 Takeover。
        // S3：阶段 B 超时保护——已选出新 host 却迟迟收不到 Takeover（如新 host 也掉线）→ 放弃迁移，
        // 退回主菜单，避免永久冻结在「已冻结世界」状态。阈值远大于探测超时，给足重连窗口。
        const MIGRATE_BAIL_TICKS: u64 = 600;
        if self.steam_migrate_ticks >= MIGRATE_BAIL_TICKS {
            eprintln!(
                "[steam-client] migration Phase-B STALL: 收不到新 host 的 Takeover（{MIGRATE_BAIL_TICKS} 帧），放弃并退回主菜单"
            );
            self.reset_to_main_menu();
            // S3：失败不再静默——回菜单后在主菜单底部提示原因。
            self.menu_hint = i18n::t("连接未能恢复，已返回主菜单").to_string();
            self.accumulator = 0.0;
            return Ok(None); // 不归还 cli（reset_to_main_menu 已清 steam_cli_ls），避免呆cli被重新存回。
        }
        if self.steam_new_host_id == self.steam_my_id {
            self.steam_do_takeover(cli, rcv)?;
            Ok(None)
        } else {
            // 优先用 fighting 阶段缓存的 Takeover，否则从传输收（新 host 会持续广播直到首个 client 连上）。
            let takeover = cli.take_latest_takeover().or_else(|| cli.recv_takeover(rcv).ok().flatten());
            if let Some((from, seq, online)) = takeover {
                // 收到新 host 的 Takeover → 重定向 + 用其快照重建 + 对齐续打；并同步更新在线参与集。
                self.steam_online = online; // 排除掉线 host 后的在线参与集（供下一次迁移选举）
                cli.retarget_host(from);
                // S7：用 Takeover 携带的基线 seq 把本端期望帧对齐到新 host（接管基线），
                // 避免各端因帧序不一致在接管后卡在缺口处反复请求重传（包括无缓存快照的早期接管）。
                cli.set_start_seq(seq);
                if let Ok(Some((wb, _))) = cli.recv_snapshot(rcv) {
                    if let Some((w, m)) = game_core::world_ser::snapshot_from_bytes(&wb) {
                        self.world = w;
                        self.meta = m; // R2：快照带 meta
                        self.clear_transient_input();
                    }
                }
                cli.apply_resync(rcv).ok();
                self.steam_migrating = false;
                self.steam_migrate_ticks = 0;
                self.steam_new_host_id = 0;
                eprintln!("[steam-client] migrated to new host (seq={seq}), resuming lockstep");
                Ok(Some(cli))
            } else {
                Ok(Some(cli))
            }
        }
    }

    /// 迁移接管：本端被选为新 host。把原 client lockstep 转为 host lockstep，从缓存快照续打，
    /// 广播 `Takeover`+`Snapshot` 让其余端重定向并对齐。
    /// 用**原始** `steam_participants` 定位 world index（对局开始时确定、迁移不变）与掉线旧 host 的 index；
    /// 用 `steam_online`（排除掉线 host）作为选举/广播的新在线集，保证下一次迁移仍能正确选新 host。
    #[cfg(feature = "steam")]
    pub(crate) fn steam_do_takeover(&mut self, cli: net::lockstep::ClientLockstep<net_steam::SteamTransport>, _rcv: &mut [u8]) -> GameResult {
        // 取本端缓存的快照重建 world（迁移基线）。
        // S7：若原 host 在首个 `SNAPSHOT_EVERY` 周期前掉线（从未广播过快照），`cached_snapshot()` 为 None，
        // 则用本端已回放的最新 world（self.world）+ 当前期望帧 seq 作为接管基线，保证仍能广播 Takeover + 快照接管。
        // 本端自己的 World 基线（期望帧 seq）。
        let wb = game_core::world_ser::world_to_bytes(&self.world);
        let own = (game_core::world_ser::pack_snapshot(&wb, &self.meta), cli.expect_seq());
        let cached = cli.cached_snapshot();
        // 取「本端 world」与「缓存快照」中 seq 更新的一份作为接管基线：
        // 低频/无周期广播快照时，本端自己可能反而更新，用 max 避免无谓回滚到旧缓存。
        let effective_snap: (Vec<u8>, u64) = net::lockstep::newer_snapshot(cached.clone(), Some(own))
            .expect("own 基线必然存在");
        // S2：接管前先记下旧 host 的 peer，接管后单发 `Takeover` 通知它已被取缔（防脑裂/孤儿 host 续产帧）。
        let old_host_peer = cli.host_peer();
        let old_host_id = match old_host_peer {
            net::transport::Peer::Steam { id, .. } => id,
            _ => 0,
        };
        // 本端 world index = 在原始参与列表中的位置（对局开始时确定，迁移不变）。
        let my_index = self.steam_participants.iter().position(|&id| id == self.steam_my_id).unwrap_or(0) as u8;
        let total = self.steam_participants.len().max(1);
        if let Some((w, m)) = game_core::world_ser::snapshot_from_bytes(&effective_snap.0) {
            self.world = w;
            self.meta = m; // R2：快照带 meta
        }
        // 更新在线参与集：排除掉线的旧 host（供下一次迁移选举）。
        let new_online: Vec<u64> = self.steam_online.iter().filter(|&&id| id != old_host_id).copied().collect();
        self.steam_online = new_online.clone();
        // 其余参与端（按原始 world index）；不在新在线集里的玩家（历次掉线的 host）用默认输入占位。
        let mut other_indices = Vec::new();
        let mut peers = Vec::new();
        let mut dropped = Vec::new();
        let mut identities = Vec::new();
        for i in 0..total {
            let iu = i as u8;
            if iu != my_index {
                other_indices.push(iu);
                let gone = !new_online.contains(&self.steam_participants[i]); // 掉线占位
                peers.push(if gone {
                    None
                } else {
                    Some(net::transport::Peer::Steam { id: self.steam_participants[i], conn: None })
                });
                dropped.push(gone);
                identities.push(Some(self.steam_participants[i]));
            }
        }
        // 把选定的基线（已含 max 逻辑）交给 takeover；无缓存时它就是本端 world。
        let fallback = Some(effective_snap.clone());
        let mut host = net::lockstep::HostLockstep::takeover(
            cli,
            my_index,
            total,
            other_indices,
            peers,
            dropped,
            identities,
            fallback,
        );
        // 广播 Takeover（带更新后的在线参与集）+ Snapshot（接管基线）给其余在线端。
        // S7：即便没有缓存快照也必广播（基线取自本端 world），否则其余端收不到 Takeover → 零 host。
        let seq = host.next_seq();
        let (wb, _) = &effective_snap;
        // S1 诊断：广播的快照字节数（迁移基线）。若接近接收端缓冲上限（256KiB）需留意，避免被 transport 静默丢弃。
        eprintln!("[steam-host] TAKEOVER broadcast snapshot: {} bytes (seq={seq})", wb.len());
        host.broadcast_takeover(seq, new_online.clone());
        host.broadcast_snapshot(wb.clone(), seq);
        // S2：单发 Takeover 给旧 host，令其标记 superseded 并停止作为权威（防脑裂）。
        host.notify_old_host_takeover(old_host_peer, seq, new_online.clone());
        eprintln!("[steam-host] TAKEOVER notified old host ({old_host_id}) it is superseded");
        self.steam_my_index = my_index;
        eprintln!("[steam-host] TAKEOVER: I am new host (player {my_index}/{total}), resume seq={seq}, online={new_online:?}");
        self.steam_host_ls = Some(host);
        self.steam_cli_ls = None;
        self.steam_migrating = false;
        self.steam_migrate_ticks = 0;
        self.steam_new_host_id = 0;
        // 接管后持续广播 Takeover，直到首个在线 client 连上（产帧成功）才停，避免晚进入迁移的 client 错过。
        self.steam_host_broadcasting_takeover = true;
        Ok(())
    }

    /// 清空本机临时的输入/目标残留（重连/迁移重建世界后用，避免把掉线期间的输入误带到接回后）。
    #[cfg(feature = "steam")]
    fn clear_transient_input(&mut self) {
        self.player_target = None;
        self.pending_cast = None;
        self.pending_skill = None;
        self.queued_cmds.clear();
        self.pending_shift_skill = None;
        self.pending_clear_signal = false;
        self.pending_stop_signal = false;
    }

    /// 读取当前房间名与备注，host 从 matchmaking 读，无房间或非 host 时返回默认，返回二元组。
    #[cfg(feature = "steam")]
    pub(crate) fn steam_current_room_info(&self) -> (String, String) {
        // host 建房后由 lockstep 持有 transport（steam_host_ls）；客户端用进房前的 cli transport（steam_cli_ls）。
        // 两者取其一即可读取房间数据；都缺失（尚未进房）才回退默认。
        let t = match self.steam_host_ls.as_ref() {
            Some(ls) => ls.transport_ref(),
            None => match self.steam_cli_ls.as_ref() {
                Some(ls) => ls.transport_ref(),
                None => return (i18n::t("未命名房间").to_string(), String::new()),
            },
        };
        let Some(lid) = self.steam_lobby_id else {
            return (i18n::t("未命名房间").to_string(), String::new());
        };
        let mm = t.matchmaking();
        let lobby = net_steam::steamworks::LobbyId::from_raw(lid);
        let name = mm
            .lobby_data(lobby, net_steam::session::ROOM_NAME_KEY)
            .unwrap_or_else(|| i18n::t("未命名房间").to_string());
        let note = mm.lobby_data(lobby, net_steam::session::ROOM_NOTE_KEY).unwrap_or_default();
        (name, note)
    }

    /// 客户端：从大厅元数据回读房名/备注/人数上限到 `room_meta`，供只读设置编辑器显示房主设置。
    ///
    /// 房主自持权威 `room_meta`（`publish_room_cfg` 写入），此处**不覆盖房主**；
    /// 不在房间 / 没有 client 传输时不动。`MatchConfig` 部分由 `room_cfg` 串单独同步，不在此处。
    #[cfg(feature = "steam")]
    pub(crate) fn steam_sync_room_meta(&mut self) {
        if self.steam_host_ls.is_some() {
            return;
        }
        let info = (|| {
            let t = self.steam_cli_ls.as_ref()?.transport_ref();
            let lid = self.steam_lobby_id?;
            let mm = t.matchmaking();
            let lobby = net_steam::steamworks::LobbyId::from_raw(lid);
            Some((
                mm.lobby_data(lobby, net_steam::session::ROOM_NAME_KEY),
                mm.lobby_data(lobby, net_steam::session::ROOM_NOTE_KEY),
                mm.lobby_member_limit(lobby),
            ))
        })();
        let Some((name, note, limit)) = info else { return };
        if let Some(name) = name {
            self.room_meta.name = name;
        }
        if let Some(note) = note {
            self.room_meta.note = note;
        }
        if let Some(limit) = limit {
            self.room_meta.player_limit = limit.clamp(2, STEAM_MAX_PLAYERS as usize) as u32;
        }
    }

    /// 当前可用的 Steam 传输：进房后归 lockstep 持有（`into_transport`），进房前在 `steam_sess` 里。
    /// 好友邀请 / Rich Presence 只需 `&SteamTransport`（它持有唯一的 `steamworks::Client`）。
    #[cfg(feature = "steam")]
    pub(crate) fn steam_transport(&self) -> Option<&net_steam::SteamTransport> {
        if let Some(ls) = self.steam_host_ls.as_ref() {
            return Some(ls.transport_ref());
        }
        if let Some(ls) = self.steam_cli_ls.as_ref() {
            return Some(ls.transport_ref());
        }
        self.steam_sess.as_ref().map(|s| &s.transport)
    }

    /// 在覆盖层打开本作创意工坊页（订阅音频包）。
    #[cfg(feature = "steam")]
    pub(crate) fn steam_open_workshop(&self) {
        let url = crate::appid::workshop_url();
        match self.steam_transport() {
            Some(t) => t.open_url(&url),
            None => eprintln!("[workshop] Steam 未初始化，无法打开创意工坊"),
        }
    }

    /// 已订阅/已就绪物品数（`None` = Steam 不可用）。
    #[cfg(feature = "steam")]
    pub(crate) fn steam_subscribed_counts(&self) -> Option<(usize, usize)> {
        self.steam_transport().map(|t| t.subscribed_item_counts())
    }

    /// 写 Rich Presence（内容变化立即写；不变则按 `STEAM_PRESENCE_INTERVAL_SECS` 节流，Steam 对频繁 set 有限速）。
    #[cfg(feature = "steam")]
    pub(crate) fn steam_set_presence(&mut self, now: f64, status: &str, connect: Option<&str>) {
        let key = format!("{status}|{}", connect.unwrap_or(""));
        let changed = key != self.steam_presence_text;
        if !changed && now - self.steam_presence_last < STEAM_PRESENCE_INTERVAL_SECS {
            return;
        }
        let Some(t) = self.steam_transport() else { return };
        net_steam::session::set_presence(t, status, connect);
        self.steam_presence_text = key;
        self.steam_presence_last = now;
        if changed {
            eprintln!("[steam-presence] status='{status}' connect={connect:?}");
        }
    }

    /// 清空 Rich Presence（回主菜单/退出房间：好友不再看到「加入游戏」）。
    #[cfg(feature = "steam")]
    pub(crate) fn steam_clear_presence(&mut self) {
        if self.steam_transport().is_none() {
            return;
        }
        if self.steam_presence_text.is_empty() {
            return;
        }
        if let Some(t) = self.steam_transport() {
            net_steam::session::clear_presence(t);
        }
        self.steam_presence_text = String::new();
        self.steam_presence_last = -999.0;
        eprintln!("[steam-presence] cleared");
    }

    /// 按当前所处阶段刷新 Rich Presence（每帧调用，内部节流）：主菜单/无房间 → 清空；
    /// 房间 → 房名 + 人数 + 等待中；配置阶段 → 配置中；对局中 → 对局中（第 N 局）。
    /// 处于房间里时带 `connect` 串 → 好友在 Steam 好友列表里看到「加入游戏」，点了能直接进同一房间。
    #[cfg(feature = "steam")]
    pub(crate) fn steam_refresh_presence(&mut self, now: f64) {
        // 没有 Steam 传输（未初始化 / 非 Steam 模式）→ 无事可做。
        if self.steam_transport().is_none() {
            return;
        }
        let in_room = self.steam_lobby_id.is_some();
        if !in_room {
            self.steam_clear_presence();
            return;
        }
        let connect = net_steam::lobby::format_connect_string(self.steam_lobby_id.unwrap_or(0));
        let status = if self.steam_in_lobby {
            let (name, _) = self.steam_current_room_info();
            let n = self.steam_roster.len();
            let limit = self.world.players.len().max(n);
            i18n::tf(
                "房间「{name}」{n}/{limit} 等待中",
                &[("name", name), ("n", n.to_string()), ("limit", limit.to_string())],
            )
        } else if self.pre_game_config {
            i18n::t("正在配置技能").to_string()
        } else {
            i18n::tf("对局中（第 {round} 局）", &[("round", self.meta.round.to_string())])
        };
        self.steam_set_presence(now, &status, Some(&connect));
    }

    /// 某位成员的 ping（毫秒）；没测到返回 `None`（界面显示“--”，不要显示 0 误导）。
    #[cfg(feature = "steam")]
    pub(crate) fn steam_ping_of(&self, id: u64) -> Option<i32> {
        self.steam_pings.iter().find(|(k, _)| *k == id).map(|(_, ms)| *ms)
    }

    /// 节流刷新网络信息：各成员 ping + 补拉缺失头像（每 30 帧一次）。
    /// 头像只补没缓存过的（Steam 首次进房常拉不到，下一轮自动重试）。
    #[cfg(feature = "steam")]
    pub(crate) fn steam_refresh_network_info(&mut self, ctx: &Context) {
        self.steam_net_ticks = self.steam_net_ticks.wrapping_add(1);
        if self.steam_net_ticks % 30 != 1 {
            return;
        }
        // 顺手把同房/同局玩家标记为「近期一起玩过」（填充 Steam 自带的 Recently Played With 列表）。
        self.steam_mark_played_with();
        // R0 诊断（`--netdiag`）：大厅/连接状态日志（默认关）。
        self.steam_log_net_diag();
        // 先把要查的 SteamID 抄出来（避免 `steam_transport()` 的借用挡住后面的 &mut self）。
        // 含房间成员 +（邀请面板展开时）好友列表里的人，好让两边都能显示头像。
        let member_ids: Vec<u64> = self.steam_roster.iter().map(|(_, _, id)| *id).collect();
        let mut ids = member_ids.clone();
        if self.steam_friend_list {
            for f in self.steam_friends.iter() {
                if !ids.contains(&f.id) {
                    ids.push(f.id);
                }
            }
        }
        let Some(t) = self.steam_transport() else { return };
        let my_id = t.steam_id();
        // 触发头像/昵称下载：steamworks 不会自动拉**非好友**的 lobby 成员头像，
        // 不显式 request 的话 medium_avatar() 可能一直返回 None，导致房友（随机匹配来的）头像永远空白。
        // 好友已在 list_friends 里 request 过，这里再 request 是幂等 no-op；非阻塞（false）避免卡帧。
        for id in ids.iter().copied() {
            t.friends()
                .request_user_information(net_steam::steamworks::SteamId::from_raw(id), false);
        }
        // ping：只查房间成员里的别人（自己到自己是 0，没意义；好友没建会话也测不出来）。
        let mut pings = Vec::new();
        for id in member_ids.iter().copied().filter(|id| *id != my_id) {
            if let Some(ms) = net_steam::session::ping_to(t, id) {
                pings.push((id, ms));
            }
        }
        // 头像：只补缺失的。先把字节取出来（此时仍在借用 t），等 t 用完了再写回 self。
        let mut fetched: Vec<(u64, Vec<u8>, u32)> = Vec::new();
        for id in ids {
            if self.steam_avatars.iter().any(|(k, _)| *k == id) {
                continue;
            }
            // Medium(64) 而非 Small(32)：裁成内切圆后要缩放到角色身上显示，
            // 64px 源图的圆边明显比 32px 平滑（羽化后不糊）。
            if let Some((rgba, side)) = net_steam::session::avatar_rgba(t, id, net_steam::session::AvatarSize::Medium) {
                fetched.push((id, rgba, side));
            }
        }
        // t 到此不再使用 → 可以改 self 了。
        self.steam_pings = pings;
        for (id, mut rgba, side) in fetched {
            // 裁成内切圆：角色/化身上的头像是叠在圆形角色体里的，方形图会在四角露方角。
            net_steam::session::circular_crop_rgba(&mut rgba, side);
            let img = graphics::Image::from_pixels(
                &ctx.gfx,
                &rgba,
                graphics::ImageFormat::Rgba8UnormSrgb,
                side,
                side,
            );
            self.steam_avatars.push((id, img));
        }
    }

    /// 画某位成员的头像（有缓存才画）；返回是否画了，调用方据此调整文字缩进。
    #[cfg(feature = "steam")]
    pub(crate) fn steam_draw_avatar(&self, canvas: &mut Canvas, id: u64, x: f32, y: f32, size: f32) -> bool {
        let Some((_, img)) = self.steam_avatars.iter().find(|(k, _)| *k == id) else {
            return false;
        };
        let s = size / 64.0; // 缓存的是 64x64 头像（已裁成内切圆）
        canvas.draw(img, graphics::DrawParam::new().dest(Point2 { x, y }).scale([s, s]));
        true
    }

    /// 排行榜句柄：每会话只查找一次（Steam 的查找是异步回调，结果写回 `steam_lb_slot`）。
    /// 建房后待在房间时就会查好，整场结束要用时直接取。
    #[cfg(feature = "steam")]
    pub(crate) fn steam_ensure_leaderboard(&mut self) {
        if !crate::appid::stats_enabled() {
            return; // demo：不提供排行榜（也不发无谓的异步请求）
        }
        if self.steam_lb_requested {
            return;
        }
        let Some(t) = self.steam_transport() else { return };
        net_steam::session::request_leaderboard(t, net_steam::stats::LEADERBOARD, &self.steam_lb_slot);
        self.steam_lb_requested = true;
    }

    /// 整场结束（进入 Finished）时把战绩上报 Steam：统计 + 成就 + 排行榜，只上报一次。
    /// 统计/成就/排行榜都要在 Steamworks 后台先定义 key，没配置时只会打日志、不影响游戏。
    #[cfg(feature = "steam")]
    pub(crate) fn steam_record_match_result(&mut self, now: f64) {
        if !crate::appid::stats_enabled() {
            // demo：不上报统计/成就/天梯。**在“生产端”早退**（而不是改绘制）：
            // `steam_stats_snapshot` 保持 `None` → 结算画面的“统计 + 天梯 TOP5”整块自然不画，
            // 也不会弹成就提示、不会出现“暂无数据（需在 Steamworks 后台创建）”这类脏文案。
            return;
        }
        if self.steam_stats_recorded {
            return;
        }
        self.steam_stats_recorded = true;
        let Some(t) = self.steam_transport() else { return };
        // 本场战绩摘要：从我方档案取（击杀/最佳名次/存活局数），人数与局数从 world/meta 取。
        let me = self.self_index();
        let (kills, best_placement, rounds_survived) = self
            .meta
            .profiles
            .iter()
            .find(|p| p.player_id == me)
            .map(|p| (p.total_kills, p.best_placement, p.rounds_survived))
            .unwrap_or((0, 0, 0));
        let summary = net_steam::stats::MatchSummary {
            kills,
            best_placement,
            players: self.world.players.len().max(1) as u32,
            rounds: self.meta.round.max(1),
            rounds_survived,
        };
        let report = net_steam::session::record_match_result(t, summary);
        // 排行榜：句柄查到了就上传分数；没查到（后台没建榜/还没回调）就跳过。
        let lb = self.steam_lb_slot.lock().unwrap().clone();
        if let Some(lb) = lb.as_ref() {
            net_steam::session::upload_leaderboard_score(t, lb, report.score);
        }
        // 结算界面要展示：读回统计 + 拉一次榜单前 5（都是异步/只读，失败不影响）。
        let snap = net_steam::session::stats_snapshot(t);
        if let Some(lb) = lb.as_ref() {
            net_steam::session::request_leaderboard_top(t, lb, 5, &self.steam_lb_rows);
        }
        // t 到此不再使用 → 写回 self。
        self.steam_stats_snapshot = Some(snap);
        let msg = if !report.achievements.is_empty() {
            let names: Vec<&str> = report
                .achievements
                .iter()
                .map(|k| net_steam::stats::achievement_label(k))
                .collect();
            i18n::tf("成就已上报：{names}", &[("names", names.join(i18n::t("、")))])
        } else if report.had_failure {
            i18n::t("战绩上报未生效（需在 Steamworks 后台配置统计/成就）").to_string()
        } else {
            String::new()
        };
        if !msg.is_empty() {
            self.steam_toast = (msg, now + 6.0);
        }
    }

    /// 读 **Steam 语言设置**并应用（进入主菜单/大厅时调用；无 Steam 会话时不清空已有值）。
    ///
    /// 语义：`LangPref::Auto` 跟随 Steam；`LangPref::Fixed` 无视 Steam（手动覆盖）。
    /// 也负责把最新 Steam 值缓存到 `self.steam_lang`，供设置界面里切换“自动/固定”时重新解析。
    #[cfg(feature = "steam")]
    pub(crate) fn steam_sync_language(&mut self) {
        // 取出 Steam 游戏语言码（自有 String，借此结束对 `self` 的不可变借用）。
        let code = self.steam_transport().and_then(|t| t.current_game_language());
        let steam = code.as_deref().and_then(i18n::Lang::from_steam_code);
        if steam == self.steam_lang {
            return;
        }
        if let Some(code) = code.as_deref() {
            eprintln!("[i18n] Steam game language: {code} -> {steam:?}");
        }
        self.steam_lang = steam;
        let resolved = self.lang_pref().resolve(steam);
        i18n::set_lang(resolved);
    }

    /// 把房间成员 + 对局参与者标记为 Steam「近期一起玩过」（填充 Steam 客户端自带列表）。
    /// Steam 要求当前用户与对方在同一游戏里才会建立关联，故在房间/对局中调用；
    /// 仅当目标集合**变化**时才真正调 Steam（避免每 0.5s 重复标记同一批人）。
    /// 建议放在节流的 `steam_refresh_network_info` 里调用。
    #[cfg(feature = "steam")]
    pub(crate) fn steam_mark_played_with(&mut self) {
        // 汇总房间成员 + 对局参与者（两处各自可能包含对方没有的 id）。
        let mut ids: Vec<u64> = self.steam_roster.iter().map(|(_, _, id)| *id).collect();
        ids.extend(self.steam_participants.iter().copied());
        let Some(t) = self.steam_transport() else { return };
        let targets = net_steam::session::played_with_targets(t.steam_id(), &ids);
        if targets.is_empty() || targets == self.steam_played_with {
            return; // 没人可标 / 集合没变 → 不重复调 Steam。
        }
        let n = net_steam::session::mark_played_with(t, &ids);
        eprintln!("[steam-coplay] marked {n} player(s) as recently-played-with: {targets:?}");
        self.steam_played_with = targets;
    }

    /// 诊断（R0，`--netdiag`）：打印大厅 owner/成员 + 各 peer 连接状态，并广播大厅聊天探针。
    /// 用途：真机验证 V1（房主离开后大厅是否存活 / 大厅聊天是否可用）与 V2（断链时的连接状态取值）。
    /// 默认关闭，不影响正常游玩。
    #[cfg(feature = "steam")]
    pub(crate) fn steam_log_net_diag(&self) {
        if !self.net_diag {
            return;
        }
        let Some(t) = self.steam_transport() else { return };
        let me = t.steam_id();
        // 每 30 帧（~0.5s）：各 peer 的 Steam 连接状态（V2，细粒度观察断链时的状态迁移）。
        let ids: Vec<u64> = self.steam_roster.iter().map(|(_, _, id)| *id).collect();
        for id in ids {
            if id == me {
                continue;
            }
            eprintln!("[netdiag] conn[{id}]={:?}", net_steam::session::peer_connection_state(t, id));
        }
        // 每 150 帧（~2.5s）：大厅 owner/成员 + 发一条大厅聊天探针（V1，看房主离开后大厅是否还在）。
        if self.steam_net_ticks % 150 == 1 {
            if let Some(lid) = self.steam_lobby_id {
                let lobby = net_steam::steamworks::LobbyId::from_raw(lid);
                let mm = t.matchmaking();
                let owner = mm.lobby_owner(lobby).raw();
                let members: Vec<u64> = mm.lobby_members(lobby).iter().map(|s| s.raw()).collect();
                eprintln!("[netdiag] lobby={lid} owner={owner} members={members:?} me={me}");
                let msg = format!("[CB1]diag from={me}");
                let ok = net_steam::session::send_lobby_chat(t, lid, msg.as_bytes());
                eprintln!("[netdiag] lobby-chat send ok={ok} bytes={}", msg.len());
            }
        }
    }

    /// 刷新好友列表（展开邀请面板时调一次；R 手动刷新）。
    #[cfg(feature = "steam")]
    pub(crate) fn steam_refresh_friends(&mut self) {
        let lobby = self.steam_lobby_id;
        let Some(t) = self.steam_transport() else { return };
        let friends = net_steam::session::list_friends(t, lobby);
        self.steam_friends = friends;
        if self.steam_friend_selection >= self.steam_friends.len() {
            self.steam_friend_selection = self.steam_friends.len().saturating_sub(1);
        }
    }

    /// 主菜单：best-effort 初始化一次 Steam 会话，好让好友从 Steam 好友列表点「加入游戏」时
    /// 我们这边的 `GameLobbyJoinRequested` 回调能收到（回调只在 `run_callbacks` 时泵出，必须有 Client）。
    /// 失败（Steam 未运行/未登录）不影响单机与局域网，只是收不到邀请。
    #[cfg(feature = "steam")]
    pub(crate) fn steam_ensure_session(&mut self) {
        if self.steam_sess.is_some() || self.steam_session_tried {
            return;
        }
        self.steam_session_tried = true;
        match net_steam::session::SteamSession::init(APP_ID, STEAM_VIRTUAL_PORT) {
            Ok(s) => {
                self.steam_my_display_name = s
                    .transport
                    .friends()
                    .get_friend(net_steam::steamworks::SteamId::from_raw(s.transport.steam_id()))
                    .name();
                eprintln!("[steam] session ready, display name='{}'", self.steam_my_display_name);
                self.steam_sess = Some(s);
            }
            Err(e) => eprintln!("[steam] session init failed (邀请将不可用): {e:?}"),
        }
    }

    /// 处理好友从 Steam 发起的「加入游戏」请求（主菜单/大厅界面每帧调用）：
    /// 需要已初始化会话（pump 回调才拿得到）、且当前不在房间里；命中则按 lobby id 直接进房。
    #[cfg(feature = "steam")]
    pub(crate) fn steam_poll_join_requests(&mut self, ctx: &mut Context) {
        if let Some(s) = self.steam_sess.as_ref() {
            s.run_callbacks();
        }
        let Some(req) = self.steam_sess.as_ref().and_then(|s| s.take_join_request()) else {
            return;
        };
        if self.steam_in_lobby || self.steam_host_ls.is_some() || self.steam_cli_ls.is_some() {
            eprintln!("[steam-invite] ignoring join request: already in a room");
            return;
        }
        eprintln!("[steam-invite] friend {} invited us to lobby {}", req.from, req.lobby);
        self.steam_join_lobby_id = Some(req.lobby);
        self.steam_lobby_menu = false;
        self.steam_lobby_create = false;
        self.steam_lobby_list = false;
        self.steam_friend_hint = "已从邀请加入房间".to_string();
        self.enter_steam_mode(ctx, false, 2, None, None);
    }
}

#[cfg(test)]
mod tests {
    use super::elect_new_host;

    /// R3'：owner 合法（是参与成员且非旧 host）时优先选 owner——即使它不是最小 SteamID。
    #[test]
    fn elect_prefers_lobby_owner_when_valid() {
        assert_eq!(elect_new_host(500, 100, &[100, 500, 300]), 500);
    }

    /// R4②：自栅栏判定。
    #[test]
    fn self_fence_only_when_owner_is_someone_else() {
        assert!(!super::should_self_fence(7, 7, &[7, 9]), "自己是 owner，不退位");
        assert!(!super::should_self_fence(0, 7, &[7, 9]), "无 owner，不退位");
        assert!(super::should_self_fence(9, 7, &[7, 9]), "owner 是别的参与成员 → 退位");
        assert!(super::should_self_fence(9, 7, &[]), "参与集为空（原 host）也退位");
        assert!(!super::should_self_fence(9, 7, &[7]), "owner 不在参与集 → 不退位（保守）");
    }

    /// owner 不可用（未移交 / 无效 / 不在参与集）时**不选举**（返回 0，不再做最小 ID 回退）。
    #[test]
    fn elect_returns_zero_when_owner_invalid() {
        // owner 仍是旧 host（Steam 移交未反映）→ 不选举。
        assert_eq!(elect_new_host(100, 100, &[100, 500, 300]), 0);
        // 无大厅 / owner 无效（0）→ 不选举。
        assert_eq!(elect_new_host(0, 100, &[100, 500, 300]), 0);
        // owner 不在本局在线参与集（如只在大厅但未参与）→ 不选举。
        assert_eq!(elect_new_host(999, 100, &[100, 500, 300]), 0);
    }
}
