//! Club affiliations: parent/feeder relationships, loan limits, fees, and friendship benefits.

use serde::{Deserialize, Serialize};

/// Type of affiliation between two clubs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum AffiliationType {
    #[default]
    Normal,         // 0: Normal Affiliated Club
    ParentClub,     // 1: Parent Club
    FeederClub,     // 2: Feeder Club
    BClub,          // 3: B Club
    GoodRelations,  // 4: Good Relations
    WomensClub,     // 5: Women's Club
    FirstOptionBuy, // 6: First Option to Buy
    Commercial,     // 7: Commercial Link
    WorkPermit,     // 8: Work Permit Link
    LocalPartner,   // 9: Local Partnership
    Other(u32),     // Custom/unmapped
}

impl AffiliationType {
    pub const ALL: &[Self] = &[
        Self::BClub,
        Self::Normal,
        Self::GoodRelations,
        Self::WomensClub,
        Self::ParentClub,
        Self::FeederClub,
        Self::FirstOptionBuy,
        Self::Commercial,
        Self::WorkPermit,
        Self::LocalPartner,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Normal => "Normal Affiliated Club",
            Self::ParentClub => "Parent Club",
            Self::FeederClub => "Feeder Club",
            Self::BClub => "B Club",
            Self::GoodRelations => "Good Relations",
            Self::WomensClub => "Women's Club",
            Self::FirstOptionBuy => "First Option to Buy",
            Self::Commercial => "Commercial Link",
            Self::WorkPermit => "Work Permit Link",
            Self::LocalPartner => "Local Partnership",
            Self::Other(_) => "Custom Affiliation",
        }
    }

    pub fn from_u32(val: u32) -> Self {
        match val {
            0 => Self::Normal,
            1 => Self::ParentClub,
            2 => Self::FeederClub,
            3 => Self::BClub,
            4 => Self::GoodRelations,
            5 => Self::WomensClub,
            6 => Self::FirstOptionBuy,
            7 => Self::Commercial,
            8 => Self::WorkPermit,
            9 => Self::LocalPartner,
            other => Self::Other(other),
        }
    }

    pub fn to_u32(self) -> u32 {
        match self {
            Self::Normal => 0,
            Self::ParentClub => 1,
            Self::FeederClub => 2,
            Self::BClub => 3,
            Self::GoodRelations => 4,
            Self::WomensClub => 5,
            Self::FirstOptionBuy => 6,
            Self::Commercial => 7,
            Self::WorkPermit => 8,
            Self::LocalPartner => 9,
            Self::Other(val) => val,
        }
    }
}

/// 17 clauses/benefits that can be agreed between affiliated clubs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct FriendshipBenefits {
    pub flags: u32,
}

impl FriendshipBenefits {
    pub const PLAYERS_MOVE_FREELY: u32             = 1 << 0;
    pub const FIRST_OPTION_TO_BUY: u32             = 1 << 1;
    pub const YOUTH_PLAYERS_TRAINING: u32          = 1 << 2;
    pub const SAME_BOARD: u32                      = 1 << 3;
    pub const HELP_WITH_FACILITIES: u32            = 1 << 4;
    pub const MARKETING_RELATIONS: u32             = 1 << 5;
    pub const CANNOT_PLAY_IN_SAME_DIVISION: u32    = 1 << 6;
    pub const LOAN_PLAYERS_FROM_SUB_TEAM: u32      = 1 << 7;
    pub const USES_SAME_KIT: u32                   = 1 << 8;
    pub const PLAYERS_GO_ON_LOAN: u32              = 1 << 9;
    pub const PLAY_FRIENDLIES: u32                 = 1 << 10;
    pub const PERMANENT_DEAL: u32                  = 1 << 11;
    pub const FINANCIAL_HELP: u32                  = 1 << 12;
    pub const RENEWABLE: u32                       = 1 << 13;
    pub const SCOUTING_KNOWLEDGE_SHARED: u32       = 1 << 14;
    pub const USES_ALL_PARENT_FACILITIES: u32      = 1 << 15;
    pub const USES_SUB_TEAM_TRAINING: u32          = 1 << 16;

    pub fn has(&self, flag: u32) -> bool {
        (self.flags & flag) != 0
    }

    pub fn set(&mut self, flag: u32, value: bool) {
        if value {
            self.flags |= flag;
        } else {
            self.flags &= !flag;
        }
    }
}

