//! Read-only lineup selection over one attached save's normalized cache.
use crate::cache::{CachedPlayer, EntityCache};
use crate::fields::{Encoding, FieldLocation, VersionedSchemas};
use std::collections::HashSet;

#[derive(Clone, Copy, Debug)]
pub struct Slot {
    pub label: &'static str,
    pub position: &'static str,
    /// Pitch coordinates, attacking at the top.
    pub x: f32,
    pub y: f32,
}

const fn slot(label: &'static str, position: &'static str, x: f32, y: f32) -> Slot {
    Slot {
        label,
        position,
        x,
        y,
    }
}
const GK: Slot = slot("GK", "pos_gk", 0.5, 0.9);
const DL: Slot = slot("DL", "pos_dl", 0.12, 0.72);
const DCL: Slot = slot("DC (L)", "pos_dc", 0.37, 0.72);
const DCR: Slot = slot("DC (R)", "pos_dc", 0.63, 0.72);
const DR: Slot = slot("DR", "pos_dr", 0.88, 0.72);
const MCL: Slot = slot("MC (L)", "pos_mc", 0.36, 0.4);
const MCR: Slot = slot("MC (R)", "pos_mc", 0.64, 0.4);
const AML: Slot = slot("AML", "pos_aml", 0.15, 0.23);
const AMR: Slot = slot("AMR", "pos_amr", 0.85, 0.23);
const ST: Slot = slot("ST", "pos_st", 0.5, 0.08);

pub struct Formation {
    pub name: &'static str,
    pub slots: [Slot; 11],
}

pub const FORMATIONS: [Formation; 5] = [
    Formation {
        name: "4-1-2-2-1",
        slots: [
            GK,
            DL,
            DCL,
            DCR,
            DR,
            slot("DM", "pos_dm", 0.5, 0.56),
            MCL,
            MCR,
            AML,
            AMR,
            ST,
        ],
    },
    Formation {
        name: "4-2-3-1",
        slots: [
            GK,
            DL,
            DCL,
            DCR,
            DR,
            slot("DM (L)", "pos_dm", 0.36, 0.53),
            slot("DM (R)", "pos_dm", 0.64, 0.53),
            AML,
            slot("AMC", "pos_amc", 0.5, 0.27),
            AMR,
            ST,
        ],
    },
    Formation {
        name: "4-4-2",
        slots: [
            GK,
            DL,
            DCL,
            DCR,
            DR,
            slot("ML", "pos_ml", 0.12, 0.4),
            MCL,
            MCR,
            slot("MR", "pos_mr", 0.88, 0.4),
            slot("ST (L)", "pos_st", 0.36, 0.12),
            slot("ST (R)", "pos_st", 0.64, 0.12),
        ],
    },
    Formation {
        name: "4-3-1-2",
        slots: [
            GK,
            DL,
            DCL,
            DCR,
            DR,
            slot("MC (L)", "pos_mc", 0.25, 0.46),
            slot("MC", "pos_mc", 0.5, 0.46),
            slot("MC (R)", "pos_mc", 0.75, 0.46),
            slot("AMC", "pos_amc", 0.5, 0.28),
            slot("ST (L)", "pos_st", 0.36, 0.1),
            slot("ST (R)", "pos_st", 0.64, 0.1),
        ],
    },
    Formation {
        name: "3-4-2-1",
        slots: [
            GK,
            slot("DC (L)", "pos_dc", 0.25, 0.72),
            slot("DC", "pos_dc", 0.5, 0.72),
            slot("DC (R)", "pos_dc", 0.75, 0.72),
            slot("WBL", "pos_wbl", 0.12, 0.49),
            MCL,
            MCR,
            slot("WBR", "pos_wbr", 0.88, 0.49),
            AML,
            AMR,
            ST,
        ],
    },
];

/// Only current membership counts. Contract ownership would include away loans.
pub fn in_scope(player: &CachedPlayer, cache: &EntityCache, club: Option<u32>) -> bool {
    player.club_id.is_some_and(|id| match club {
        Some(selected) => id == selected,
        None => cache
            .my_clubs
            .as_ref()
            .is_some_and(|c| c.club_ids.contains(&id)),
    })
}

