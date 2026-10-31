//! Shared related-entity links, resolved by cached identity rather than guessed names.
use std::sync::Arc;

use crate::cache::EntityCache;
use eframe::egui;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClubTarget {
    pub addr: u64,
    pub index: usize,
    pub name: String,
}

pub fn publish_cache(ctx: &egui::Context, cache: Option<Arc<EntityCache>>) {
    ctx.data_mut(|d| d.insert_temp(egui::Id::new("entity_link_cache"), cache));
}

fn resolve_club(cache: &EntityCache, name: &str, addr: Option<u64>) -> Option<ClubTarget> {
    if addr.is_some_and(|p| !crate::player::is_valid_ptr(p)) {
        return None;
    }
    let mut matches = cache.clubs.iter().filter(|c| match addr {
        Some(addr) => c.addr == addr,
        None => c.name.eq_ignore_ascii_case(name.trim()),
    });
    let club = matches.next()?;
    if matches.next().is_some() {
        return None;
    }
    Some(ClubTarget {
        addr: club.addr,
        index: club.index,
        name: club.name.clone(),
    })
}

/// Render a club relationship without inventing a destination for unknown names.
pub fn club_link(ui: &mut egui::Ui, name: Option<&str>, addr: Option<u64>) {
    let Some(name) = name.filter(|s| !s.trim().is_empty()) else {
        ui.label("Club: Unknown")
            .on_hover_text("Club relationship is unavailable for this record.");
        return;
    };
    let cache = ui
        .ctx()
        .data_mut(|d| d.get_temp::<Option<Arc<EntityCache>>>(egui::Id::new("entity_link_cache")))
        .flatten();
    let target = cache.as_ref().and_then(|c| resolve_club(c, name, addr));
    ui.label("Club:");
    if let Some(target) = target {
        if ui.link(name).on_hover_text("Open club").clicked() {
            ui.ctx()
                .data_mut(|d| d.insert_temp(egui::Id::new("open_related_club"), target));
        }
    } else {
        ui.label(name)
            .on_hover_text("No unique club destination is available in the current cache.");
    }
}

pub fn take_club_target(ctx: &egui::Context) -> Option<ClubTarget> {
    ctx.data_mut(|d| {
        let id = egui::Id::new("open_related_club");
        let target = d.get_temp::<ClubTarget>(id);
        d.remove::<ClubTarget>(id);
        target
    })
}

/// Editing availability is published once by the app; standalone panel previews are read-only.
pub fn set_editing_enabled(ctx: &egui::Context, enabled: bool) {
    ctx.data_mut(|d| d.insert_temp(egui::Id::new("editing_enabled"), enabled));
}

pub fn editing_enabled(ui: &egui::Ui) -> bool {
    ui.ctx()
        .data_mut(|d| d.get_temp::<bool>(egui::Id::new("editing_enabled")))
        .unwrap_or(false)
}

/// Capture focus before text widgets can surrender it on Enter this frame.
pub fn set_result_keyboard_enabled(ctx: &egui::Context, enabled: bool) {
    ctx.data_mut(|d| d.insert_temp(egui::Id::new("result_keyboard_enabled"), enabled));
}

pub fn result_keyboard_enabled(ui: &egui::Ui) -> bool {
    ui.ctx()
        .data_mut(|d| d.get_temp::<bool>(egui::Id::new("result_keyboard_enabled")))
        .unwrap_or(false)
}
/// Browser-style history shared by all entity types and search pages.
pub struct BrowserHistory<T> {
    entries: Vec<T>,
    cursor: usize,
}

impl<T: Clone + PartialEq> BrowserHistory<T> {
    pub fn new(initial: T) -> Self {
        Self {
            entries: vec![initial],
            cursor: 0,
        }
    }

    pub fn visit(&mut self, entry: T) {
        if self.entries[self.cursor] == entry {
            return;
        }
        self.entries.truncate(self.cursor + 1);
        self.entries.push(entry);
        self.cursor += 1;
    }

    pub fn can_back(&self) -> bool {
        self.cursor > 0
    }
    pub fn can_forward(&self) -> bool {
        self.cursor + 1 < self.entries.len()
    }
    pub fn back(&mut self) -> Option<T> {
        if !self.can_back() {
            return None;
        }
        self.cursor -= 1;
        Some(self.entries[self.cursor].clone())
    }
    pub fn forward(&mut self) -> Option<T> {
        if !self.can_forward() {
            return None;
        }
        self.cursor += 1;
        Some(self.entries[self.cursor].clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cache::CachedClub;
    fn cache(names: &[&str]) -> EntityCache {
        EntityCache {
            my_clubs: None,
            players: vec![],
            staff: vec![],
            clubs: names
                .iter()
                .enumerate()
                .map(|(i, name)| CachedClub {
                    addr: 100_000 + i as u64,
                    index: i,
                    uid: i as u32,
                    name: name.to_string(),
                    gender: 0,
                    transfer_budget: None,
                    senior_team_addr: None,
                    nation_id: None,
                    status: None,
                    reputation: None,
                })
                .collect(),
            nations: vec![],
            club_names: Default::default(),
        }
    }
    #[test]
    fn club_links_require_a_unique_cached_name() {
        let c = cache(&["Arsenal", "Chelsea"]);
        assert_eq!(resolve_club(&c, " arsenal ", None).unwrap().index, 0);
        assert!(resolve_club(&c, "Missing", None).is_none());
        assert!(resolve_club(&cache(&["Arsenal", "ARSENAL"]), "Arsenal", None).is_none());
        assert_eq!(
            resolve_club(&c, "Full name differs", Some(100_001))
                .unwrap()
                .index,
            1
        );
        assert!(resolve_club(&c, "Arsenal", Some(1)).is_none());
        assert!(resolve_club(&c, "Arsenal", Some(200_000)).is_none());
    }
    #[test]
    fn result_keyboard_permission_survives_focus_changes_within_frame() {
        let ctx = egui::Context::default();
        let _ = ctx.run(Default::default(), |ctx| {
            set_result_keyboard_enabled(ctx, false);
            egui::CentralPanel::default().show(ctx, |ui| {
                assert!(!result_keyboard_enabled(ui));
                set_result_keyboard_enabled(ctx, true);
                assert!(result_keyboard_enabled(ui));
            });
        });
    }
    #[test]
    fn history_crosses_entity_types_and_preserves_forward_until_new_visit() {
        let mut h = BrowserHistory::new("results");
        h.visit("player");
        h.visit("club");
        assert_eq!(h.back(), Some("player"));
        assert_eq!(h.back(), Some("results"));
        assert_eq!(h.back(), None);
        assert_eq!(h.forward(), Some("player"));
        h.visit("staff");
        assert!(!h.can_forward());
        h.visit("staff");
        assert_eq!(h.back(), Some("player"));
    }
}
