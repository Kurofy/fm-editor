//! Bulk entity cache loader — reads every player, staff member, and club
//! from game memory in one sequential pass and builds an `EntityCache`.
//!
//! Called by the worker immediately after `Connected`; also triggered by
//! the Reload Save button.  Sends `CacheProgress` every 5 000 entities so
//! the progress bar stays responsive.  Returns the completed cache wrapped
//! in an `Arc`; the worker sends `CacheReady(arc)`.

use std::collections::HashMap;
use std::sync::{mpsc, Arc};

use crate::cache::{CachedClub, CachedNation, CachedPlayer, CachedStaff, EntityCache};
use crate::process::FmProcess;

use super::cmd::BridgeResult;
use super::worker::ArrayCache;

const PROGRESS_INTERVAL: usize = 5_000;

pub(super) fn load_entity_cache(
    fm: &FmProcess,
    ac: &ArrayCache,
    tx: &mpsc::Sender<BridgeResult>,
) -> Result<Arc<EntityCache>, String> {
    let schemas = fm.schemas();
    let total = ac.humans_count + ac.staff_count + ac.clubs_count;
    let mut done: usize = 0;

    // ------------------------------------------------------------------
    // Resolve field offsets once — avoids repeated schema lookups in the
    // hot loops below.
    // ------------------------------------------------------------------

    // Player offsets (p_name not needed here — read_player_name handles both name blocks)
    let p_uid_off = schemas
        .field("p_uid")
        .ok_or("p_uid missing")?
        .offset
        .ok_or("p_uid no offset")?
        .offset();
    let p_ca_off = schemas
        .field("p_ca")
        .ok_or("p_ca missing")?
        .offset
        .ok_or("p_ca no offset")?
        .offset();
    let p_pa_off = schemas
        .field("p_pa")
        .ok_or("p_pa missing")?
        .offset
        .ok_or("p_pa no offset")?
        .offset();
    // p_dob is a DateDayYear field; year is the u16 at offset+2.
    let p_dob_year_off = schemas
        .field("p_dob")
        .ok_or("p_dob missing")?
        .offset
        .ok_or("p_dob no offset")?
        .offset()
        + 2;

    // Staff offsets (s_name not needed here — read_staff_name handles FmString + sub-record fallback)
    let s_id_off = schemas
        .field("s_id")
        .ok_or("s_id missing")?
        .offset
        .ok_or("s_id no offset")?
        .offset();
    let s_ca_off = schemas
        .field("s_ca")
        .ok_or("s_ca missing")?
        .offset
        .ok_or("s_ca no offset")?
        .offset();
    let s_pa_off = schemas
        .field("s_pa")
        .ok_or("s_pa missing")?
        .offset
        .ok_or("s_pa no offset")?
        .offset();
    let s_dob_year_off = schemas
        .field("s_dob")
        .ok_or("s_dob missing")?
        .offset
        .ok_or("s_dob no offset")?
        .offset()
        + 2;
    // Optional: FM27 has no mapped role-ratings pointer.
    let s_role_ptr_off = schemas.offset_of("s_role_ptr");

    // Club offsets
    let c_uid_off = schemas
        .field("c_uid")
        .ok_or("c_uid missing")?
        .offset
        .ok_or("c_uid no offset")?
        .offset();
    let c_name_off = schemas
        .field("c_name")
        .ok_or("c_name missing")?
        .offset
        .ok_or("c_name no offset")?
        .offset();
    let c_full_name_off = schemas
        .field("c_full_name")
        .ok_or("c_full_name missing")?
        .offset
        .ok_or("c_full_name no offset")?
        .offset();
    // Optional: FM24 has no mapped gender flag.
    let c_gender_off = schemas.offset_of("c_gender").map(|o| o as u64);

    // ------------------------------------------------------------------
    // Phase 1 — players & nations discovery
    // ------------------------------------------------------------------
    let mut players = Vec::with_capacity(ac.humans_count);
    let mut nations_map: HashMap<u64, String> = HashMap::new();
    let nat_ptr_off = schemas.offset_of("p_nation_ptr");

    for i in 0..ac.humans_count {
        let ptr = fm.read_u64(ac.humans_data + i as u64 * 8).unwrap_or(0);
        if ptr < 0x10000 || ptr > 0x7FFFFFFFFFFF {
            done += 1;
            continue;
        }

        let (raw, person_shift) = match crate::player::read_player_raw(fm, ptr) {
            Ok(r) => r,
            Err(_) => {
                done += 1;
                continue;
            }
        };

        if let Some(off) = nat_ptr_off {
            if off + 8 <= raw.len() {
                let nat_ptr = read_u64_le(&raw, off);
                if nat_ptr > 0x10000 && nat_ptr < 0x7FFFFFFFFFFF && !nations_map.contains_key(&nat_ptr) {
                    if let Ok(name) = fm.read_fm_name(nat_ptr + 0x18) {
                        let trimmed = name.trim().to_string();
                        if !trimmed.is_empty() {
                            nations_map.insert(nat_ptr, trimmed);
                        }
                    }
                }
            }
        }

        let uid = read_u32_le(&raw, p_uid_off as usize);
        let ca = raw.get(p_ca_off as usize).copied().unwrap_or(0);
        let pa = raw.get(p_pa_off as usize).copied().unwrap_or(0);
        let dob_year = read_u16_le(&raw, p_dob_year_off as usize);

        let name = crate::player::read_player_name(fm, &raw).unwrap_or_default();

        players.push(CachedPlayer {
            owning_club_id: None,
            addr: ptr,
            index: i,
            name,
            uid,
            ca,
            pa,
            dob_year,
            raw,
            person_shift,
            nation_id: None,
            club_id: None,
            value: None,
        });

        done += 1;
        if done % PROGRESS_INTERVAL == 0 {
            let _ = tx.send(BridgeResult::CacheProgress { done, total });
        }
    }

    // ------------------------------------------------------------------
    // Phase 2 — staff
    // ------------------------------------------------------------------
    let mut staff = Vec::with_capacity(ac.staff_count);

    for i in 0..ac.staff_count {
        let ptr = fm.read_u64(ac.staff_data + i as u64 * 8).unwrap_or(0);
        if ptr < 0x10000 || ptr > 0x7FFFFFFFFFFF {
            done += 1;
            continue;
        }

        let (raw, identity_shift) = match crate::staff::read_staff_raw(fm, ptr) {
            Ok(r) => r,
            Err(_) => {
                done += 1;
                continue;
            }
        };

        let uid = read_u32_le(&raw, s_id_off as usize);
        let ca = raw.get(s_ca_off as usize).copied().unwrap_or(0);
        let pa = raw.get(s_pa_off as usize).copied().unwrap_or(0);
        let dob_year = read_u16_le(&raw, s_dob_year_off as usize);

        // The platform-account entry is not part of the game world; leave
        // it out of the staff list (see `is_account_placeholder`).
        if crate::staff::is_account_placeholder(identity_shift, dob_year) {
            done += 1;
            continue;
        }

        // Try s_name FmString → sub-records at +0x150/+0x158.
        // ~72% of staff have null FmString; sub-records supply their names.
        let name = crate::staff::read_staff_name(fm, &raw).unwrap_or_default();

        let role_ptr = s_role_ptr_off.map_or(0, |o| read_u64_le(&raw, o));
        let roles = if role_ptr > 0x10000 && role_ptr < 0x7FFFFFFFFFFF {
            fm.read_bytes(role_ptr, 15)
                .ok()
                .and_then(|b| <[u8; 15]>::try_from(b).ok())
                .unwrap_or([0u8; 15])
        } else {
            [0u8; 15]
        };

        staff.push(CachedStaff {
            addr: ptr,
            index: i,
            name,
            uid,
            ca,
            pa,
            dob_year,
            roles,
            role_ptr,
            raw,
            identity_shift,
            nation_id: None,
            club_id: None,
        });

        done += 1;
        if done % PROGRESS_INTERVAL == 0 {
            let _ = tx.send(BridgeResult::CacheProgress { done, total });
        }
    }

    // ------------------------------------------------------------------
    // Phase 3 — clubs
    // ------------------------------------------------------------------
    let mut clubs = Vec::with_capacity(ac.clubs_count);

    for i in 0..ac.clubs_count {
        let ptr = fm.read_u64(ac.clubs_data + i as u64 * 8).unwrap_or(0);
        if ptr < 0x10000 || ptr > 0x7FFFFFFFFFFF {
            done += 1;
            continue;
        }

        let uid = fm.read_u32(ptr + c_uid_off).unwrap_or(0);
        let gender = c_gender_off
            .and_then(|o| fm.read_u8(ptr + o).ok())
            .unwrap_or(0);

        let short_ptr = fm.read_u64(ptr + c_name_off).unwrap_or(0);
        let full_ptr = fm.read_u64(ptr + c_full_name_off).unwrap_or(0);
        let name = fm
            .read_fm_name(short_ptr)
            .or_else(|_| fm.read_fm_name(full_ptr))
            .unwrap_or_default();

        // A club with neither name is not a real club: FM27 keeps three
        // nameless placeholder objects of their own class (UIDs 2000000000,
        // 2000000002, 2000000003; no squads) in the club array. They can't
        // be searched for or recognised, so leave them out of the list.
        if name.is_empty() {
            done += 1;
            continue;
        }

        let transfer_budget = super::entity_load::read_club_field_u32(fm, ptr, "c_transfer_budget");
        let senior_team_addr = crate::fields::resolve_sub_ptr(fm, ptr, "senior_team", schemas.club_sub_records)
            .or_else(|| {
                let teams_ptr = fm.read_u64(ptr + 0x18).ok()?;
                if teams_ptr > 0x10000 && teams_ptr < 0x7FFFFFFFFFFF {
                    let st = fm.read_u64(teams_ptr).ok()?;
                    if st > 0x10000 && st < 0x7FFFFFFFFFFF {
                        return Some(st);
                    }
                }
                None
            });

        clubs.push(CachedClub {
            addr: ptr,
            index: i,
            uid,
            name,
            gender,
            transfer_budget,
            senior_team_addr,
            nation_id: None,
            status: None,
            reputation: None,
        });

        done += 1;
        if done % PROGRESS_INTERVAL == 0 {
            let _ = tx.send(BridgeResult::CacheProgress { done, total });
        }
    }

    // ------------------------------------------------------------------
    // Build club_names lookup and wrap
    // ------------------------------------------------------------------
    // Resolve membership only through the attached build's existing schema.
    // Game Pass inherits Steam layouts but has not been verified.
    if fm.build != crate::game_version::GameBuild::Fm27GamePass {
        let clubs_by_addr: HashMap<u64, u32> = clubs.iter().map(|c| (c.addr, c.uid)).collect();
        for player in &mut players {
            let phys = |off| crate::player::player_phys_offset(schemas, off, player.person_shift);
            if fm.build == crate::game_version::GameBuild::Fm27Steam {
                player.owning_club_id = crate::fields::resolve_sub_ptr_mapped(
                    fm,
                    player.addr,
                    "owning_club",
                    schemas.player_sub_records,
                    &phys,
                )
                .and_then(|addr| clubs_by_addr.get(&addr).copied());
            }
            player.club_id = crate::fields::resolve_sub_ptr_mapped(
                fm,
                player.addr,
                "club",
                schemas.player_sub_records,
                &phys,
            )
            .and_then(|addr| clubs_by_addr.get(&addr).copied());
        }
    }
    let club_names: HashMap<u32, String> = clubs.iter().map(|c| (c.uid, c.name.clone())).collect();

    let mut nations: Vec<CachedNation> = nations_map
        .into_iter()
        .map(|(addr, name)| CachedNation { name, addr })
        .collect();
    nations.sort_by(|a, b| a.name.cmp(&b.name));

    let my_clubs = load_my_clubs(fm, &clubs);
    Ok(Arc::new(EntityCache {
        players,
        staff,
        clubs,
        nations,
        club_names,
        my_clubs,
    }))
}

