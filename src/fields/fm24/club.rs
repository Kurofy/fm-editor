//! Club field definitions for FM24. Aggregated via `fields::fm24::CLUB_FIELDS`
//! and `fields::fm24::CLUB_SUB_RECORDS`.
//!
//! Club objects are 0x170 bytes (pool spacing), vtable fm.exe+0x5A51F38.
//! Top-level identity and the finance / budget / attendance sub-records match
//! FM26; the info sub-record layout is FM26 shifted by +8.
//!
//! Evidence (FC Bayern UID 915, Tottenham UID 728): names, abbreviations
//! FCB/FCB/BAY and Spurs/TOT/THFC, founded 1900/1882, facilities 19/19/16/18/17
//! and 20/20/16/16/18, supporter percentages summing to 100, supporter
//! attributes identical to the FM26 witnesses, attendance 75024/72000/93780,
//! Spurs ticket prices 65 / 1296.

use crate::fields::{fm_string, pointer, raw, sub_direct, sub_f32, sub_fm_string, sub_raw, Field, SubRecord, SubRecordParent};
use crate::fields::Category::*;
use crate::fields::Entity::Club;

/// Sub-records reachable from the club base pointer. Parents before children.
pub(crate) const CLUB_SUB_RECORDS: &[SubRecord] = &[
    SubRecord { id: "info",         parent: SubRecordParent::Direct,            ptr_offset: 0x100, size: 0x138 },
    SubRecord { id: "finance",      parent: SubRecordParent::Direct,            ptr_offset: 0x150, size: 0x830 },
    SubRecord { id: "budget",       parent: SubRecordParent::In("finance"),     ptr_offset: 0x48,  size: 0x20  },
    SubRecord { id: "stadium",      parent: SubRecordParent::Direct,            ptr_offset: 0xB0,  size: 0x98  },
    // Home stadium: club+0x18 team vector -> [0] senior team (team UID == club
    // UID for Bayern/Spurs) -> +0x78 stadium entity. Same chain as FM26.
    SubRecord { id: "teams",        parent: SubRecordParent::Direct,            ptr_offset: 0x18,  size: 0x08  },
    SubRecord { id: "senior_team",  parent: SubRecordParent::In("teams"),       ptr_offset: 0x00,  size: 0x80  },
    SubRecord { id: "home_stadium", parent: SubRecordParent::In("senior_team"), ptr_offset: 0x78,  size: 0xB0  },
];

