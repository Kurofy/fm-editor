//! Detail-view loaders — the "Load *" commands. Each reads an entity struct
//! from memory (plus relevant sub-records) and builds the View struct the UI
//! consumes.
//!
//! Also owns the human-manager discovery logic (`find_human_manager`) and
//! the shifted-field layout constants used by both loaders and writer code.

use crate::fields::{FieldLocation, SubRecordId, resolve_sub_ptr};
use crate::process::FmProcess;
use crate::staff::{self, StaffRecord};

use super::cmd::{ClubView, StaffView};

// ---------------------------------------------------------------------------
// Staff / club detail loaders
// ---------------------------------------------------------------------------

pub(super) fn load_staff_view(
    fm: &FmProcess,
    addr: u64,
    index: usize,
) -> Result<StaffView, String> {
    let record = staff::read_staff_at(fm, addr, index)?;
    let name = staff::read_staff_name(fm, &record.raw);

    let role_ptr = record.field_u64("s_role_ptr").unwrap_or(0);
    let roles = if role_ptr > 0x10000 && role_ptr < 0x7FFFFFFFFFFF {
        fm.read_bytes(role_ptr, 15)
            .ok()
            .and_then(|b| <[u8; 15]>::try_from(b).ok())
    } else {
        None
    };

    let extras = load_staff_extras(fm, addr + record.identity_shift as u64);
    Ok(StaffView {
        record,
        name,
        roles,
        role_ptr,
        nationality: extras.nationality,
        club_name: extras.club_name,
        club_addr: extras.club_addr,
        birthplace: extras.birthplace,
        contract_raw: extras.contract_raw,
        contract_addr: extras.contract_addr,
    })
}

/// Strings and contract bytes reached through the staff sub-record chains.
pub(super) struct StaffExtras {
    pub nationality: Option<String>,
    pub club_name: Option<String>,
    pub club_addr: Option<u64>,
    pub birthplace: Option<String>,
    pub contract_raw: Option<Vec<u8>>,
    pub contract_addr: Option<u64>,
}

/// Resolve the staff sub-records from `base` — the staff struct for AI staff,
/// or the human manager's struct + `human_manager_shift` (his identity block,
/// including the nation/contract pointers, lives at the shifted offsets).
pub(super) fn load_staff_extras(fm: &FmProcess, base: u64) -> StaffExtras {
    let subs = fm.schemas().staff_sub_records;
    let contract_addr = resolve_sub_ptr(fm, base, "contract", subs);
    let contract_raw = contract_addr.and_then(|ptr| {
        let size = subs.iter().find(|s| s.id == "contract")?.size;
        fm.read_bytes(ptr, size).ok()
    });
    // Keep the address only when the bytes were read (write-back safety).
    let contract_addr = contract_raw.as_ref().and(contract_addr);
    StaffExtras {
        nationality: crate::fields::read_sub_fm_string(fm, base, "s_nationality", subs, &|o| o),
        club_name: crate::fields::read_sub_fm_string(fm, base, "s_club", subs, &|o| o),
        club_addr: resolve_sub_ptr(fm, base, "club", subs),
        birthplace: crate::fields::read_sub_fm_string(fm, base, "s_birthplace", subs, &|o| o),
        contract_raw,
        contract_addr,
    }
}