// ---------------------------------------------------------------------------
// Byte-slice helpers — avoids pulling in byteorder for three simple reads
// ---------------------------------------------------------------------------

fn read_u16_le(buf: &[u8], off: usize) -> u16 {
    buf.get(off..off + 2)
        .and_then(|b| b.try_into().ok())
        .map(u16::from_le_bytes)
        .unwrap_or(0)
}

fn read_u32_le(buf: &[u8], off: usize) -> u32 {
    buf.get(off..off + 4)
        .and_then(|b| b.try_into().ok())
        .map(u32::from_le_bytes)
        .unwrap_or(0)
}

fn read_u64_le(buf: &[u8], off: usize) -> u64 {
    buf.get(off..off + 8)
        .and_then(|b| b.try_into().ok())
        .map(u64::from_le_bytes)
        .unwrap_or(0)
}

fn load_my_clubs(fm: &FmProcess, clubs: &[CachedClub]) -> Option<crate::cache::MyClubs> {
    if fm.build != crate::game_version::GameBuild::Fm27Steam {
        return None;
    }
    let manager = super::entity_load::load_human_manager(fm).ok()?;
    let root = clubs.iter().find(|c| Some(c.addr) == manager.club_addr)?;
    let schema = fm.schemas();
    let begin_off = schema.offset_of("c_reserve_teams_begin")? as u64;
    let end_off = schema.offset_of("c_reserve_teams_end")? as u64;
    let club_off = schema
        .player_sub_records
        .iter()
        .find(|s| s.id == "club")?
        .ptr_offset;
    let club_ids = collect_club_group(root.addr, clubs, begin_off, end_off, club_off, |addr| {
        fm.read_u64(addr).ok()
    })?;
    Some(crate::cache::MyClubs {
        club_ids,
        label: format!("{} (UID {})", root.name, root.uid),
    })
}

