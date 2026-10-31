//! Field definitions — the single source of truth for every known field on
//! every entity type.
//!
//! Per-entity field vectors live in `player.rs`, `staff.rs`, and `club.rs`.
//! This module owns the shared types (`Field`, `Entity`, `Category`,
//! `FieldLocation`, `Encoding`) and exposes the lookup helpers the rest of
//! the codebase uses.
//!
//! When adding or reverse-engineering a field, touch only the sub-module
//! for that entity — there should be no reason to edit more than one of
//! `player.rs` / `staff.rs` / `club.rs` in a given session.

use std::collections::HashMap;
use std::fmt;
use std::sync::LazyLock;

pub mod fm26;
pub mod fm24;
pub mod fm27;

/// A field definition: key, display label, entity type, category, and optional offset.
#[derive(Debug, Clone)]
pub struct Field {
    pub key: &'static str,
    pub label: &'static str,
    pub entity: Entity,
    pub category: Category,
    pub offset: Option<FieldLocation>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Entity {
    Player,
    Staff,
    Club,
    /// Stadium entity — flat inline array (stride 0x170), NOT a pointer array.
    /// Access chain from GP: GP+RVA → base → base+0x80 → type → vec → data → data+0x40 + i*stride.
    /// Not yet wired to UI or entity loading.
    Stadium,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Category {
    Ability,
    BoardRelations,
    Coaching,
    Contract,
    Condition,
    Facilities,
    Finance,
    Footedness,
    General,
    Goalkeeping,
    Hidden,
    /// Schema-only helper fields (name sub-record pointers, etc.) — never rendered in any UI panel.
    Internal,
    Knowledge,
    Medical,
    Mental,
    Personality,
    Physical,
    Positions,
    Reputation,
    SetPieces,
    /// Role ratings stored in the staff-role sub-record (15 × u8 at role_ptr+0).
    /// Rendered via `roles_column` — not editable through the standard widget path.
    StaffRole,
    TacticalStyle,
    Technical,
    Traits,
}

impl fmt::Display for Category {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BoardRelations => write!(f, "Board Relations"),
            Self::SetPieces => write!(f, "Set Pieces"),
            Self::StaffRole => write!(f, "Staff Roles"),
            Self::TacticalStyle => write!(f, "Tactical Style"),
            other => write!(f, "{other:?}"),
        }
    }
}

/// Stable string identifier for a sub-record within an entity.
/// Examples for club: "info", "finance", "budget", "stadium".
pub type SubRecordId = &'static str;

/// Where a field lives: either flat in the entity's top-level bytes,
/// or inside one of the entity's named sub-records.
#[derive(Debug, Clone, Copy)]
pub enum FieldLocation {
    /// Field lives at `offset` inside the entity's top-level raw buffer.
    Direct {
        offset: u64,
        encoding: Encoding,
    },
    /// Field lives at `offset` inside a named sub-record buffer.
    SubRecord {
        container: SubRecordId,
        offset: u64,
        encoding: Encoding,
    },
}

impl FieldLocation {
    /// Byte offset within whichever buffer holds this field.
    pub fn offset(&self) -> u64 {
        match self {
            Self::Direct { offset, .. } | Self::SubRecord { offset, .. } => *offset,
        }
    }

    /// Encoding of this field's raw value.
    pub fn encoding(&self) -> Encoding {
        match self {
            Self::Direct { encoding, .. } | Self::SubRecord { encoding, .. } => *encoding,
        }
    }

    /// Decode a raw byte to displayed value.
    pub fn decode_u8(&self, raw: u8) -> u8 {
        match self.encoding() {
            Encoding::Direct | Encoding::Raw => raw,
            Encoding::Encoded => (raw as u16 + 2) as u8 / 5,
            _ => raw,
        }
    }

