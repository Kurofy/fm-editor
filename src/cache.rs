//! In-memory entity cache — built once on connect, never mutated post-load.
//!
//! `EntityCache` holds flat structs for every player, staff member, and club
//! in the current save.  The worker thread builds it via `bulk_load` and
//! ships it to the app thread wrapped in an `Arc`; the app stores a clone of
//! the Arc so no data is copied across the channel.
//!
//! `raw` byte slices carry the full struct bytes so detail panels can read
//! any field without a bridge round-trip.  The per-entity Option fields
//! are scaffolded for columns that aren't mapped yet; they slot in once the
//! relevant offsets are added to `src/fields/fm26/`.

use std::collections::HashMap;

// ---------------------------------------------------------------------------
// Per-entity cached structs
// ---------------------------------------------------------------------------

pub struct CachedPlayer {
    /// Contract employer, which can differ from current club during a loan.
    pub owning_club_id: Option<u32>,
    pub addr: u64,
    pub index: usize,
    pub name: String,
    pub uid: u32,
    pub ca: u8,
    pub pa: u8,
    pub dob_year: u16,
    /// Full 968-byte player struct — used by the detail panel for instant
    /// field reads without a bridge round-trip.
    pub raw: Vec<u8>,
    /// Person-block shift in memory (player-coaches); see `player::read_player_raw`.
    pub person_shift: usize,
    // Optional list fields; unavailable mappings remain None.
    pub nation_id: Option<u32>,
    /// Current club UID, resolved by object identity in this cache.
    pub club_id: Option<u32>,
    pub value: Option<u64>,
}

pub struct CachedStaff {
    pub addr: u64,
    pub index: usize,
    pub name: String,
    pub uid: u32,
    pub ca: u8,
    pub pa: u8,
    pub dob_year: u16,
    pub roles: [u8; 15],
    pub role_ptr: u64,
    /// Full staff struct in schema layout — used by the detail panel.
    pub raw: Vec<u8>,
    /// Identity-field shift in memory (human managers); see `staff::read_staff_raw`.
    pub identity_shift: usize,
    // Unmapped list-column fields:
    pub nation_id: Option<u32>,
    pub club_id: Option<u32>,
}

pub struct CachedClub {
    pub addr: u64,
    pub index: usize,
    pub uid: u32,
    pub name: String,
    pub gender: u8,
    pub transfer_budget: Option<u32>,
    /// Senior team address (e.g. club+0x18 -> [0]); required to move players to this club.
    pub senior_team_addr: Option<u64>,
    // Unmapped list-column fields:
    pub nation_id: Option<u32>,
    pub status: Option<u8>,
    pub reputation: Option<u16>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CachedNation {
    pub name: String,
    pub addr: u64,
}

// ---------------------------------------------------------------------------
// Top-level cache
// ---------------------------------------------------------------------------

pub struct MyClubs {
    pub club_ids: std::collections::HashSet<u32>,
    pub label: String,
}

pub struct EntityCache {
    pub my_clubs: Option<MyClubs>,
    pub players: Vec<CachedPlayer>,
    pub staff: Vec<CachedStaff>,
    pub clubs: Vec<CachedClub>,
    pub nations: Vec<CachedNation>,
    /// uid → short name; built from `clubs` on cache completion.
    /// Used to display the Club column in player/staff list rows once
    /// club_id fields are mapped.
    pub club_names: HashMap<u32, String>,
}