/// Cached raw bytes are already normalized for player-coaches by the loader.
pub fn attribute(
    schemas: &'static VersionedSchemas,
    player: &CachedPlayer,
    key: &str,
) -> Option<u8> {
    let FieldLocation::Direct { offset, encoding } = schemas.field(key)?.offset? else {
        return None;
    };
    let raw = *player.raw.get(usize::try_from(offset).ok()?)?;
    let value = match encoding {
        Encoding::Direct => u16::from(raw),
        Encoding::Encoded => (u16::from(raw) + 2) / 5,
        _ => return None,
    };
    (1..=20).contains(&value).then_some(value as u8)
}

pub fn ability(
    schemas: &'static VersionedSchemas,
    player: &CachedPlayer,
    key: &str,
) -> Option<u16> {
    let value = crate::fields::read_field_int(&(), &player.raw, schemas.field(key)?)?;
    (1..=200).contains(&value).then_some(value as u16)
}

pub fn headroom(schemas: &'static VersionedSchemas, player: &CachedPlayer) -> Option<u16> {
    Some(ability(schemas, player, "p_pa")?.saturating_sub(ability(schemas, player, "p_ca")?))
}

#[derive(Clone, Copy, Debug)]
pub struct Candidate {
    /// Index into the exact EntityCache used for this solution, never a process index.
    pub player: usize,
    pub familiarity: u8,
    pub rating: f64,
    pub fit: f64,
    pub headroom: Option<u16>,
    merit: f64,
}

pub struct Lineup {
    pub pool_size: usize,
    pub selected: Vec<Option<Candidate>>,
    pub ranked: Vec<Vec<Candidate>>,
}

pub fn calculate(
    schemas: &'static VersionedSchemas,
    cache: &EntityCache,
    club: Option<u32>,
    formation: usize,
    growth: bool,
    minimum: u8,
) -> Lineup {
    let formation = &FORMATIONS[formation.min(FORMATIONS.len() - 1)];
    let mut seen = HashSet::new();
    let pool: Vec<_> = cache
        .players
        .iter()
        .enumerate()
        .filter(|(_, p)| {
            p.addr > 0x10000
                && p.addr < 0x7FFFFFFFFFFF
                && in_scope(p, cache, club)
                && seen.insert(p.addr)
        })
        .collect();
    let mut ranked = Vec::new();
    let mut scores = Vec::new();
    for slot in &formation.slots {
        let mut candidates = Vec::new();
        let mut row = vec![None; pool.len()];
        for (column, &(player, p)) in pool.iter().enumerate() {
            let Some(familiarity) =
                attribute(schemas, p, slot.position).filter(|v| *v >= minimum.max(1))
            else {
                continue;
            };
            let Some(rating) = crate::ratings::checked_position_rating(slot.position, |key| {
                attribute(schemas, p, key)
            }) else {
                continue;
            };
            let headroom = headroom(schemas, p);
            if growth && headroom.is_none() {
                continue;
            }
            let fit = rating * f64::from(familiarity) / 20.0;
            // One CA point outranks any aggregate fit difference across eleven
            // players. Growth is capacity, not an invented monthly forecast.
            let merit = if growth {
                f64::from(headroom.unwrap()) * 2000.0 + fit
            } else {
                fit
            };
            candidates.push(Candidate {
                player,
                familiarity,
                rating,
                fit,
                headroom,
                merit,
            });
            row[column] = Some(merit);
        }
        candidates.sort_by(|a, b| {
            b.merit
                .total_cmp(&a.merit)
                .then_with(|| {
                    cache.players[a.player]
                        .uid
                        .cmp(&cache.players[b.player].uid)
                })
                .then_with(|| {
                    cache.players[a.player]
                        .addr
                        .cmp(&cache.players[b.player].addr)
                })
        });
        ranked.push(candidates);
        scores.push(row);
    }
    let selected = assignment(&scores)
        .into_iter()
        .enumerate()
        .map(|(slot, column)| {
            let player = pool.get(column?)?.0;
            ranked[slot].iter().find(|c| c.player == player).copied()
        })
        .collect();
    Lineup {
        pool_size: pool.len(),
        selected,
        ranked,
    }
}