pub(crate) const FIELDS: &[Field] = &[
    Field { key: "c_uid",                label: "UID",                entity: Club, category: General,    offset: raw(0x0C)      },
    Field { key: "c_name",               label: "Name",               entity: Club, category: General,    offset: fm_string(0xC8)},
    Field { key: "c_full_name",          label: "Full Name",          entity: Club, category: General,    offset: fm_string(0xC0)},
    Field { key: "c_stadium",            label: "Stadium",            entity: Club, category: General,    offset: pointer(0xB0)  },
    Field { key: "c_index",              label: "Array Index",        entity: Club, category: General,    offset: raw(0x08)      },
    Field { key: "c_abbrev1",            label: "Abbreviation 1",     entity: Club, category: General,    offset: sub_fm_string("info", 0x00) },
    Field { key: "c_abbrev2",            label: "Abbreviation 2",     entity: Club, category: General,    offset: sub_fm_string("info", 0x08) },
    Field { key: "c_abbrev3",            label: "Abbreviation 3",     entity: Club, category: General,    offset: sub_fm_string("info", 0x10) },
    Field { key: "c_year_founded",       label: "Year Founded",       entity: Club, category: General,    offset: sub_raw("info", 0xD8) },
    Field { key: "c_owner_status",       label: "Owner Status",       entity: Club, category: General,    offset: sub_raw("info", 0xDE) },
    Field { key: "c_season_tickets",     label: "Season Tickets",     entity: Club, category: General,    offset: sub_raw("info", 0xC0) },
    Field { key: "c_social_media",       label: "Social Media",       entity: Club, category: General,    offset: sub_raw("info", 0xF8) },
    // Arsenal (FM24 Club Info > Facilities, 2026-10-30): info+0x120 = 19
    // "Superb training facilities", +0x12B = 16 "Excellent youth facilities";
    // junior coaching 17 "Excellent" and youth recruitment 15 "Good" fit the
    // same word scale. Was swapped before; FM27 uses the same two offsets.
    Field { key: "c_training",           label: "Training",           entity: Club, category: Facilities, offset: sub_direct("info", 0x120)},
    Field { key: "c_youth_fac",          label: "Youth Fac",          entity: Club, category: Facilities, offset: sub_direct("info", 0x12B)},
    Field { key: "c_youth_improve",      label: "Youth Improvement",  entity: Club, category: Facilities, offset: sub_direct("info", 0x126)},
    Field { key: "c_youth_coach",        label: "Youth Coaching",     entity: Club, category: Facilities, offset: sub_direct("info", 0x12C)},
    Field { key: "c_youth_recruit",      label: "Youth Recruitment",  entity: Club, category: Facilities, offset: sub_direct("info", 0x12D)},
    Field { key: "c_youth_academy",      label: "Youth Academy",      entity: Club, category: Facilities, offset: sub_direct("info", 0x121)},
    Field { key: "c_sup_hardcore",    label: "Hardcore %",    entity: Club, category: General, offset: sub_direct("info", 0x106)  },
    Field { key: "c_sup_core",        label: "Core %",        entity: Club, category: General, offset: sub_direct("info", 0x107)  },
    Field { key: "c_sup_family",      label: "Family %",      entity: Club, category: General, offset: sub_direct("info", 0x108) },
    Field { key: "c_sup_fair_weather",label: "Fair Weather %",entity: Club, category: General, offset: sub_direct("info", 0x109) },
    Field { key: "c_sup_casual",      label: "Casual %",      entity: Club, category: General, offset: sub_direct("info", 0x10A) },
    Field { key: "c_sup_other",       label: "Other %",       entity: Club, category: General, offset: sub_direct("info", 0x10B) },
    Field { key: "c_sup_loyalty",     label: "Loyalty",       entity: Club, category: General, offset: sub_direct("info", 0x114) },
    Field { key: "c_sup_passion",     label: "Passion",       entity: Club, category: General, offset: sub_direct("info", 0x115) },
    Field { key: "c_sup_patience",    label: "Patience",      entity: Club, category: General, offset: sub_direct("info", 0x116) },
    Field { key: "c_sup_affluence",   label: "Affluence",     entity: Club, category: General, offset: sub_direct("info", 0x117) },
    Field { key: "c_sup_temperament", label: "Temperament",   entity: Club, category: General, offset: sub_direct("info", 0x118) },
    Field { key: "c_sup_expectations",label: "Expectations",  entity: Club, category: General, offset: sub_direct("info", 0x119) },
    Field { key: "c_balance",            label: "Balance",            entity: Club, category: Finance,    offset: sub_raw("finance", 0x14) },
    Field { key: "c_avg_ticket",         label: "Avg Ticket Price",   entity: Club, category: Finance,    offset: sub_f32("finance", 0x18) },
    Field { key: "c_avg_season_ticket",  label: "Avg Season Ticket",  entity: Club, category: Finance,    offset: sub_f32("finance", 0x1C) },
    Field { key: "c_transfer_budget",    label: "Transfer Budget",    entity: Club, category: Finance,    offset: sub_raw("finance", 0x7CC) },
    Field { key: "c_transfer_remaining", label: "Transfer Remaining", entity: Club, category: Finance,    offset: sub_raw("finance", 0x7D0) },
    // Wage budget i32 £/week in the finance object. Spurs' finance screen
    // (2026-10-30): Wage Budget £3,421,987 p/w = finance+0x810, Currently
    // spending £3,271,987 = +0x81C, Next season's minimum guaranteed transfer
    // budget £25.92M = +0x7D4. budget+0x0C (used before) read 3,464,579 for
    // Spurs; it equals +0x810 for Arsenal, Barcelona and Derby only.
    Field { key: "c_wage_budget",        label: "Wage Budget",        entity: Club, category: Finance,    offset: sub_raw("finance", 0x810) },
    Field { key: "c_wage_spending",      label: "Currently Spending", entity: Club, category: Finance,    offset: sub_raw("finance", 0x81C) },
    Field { key: "c_transfer_next_season", label: "Next Season Min Transfer Budget", entity: Club, category: Finance, offset: sub_raw("finance", 0x7D4) },
    Field { key: "c_avg_attendance",     label: "Avg Attendance",     entity: Club, category: General,    offset: sub_raw("stadium", 0x68) },
    Field { key: "c_min_attendance",     label: "Min Attendance",     entity: Club, category: General,    offset: sub_raw("stadium", 0x6C) },
    Field { key: "c_max_attendance",     label: "Max Attendance",     entity: Club, category: General,    offset: sub_raw("stadium", 0x70) },

    // Not yet located in FM24. c_gender: +0x161 holds 0/2/4/8/10/32 here, not
    // the FM26 0/1 flag. c_nickname: stadium-sub +0x90 is a short name in FM24.
    Field { key: "c_gender",             label: "Gender",             entity: Club, category: General,    offset: None     },
    Field { key: "c_nickname",           label: "Nickname",           entity: Club, category: General,    offset: None },
    Field { key: "c_team_uid",           label: "Team UID",           entity: Club, category: General,    offset: None },
    Field { key: "c_stadium_entity_ptr", label: "Training Ground",    entity: Club, category: Internal,   offset: None },
    Field { key: "c_reputation",         label: "Reputation",         entity: Club, category: General,    offset: None           },
    Field { key: "c_nation",             label: "Nation",             entity: Club, category: General,    offset: None           },
    Field { key: "c_stand_type_1",       label: "Stand Type 1",       entity: Club, category: General,    offset: None },
    Field { key: "c_stand_type_2",       label: "Stand Type 2",       entity: Club, category: General,    offset: None },
    Field { key: "c_stand_type_3",       label: "Stand Type 3",       entity: Club, category: General,    offset: None },
    Field { key: "c_stand_type_4",       label: "Stand Type 4",       entity: Club, category: General,    offset: None },
];