pub(super) fn load_club_view(
    fm: &FmProcess,
    addr: u64,
    index: usize,
) -> Result<ClubView, String> {
    let schema = fm.schemas();
    let raw = fm.read_bytes(addr, schema.club_read_size)?;

    // Direct top-level club offsets via the field table.
    let uid_offset = schema
        .field("c_uid")
        .and_then(|f| f.offset)
        .unwrap()
        .offset() as usize;
    let full_name_offset = schema
        .field("c_full_name")
        .and_then(|f| f.offset)
        .unwrap()
        .offset() as usize;
    let short_name_offset = schema
        .field("c_name")
        .and_then(|f| f.offset)
        .unwrap()
        .offset() as usize;

    let uid = u32::from_le_bytes(raw[uid_offset..uid_offset + 4].try_into().unwrap());
    let full_name_ptr = u64::from_le_bytes(
        raw[full_name_offset..full_name_offset + 8]
            .try_into()
            .unwrap(),
    );
    let full_name = fm.read_fm_name(full_name_ptr).unwrap_or_default();
    let short_name_ptr = u64::from_le_bytes(
        raw[short_name_offset..short_name_offset + 8]
            .try_into()
            .unwrap(),
    );
    let short_name = fm.read_fm_name(short_name_ptr).unwrap_or_default();

    // Sub-record loading — schema-driven pointer chains, no hardcoded offsets.
    let srs = schema.club_sub_records;

    let info_addr = resolve_sub_ptr(fm, addr, "info", srs);
    let finance_addr = resolve_club_sub_ptr(fm, addr, "finance");
    let budget_addr = resolve_sub_ptr(fm, addr, "budget", srs);
    let stadium_addr = resolve_sub_ptr(fm, addr, "stadium", srs);

    let info_raw = read_sub_bytes(fm, info_addr, srs, "info");
    let finance_raw = read_sub_bytes(fm, finance_addr, srs, "finance");
    let budget_raw = read_sub_bytes(fm, budget_addr, srs, "budget");
    let stadium_raw = read_sub_bytes(fm, stadium_addr, srs, "stadium");
    let senior_team_addr = resolve_sub_ptr(fm, addr, "senior_team", srs);
    let senior_team_raw = read_sub_bytes(fm, senior_team_addr, srs, "senior_team");
    let senior_team_addr = senior_team_raw.as_ref().and(senior_team_addr);

    // Only retain addr when the read actually succeeded (write-back safety).
    let info_addr = info_raw.as_ref().map(|_| info_addr.unwrap());
    let finance_addr = finance_raw.as_ref().map(|_| finance_addr.unwrap());
    let budget_addr = budget_raw.as_ref().map(|_| budget_addr.unwrap());

    // Club nickname — stored at stadium sub-record +0x90 as an FmString pointer.
    // Confirmed 2026-04-16: Bayern="Die Bayern", Spurs="The Lilywhites", Madrid="Los Merengues".
    let nickname = stadium_raw.as_ref().and_then(|s| {
        let off = schema
            .field("c_nickname")
            .and_then(|f| f.offset)
            .map(|l| l.offset() as usize)?;
        let np = u64::from_le_bytes(s.get(off..off + 8)?.try_into().ok()?);
        fm.read_fm_name(np).ok()
    });

    // Home stadium entity — flat inline array, reached via the senior team
    // (club → teams → senior_team → home_stadium chain in the schema).
    let stadium_entity_raw = resolve_sub_ptr(fm, addr, "home_stadium", srs)
        .and_then(|ptr| fm.read_bytes(ptr, schema.stadium_entity_stride).ok());

    // Stadium names: FmString pointers inside the stadium entity
    // (`st_name` = the name FM shows, `st_name2` = official/alternative name).
    let stadium_string = |key: &str| -> Option<String> {
        let se = stadium_entity_raw.as_ref()?;
        let off = schema.field(key)?.offset?.offset() as usize;
        let np = u64::from_le_bytes(se.get(off..off + 8)?.try_into().ok()?);
        fm.read_fm_name(np).ok()
    };
    let stadium_name = stadium_string("st_name");
    let stadium_name2 = stadium_string("st_name2");

    let affiliations = load_club_affiliations(fm, addr, uid, &full_name, info_addr);

    Ok(ClubView {
        addr,
        index,
        raw,
        uid,
        full_name,
        short_name,
        info_raw,
        info_addr,
        finance_raw,
        finance_addr,
        stadium_raw,
        senior_team_raw,
        senior_team_addr,
        budget_raw,
        budget_addr,
        nickname,
        stadium_name,
        stadium_name2,
        stadium_entity_raw,
        affiliations,
    })
}

/// Read the bytes of a named sub-record given its already-resolved address.
fn read_sub_bytes(
    fm: &FmProcess,
    ptr: Option<u64>,
    sub_records: &[crate::fields::SubRecord],
    id: SubRecordId,
) -> Option<Vec<u8>> {
    let ptr = ptr?;
    let size = sub_records.iter().find(|s| s.id == id)?.size;
    fm.read_bytes(ptr, size).ok()
}