fn collect_club_group(
    root: u64,
    clubs: &[CachedClub],
    begin_off: u64,
    end_off: u64,
    club_off: u64,
    read: impl Fn(u64) -> Option<u64>,
) -> Option<std::collections::HashSet<u32>> {
    use std::collections::HashSet;
    let by_addr: HashMap<_, _> = clubs.iter().map(|c| (c.addr, c)).collect();
    let mut club_ids = HashSet::new();
    let mut visited = HashSet::new();
    let mut pending = vec![root];
    let valid = |p| p > 0x10000 && p < 0x7FFFFFFFFFFF;
    let read_pointer = |addr: u64| {
        if !valid(addr) || !valid(addr.checked_add(7)?) {
            return None;
        }
        read(addr)
    };
    while let Some(addr) = pending.pop() {
        if !valid(addr) {
            return None;
        }
        if !visited.insert(addr) {
            continue;
        }
        if visited.len() > 128 {
            return None;
        }
        club_ids.insert(by_addr.get(&addr)?.uid);
        let begin = read_pointer(addr.checked_add(begin_off)?)?;
        let end = read_pointer(addr.checked_add(end_off)?)?;
        if begin == 0 && end == 0 {
            continue;
        }
        if !valid(begin) || !valid(end) {
            return None;
        }
        let bytes = end.checked_sub(begin)?;
        if bytes % 8 != 0 || bytes > 128 * 8 {
            return None;
        }
        for i in 0..bytes / 8 {
            let team = read_pointer(begin.checked_add(i * 8)?)?;
            if !valid(team) {
                return None;
            }
            let club = read_pointer(team.checked_add(club_off)?)?;
            if !valid(club) || !by_addr.contains_key(&club) {
                return None;
            }
            pending.push(club);
        }
    }
    Some(club_ids)
}

