//! Player field definitions for FM24. Aggregated via `fields::fm24::PLAYER_FIELDS`.
//!
//! Mapped 2026-10-22 against a fresh FM24 save (3 Jul 2023). Player objects are
//! 0x3E8 bytes (pool stride), vtable fm.exe+0x5A4E958. The position + attribute
//! block keeps the FM26 order shifted by +0xB8 (positions 0x208..0x216,
//! attributes 0x217..0x24C); identity fields moved individually.
//!
//! Evidence: Harry Kane (UID 28049320, DOB day 209/1993, height 188, club ptr ->
//! Tottenham, positions ST 20 / AMC 15 / MC+AML 12 / AMR 10, name subs
//! "Harry"/"Kane") plus a 1500-player sample: GK position vs handling r=0.93,
//! handling/reflexes r=0.89, DL/WBL r=0.82, every attribute byte <= 100,
//! max(left, right foot) = 20 for 100%, CA <= PA, world rep <= home rep 99.7%.

use crate::fields::{date, direct, encoded, fm_string, pointer, raw, raw_u32, sub_date, sub_direct, sub_fm_string, sub_raw_u32, Field, SubRecord, SubRecordParent};
use crate::fields::Category::*;
use crate::fields::Entity::Player;

/// Player sub-records, resolved by `player::read_player_at`. Located from
/// FM27€0's FM24 table (leads) and confirmed 2026-10-30 against Harry Kane,
/// Lionel Messi, Cristiano Ronaldo, Kevin De Bruyne and Joe Ward; see
/// docs/fm24/memory-layout.md.
pub(crate) const PLAYER_SUB_RECORDS: &[SubRecord] = &[
    // Contract (player+0x340): wage, expiry, squad status and number match
    // the five witnesses (Kane 30 Jun 2027 / #9, Messi 31 Dec 2025 / #10,
    // Ronaldo 30 Jun 2025 / #7, De Bruyne 30 Jun 2025 / #17).
    SubRecord { id: "contract",   parent: SubRecordParent::Direct,     ptr_offset: 0x340, size: 0x70 },
    // Team the player plays for (player+0x130); its club at team+0x30.
    // FC Bayern, Inter Miami, Al-Nassr, Man City, Derby.
    SubRecord { id: "team",       parent: SubRecordParent::Direct,     ptr_offset: 0x130, size: 0xB0 },
    SubRecord { id: "club",       parent: SubRecordParent::In("team"), ptr_offset: 0x30,  size: 0xD0 },
    // Nation object (player+0x2E8); name FmString at nation+0x18.
    // England, Argentina, Portugal, Belgium, England.
    SubRecord { id: "nation",     parent: SubRecordParent::Direct,     ptr_offset: 0x2E8, size: 0x20 },
    // Place-of-birth city (player+0x300); name FmString at city+0x18.
    // London, Rosario, Funchal, Ghent, Chelmsford.
    SubRecord { id: "birth_city", parent: SubRecordParent::Direct,     ptr_offset: 0x300, size: 0x20 },
];