/// Information and rules for an affiliation between two clubs.
#[derive(Clone, Debug, PartialEq)]
pub struct AffiliationView {
    /// In-memory address of this affiliation object (0 if standalone/mock).
    pub addr: u64,
    /// Parent club UID.
    pub parent_club_uid: u32,
    /// Parent club display name.
    pub parent_club_name: String,
    /// Feeder club UID.
    pub feeder_club_uid: u32,
    /// Feeder club display name.
    pub feeder_club_name: String,
    /// Start date (day of year 1..366, year).
    pub start_doy: u16,
    pub start_year: u16,
    /// End date (day of year 1..366, year). Year 1900 indicates permanent.
    pub end_doy: u16,
    pub end_year: u16,
    /// Affiliation type category.
    pub affiliation_type: AffiliationType,
    /// Annual fee paid by parent to feeder in GBP.
    pub annual_fee: i32,
    /// Whether percentage chance of playing a friendly is configured.
    pub friendly_chance_enabled: bool,
    /// % Chance of friendly (0-100).
    pub friendly_chance: u8,
    /// Whether loan player limit is configured.
    pub max_loaned_enabled: bool,
    /// Maximum number of players loaned (0-99).
    pub max_loaned: u8,
    /// Whether loan player wage percentage is configured.
    pub wage_percentage_enabled: bool,
    /// Percentage of loan player wage paid by parent club (0-100).
    pub wage_percentage: u8,
    /// Friendship benefits bitfield.
    pub benefits: FriendshipBenefits,
    /// Retained raw memory buffer (for byte-level diffs and write-back safety).
    pub raw: Vec<u8>,
}

impl AffiliationView {
    pub fn new_default(club_uid: u32, club_name: &str) -> Self {
        Self {
            addr: 0,
            parent_club_uid: club_uid,
            parent_club_name: club_name.to_string(),
            feeder_club_uid: 0,
            feeder_club_name: "Affiliated Club".to_string(),
            start_doy: 205, // 24 July
            start_year: 2023,
            end_doy: 1, // 1 Jan
            end_year: 1900, // Permanent deal
            affiliation_type: AffiliationType::BClub,
            annual_fee: 0,
            friendly_chance_enabled: true,
            friendly_chance: 50,
            max_loaned_enabled: false,
            max_loaned: 0,
            wage_percentage_enabled: true,
            wage_percentage: 100,
            benefits: FriendshipBenefits {
                flags: FriendshipBenefits::PLAYERS_MOVE_FREELY
                    | FriendshipBenefits::SAME_BOARD
                    | FriendshipBenefits::CANNOT_PLAY_IN_SAME_DIVISION
                    | FriendshipBenefits::PLAY_FRIENDLIES
                    | FriendshipBenefits::PERMANENT_DEAL
                    | FriendshipBenefits::USES_ALL_PARENT_FACILITIES,
            },
            raw: vec![0; 0x40],
        }
    }

    pub fn start_date_dmy(&self) -> (u8, u8, u16) {
        doy_to_dmy(self.start_year, self.start_doy)
    }

    pub fn set_start_date_dmy(&mut self, day: u8, month: u8, year: u16) {
        self.start_year = year;
        self.start_doy = dmy_to_doy(day, month, year);
    }

    pub fn end_date_dmy(&self) -> (u8, u8, u16) {
        doy_to_dmy(self.end_year, self.end_doy)
    }

    pub fn set_end_date_dmy(&mut self, day: u8, month: u8, year: u16) {
        self.end_year = year;
        self.end_doy = dmy_to_doy(day, month, year);
    }
}

pub fn doy_to_dmy(year: u16, doy: u16) -> (u8, u8, u16) {
    let is_leap = (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0);
    let days_in_months = if is_leap {
        [31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    } else {
        [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    };
    let mut rem = (doy as i32).max(1);
    for (m_idx, &days) in days_in_months.iter().enumerate() {
        if rem <= days {
            return (rem as u8, (m_idx + 1) as u8, year);
        }
        rem -= days;
    }
    (31, 12, year)
}

pub fn dmy_to_doy(day: u8, month: u8, year: u16) -> u16 {
    let is_leap = (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0);
    let days_in_months = if is_leap {
        [31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    } else {
        [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    };
    let mut doy = 0;
    let m = (month as usize).clamp(1, 12);
    for i in 0..(m - 1) {
        doy += days_in_months[i];
    }
    doy += (day as i32).clamp(1, days_in_months[m - 1]);
    doy.clamp(1, 366) as u16
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn date_conversions_round_trip() {
        let (d, m, y) = doy_to_dmy(2022, 205);
        assert_eq!((d, m, y), (24, 7, 2022));
        assert_eq!(dmy_to_doy(24, 7, 2022), 205);

        let (d, m, y) = doy_to_dmy(1900, 1);
        assert_eq!((d, m, y), (1, 1, 1900));
        assert_eq!(dmy_to_doy(1, 1, 1900), 1);
    }

    #[test]
    fn friendship_benefits_flags_toggle() {
        let mut b = FriendshipBenefits::default();
        assert!(!b.has(FriendshipBenefits::PLAYERS_MOVE_FREELY));
        b.set(FriendshipBenefits::PLAYERS_MOVE_FREELY, true);
        assert!(b.has(FriendshipBenefits::PLAYERS_MOVE_FREELY));
        b.set(FriendshipBenefits::PLAYERS_MOVE_FREELY, false);
        assert!(!b.has(FriendshipBenefits::PLAYERS_MOVE_FREELY));
    }
}