fn load_club_affiliations(
    fm: &FmProcess,
    club_addr: u64,
    club_uid: u32,
    club_name: &str,
    info_addr: Option<u64>,
) -> Vec<crate::affiliation::AffiliationView> {
    use crate::affiliation::{AffiliationType, AffiliationView, FriendshipBenefits};
    use std::collections::HashSet;

    let mut results = Vec::new();
    let mut seen_names = HashSet::new();
    seen_names.insert(club_name.trim().to_lowercase());

    // Helper: find club name and uid given an address (club or team)
    let resolve_club = |ptr: u64| -> Option<(String, u32, u64)> {
        if !(ptr > 0x10000 && ptr < 0x7FFFFFFFFFFF) {
            return None;
        }
        // Direct memory read of club: UID at +0x0C, name at +0xC0 or +0xC8
        if let Ok(uid) = fm.read_u32(ptr + 0x0C) {
            let np_res = fm.read_u64(ptr + 0xC0).or_else(|_| fm.read_u64(ptr + 0xC8));
            if let Ok(name_ptr) = np_res {
                if let Ok(name) = fm.read_fm_name(name_ptr) {
                    if !name.trim().is_empty() {
                        return Some((name, uid, ptr));
                    }
                }
            }
        }
        // Fallback: team+0x30
        if let Ok(cptr) = fm.read_u64(ptr + 0x30) {
            if cptr > 0x10000 && cptr < 0x7FFFFFFFFFFF && cptr != ptr {
                if let Ok(uid) = fm.read_u32(cptr + 0x0C) {
                    let np_res = fm.read_u64(cptr + 0xC0).or_else(|_| fm.read_u64(cptr + 0xC8));
                    if let Ok(name_ptr) = np_res {
                        if let Ok(name) = fm.read_fm_name(name_ptr) {
                            if !name.trim().is_empty() {
                                return Some((name, uid, cptr));
                            }
                        }
                    }
                }
            }
        }
        None
    };

    // Vector scan ranges:
    // 1. Club struct vectors: check every 8-byte boundary from 0x18 to 0x160
    // 2. Info struct vectors: check every 8-byte boundary from 0x00 to 0x130
    let mut vec_bases = vec![(club_addr, 0x18u64..0x160u64)];
    if let Some(ia) = info_addr {
        vec_bases.push((ia, 0x00u64..0x130u64));
    }

    for (base, range) in vec_bases {
        for vec_off in range.step_by(8) {
            let Ok(begin) = fm.read_u64(base + vec_off) else { continue };
            let Ok(end) = fm.read_u64(base + vec_off + 8) else { continue };

            if !(begin > 0x10000 && begin < 0x7FFFFFFFFFFF && end >= begin && end < 0x7FFFFFFFFFFF) {
                continue;
            }

            let bytes = end - begin;
            if bytes == 0 || bytes % 8 != 0 || bytes > 128 * 8 {
                continue;
            }

            let count = (bytes / 8) as usize;
            for i in 0..count {
                let Ok(item_ptr) = fm.read_u64(begin + (i as u64) * 8) else { break };
                if !(item_ptr > 0x10000 && item_ptr < 0x7FFFFFFFFFFF) {
                    break;
                }

                // Try direct match (Club or Team)
                if let Some((target_name, target_uid, target_addr)) = resolve_club(item_ptr) {
                    if target_addr == club_addr || target_uid == club_uid {
                        continue;
                    }
                    let key = target_name.trim().to_lowercase();
                    if !seen_names.contains(&key) {
                        seen_names.insert(key);

                        let is_b_club = target_name.ends_with(" B")
                            || target_name.contains(" B ")
                            || target_name.contains(" B-")
                            || target_name.ends_with(" II")
                            || target_name.ends_with(" Reserves");

                        let is_womens = target_name.to_lowercase().contains("women")
                            || target_name.to_lowercase().contains("femin");

                        let aff_type = if is_b_club {
                            AffiliationType::BClub
                        } else if is_womens {
                            AffiliationType::WomensClub
                        } else {
                            AffiliationType::Normal
                        };

                        let benefits_flags = if is_b_club {
                            FriendshipBenefits::PLAYERS_MOVE_FREELY
                                | FriendshipBenefits::SAME_BOARD
                                | FriendshipBenefits::CANNOT_PLAY_IN_SAME_DIVISION
                                | FriendshipBenefits::PLAY_FRIENDLIES
                                | FriendshipBenefits::PERMANENT_DEAL
                                | FriendshipBenefits::USES_ALL_PARENT_FACILITIES
                        } else {
                            FriendshipBenefits::PLAY_FRIENDLIES | FriendshipBenefits::PERMANENT_DEAL
                        };

                        results.push(AffiliationView {
                            addr: item_ptr,
                            parent_club_uid: club_uid,
                            parent_club_name: club_name.to_string(),
                            feeder_club_uid: target_uid,
                            feeder_club_name: target_name,
                            start_doy: 205,
                            start_year: 2022,
                            end_doy: 1,
                            end_year: 1900,
                            affiliation_type: aff_type,
                            annual_fee: 0,
                            friendly_chance_enabled: true,
                            friendly_chance: 50,
                            max_loaned_enabled: false,
                            max_loaned: 0,
                            wage_percentage_enabled: is_b_club,
                            wage_percentage: if is_b_club { 100 } else { 0 },
                            benefits: FriendshipBenefits { flags: benefits_flags },
                            raw: Vec::new(),
                        });
                        continue;
                    }
                }

                // Try Affiliation composite record (reads up to 64 bytes)
                let Ok(raw) = fm.read_bytes(item_ptr, 0x40) else { continue };
                // Scan any 8-byte pointer in the first 0x28 bytes of raw
                let mut found_other = None;
                for p_off in (0..0x28).step_by(8) {
                    if p_off + 8 <= raw.len() {
                        let candidate_ptr = u64::from_le_bytes(raw[p_off..p_off + 8].try_into().unwrap());
                        if let Some((other_name, other_uid, other_addr)) = resolve_club(candidate_ptr) {
                            if other_addr != club_addr && other_uid != club_uid {
                                found_other = Some((other_name, other_uid, other_addr));
                                break;
                            }
                        }
                    }
                }

                if let Some((other_name, other_uid, _other_addr)) = found_other {
                    let key = other_name.trim().to_lowercase();
                    if !seen_names.contains(&key) {
                        seen_names.insert(key);

                        let start_doy = u16::from_le_bytes(raw[0x18..0x1A].try_into().unwrap_or([0, 0]));
                        let start_year = u16::from_le_bytes(raw[0x1A..0x1C].try_into().unwrap_or([0, 0]));
                        let end_doy = u16::from_le_bytes(raw[0x1C..0x1E].try_into().unwrap_or([0, 0]));
                        let end_year = u16::from_le_bytes(raw[0x1E..0x20].try_into().unwrap_or([0, 0]));
                        let type_val = u32::from_le_bytes(raw[0x20..0x24].try_into().unwrap_or([0, 0, 0, 0]));
                        let annual_fee = i32::from_le_bytes(raw[0x24..0x28].try_into().unwrap_or([0, 0, 0, 0]));
                        let friendly_chance = raw.get(0x28).copied().unwrap_or(0);
                        let max_loaned = raw.get(0x29).copied().unwrap_or(0);
                        let wage_percentage = raw.get(0x2A).copied().unwrap_or(0);
                        let flags = u32::from_le_bytes(raw[0x2C..0x30].try_into().unwrap_or([0, 0, 0, 0]));

                        let is_b_club = other_name.ends_with(" B") || other_name.contains(" B ") || other_name.ends_with(" II");
                        let aff_type = if type_val > 0 {
                            AffiliationType::from_u32(type_val)
                        } else if is_b_club {
                            AffiliationType::BClub
                        } else {
                            AffiliationType::Normal
                        };

                        results.push(AffiliationView {
                            addr: item_ptr,
                            parent_club_uid: club_uid,
                            parent_club_name: club_name.to_string(),
                            feeder_club_uid: other_uid,
                            feeder_club_name: other_name,
                            start_doy: if start_doy == 0 { 205 } else { start_doy },
                            start_year: if start_year == 0 { 2022 } else { start_year },
                            end_doy: if end_doy == 0 { 1 } else { end_doy },
                            end_year: if end_year == 0 { 1900 } else { end_year },
                            affiliation_type: aff_type,
                            annual_fee,
                            friendly_chance_enabled: friendly_chance > 0,
                            friendly_chance: friendly_chance.min(100),
                            max_loaned_enabled: max_loaned > 0,
                            max_loaned,
                            wage_percentage_enabled: wage_percentage > 0 || is_b_club,
                            wage_percentage: if wage_percentage > 0 { wage_percentage.min(100) } else if is_b_club { 100 } else { 0 },
                            benefits: FriendshipBenefits { flags: if flags != 0 { flags } else if is_b_club {
                                FriendshipBenefits::PLAYERS_MOVE_FREELY
                                    | FriendshipBenefits::SAME_BOARD
                                    | FriendshipBenefits::CANNOT_PLAY_IN_SAME_DIVISION
                                    | FriendshipBenefits::PLAY_FRIENDLIES
                                    | FriendshipBenefits::PERMANENT_DEAL
                                    | FriendshipBenefits::USES_ALL_PARENT_FACILITIES
                            } else {
                                FriendshipBenefits::PLAY_FRIENDLIES | FriendshipBenefits::PERMANENT_DEAL
                            }},
                            raw,
                        });
                    }
                }
            }
        }
    }

    results
}

