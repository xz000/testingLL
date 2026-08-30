//! Steam 联机逻辑（`feature = "steam"` 门控）。
//!
//! 这些方法原本散布在 `main.rs` 的 `impl Game` 里，现集中到本模块以便独立阅读与维护。
//! **关键**：所有 `steam_*` 字段仍属 `Game`，方法仍通过 `impl Game` 访问 `self`——
//! 因此这是**纯逻辑分组**，不改变任何行为与借用关系（局域网/单机路径不受影响）。
//!
//! **当前包含**：掉线重连（`poll_steam_reconnect`）+ 主机迁移/接管（`poll_steam_migration`/`steam_do_takeover`）+ 重建清残留（`clear_transient_input`）。
//! 其余 Steam 辅助方法（大厅/建房/社交/统计 UI）仍在 `main.rs`，后续可按此模式继续迁入本模块。
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
}
