//! Steam 联机逻辑（`feature = "steam"` 门控）。
//!
//! 这些方法原本散布在 `main.rs` 的 `impl Game` 里，现集中到本模块以便独立阅读与维护。
//! **关键**：所有 `steam_*` 字段仍属 `Game`，方法仍通过 `impl Game` 访问 `self`——
//! 因此这是**纯逻辑分组**，不改变任何行为与借用关系（局域网/单机路径不受影响）。
//!
//! **当前包含**（逻辑类已全部迁入）：
//! - 掉线/重连/迁移/接管：`poll_steam_reconnect` · `poll_steam_migration` · `steam_do_takeover` · `clear_transient_input`
//! - presence：`steam_transport` · `steam_current_room_info` · `steam_set/clear/refresh_presence`
//! - 社交/状态/工具：`steam_ping_of` · `steam_refresh_network_info` · `steam_draw_avatar` · `steam_ensure_leaderboard` · `steam_record_match_result`
//! - 好友/会话：`steam_refresh_friends` · `steam_ensure_session` · `steam_poll_join_requests`
//!
//! **剩余仍在 `main.rs`**（多为 UI/大厅流程/渲染长方法，暂不迁移）：`steam_lobby_update` · `steam_lobby_act` ·
//! `steam_lobby_create_update` · `steam_lobby_list_update` · `enter_steam_mode` · `steam_config_update` ·
//! `steam_friend_list_update` · `steam_room_edit_update` · `steam_refresh_roster` · `steam_leave_room` ·
//! `draw_steam_ready_overlay` · `draw_steam_friend_panel` · `draw_steam_room_edit` · `draw_steam_create_lobby` · `draw_steam_lobby_list`。
//!
//! 编译说明：默认构建（不启用 `steam` feature）时本模块所有方法都不编译，
//! `main.rs` 的调用点（如 `update` 里的 steam 分支）同样被 `#[cfg(feature = "steam")]` 门控，
//! 故默认路径完全不含 Steam 逻辑。启用 `--features client/steam` 才编译本模块。