/// Read a u32 club field from live process memory — used by search hot path
/// to avoid a full `load_club_view` round-trip.
/// Resolve a club sub-record, rejecting a finance object of the wrong class.
///
/// The finance fields describe one finance class. A club whose finance object
/// is another class (FM27's smaller one for minor clubs) gets no finance
/// sub-record, so its fields stay hidden instead of reading — and offering to
/// write — bytes outside the object.
fn resolve_club_sub_ptr(fm: &FmProcess, club_ptr: u64, id: SubRecordId) -> Option<u64> {
    let schema = fm.schemas();
    let ptr = resolve_sub_ptr(fm, club_ptr, id, schema.club_sub_records)?;
    let rva = schema.full_finance_vtable_rva;
    if id == "finance" && rva != 0 && !fm.read_u64(ptr).is_ok_and(|vt| vt == fm.gp_base + rva) {
        return None;
    }
    Some(ptr)
}

pub(super) fn read_club_field_u32(fm: &FmProcess, club_ptr: u64, key: &str) -> Option<u32> {
    let schema = fm.schemas();
    let field = schema.field(key)?;
    match field.offset.as_ref()? {
        FieldLocation::Direct { offset, .. } => fm.read_u32(club_ptr + offset).ok(),
        FieldLocation::SubRecord {
            container, offset, ..
        } => {
            let sub_ptr = resolve_club_sub_ptr(fm, club_ptr, container)?;
            fm.read_u32(sub_ptr + offset).ok()
        }
    }
}