pub(crate) const FIELDS: &[Field] = &[
    Field { key: "crossing",       label: "Crossing",        entity: Player, category: Technical,   offset: encoded(0x0217) },
    Field { key: "dribbling",      label: "Dribbling",       entity: Player, category: Technical,   offset: encoded(0x0218) },
    Field { key: "finishing",      label: "Finishing",        entity: Player, category: Technical,   offset: encoded(0x0219) },
    Field { key: "heading",        label: "Heading",          entity: Player, category: Technical,   offset: encoded(0x021A) },
    Field { key: "long_shots",     label: "Long Shots",       entity: Player, category: Technical,   offset: encoded(0x021B) },
    Field { key: "marking",        label: "Marking",          entity: Player, category: Technical,   offset: encoded(0x021C) },
    Field { key: "passing",        label: "Passing",          entity: Player, category: Technical,   offset: encoded(0x021E) },
    Field { key: "tackling",       label: "Tackling",         entity: Player, category: Technical,   offset: encoded(0x0220) },
    Field { key: "technique",      label: "Technique",        entity: Player, category: Technical,   offset: encoded(0x022E) },
    Field { key: "first_touch",    label: "First Touch",      entity: Player, category: Technical,   offset: encoded(0x022D) },
    Field { key: "corners",        label: "Corners",          entity: Player, category: SetPieces,   offset: encoded(0x0232) },
    Field { key: "free_kicks",     label: "Free Kicks",       entity: Player, category: SetPieces,   offset: encoded(0x023A) },
    Field { key: "long_throws",    label: "Long Throws",      entity: Player, category: SetPieces,   offset: encoded(0x0235) },
    Field { key: "penalty_taking", label: "Penalty Taking",   entity: Player, category: SetPieces,   offset: encoded(0x021F) },
    Field { key: "aggression",     label: "Aggression",       entity: Player, category: Mental,      offset: encoded(0x0244) },
    Field { key: "anticipation",   label: "Anticipation",     entity: Player, category: Mental,      offset: encoded(0x0228) },
    Field { key: "bravery",        label: "Bravery",          entity: Player, category: Mental,      offset: encoded(0x0242) },
    Field { key: "composure",      label: "Composure",        entity: Player, category: Mental,      offset: encoded(0x024B) },
    Field { key: "concentration",  label: "Concentration",    entity: Player, category: Mental,      offset: encoded(0x024C) },
    Field { key: "decisions",      label: "Decisions",        entity: Player, category: Mental,      offset: encoded(0x0229) },
    Field { key: "flair",          label: "Flair",            entity: Player, category: Mental,      offset: encoded(0x0231) },
    Field { key: "leadership",     label: "Leadership",       entity: Player, category: Mental,      offset: encoded(0x023F) },
    Field { key: "off_the_ball",   label: "Off The Ball",     entity: Player, category: Mental,      offset: encoded(0x021D) },
    Field { key: "positioning",    label: "Positioning",      entity: Player, category: Mental,      offset: encoded(0x022B) },
    Field { key: "teamwork",       label: "Teamwork",         entity: Player, category: Mental,      offset: encoded(0x0233) },
    Field { key: "vision",         label: "Vision",           entity: Player, category: Mental,      offset: encoded(0x0221) },
    Field { key: "work_rate",      label: "Work Rate",        entity: Player, category: Mental,      offset: encoded(0x0234) },
    Field { key: "determination",  label: "Determination",    entity: Player, category: Mental,      offset: encoded(0x024A) },
    Field { key: "acceleration",   label: "Acceleration",     entity: Player, category: Physical,    offset: encoded(0x0239) },
    Field { key: "agility",        label: "Agility",          entity: Player, category: Physical,    offset: encoded(0x0245) },
    Field { key: "balance",        label: "Balance",          entity: Player, category: Physical,    offset: encoded(0x0241) },
    Field { key: "jumping_reach",  label: "Jumping Reach",    entity: Player, category: Physical,    offset: encoded(0x023E) },
    Field { key: "natural_fitness",label: "Natural Fitness",   entity: Player, category: Physical,    offset: encoded(0x0249) },
    Field { key: "pace",           label: "Pace",             entity: Player, category: Physical,    offset: encoded(0x023D) },
    Field { key: "stamina",        label: "Stamina",          entity: Player, category: Physical,    offset: encoded(0x023C) },
    Field { key: "strength",       label: "Strength",         entity: Player, category: Physical,    offset: encoded(0x023B) },
    Field { key: "handling",         label: "Handling",         entity: Player, category: Goalkeeping, offset: encoded(0x0222) },
    Field { key: "aerial_reach",     label: "Aerial Reach",     entity: Player, category: Goalkeeping, offset: encoded(0x0223) },
    Field { key: "command_of_area",  label: "Command of Area",  entity: Player, category: Goalkeeping, offset: encoded(0x0224) },
    Field { key: "communication",    label: "Communication",    entity: Player, category: Goalkeeping, offset: encoded(0x0225) },
    Field { key: "kicking",          label: "Kicking",          entity: Player, category: Goalkeeping, offset: encoded(0x0226) },
    Field { key: "throwing",         label: "Throwing",         entity: Player, category: Goalkeeping, offset: encoded(0x0227) },
    Field { key: "one_on_ones",      label: "One on Ones",      entity: Player, category: Goalkeeping, offset: encoded(0x022A) },
    Field { key: "reflexes",         label: "Reflexes",         entity: Player, category: Goalkeeping, offset: encoded(0x022C) },
    Field { key: "eccentricity",     label: "Eccentricity",     entity: Player, category: Goalkeeping, offset: encoded(0x0236) },
    Field { key: "rushing_out",      label: "Rushing Out",      entity: Player, category: Goalkeeping, offset: encoded(0x0237) },
    Field { key: "punching",         label: "Punching",         entity: Player, category: Goalkeeping, offset: encoded(0x0238) },
    Field { key: "left_foot",      label: "Left Foot",        entity: Player, category: Footedness,  offset: encoded(0x022F) },
    Field { key: "right_foot",     label: "Right Foot",       entity: Player, category: Footedness,  offset: encoded(0x0230) },
    Field { key: "consistency",       label: "Consistency",       entity: Player, category: Hidden,   offset: encoded(0x0243) },
    Field { key: "injury_proneness",  label: "Injury Proneness",  entity: Player, category: Hidden,   offset: encoded(0x0247) },
    Field { key: "important_matches", label: "Important Matches", entity: Player, category: Hidden,   offset: encoded(0x0246) },
    Field { key: "p_dirtiness",       label: "Dirtiness",         entity: Player, category: Hidden,   offset: encoded(0x0240) },
    Field { key: "p_versatility",     label: "Versatility",       entity: Player, category: Hidden,   offset: encoded(0x0248) },
    Field { key: "p_adaptability",    label: "Adaptability",     entity: Player, category: Personality, offset: direct(0x02F0) },
    Field { key: "p_ambition",        label: "Ambition",         entity: Player, category: Personality, offset: direct(0x02F1) },
    Field { key: "p_loyalty",         label: "Loyalty",          entity: Player, category: Personality, offset: direct(0x02F2) },
    Field { key: "p_pressure",        label: "Pressure",         entity: Player, category: Personality, offset: direct(0x02F3) },
    Field { key: "p_professionalism", label: "Professionalism",  entity: Player, category: Personality, offset: direct(0x02F4) },
    Field { key: "p_sportsmanship",   label: "Sportsmanship",    entity: Player, category: Personality, offset: direct(0x02F5) },
    Field { key: "p_temperament",     label: "Temperament",      entity: Player, category: Personality, offset: direct(0x02F6) },
    Field { key: "p_controversy",     label: "Controversy",      entity: Player, category: Personality, offset: direct(0x02F7) },
    Field { key: "p_name",             label: "Name",             entity: Player, category: General,  offset: fm_string(0x02C0) },
    Field { key: "p_name_first_sub",   label: "Name First Sub",   entity: Player, category: Internal, offset: pointer(0x02D0) },
    Field { key: "p_name_last_sub",    label: "Name Last Sub",     entity: Player, category: Internal, offset: pointer(0x02D8) },
    Field { key: "p_weight",       label: "Weight",           entity: Player, category: General,     offset: raw(0x014C) },
    Field { key: "p_height",       label: "Height",           entity: Player, category: General,     offset: raw(0x014E) },
    Field { key: "p_dob",          label: "Date of Birth",    entity: Player, category: General,     offset: date(0x02BC) },
    Field { key: "p_ethnicity",    label: "Ethnicity",        entity: Player, category: General,     offset: direct(0x0309) },
    Field { key: "p_hair_colour",  label: "Hair Colour",      entity: Player, category: General,     offset: direct(0x030A) },
    Field { key: "p_hair_length",  label: "Hair Length",      entity: Player, category: General,     offset: direct(0x030B) },
    Field { key: "p_skin_tone",    label: "Skin Tone",        entity: Player, category: General,     offset: direct(0x030C) },
    Field { key: "p_asking_price", label: "Asking Price",     entity: Player, category: Contract,    offset: raw_u32(0x01D4) },
    Field { key: "p_club_ptr",       label: "Club Entity",       entity: Player, category: Internal,    offset: pointer(0x03B0) },
    Field { key: "p_ca",           label: "Current Ability",  entity: Player, category: Ability,     offset: raw(0x0200) },
    Field { key: "p_pa",           label: "Potential Ability", entity: Player, category: Ability,     offset: raw(0x0202) },
    Field { key: "p_home_rep",     label: "Home Rep",         entity: Player, category: Reputation,  offset: raw(0x01FA) },
    Field { key: "p_current_rep",  label: "Current Rep",      entity: Player, category: Reputation,  offset: raw(0x01FC) },
    Field { key: "p_world_rep",    label: "World Rep",        entity: Player, category: Reputation,  offset: raw(0x01FE) },
    Field { key: "pos_gk",  label: "GK",  entity: Player, category: Positions, offset: direct(0x0208) },
    Field { key: "pos_sw",  label: "SW",  entity: Player, category: Positions, offset: direct(0x0209) },
    Field { key: "pos_dl",  label: "DL",  entity: Player, category: Positions, offset: direct(0x020A) },
    Field { key: "pos_dc",  label: "DC",  entity: Player, category: Positions, offset: direct(0x020B) },
    Field { key: "pos_dr",  label: "DR",  entity: Player, category: Positions, offset: direct(0x020C) },
    Field { key: "pos_dm",  label: "DM",  entity: Player, category: Positions, offset: direct(0x020D) },
    Field { key: "pos_ml",  label: "ML",  entity: Player, category: Positions, offset: direct(0x020E) },
    Field { key: "pos_mc",  label: "MC",  entity: Player, category: Positions, offset: direct(0x020F) },
    Field { key: "pos_mr",  label: "MR",  entity: Player, category: Positions, offset: direct(0x0210) },
    Field { key: "pos_aml", label: "AML", entity: Player, category: Positions, offset: direct(0x0211) },
    Field { key: "pos_amc", label: "AMC", entity: Player, category: Positions, offset: direct(0x0212) },
    Field { key: "pos_amr", label: "AMR", entity: Player, category: Positions, offset: direct(0x0213) },
    Field { key: "pos_st",  label: "ST",  entity: Player, category: Positions, offset: direct(0x0214) },
    Field { key: "pos_wbl", label: "WBL", entity: Player, category: Positions, offset: direct(0x0215) },
    Field { key: "pos_wbr", label: "WBR", entity: Player, category: Positions, offset: direct(0x0216) },
    Field { key: "p_uid",          label: "UID",              entity: Player, category: General,     offset: raw(0x0284) },
    // International record: four u8 in a row (FM27 widens caps/goals to u16).
    // Kane 84 caps / 58 goals, U21 14 / 8; Messi 175 / 103, U20 18 / 14;
    // Ronaldo 200 / 123, U21 10 / 3; De Bruyne 99 / 26.
    Field { key: "p_intl_caps",    label: "International Caps",  entity: Player, category: General,  offset: direct(0x03DC) },
    Field { key: "p_u21_caps",     label: "U21 Caps",            entity: Player, category: General,  offset: direct(0x03DD) },
    Field { key: "p_intl_goals",   label: "International Goals", entity: Player, category: General,  offset: direct(0x03DE) },
    Field { key: "p_u21_goals",    label: "U21 Goals",           entity: Player, category: General,  offset: direct(0x03DF) },
    Field { key: "p_team_ptr",         label: "Team",             entity: Player, category: Internal,   offset: pointer(0x0130) },
    Field { key: "p_current_club",     label: "Club",             entity: Player, category: General,    offset: sub_fm_string("club", 0xC0) },
    Field { key: "p_nation_ptr",       label: "Nation",           entity: Player, category: Internal,   offset: pointer(0x02E8) },
    Field { key: "p_nationality",      label: "Nationality",      entity: Player, category: General,    offset: sub_fm_string("nation", 0x18) },
    Field { key: "p_birthplace",       label: "Place of Birth",   entity: Player, category: General,    offset: sub_fm_string("birth_city", 0x18) },
    Field { key: "p_contract_ptr",     label: "Contract",         entity: Player, category: Internal,   offset: pointer(0x0340) },
    Field { key: "p_contract_wage",    label: "Wage (£ p/w)",     entity: Player, category: Contract,   offset: sub_raw_u32("contract", 0x18) },
    Field { key: "p_contract_expiry",  label: "Contract Expiry",  entity: Player, category: Contract,   offset: sub_date("contract", 0x40) },
    Field { key: "p_squad_status",     label: "Squad Status",     entity: Player, category: Contract,   offset: sub_direct("contract", 0x4C) },
    Field { key: "p_list_flags",       label: "Listing Flags",    entity: Player, category: Contract,   offset: sub_direct("contract", 0x4F) },
    Field { key: "p_squad_number",     label: "Squad Number",     entity: Player, category: Contract,   offset: sub_direct("contract", 0x55) },

    // Condition, morale, and traits (0x1F4..0x1F8, 0x25E, 0x368).
    Field { key: "p_sharpness",    label: "Sharpness",        entity: Player, category: Condition,   offset: raw(0x01F4) },
    Field { key: "p_fatigue",      label: "Fatigue",           entity: Player, category: Condition,   offset: raw(0x01F6) },
    Field { key: "p_fitness",      label: "Fitness",           entity: Player, category: Condition,   offset: raw(0x01F8) },
    Field { key: "p_name2",            label: "Name (secondary)",  entity: Player, category: Internal, offset: None },
    Field { key: "p_name2_first_sub",  label: "Name2 First Sub",   entity: Player, category: Internal, offset: None },
    Field { key: "p_name2_last_sub",   label: "Name2 Last Sub",    entity: Player, category: Internal, offset: None },
    Field { key: "p_value",        label: "Market Value",     entity: Player, category: General,     offset: None },
    Field { key: "p_weekly_wage",  label: "Weekly Wage",      entity: Player, category: General,     offset: None },
    Field { key: "p_senior_apps",  label: "Snr Apps",         entity: Player, category: General,     offset: None },
    Field { key: "p_senior_goals", label: "Snr Goals",        entity: Player, category: General,     offset: None },
    Field { key: "p_prev_club",    label: "Previous Club",    entity: Player, category: General,     offset: None },
    Field { key: "p_transfer_date",  label: "Transfer Date",    entity: Player, category: General,     offset: None },
    Field { key: "p_contract_start", label: "Contract Start",   entity: Player, category: Contract,    offset: None },
    Field { key: "p_join_date",      label: "Join Club Date",   entity: Player, category: Contract,    offset: None },
    Field { key: "p_debut_date",     label: "Senior Debut",     entity: Player, category: General,     offset: None },
    Field { key: "p_listing_date",   label: "Listing Date",     entity: Player, category: Contract,    offset: None },
    Field { key: "p_squad_ptr",      label: "Squad Record",      entity: Player, category: Internal,    offset: None },
    Field { key: "p_contract_type",    label: "Contract Type",    entity: Player, category: Contract,   offset: None     },
    Field { key: "p_contract_signed",  label: "Contract Signed",  entity: Player, category: Contract,   offset: None    },
    Field { key: "p_morale",       label: "Morale",           entity: Player, category: Condition,   offset: direct(0x025E) },
    Field { key: "p_traits",       label: "Traits (PPMs)",    entity: Player, category: Traits,      offset: raw(0x0368) },
];
