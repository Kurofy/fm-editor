//! Worker thread loop — the heart of the bridge. Owns the `FmProcess` and
//! the `ArrayCache`, dispatches each `BridgeCmd` to the appropriate handler,
//! and implements the Attach/Ping state machine with count-stability
//! save-ready detection.
//!
//! Adding a new command: drop a handler module (e.g. `bridge/my_thing.rs`)
//! with a `pub(super) fn handle(...)` and add one match arm below. The arm
//! should be a single-line call into the new module's handler — keeping
//! this file boring is the whole point.

use std::sync::mpsc;

use crate::club;
use crate::player;
use crate::process::{self, FmProcess};
use crate::staff;

use super::bulk_load;
use super::cmd::{BridgeCmd, BridgeResult, EntityCounts};
use super::entity_load;

// ---------------------------------------------------------------------------
// Shared state
// ---------------------------------------------------------------------------

pub(super) struct ArrayCache {
    pub(super) humans_data: u64,
    pub(super) humans_count: usize,
    pub(super) staff_data: u64,
    pub(super) staff_count: usize,
    pub(super) clubs_data: u64,
    pub(super) clubs_count: usize,
}

pub(super) fn build_cache(fm: &FmProcess) -> ArrayCache {
    let (hd, _, hc) = player::humans_array(fm).unwrap_or((0, 0, 0));
    let (sd, _, sc) = staff::staff_array(fm).unwrap_or((0, 0, 0));
    let (cd, _, cc) = club::clubs_array(fm).unwrap_or((0, 0, 0));
    ArrayCache {
        humans_data: hd,
        humans_count: hc,
        staff_data: sd,
        staff_count: sc,
        clubs_data: cd,
        clubs_count: cc,
    }
}

/// Cheap plausibility check on an ArrayCache. Non-null userspace pointers
/// and counts in realistic FM26 ranges. Does NOT guarantee save-ready —
/// must be paired with count-stability tracking to reject save-creation
/// mid-populate.
fn counts_are_plausible(ac: &ArrayCache) -> bool {
    if ac.humans_data < 0x10000 || ac.humans_data > 0x7FFFFFFFFFFF {
        return false;
    }
    if ac.staff_data < 0x10000 || ac.staff_data > 0x7FFFFFFFFFFF {
        return false;
    }
    if ac.clubs_data < 0x10000 || ac.clubs_data > 0x7FFFFFFFFFFF {
        return false;
    }
    (1000..=2_000_000).contains(&ac.humans_count)
        && (1000..=2_000_000).contains(&ac.staff_count)
        && (100..=200_000).contains(&ac.clubs_count)
}

fn counts_tuple(ac: &ArrayCache) -> (usize, usize, usize) {
    (ac.humans_count, ac.staff_count, ac.clubs_count)
}


// ---------------------------------------------------------------------------
// Worker loop
// ---------------------------------------------------------------------------