/// Rectangular Hungarian assignment: maximize filled slots, then total merit.
/// An empty dummy column per slot allows incomplete squads without forcing an
/// invalid assignment. O(slots^2 * players), with only eleven slots.
fn assignment(scores: &[Vec<Option<f64>>]) -> Vec<Option<usize>> {
    let n = scores.len();
    if n == 0 {
        return Vec::new();
    }
    let players = scores[0].len();
    let m = players + n;
    let max_merit = scores
        .iter()
        .flatten()
        .flatten()
        .copied()
        .fold(0.0, f64::max);
    let filled_bonus = (max_merit + 1.0) * (n + 1) as f64;
    let cost = |row: usize, col: usize| -> f64 {
        if col >= players {
            0.0
        } else {
            scores[row][col].map_or(filled_bonus, |s| -filled_bonus - s)
        }
    };
    let (mut u, mut v) = (vec![0.0; n + 1], vec![0.0; m + 1]);
    let (mut p, mut way) = (vec![0; m + 1], vec![0; m + 1]);
    for i in 1..=n {
        p[0] = i;
        let mut j0 = 0;
        let mut minv = vec![f64::INFINITY; m + 1];
        let mut used = vec![false; m + 1];
        loop {
            used[j0] = true;
            let i0 = p[j0];
            let (mut delta, mut j1) = (f64::INFINITY, 0);
            for j in 1..=m {
                if used[j] {
                    continue;
                }
                let cur = cost(i0 - 1, j - 1) - u[i0] - v[j];
                if cur < minv[j] {
                    minv[j] = cur;
                    way[j] = j0;
                }
                if minv[j] < delta {
                    delta = minv[j];
                    j1 = j;
                }
            }
            for j in 0..=m {
                if used[j] {
                    u[p[j]] += delta;
                    v[j] -= delta;
                } else {
                    minv[j] -= delta;
                }
            }
            j0 = j1;
            if p[j0] == 0 {
                break;
            }
        }
        loop {
            let j1 = way[j0];
            p[j0] = p[j1];
            j0 = j1;
            if j0 == 0 {
                break;
            }
        }
    }
    let mut result = vec![None; n];
    for j in 1..=players {
        if p[j] > 0 && scores[p[j] - 1][j - 1].is_some() {
            result[p[j] - 1] = Some(j - 1);
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game_version::GameBuild;

    fn player(
        schemas: &'static VersionedSchemas,
        id: u32,
        value: u8,
        ca: u16,
        pa: u16,
    ) -> CachedPlayer {
        let mut p = CachedPlayer {
            addr: 100_000 + u64::from(id),
            index: id as usize,
            uid: id,
            name: format!("Player {id}"),
            ca: ca as u8,
            pa: pa as u8,
            dob_year: 2008,
            raw: vec![0; 4096],
            person_shift: 0,
            nation_id: None,
            club_id: Some(42),
            owning_club_id: Some(42),
            value: None,
        };
        for f in schemas.player_fields {
            if let Some(FieldLocation::Direct { offset, encoding }) = f.offset {
                match encoding {
                    Encoding::Encoded => p.raw[offset as usize] = value * 5 - 2,
                    Encoding::Direct => p.raw[offset as usize] = value,
                    _ => {}
                }
            }
        }
        for (key, value) in [("p_ca", ca), ("p_pa", pa)] {
            let loc = schemas.field(key).unwrap().offset.unwrap();
            let off = loc.offset() as usize;
            if loc.encoding() == Encoding::Direct {
                p.raw[off] = value as u8;
            } else {
                p.raw[off..off + 2].copy_from_slice(&value.to_le_bytes());
            }
        }
        p
    }

    fn cache(players: Vec<CachedPlayer>) -> EntityCache {
        EntityCache {
            players,
            staff: vec![],
            clubs: vec![],
            nations: vec![],
            club_names: Default::default(),
            my_clubs: Some(crate::cache::MyClubs {
                club_ids: HashSet::from([42, 43]),
                label: "Club + B".into(),
            }),
        }
    }

    #[test]
    fn global_assignment_preserves_specialist_and_fills_every_possible_slot() {
        assert_eq!(
            assignment(&[vec![Some(100.0), Some(99.0)], vec![Some(98.0), None]]),
            vec![Some(1), Some(0)]
        );
        assert_eq!(
            assignment(&[vec![Some(0.0)], vec![None]]),
            vec![Some(0), None]
        );
        assert_eq!(assignment(&vec![vec![]; 11]), vec![None; 11]);
    }

    #[test]
    fn optimizer_matches_exhaustive_small_squads() {
        // Compare against all distinct assignments, including unfilled slots.
        for seed in 0..100usize {
            let scores: Vec<Vec<_>> = (0..3)
                .map(|i| {
                    (0..4)
                        .map(|j| {
                            let n = (seed * (i + 1) + j * 17 + i * 23) % 13;
                            (n % 4 != 0).then_some(n as f64)
                        })
                        .collect()
                })
                .collect();
            let actual = assignment(&scores);
            let objective = |slots: &[Option<usize>]| {
                (
                    slots.iter().flatten().count(),
                    slots
                        .iter()
                        .enumerate()
                        .filter_map(|(i, p)| p.and_then(|p| scores[i][p]))
                        .sum::<f64>() as u32,
                )
            };
            let mut best = (0, 0);
            for a in 0..5 {
                for b in 0..5 {
                    for c in 0..5 {
                        let slots = [a, b, c].map(|p| (p < 4).then_some(p));
                        let selected: Vec<_> = slots.iter().flatten().copied().collect();
                        if selected.iter().copied().collect::<HashSet<_>>().len() != selected.len()
                        {
                            continue;
                        }
                        if slots
                            .iter()
                            .enumerate()
                            .any(|(i, p)| p.is_some_and(|p| scores[i][p].is_none()))
                        {
                            continue;
                        }
                        best = best.max(objective(&slots));
                    }
                }
            }
            assert_eq!(objective(&actual), best, "seed {seed}");
        }
    }

    #[test]
    fn scope_uses_current_identity_and_excludes_away_loans_and_unknown_clubs() {
        let s = GameBuild::Fm27Steam.schemas();
        let mut p = player(s, 1, 20, 100, 180);
        let cache = cache(vec![]);
        assert!(in_scope(&p, &cache, None));
        p.club_id = Some(43);
        assert!(in_scope(&p, &cache, None));
        assert!(!in_scope(&p, &cache, Some(42)));
        p.club_id = Some(99);
        assert!(!in_scope(&p, &cache, None));
        p.club_id = None;
        assert!(!in_scope(&p, &cache, None));
    }

    #[test]
    fn modes_rank_current_strength_and_remaining_capacity_differently() {
        let s = GameBuild::Fm27Steam.schemas();
        let cache = cache(vec![player(s, 1, 20, 180, 190), player(s, 2, 16, 80, 170)]);
        let current = calculate(s, &cache, None, 0, false, 15);
        let growth = calculate(s, &cache, None, 0, true, 15);
        assert_eq!(current.ranked[0][0].player, 0);
        assert_eq!(growth.ranked[0][0].player, 1);
        assert_eq!(growth.ranked[0][0].headroom, Some(90));
        assert_eq!(current.selected.iter().flatten().count(), 2);
    }

    #[test]
    fn all_formations_use_eleven_unique_players_and_keep_ranked_cover() {
        let s = GameBuild::Fm27Steam.schemas();
        let cache = cache((1..=16).map(|i| player(s, i, 20, 100, 180)).collect());
        for formation in 0..FORMATIONS.len() {
            for growth in [false, true] {
                let result = calculate(s, &cache, None, formation, growth, 15);
                let selected: HashSet<_> =
                    result.selected.iter().flatten().map(|p| p.player).collect();
                assert_eq!(selected.len(), 11);
                assert!(result.ranked.iter().all(|r| r.len() == 16));
            }
        }
    }

    #[test]
    fn missing_invalid_and_duplicate_records_never_create_phantom_players() {
        let s = GameBuild::Fm27Steam.schemas();
        let p = player(s, 1, 20, 100, 180);
        let duplicate = player(s, 1, 20, 100, 180);
        let mut incomplete = player(s, 2, 20, 100, 180);
        incomplete.raw.clear();
        let mut corrupt = player(s, 3, 20, 100, 180);
        corrupt.raw.fill(255);
        let mut bad_pointer = player(s, 4, 20, 100, 180);
        bad_pointer.addr = 0x10000;
        let cache = cache(vec![p, duplicate, incomplete, corrupt, bad_pointer]);
        for growth in [false, true] {
            let result = calculate(s, &cache, None, 0, growth, 15);
            assert_eq!(result.selected.iter().flatten().count(), 1);
            assert!(result.ranked.iter().all(|r| r.len() == 1));
        }
    }

    #[test]
    fn schema_specific_decoding_and_headroom_boundaries() {
        for build in [
            GameBuild::Fm24Steam,
            GameBuild::Fm26Steam,
            GameBuild::Fm27Steam,
        ] {
            let s = build.schemas();
            let p = player(s, 1, 16, 150, 140);
            assert_eq!(headroom(s, &p), Some(0));
            assert_eq!(attribute(s, &p, "pace"), Some(16));
            assert_eq!(headroom(s, &player(s, 2, 16, 0, 180)), None);
            assert_eq!(headroom(s, &player(s, 2, 16, 150, 65535)), None);
        }
    }
}