    /// Encode a displayed value to raw byte for writing.
    pub fn encode_u8(&self, displayed: u8) -> u8 {
        match self.encoding() {
            Encoding::Direct | Encoding::Raw => displayed,
            Encoding::Encoded => displayed * 5 - 2,
            _ => displayed,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Encoding {
    Direct,      // stored = displayed (u8 1-20, or raw value)
    Encoded,     // displayed = (raw + 2) / 5
    Raw,            // u16 little-endian, no transformation (reputation, CA, PA)
    RawCapped(u16), // u16 little-endian with an explicit max (e.g. CA/PA → 200, rep → 10000)
    RawU32,         // u32 little-endian, no transformation (wages, budgets)
    DateDayYear, // [u16 day_of_year][u16 year]
    Pointer,     // u64 pointer to sub-object
    FmString,    // u64 pointer to [u32 len][UTF-8]
    F32Raw,      // f32 little-endian
    RoleIndex,   // index into [u8; 15] staff-role sub-record (offset field = byte index)
}

/// A sub-record — a block of data reached via pointer chase from the
/// entity base, optionally nested inside another sub-record.
pub struct SubRecord {
    pub id: SubRecordId,
    pub parent: SubRecordParent,
    /// Byte offset inside the parent where the 8-byte pointer lives.
    pub ptr_offset: u64,
    /// Number of bytes to read from the sub-record buffer.
    pub size: usize,
}

/// Where a sub-record's pointer lives.
pub enum SubRecordParent {
    /// Pointer lives at `entity_base + ptr_offset`.
    Direct,
    /// Pointer lives at another named sub-record's buffer + ptr_offset.
    In(SubRecordId),
}

/// Dispatch seam for records that may expose named sub-record buffers.
/// Player and staff records implement this returning `None` for every id.
/// `ClubView` implements it returning the appropriate `Option<&[u8]>` per id.
pub trait HasSubRecords {
    fn sub_record(&self, id: SubRecordId) -> Option<&[u8]>;
}

/// No-sub-records sentinel — useful for calling `read_field_*` when
/// you only have a raw buffer and no entity view (e.g., mid-construction).
impl HasSubRecords for () {
    fn sub_record(&self, _id: SubRecordId) -> Option<&[u8]> {
        None
    }
}

// ============================================================
// Shorthand constructors — used inside const field tables.
// All produce FieldLocation::Direct variants; sub-record
// variants use the sub_* constructors below.
// ============================================================

pub(crate) const fn direct(offset: u64) -> Option<FieldLocation> {
    Some(FieldLocation::Direct { offset, encoding: Encoding::Direct })
}
pub(crate) const fn encoded(offset: u64) -> Option<FieldLocation> {
    Some(FieldLocation::Direct { offset, encoding: Encoding::Encoded })
}
pub(crate) const fn raw(offset: u64) -> Option<FieldLocation> {
    Some(FieldLocation::Direct { offset, encoding: Encoding::Raw })
}
pub(crate) const fn raw_capped(offset: u64, max: u16) -> Option<FieldLocation> {
    Some(FieldLocation::Direct { offset, encoding: Encoding::RawCapped(max) })
}
pub(crate) const fn raw_u32(offset: u64) -> Option<FieldLocation> {
    Some(FieldLocation::Direct { offset, encoding: Encoding::RawU32 })
}
pub(crate) const fn date(offset: u64) -> Option<FieldLocation> {
    Some(FieldLocation::Direct { offset, encoding: Encoding::DateDayYear })
}
pub(crate) const fn pointer(offset: u64) -> Option<FieldLocation> {
    Some(FieldLocation::Direct { offset, encoding: Encoding::Pointer })
}
pub(crate) const fn fm_string(offset: u64) -> Option<FieldLocation> {
    Some(FieldLocation::Direct { offset, encoding: Encoding::FmString })
}

/// Index into the 15-byte staff-role sub-record buffer.
/// The `offset` field on the produced `FieldLocation` carries the byte index (0–14),
/// not a memory offset into `record.raw`. Rendered by `roles_column` only.
pub(crate) const fn role_index(idx: u8) -> Option<FieldLocation> {
    Some(FieldLocation::Direct { offset: idx as u64, encoding: Encoding::RoleIndex })
}

// Sub-record shorthand constructors:
pub(crate) const fn sub_direct(container: SubRecordId, offset: u64) -> Option<FieldLocation> {
    Some(FieldLocation::SubRecord { container, offset, encoding: Encoding::Direct })
}
pub(crate) const fn sub_raw(container: SubRecordId, offset: u64) -> Option<FieldLocation> {
    Some(FieldLocation::SubRecord { container, offset, encoding: Encoding::Raw })
}
pub(crate) const fn sub_raw_u32(container: SubRecordId, offset: u64) -> Option<FieldLocation> {
    Some(FieldLocation::SubRecord { container, offset, encoding: Encoding::RawU32 })
}
pub(crate) const fn sub_date(container: SubRecordId, offset: u64) -> Option<FieldLocation> {
    Some(FieldLocation::SubRecord { container, offset, encoding: Encoding::DateDayYear })
}
pub(crate) const fn sub_f32(container: SubRecordId, offset: u64) -> Option<FieldLocation> {
    Some(FieldLocation::SubRecord { container, offset, encoding: Encoding::F32Raw })
}
pub(crate) const fn sub_fm_string(container: SubRecordId, offset: u64) -> Option<FieldLocation> {
    Some(FieldLocation::SubRecord { container, offset, encoding: Encoding::FmString })
}

fn key_map(s: &'static VersionedSchemas) -> HashMap<&'static str, &'static Field> {
    s.player_fields
        .iter()
        .chain(s.staff_fields)
        .chain(s.club_fields)
        .chain(s.stadium_fields)
        .map(|f| (f.key, f))
        .collect()
}

static FM26_KEYS: LazyLock<HashMap<&'static str, &'static Field>> =
    LazyLock::new(|| key_map(&fm26::FM26));
static FM24_KEYS: LazyLock<HashMap<&'static str, &'static Field>> =
    LazyLock::new(|| key_map(&fm24::FM24));
static FM24_EPIC_KEYS: LazyLock<HashMap<&'static str, &'static Field>> =
    LazyLock::new(|| key_map(&fm24::FM24_EPIC));
static FM27_KEYS: LazyLock<HashMap<&'static str, &'static Field>> =
    LazyLock::new(|| key_map(&fm27::FM27));
static FM27_GAMEPASS_KEYS: LazyLock<HashMap<&'static str, &'static Field>> =
    LazyLock::new(|| key_map(&fm27::FM27_GAMEPASS));

fn decode_u8_enc(byte: u8, encoding: Encoding) -> u8 {
    match encoding {
        Encoding::Direct | Encoding::Raw => byte,
        Encoding::Encoded => (byte as u16 + 2) as u8 / 5,
        _ => byte,
    }
}

/// Resolve the in-process address of a named sub-record from the entity base.
///
/// Recursively chases `SubRecordParent::In(pid)` chains (finance → budget,
/// team → club) so callers never special-case a hop.
pub fn resolve_sub_ptr(
    fm: &crate::process::FmProcess,
    base_ptr: u64,
    target_id: SubRecordId,
    sub_records: &[SubRecord],
) -> Option<u64> {
    resolve_sub_ptr_mapped(fm, base_ptr, target_id, sub_records, &|o| o)
}

/// `resolve_sub_ptr` for an entity whose physical layout differs from the
/// schema's: `phys` maps a schema offset in the entity itself to its real
/// offset (player-coaches). Offsets inside sub-records are not mapped.
pub fn resolve_sub_ptr_mapped(
    fm: &crate::process::FmProcess,
    base_ptr: u64,
    target_id: SubRecordId,
    sub_records: &[SubRecord],
    phys: &dyn Fn(u64) -> u64,
) -> Option<u64> {
    if !(base_ptr > 0x10000 && base_ptr < 0x7FFFFFFFFFFF) { return None; }
    let sr = sub_records.iter().find(|s| s.id == target_id)?;
    let ptr_addr = match &sr.parent {
        SubRecordParent::Direct => base_ptr.checked_add(phys(sr.ptr_offset))?,
        SubRecordParent::In(pid) => {
            resolve_sub_ptr_mapped(fm, base_ptr, pid, sub_records, phys)?.checked_add(sr.ptr_offset)?
        }
    };
    if !(ptr_addr > 0x10000 && ptr_addr.checked_add(8)? < 0x7FFFFFFFFFFF) { return None; }
    let ptr = fm.read_u64(ptr_addr).ok()?;
    (ptr > 0x10000 && ptr < 0x7FFFFFFFFFFF).then_some(ptr)
}

/// Read an FmString field that lives in a sub-record (e.g. a player's club
/// name via team → club). `None` when the process's schema leaves the key
/// unmapped, it is not a sub-record FmString, or any hop is invalid.
pub fn read_sub_fm_string(
    fm: &crate::process::FmProcess,
    base_ptr: u64,
    key: &str,
    sub_records: &[SubRecord],
    phys: &dyn Fn(u64) -> u64,
) -> Option<String> {
    let FieldLocation::SubRecord { container, offset, encoding: Encoding::FmString } =
        fm.schemas().field(key)?.offset?
    else {
        return None;
    };
    let sub_ptr = resolve_sub_ptr_mapped(fm, base_ptr, container, sub_records, phys)?;
    let name_ptr = fm.read_u64(sub_ptr + offset).ok()?;
    fm.read_fm_name(name_ptr).ok()
}

/// Read a u8 field, dispatching on Direct vs SubRecord.
pub fn read_field_u8<V: HasSubRecords>(view: &V, raw: &[u8], f: &Field) -> Option<u8> {
    match f.offset.as_ref()? {
        FieldLocation::Direct { offset, encoding } => {
            Some(decode_u8_enc(*raw.get(*offset as usize)?, *encoding))
        }
        FieldLocation::SubRecord { container, offset, encoding } => {
            let sub = view.sub_record(container)?;
            Some(decode_u8_enc(*sub.get(*offset as usize)?, *encoding))
        }
    }
}

/// Read an unsigned integer field at the width its encoding declares:
/// one byte for `Direct`, two for `Raw`/`RawCapped`, four for `RawU32`.
/// `None` for non-integer encodings, so the same key can be a u8 in one
/// build and a u16 in another (FM24 vs FM27 international caps).
pub fn read_field_int<V: HasSubRecords>(view: &V, raw: &[u8], f: &Field) -> Option<u32> {
    match f.offset.as_ref()?.encoding() {
        Encoding::Direct => read_field_u8(view, raw, f).map(u32::from),
        Encoding::Raw | Encoding::RawCapped(_) => read_field_u16(view, raw, f).map(u32::from),
        Encoding::RawU32 => read_field_u32(view, raw, f),
        _ => None,
    }
}

/// Read a u16 field.
pub fn read_field_u16<V: HasSubRecords>(view: &V, raw: &[u8], f: &Field) -> Option<u16> {
    match f.offset.as_ref()? {
        FieldLocation::Direct { offset, .. } => {
            let off = *offset as usize;
            if off + 2 > raw.len() { return None; }
            Some(u16::from_le_bytes(raw[off..off + 2].try_into().unwrap()))
        }
        FieldLocation::SubRecord { container, offset, .. } => {
            let sub = view.sub_record(container)?;
            let off = *offset as usize;
            if off + 2 > sub.len() { return None; }
            Some(u16::from_le_bytes(sub[off..off + 2].try_into().unwrap()))
        }
    }
}

/// Read a u32 field.
pub fn read_field_u32<V: HasSubRecords>(view: &V, raw: &[u8], f: &Field) -> Option<u32> {
    match f.offset.as_ref()? {
        FieldLocation::Direct { offset, .. } => {
            let off = *offset as usize;
            if off + 4 > raw.len() { return None; }
            Some(u32::from_le_bytes(raw[off..off + 4].try_into().unwrap()))
        }
        FieldLocation::SubRecord { container, offset, .. } => {
            let sub = view.sub_record(container)?;
            let off = *offset as usize;
            if off + 4 > sub.len() { return None; }
            Some(u32::from_le_bytes(sub[off..off + 4].try_into().unwrap()))
        }
    }
}

/// Read a u64 field.
pub fn read_field_u64<V: HasSubRecords>(view: &V, raw: &[u8], f: &Field) -> Option<u64> {
    match f.offset.as_ref()? {
        FieldLocation::Direct { offset, .. } => {
            let off = *offset as usize;
            if off + 8 > raw.len() { return None; }
            Some(u64::from_le_bytes(raw[off..off + 8].try_into().unwrap()))
        }
        FieldLocation::SubRecord { container, offset, .. } => {
            let sub = view.sub_record(container)?;
            let off = *offset as usize;
            if off + 8 > sub.len() { return None; }
            Some(u64::from_le_bytes(sub[off..off + 8].try_into().unwrap()))
        }
    }
}

/// Read a f32 field (F32Raw encoding).
pub fn read_field_f32<V: HasSubRecords>(view: &V, raw: &[u8], f: &Field) -> Option<f32> {
    match f.offset.as_ref()? {
        FieldLocation::Direct { offset, .. } => {
            let off = *offset as usize;
            if off + 4 > raw.len() { return None; }
            Some(f32::from_le_bytes(raw[off..off + 4].try_into().unwrap()))
        }
        FieldLocation::SubRecord { container, offset, .. } => {
            let sub = view.sub_record(container)?;
            let off = *offset as usize;
            if off + 4 > sub.len() { return None; }
            Some(f32::from_le_bytes(sub[off..off + 4].try_into().unwrap()))
        }
    }
}

// =========================================================================
// VersionedSchemas — one static per supported game version
// =========================================================================

/// Complete field / layout / RVA schema for one game build.
///
/// There is exactly one `static` of this type per supported build
/// (`fm24::FM24`, `fm26::FM26`, `fm27::FM27`, `fm27::FM27_GAMEPASS`; see
/// `GameBuild::schemas`); `VersionedSchemas::field` relies on that address
/// identity. `FmProcess::schemas()` returns the
/// schema that matches the attached process. There is no global "active"
/// schema: every lookup goes through the schema of the process (or the
/// attached game the UI is showing) it reads.
pub struct VersionedSchemas {
    // Field lists per entity.
    pub player_fields: &'static [Field],
    pub staff_fields: &'static [Field],
    pub club_fields: &'static [Field],
    pub stadium_fields: &'static [Field],

    // Sub-record tables per entity. Empty slices for player/staff (no
    // sub-records today), populated for club.
    pub player_sub_records: &'static [SubRecord],
    pub staff_sub_records: &'static [SubRecord],
    pub club_sub_records: &'static [SubRecord],
    pub stadium_sub_records: &'static [SubRecord],

    // Struct sizes / read sizes.
    pub player_struct_size: usize,
    pub staff_struct_size: usize,
    pub club_read_size: usize,
    /// Stadium entity stride (inline array, not pointer array). FM26: 0xC0 = 192 bytes.
    pub stadium_entity_stride: usize,

    // Human-manager layout (per-version because the shifted-field set may
    // differ between games).
    pub human_manager_shift: usize,
    pub manager_shifted_fields: &'static [(usize, usize)],

    // RVAs from game_plugin.dll base.
    pub rva_humans_data: u64,
    pub rva_humans_end: u64,
    pub rva_staff_data: u64,
    pub rva_staff_end: u64,
    pub rva_clubs_data: u64,
    pub rva_clubs_end: u64,

    /// Stadium array — accessed via a 5-hop pointer chain, NOT a simple data/end pair:
    ///   GP+rva_stadium_base → container → container+0x80 → type_desc → vec_hdr → data_ptr
    ///   entity[i] = data_ptr + i * stadium_entity_stride
    /// This RVA points to the container object (first hop), not directly to the array.
    pub rva_stadium_base: u64,

    // FMRTE reference counts — used by `src/bin/scan_arrays.rs` to
    // validate candidate RVA pairs. Per-version because FMRTE's own
    // numbers shift between game versions.
    pub fmrte_players: u64,
    pub fmrte_staff: u64,
    pub fmrte_clubs: u64,
    pub fmrte_humans: u64,

    /// FNV-1a hash of the first 64 KB of `game_plugin.dll` for the patch
    /// this schema was validated against.
    ///
    /// Convention:
    ///   0  — not yet calibrated; attach is allowed but a notice is shown.
    ///   other — hard gate: attach fails if the running DLL hashes differently.
    ///
    /// To calibrate: launch the editor once with the target FM build, read
    /// the hash from the status bar notice, paste it here, commit.
    pub known_dll_hash: u64,

    /// RVA (module-relative) of the vtable of the club finance class whose
    /// layout the club `finance` sub-record fields describe. 0 = no check.
    /// FM27 has a full finance class (budgets, facility copies) for its ~900
    /// detailed clubs and a smaller class for the rest, where those offsets
    /// fall outside the object; the loader drops the finance sub-record for
    /// any club whose finance object has a different vtable.
    pub full_finance_vtable_rva: u64,

    /// Player-coach layout. A player-coach object is a player object followed
    /// by `player_coach_shift` bytes of staff data and then the person block,
    /// so every player field at or past `player_person_offset` (names, UID,
    /// DOB, nation, caps, contract pointer, ...) lives `player_coach_shift`
    /// bytes later than in a plain player. Identified by the object's vtable
    /// at `player_coach_vtable_rva` (module-relative); 0 = no player-coach
    /// handling.
    pub player_coach_vtable_rva: u64,
    pub player_person_offset: usize,
    pub player_coach_shift: usize,

    /// RVA (module-relative) of the vtable of the human-manager staff class.
    /// Staff objects of this class keep their identity fields at
    /// `offset + human_manager_shift` (see `manager_shifted_fields`), so every
    /// staff reader patches them back. 0 = only the manager found by
    /// `find_human_manager` is treated as shifted.
    pub human_manager_vtable_rva: u64,

    /// Year ages are computed against (game start season). FM26 keeps the
    /// editor's historical 2026; FM24 saves start in July 2023.
    pub age_reference_year: u16,
}

impl VersionedSchemas {
    /// Look up a single field by key across all entities of this build.
    ///
    /// The per-build schemas are backed by a lazily-built `HashMap` so
    /// repeated lookups (e.g. inside hot loops) are O(1); any other
    /// `VersionedSchemas` value falls back to a linear scan.
    pub fn field(&'static self, key: &str) -> Option<&'static Field> {
        let map = if std::ptr::eq(self, &fm26::FM26) {
            &*FM26_KEYS
        } else if std::ptr::eq(self, &fm24::FM24) {
            &*FM24_KEYS
        } else if std::ptr::eq(self, &fm24::FM24_EPIC) {
            &*FM24_EPIC_KEYS
        } else if std::ptr::eq(self, &fm27::FM27) {
            &*FM27_KEYS
        } else if std::ptr::eq(self, &fm27::FM27_GAMEPASS) {
            &*FM27_GAMEPASS_KEYS
        } else {
            return self
                .player_fields
                .iter()
                .chain(self.staff_fields)
                .chain(self.club_fields)
                .chain(self.stadium_fields)
                .find(|f| f.key == key);
        };
        map.get(key).copied()
    }

    /// Byte offset of a field, or `None` when the key is unknown or
    /// unmapped for this version.
    pub fn offset_of(&'static self, key: &str) -> Option<usize> {
        Some(self.field(key)?.offset?.offset() as usize)
    }

    /// Mapped fields for an entity + category, in schema order.
    pub fn fields_for_category(&self, entity: Entity, category: Category) -> Vec<Field> {
        self.fields_for(entity)
            .iter()
            .filter(|f| f.category == category && f.offset.is_some())
            .cloned()
            .collect()
    }

    /// Return the field slice for a given entity type on this version.
    pub fn fields_for(&self, entity: Entity) -> &'static [Field] {
        match entity {
            Entity::Player => self.player_fields,
            Entity::Staff => self.staff_fields,
            Entity::Club => self.club_fields,
            Entity::Stadium => self.stadium_fields,
        }
    }

    /// True if `rel_offset` (relative to the human-manager base) falls inside
    /// one of this version's shifted identity ranges, i.e. a write to it must
    /// go to `rel_offset + human_manager_shift`.
    pub fn is_manager_shifted_offset(&self, rel_offset: u64) -> bool {
        self.manager_shifted_fields
            .iter()
            .any(|&(off, len)| rel_offset >= off as u64 && rel_offset < (off + len) as u64)
    }
}

#[cfg(test)]
mod schema_lookup_tests {
    use crate::game_version::GameBuild;

    use super::*;

    #[test]
    fn field_lookup_returns_that_builds_entry() {
        for build in GameBuild::ALL {
            let schemas = build.schemas();
            let hashed = schemas.field("p_ca").unwrap();
            let scanned = schemas.player_fields.iter().find(|f| f.key == "p_ca").unwrap();
            assert!(std::ptr::eq(hashed, scanned), "{build:?}");
            assert_eq!(
                schemas.offset_of("p_ca"),
                Some(scanned.offset.unwrap().offset() as usize)
            );
        }
        assert!(GameBuild::Fm26Steam.schemas().field("no_such_key").is_none());
    }

    #[test]
    fn int_fields_read_at_their_encoding_width() {
        // FM24 stores international caps as a u8 followed by U21 caps; FM27
        // widens caps to u16. Reading by encoding must not run into the next byte.
        let fm24 = GameBuild::Fm24Steam.schemas();
        let off = fm24.offset_of("p_intl_caps").unwrap();
        let mut raw = vec![0u8; off + 4];
        raw[off] = 84;
        raw[off + 1] = 14;
        assert_eq!(read_field_int(&(), &raw, fm24.field("p_intl_caps").unwrap()), Some(84));

        let fm27 = GameBuild::Fm27Steam.schemas();
        let off = fm27.offset_of("p_intl_caps").unwrap();
        let mut raw = vec![0u8; off + 2];
        raw[off..off + 2].copy_from_slice(&300u16.to_le_bytes());
        assert_eq!(read_field_int(&(), &raw, fm27.field("p_intl_caps").unwrap()), Some(300));
    }

    #[test]
    fn category_lookup_keeps_only_mapped_fields_of_that_category() {
        for build in GameBuild::ALL {
            let fields =
                build.schemas().fields_for_category(Entity::Player, Category::Technical);
            assert!(!fields.is_empty(), "{build:?}");
            assert!(fields.iter().all(|f| f.entity == Entity::Player
                && f.category == Category::Technical
                && f.offset.is_some()));
        }
    }
}