use super::*;

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
            let _ = cli.send_reconnect_req(self.steam_my_id);
            let old_host = cli.host_peer();
            // 只接受「来自旧 host」的包作为 host 还活着的证据：
            // 否则新 host（接管后）广播的 Snapshot 会被误判成“旧 host 还活着” → 恢复却不重定向 → 永远连旧 host。
            if let Ok(Some((from, pkt))) = cli.recv_packet(rcv) {
                if from == old_host {
                    if let net::Packet::Snapshot { world_bytes, seq } = pkt {
                        cli.apply_resync(rcv).ok();
                        if let Some(w) = game_core::world_ser::world_from_bytes(&world_bytes) {
                            self.world = w;
                            self.clear_transient_input();
                            eprintln!("[steam-client] host alive, resumed from snapshot seq={seq}");
                        }
                        self.steam_migrating = false;
                        self.steam_migrate_ticks = 0;
                        return Ok(Some(cli));
                    }
                    // 来自旧 host 的其它包（如 Frame）也说明 host 还在：恢复，交给下一帧正常循环推进（丢帧由 lockstep 补发）。
                    self.steam_migrating = false;
                    self.steam_migrate_ticks = 0;
                    eprintln!("[steam-client] host alive (heartbeat from old host), resuming");
                    return Ok(Some(cli));
                }
                // 来自非旧 host（如新 host）的包：忽略，继续探测/等待。
            }
            if self.steam_migrate_ticks >= MIGRATE_PROBE_TICKS {
                // 判定 host 掉线 → 确定性选举新 host（排除当前 host、SteamID 最小者）。
                // 候选集用 `steam_online`（已排除历次掉线的 host），避免把已掉线的旧 host 再选出。
                let old_host_id = match old_host {
                    net::transport::Peer::Steam { id, .. } => id,
                    _ => 0,
                };
                let candidates: Vec<u64> = self.steam_online.iter().filter(|&&id| id != old_host_id).copied().collect();
                let new_host_id = candidates.iter().min().copied().unwrap_or(0);
                self.steam_new_host_id = new_host_id;
                eprintln!(
                    "[steam-client] host gone (probe timeout), elected new host={new_host_id} (I {}), online={:?}",
                    if new_host_id == self.steam_my_id { "am new host" } else { "am client" },
                    self.steam_online
                );
            }
            return Ok(Some(cli));
        }
        // —— 阶段 B：已决定新 host。我是新 host → 接管（消费 cli）；否则等 Takeover。
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
                if let Ok(Some((wb, _))) = cli.recv_snapshot(rcv) {
                    if let Some(w) = game_core::world_ser::world_from_bytes(&wb) {
                        self.world = w;
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

    /// Steam client 战斗端掉线后的重连入口（对齐局域网 `poll_reconnect`，但直接操作 `steam_cli_ls`）。
    /// 按 R 触发：发 `ReconnectReq`(带本机 SteamID) → host 应答 `Snapshot` → 重建 World → `apply_resync` 对齐续打。
    #[cfg(feature = "steam")]
    pub(crate) fn poll_steam_reconnect(&mut self, ctx: &Context, cli: &mut net::lockstep::ClientLockstep<net_steam::SteamTransport>) {
        use ggez::input::keyboard::Key;
        let r_pressed = ctx.keyboard.is_logical_key_just_pressed(&Key::Character("r".into()))
            || ctx.keyboard.is_logical_key_just_pressed(&Key::Character("R".into()));
        if !self.reconnect_attempting && !r_pressed {
            return; // 未按 R，不发起重连，保持空闲等待。
        }
        if !self.reconnect_attempting {
            self.reconnect_attempting = true;
            eprintln!("[steam-client] reconnect flow: sending ReconnectReq...");
        }
        // 发重连请求（带本机 SteamID 作稳定身份，host 按身份找回槽位）。
        if cli.send_reconnect_req(self.steam_my_id).is_err() {
            eprintln!("[steam-client] reconnect send failed");
            self.reconnect_attempting = false;
            return;
        }
        let mut rcv = vec![0u8; 8192];
        match cli.recv_snapshot(&mut rcv) {
            Ok(Some((world_bytes, seq))) => {
                eprintln!("[steam-client] got Snapshot seq={seq}, rebuilding World ({n} bytes)", n = world_bytes.len());
                cli.apply_resync(&mut rcv).ok();
                match game_core::world_ser::world_from_bytes(&world_bytes) {
                    Some(w) => {
                        self.world = w;
                        // 清空本地输入残留，避免把掉线期间的输入误带到接回后。
                        self.player_target = None;
                        self.pending_cast = None;
                        self.pending_skill = None;
                        self.queued_cmds.clear();
                        self.pending_shift_skill = None;
                        self.pending_clear_signal = false;
                        self.pending_stop_signal = false;
                        self.steam_cli_stale_ticks = 0;
                        self.conn_dropped = false;
                        self.reconnect_attempting = false;
                        eprintln!("[steam-client] reconnected: World rebuilt from snapshot, resuming lockstep");
                    }
                    None => {
                        eprintln!("[steam-client] failed to decode snapshot, retrying on next keypress");
                        self.reconnect_attempting = false;
                    }
                }
            }
            Ok(None) => {
                // 尚未收到快照：保持等待（下帧再试）。
            }
            Err(e) => {
                eprintln!("[steam-client] reconnect error: {e:?}");
                self.reconnect_attempting = false;
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
        let snap = cli.cached_snapshot();
        let old_host_id = match cli.host_peer() {
            net::transport::Peer::Steam { id, .. } => id,
            _ => 0,
        };
        // 本端 world index = 在原始参与列表中的位置（对局开始时确定，迁移不变）。
        let my_index = self.steam_participants.iter().position(|&id| id == self.steam_my_id).unwrap_or(0) as u8;
        let total = self.steam_participants.len().max(1);
        if let Some((wb, _)) = &snap {
            if let Some(w) = game_core::world_ser::world_from_bytes(wb) {
                self.world = w;
            }
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
        let mut host = net::lockstep::HostLockstep::takeover(
            cli,
            my_index,
            total,
            other_indices,
            peers,
            dropped,
            identities,
        );
        // 广播 Takeover（带更新后的在线参与集）+ Snapshot（接管基线）给其余在线端。
        let seq = host.next_seq();
        if let Some((wb, _)) = &snap {
            host.broadcast_takeover(seq, new_online.clone());
            host.broadcast_snapshot(wb.clone(), seq);
        }
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
        let t = match self.steam_host_ls.as_ref() {
            Some(ls) => ls.transport_ref(),
            None => return ("未命名房间".to_string(), String::new()),
        };
        let Some(lid) = self.steam_lobby_id else {
            return ("未命名房间".to_string(), String::new());
        };
        let mm = t.matchmaking();
        let lobby = net_steam::steamworks::LobbyId::from_raw(lid);
        let name = mm
            .lobby_data(lobby, net_steam::session::ROOM_NAME_KEY)
            .unwrap_or_else(|| "未命名房间".to_string());
        let note = mm.lobby_data(lobby, net_steam::session::ROOM_NOTE_KEY).unwrap_or_default();
        (name, note)
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
        let status = if self.steam_room_edit {
            "正在设置房间".to_string()
        } else if self.steam_in_lobby {
            let (name, _) = self.steam_current_room_info();
            let n = self.steam_roster.len();
            let limit = self.world.players.len().max(n);
            format!("房间「{name}」{n}/{limit} 等待中")
        } else if self.pre_game_config {
            "正在配置技能".to_string()
        } else {
            format!("对局中（第 {} 局）", self.meta.round)
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
            if let Some((rgba, side)) = net_steam::session::avatar_rgba(t, id, net_steam::session::AvatarSize::Small) {
                fetched.push((id, rgba, side));
            }
        }
        // t 到此不再使用 → 可以改 self 了。
        self.steam_pings = pings;
        for (id, rgba, side) in fetched {
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
        let s = size / 32.0; // 缓存的是 32x32 小头像
        canvas.draw(img, graphics::DrawParam::new().dest(Point2 { x, y }).scale([s, s]));
        true
    }

    /// 排行榜句柄：每会话只查找一次（Steam 的查找是异步回调，结果写回 `steam_lb_slot`）。
    /// 建房后待在房间时就会查好，整场结束要用时直接取。
    #[cfg(feature = "steam")]
    pub(crate) fn steam_ensure_leaderboard(&mut self) {
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
            format!("成就已上报：{}", names.join("、"))
        } else if report.had_failure {
            "战绩上报未生效（需在 Steamworks 后台配置统计/成就）".to_string()
        } else {
            String::new()
        };
        if !msg.is_empty() {
            self.steam_toast = (msg, now + 6.0);
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