// ---------------------------------------------------------------------------
// Human manager discovery
// ---------------------------------------------------------------------------
//
// The human manager struct is a LARGER composite than a normal AI staff.
// The first ~576 bytes mostly match the standard AI staff layout for
// attributes, CA/PA, and reputations — but the "identity" fields (staff ID,
// name ptr, personality, DOB, role sub-record ptr) are zeroed at their
// standard offsets and instead duplicated at
// `standard_offset + HUMAN_MANAGER_SHIFT`.
//
// Confirmed via dump_manager.rs against a test save's human manager:
//   +0x45C (0x10C+0x350) = staff ID 2002085700
//   +0x490 (0x140+0x350) = name ptr → the manager's name
//   +0x4C0 (0x170+0x350) = personality [14,10,10,10,10,10,10,6]
//   +0x4D8 (0x188+0x350) = DOB (day of year / year)
//   +0x520 (0x1D0+0x350) = role sub-record ptr → [20,1,1,...] (Manager=20)
//
// The shift value and shifted-field list live in `fields/fm26/staff.rs`
// as version-specific data. `load_human_manager` reads them via
// `fm.schemas()` so an FM24 schema with a different layout will slot in
// without touching the loader code.

/// Discover the human manager's struct address at runtime.
///
/// Strategy:
///   1. Try the last entry of ptr_staff (the canonical location).
///   2. Validate: does the struct look like a plausible human manager?
///      - vtable pointer in game_plugin.dll range
///      - CA in 1..=200, PA in 1..=200, PA >= CA
///      - name pointer at the SHIFTED +0x490 slot is non-null
///      - role sub-record at the SHIFTED +0x520 slot is non-null and has
///        Manager role (+0x00) > 0
///   3. If last-entry fails, scan ptr_staff from the end backwards.
///
/// NB: the AI-staff name_ptr at +0x140 and role_ptr at +0x1D0 are BOTH null
/// on the real human manager — those slots are populated in AI staff but
/// zeroed for the human. Validating against the shifted slots (+0x490,
/// +0x520) is what actually identifies him.
pub fn find_human_manager(fm: &FmProcess) -> Result<u64, String> {
    let gp_base = fm.gp_base;
    let gp_end = fm.gp_end;

    let schema = fm.schemas();
    let (data, end, count) = fm.read_ptr_array(schema.rva_staff_data, schema.rva_staff_end)?;
    if count == 0 {
        return Err("empty staff array".into());
    }
    let count = count as u64;

    // Resolve staff field offsets once from the schema; the validator
    // reads the shifted versions (standard offset + human_manager_shift).
    let ca_offset = schema
        .field("s_ca")
        .and_then(|f| f.offset)
        .map(|l| l.offset() as usize)
        .unwrap_or(0);
    let pa_offset = schema
        .field("s_pa")
        .and_then(|f| f.offset)
        .map(|l| l.offset() as usize)
        .unwrap_or(0);
    let name_offset_std = schema
        .field("s_name")
        .and_then(|f| f.offset)
        .map(|l| l.offset() as usize)
        .unwrap_or(0);
    // FM27 has no mapped role-ratings pointer; skip that check there.
    let role_offset_std = schema
        .field("s_role_ptr")
        .and_then(|f| f.offset)
        .map(|l| l.offset() as usize);
    let first_sub_offset_std = schema
        .field("s_name_first_sub")
        .and_then(|f| f.offset)
        .map(|l| l.offset() as usize)
        .unwrap_or(0);
    let shift = schema.human_manager_shift;
    let name_offset_shifted = name_offset_std + shift;
    let first_sub_offset_shifted = first_sub_offset_std + shift;
    let role_offset_shifted = role_offset_std.map(|o| o + shift);
    // Need to cover the furthest shifted slot plus 8 bytes for the u64 read.
    let validator_read_size = role_offset_shifted
        .unwrap_or(0)
        .max(first_sub_offset_shifted)
        .max(name_offset_shifted)
        + 8
        + 16;

    // Validator: does `ptr` look like the human manager?
    let validate = |ptr: u64| -> bool {
        if ptr < 0x10000 || ptr > 0x7FFFFFFFFFFF {
            return false;
        }
        let Ok(buf) = fm.read_bytes(ptr, validator_read_size) else {
            return false;
        };
        // vtable check (+0x00): should point into game_plugin.dll
        let vtable = u64::from_le_bytes(buf[0..8].try_into().unwrap());
        if vtable < gp_base || vtable >= gp_end {
            return false;
        }
        // FM27 Steam has a verified human-manager class. Generic staff can
        // otherwise pass the shifted-name heuristic by reading bytes in
        // neighbouring objects. Game Pass inherits the Steam RVA unverified, so
        // enforcing it there could reject the real manager.
        if fm.build == crate::game_version::GameBuild::Fm27Steam
            && schema.human_manager_vtable_rva != 0
        {
            if gp_base.checked_add(schema.human_manager_vtable_rva) != Some(vtable) {
                return false;
            }
            let Ok((identity, identity_shift)) = staff::read_staff_raw(fm, ptr) else { return false; };
            let Some(off) = schema.offset_of("s_dob") else { return false; };
            let Some(year) = identity.get(off + 2..off + 4)
                .and_then(|b| <[u8; 2]>::try_from(b).ok()).map(u16::from_le_bytes) else { return false; };
            if year == 0 || staff::is_account_placeholder(identity_shift, year) { return false; }
        }
        // CA / PA sanity
        let ca = buf[ca_offset];
        let pa = buf[pa_offset];
        if ca == 0 || ca > 200 || pa == 0 || pa > 200 || pa < ca {
            return false;
        }
        // Name at SHIFTED offset — human manager stores it at
        // standard_offset + human_manager_shift (standard slot is zeroed).
        // FM26 fills the direct FmString; FM24 leaves it null and only fills
        // the first-name sub-record, so either one identifies the manager.
        let ptr_at = |off: usize| u64::from_le_bytes(buf[off..off + 8].try_into().unwrap());
        let valid = |p: u64| p > 0x10000 && p < 0x7FFFFFFFFFFF;
        if !valid(ptr_at(name_offset_shifted)) && !valid(ptr_at(first_sub_offset_shifted)) {
            return false;
        }
        // Role sub-record at SHIFTED offset, when the schema maps one.
        let Some(role_offset_shifted) = role_offset_shifted else {
            return true;
        };
        let role_ptr = u64::from_le_bytes(
            buf[role_offset_shifted..role_offset_shifted + 8]
                .try_into()
                .unwrap(),
        );
        if role_ptr < 0x10000 || role_ptr > 0x7FFFFFFFFFFF {
            return false;
        }
        let Ok(roles) = fm.read_bytes(role_ptr, 15) else {
            return false;
        };
        // Manager role rating must be set
        if roles[0] == 0 {
            return false;
        }
        true
    };

    // Strategy 1: last entry
    let last_ptr = fm.read_u64(end - 8)?;
    if validate(last_ptr) {
        return Ok(last_ptr);
    }

    // Strategy 2: scan backwards from the end
    for i in (0..count).rev() {
        let slot = data + i * 8;
        let Ok(ptr) = fm.read_u64(slot) else {
            continue;
        };
        if validate(ptr) {
            return Ok(ptr);
        }
    }

    Err("no valid human manager found in ptr_staff".into())
}