pub(super) fn worker_loop(rx: mpsc::Receiver<BridgeCmd>, tx: mpsc::Sender<BridgeResult>) {
    let mut fm: Option<FmProcess> = None;
    let mut cache: Option<ArrayCache> = None;
    // Count-stability tracking: the (humans, staff, clubs) counts we saw
    // on the previous probe. Only promote to Connected when two
    // consecutive probes report the same plausible counts — rejects the
    // save-creation window where counts drift as FM populates the world.
    let mut last_counts: Option<(usize, usize, usize)> = None;

    while let Ok(cmd) = rx.recv() {
        match cmd {
            BridgeCmd::Attach(pid) => {
                let attached = match pid {
                    Some(pid) => FmProcess::attach_pid(pid),
                    None => match process::running_games() {
                        Ok(games) if games.len() > 1 => {
                            let _ = tx.send(BridgeResult::ChooseGame(games));
                            continue;
                        }
                        Ok(games) => match games.first() {
                            Some(game) => FmProcess::attach_pid(game.pid),
                            None => Err("fm.exe is not running".to_string()),
                        },
                        Err(e) => Err(e),
                    },
                };
                match attached {
                    Ok(process) => {
                        let pid = process.pid;
                        let build = process.build;
                        let notice = process.dll_hash_notice.clone();
                        let ac = build_cache(&process);
                        // Never promote on the first observation — even if
                        // counts look plausible right now, we haven't seen
                        // them stable yet. Record them and let the next
                        // Ping (2s later) decide.
                        last_counts = if counts_are_plausible(&ac) {
                            Some(counts_tuple(&ac))
                        } else {
                            None
                        };
                        cache = None;
                        fm = Some(process);
                        let _ = tx.send(BridgeResult::Attached(pid, build));
                        if let Some(msg) = notice {
                            let _ = tx.send(BridgeResult::Error(msg));
                        }
                    }
                    Err(e) => {
                        let _ = tx.send(BridgeResult::Error(e));
                    }
                }
            }

            BridgeCmd::LoadEntityCache => {
                if let (Some(f), Some(c)) = (&fm, &cache) {
                    match bulk_load::load_entity_cache(f, c, &tx) {
                        Ok(arc) => {
                            let _ = tx.send(BridgeResult::CacheReady(arc));
                        }
                        Err(e) => {
                            let _ = tx.send(BridgeResult::CacheFailed(e));
                        }
                    }
                }
            }

            BridgeCmd::LoadPlayer(addr, index) => {
                if let Some(f) = &fm {
                    match player::read_player_at(f, addr, index) {
                        Ok(rec) => {
                            let _ = tx.send(BridgeResult::PlayerLoaded(rec));
                        }
                        Err(e) => {
                            let _ = tx.send(BridgeResult::Error(e));
                        }
                    }
                }
            }

            BridgeCmd::LoadStaff(addr, index) => {
                if let Some(f) = &fm {
                    match entity_load::load_staff_view(f, addr, index) {
                        Ok(v) => {
                            let _ = tx.send(BridgeResult::StaffLoaded(v));
                        }
                        Err(e) => {
                            let _ = tx.send(BridgeResult::Error(e));
                        }
                    }
                }
            }

            BridgeCmd::LoadClub(addr, index) => {
                if let Some(f) = &fm {
                    match entity_load::load_club_view(f, addr, index) {
                        Ok(v) => {
                            let _ = tx.send(BridgeResult::ClubLoaded(v));
                        }
                        Err(e) => {
                            let _ = tx.send(BridgeResult::Error(e));
                        }
                    }
                }
            }

            BridgeCmd::LoadHumanManager => {
                if let (Some(f), Some(_c)) = (&fm, &cache) {
                    match entity_load::load_human_manager(f) {
                        Ok(v) => {
                            let _ = tx.send(BridgeResult::HumanManagerLoaded(v));
                        }
                        Err(e) => {
                            let _ = tx.send(BridgeResult::Error(e));
                        }
                    }
                }
            }

            BridgeCmd::WriteBytes { id, addr, data } => {
                let result = match &fm {
                    Some(f) => f.write_bytes(addr, &data),
                    None => Err("Cannot write: no process attached".to_owned()),
                };
                let response = match result {
                    Ok(()) => BridgeResult::WriteOk { id },
                    Err(error) => BridgeResult::WriteFail { id, error },
                };
                let _ = tx.send(response);
            }

            BridgeCmd::Ping => {
                if let Some(f) = &fm {
                    // Cheap process-alive probe first
                    if f.read_u8(f.gp_base).is_err() {
                        let had_cache = cache.is_some();
                        fm = None;
                        cache = None;
                        last_counts = None;
                        if had_cache {
                            // Game exited while save was loaded — enter ReadOnly.
                            // The app preserves its Arc<EntityCache>; send
                            // GameClosed so it disables writes but keeps browsing.
                            let _ = tx.send(BridgeResult::GameClosed);
                        } else {
                            let _ = tx.send(BridgeResult::PingFail);
                        }
                    } else {
                        let pid = f.pid;
                        let current = build_cache(f);
                        let plausible = counts_are_plausible(&current);
                        let current_tuple = counts_tuple(&current);
                        let stable = plausible && last_counts == Some(current_tuple);
                        let was_ready = cache.is_some();

                        match (was_ready, stable) {
                            (false, true) => {
                                // Two consecutive stable probes → save ready.
                                // Auto-trigger human manager + entity cache load.
                                let counts = EntityCounts {
                                    players: current.humans_count,
                                    staff: current.staff_count,
                                    clubs: current.clubs_count,
                                };
                                cache = Some(current);
                                let _ = tx.send(BridgeResult::Connected(pid, counts));
                                // Human manager (fast, goes first)
                                if let (Some(f2), Some(_)) = (&fm, &cache) {
                                    match entity_load::load_human_manager(f2) {
                                        Ok(v) => {
                                            let _ = tx.send(BridgeResult::HumanManagerLoaded(v));
                                        }
                                        Err(e) => {
                                            let _ = tx.send(BridgeResult::Error(e));
                                        }
                                    }
                                }
                                // Entity cache (slow bulk load)
                                if let (Some(f2), Some(c2)) = (&fm, &cache) {
                                    match bulk_load::load_entity_cache(f2, c2, &tx) {
                                        Ok(arc) => {
                                            let _ = tx.send(BridgeResult::CacheReady(arc));
                                        }
                                        Err(e) => {
                                            let _ = tx.send(BridgeResult::CacheFailed(e));
                                        }
                                    }
                                }
                            }
                            (true, false) => {
                                // Was Ready; counts drifted or became
                                // implausible → save unloaded
                                cache = None;
                                let _ = tx.send(BridgeResult::SaveUnloaded(pid));
                            }
                            _ => {
                                let _ = tx.send(BridgeResult::PingOk);
                            }
                        }
                        last_counts = if plausible { Some(current_tuple) } else { None };
                    }
                } else {
                    let _ = tx.send(BridgeResult::PingFail);
                }
            }

            BridgeCmd::Shutdown => break,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn write_without_attached_process_is_acknowledged_as_failure() {
        let (cmd_tx, cmd_rx) = mpsc::channel();
        let (result_tx, result_rx) = mpsc::channel();
        cmd_tx
            .send(BridgeCmd::WriteBytes {
                id: 42,
                addr: 100,
                data: vec![1],
            })
            .unwrap();
        cmd_tx.send(BridgeCmd::Shutdown).unwrap();
        worker_loop(cmd_rx, result_tx);
        match result_rx.recv().unwrap() {
            BridgeResult::WriteFail { id, error } => {
                assert_eq!(id, 42);
                assert!(error.contains("no process attached"));
            }
            _ => panic!("expected a write failure acknowledgement"),
        }
        assert!(result_rx.try_recv().is_err());
    }
}