#[cfg(test)]
mod membership_tests {
    use super::*;
    fn club(addr: u64, uid: u32) -> CachedClub {
        CachedClub {
            addr,
            uid,
            index: 0,
            name: uid.to_string(),
            gender: 0,
            transfer_budget: None,
            senior_team_addr: None,
            nation_id: None,
            status: None,
            reputation: None,
        }
    }
    #[test]
    fn group_is_root_specific_and_handles_cycles() {
        let clubs = [club(0x20000, 42), club(0x30000, 43), club(0x40000, 99)];
        // Synthetic layout, deliberately different from FM27.
        let memory: HashMap<u64, u64> = [
            (0x20008, 0x50000),
            (0x20010, 0x50008),
            (0x50000, 0x60000),
            (0x60020, 0x30000),
            (0x30008, 0x70000),
            (0x30010, 0x70008),
            (0x70000, 0x80000),
            (0x80020, 0x20000),
            (0x40008, 0),
            (0x40010, 0),
        ]
        .into_iter()
        .collect();
        assert_eq!(
            collect_club_group(0x20000, &clubs, 8, 16, 32, |a| memory.get(&a).copied()).unwrap(),
            [42, 43].into_iter().collect()
        );
        assert_eq!(
            collect_club_group(0x40000, &clubs, 8, 16, 32, |a| memory.get(&a).copied()).unwrap(),
            [99].into_iter().collect()
        );
    }
    #[test]
    fn malformed_or_unreadable_membership_never_returns_partial_group() {
        let clubs = [club(0x20000, 42)];
        for (begin, end) in [
            (0, 0x50000),
            (0x10000, 0x10008),
            (0x50008, 0x50000),
            (0x50000, 0x50003),
            (0x50000, 0x51000),
            (0x50000, 0x50008),
        ] {
            let mem: HashMap<u64, u64> = [(0x20008, begin), (0x20010, end)].into_iter().collect();
            assert!(
                collect_club_group(0x20000, &clubs, 8, 16, 32, |a| mem.get(&a).copied()).is_none()
            );
        }
        assert!(collect_club_group(0x10000, &clubs, 8, 16, 32, |_| panic!(
            "invalid pointer read"
        ))
        .is_none());
        let edge = 0x7FFFFFFFFFF0;
        assert!(collect_club_group(edge, &[club(edge, 44)], 8, 16, 32, |_| {
            panic!("pointer read crosses valid address range")
        })
        .is_none());
    }
}