pub(super) fn load_human_manager(fm: &FmProcess) -> Result<StaffView, String> {
    let ptr = find_human_manager(fm)?;

    // Read the struct with the shifted identity block patched back into the
    // standard offsets, so the existing reader/UI code sees a normal staff
    // layout. The manager is always shifted, whatever his class.
    let shift = fm.schemas().human_manager_shift;
    let record = StaffRecord {
        build: fm.build,
        addr: ptr,
        index: 0,
        raw: staff::read_staff_raw_shifted(fm, ptr, shift)?,
        identity_shift: shift,
    };

    // Resolve name and role sub-record from the patched buffer via the
    // standard staff field offsets (the shifted bytes have been copied
    // into the standard slots above).
    // FM26 has a direct name FmString; FM24 leaves it null and only fills the
    // first/last sub-records, which read_staff_name falls back to.
    let name = crate::staff::read_staff_name(fm, &record.raw);

    let role_ptr = record.field_u64("s_role_ptr").unwrap_or(0);
    let roles = if role_ptr > 0x10000 && role_ptr < 0x7FFFFFFFFFFF {
        fm.read_bytes(role_ptr, 15)
            .ok()
            .and_then(|b| <[u8; 15]>::try_from(b).ok())
    } else {
        None
    };

    let extras = load_staff_extras(fm, ptr + shift as u64);
    Ok(StaffView {
        record,
        name,
        roles,
        role_ptr,
        nationality: extras.nationality,
        club_name: extras.club_name,
        club_addr: extras.club_addr,
        birthplace: extras.birthplace,
        contract_raw: extras.contract_raw,
        contract_addr: extras.contract_addr,
    })
}
