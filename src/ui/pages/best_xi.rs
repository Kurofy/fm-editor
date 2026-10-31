//! Club lineup and position depth. This page never emits writes.
use crate::best_xi::{self, Candidate, FORMATIONS, Lineup};
use crate::cache::EntityCache;
use crate::fields::VersionedSchemas;
use eframe::egui;
use std::sync::Arc;

pub struct BestXiState {
    pub club: Option<u32>,
    formation: usize,
    growth: bool,
    minimum: u8,
    club_query: String,
    bench_only: bool,
    source: Option<Arc<EntityCache>>,
    lineup: Option<Lineup>,
}

impl Default for BestXiState {
    fn default() -> Self {
        Self {
            club: None,
            formation: 0,
            growth: false,
            minimum: 15,
            club_query: String::new(),
            bench_only: true,
            source: None,
            lineup: None,
        }
    }
}

impl BestXiState {
    /// Never retain a prior save's club selection or indexed solution.
    pub fn clear(&mut self) {
        *self = Self::default();
    }
}

pub fn show(
    ui: &mut egui::Ui,
    state: &mut BestXiState,
    schemas: &'static VersionedSchemas,
    cache: Option<&Arc<EntityCache>>,
) -> Option<usize> {
    ui.heading("Best XI");
    let Some(cache) = cache else {
        state.clear();
        ui.label("Load a save to build a lineup.");
        return None;
    };
    if !super::player_search::club_filter_supported(schemas) {
        state.clear();
        ui.label(
            "Best XI is unavailable: current club membership is not supported for this build.",
        );
        return None;
    }
    let new_source = !state
        .source
        .as_ref()
        .is_some_and(|old| Arc::ptr_eq(old, cache));
    if new_source {
        state.lineup = None;
        state.source = Some(cache.clone());
        if state
            .club
            .is_some_and(|id| !cache.clubs.iter().any(|c| c.uid == id))
        {
            state.club = None;
        }
    }
    let previous = (state.club, state.formation, state.growth, state.minimum);
    ui.horizontal_wrapped(|ui| {
        ui.label("Club:");
        let label = state.club.and_then(|id| cache.club_names.get(&id)).map(String::as_str)
            .unwrap_or("Managed clubs");
        egui::ComboBox::from_id_salt("best_xi_club").selected_text(label).width(185.0)
            .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside).show_ui(ui, |ui| {
            ui.add(egui::TextEdit::singleline(&mut state.club_query).hint_text("Name or club UID"));
            if ui.selectable_value(&mut state.club, None, "Managed clubs").clicked() { ui.memory_mut(|m| m.close_popup()); }
            let query = state.club_query.trim().to_lowercase();
            let mut clubs: Vec<_> = cache.clubs.iter().filter(|c| query.is_empty() || c.name.to_lowercase().contains(&query) || c.uid.to_string() == query).collect();
            clubs.sort_by(|a, b| a.name.cmp(&b.name).then(a.uid.cmp(&b.uid)));
            egui::ScrollArea::vertical().max_height(240.0).show(ui, |ui| {
                for c in clubs.iter().take(150) {
                    if ui.selectable_value(&mut state.club, Some(c.uid), format!("{} ({})", c.name, c.uid)).clicked() { ui.memory_mut(|m| m.close_popup()); }
                }
                if clubs.len() > 150 { ui.weak("Type a name or club UID to narrow the list."); }
            });
        });
        ui.label("Tactic:");
        egui::ComboBox::from_id_salt("best_xi_formation").selected_text(FORMATIONS[state.formation].name).show_ui(ui, |ui| {
            for (i, f) in FORMATIONS.iter().enumerate() { ui.selectable_value(&mut state.formation, i, f.name); }
        });
        ui.toggle_value(&mut state.growth, "Room to grow").on_hover_text("Off: strongest positional attributes. On: largest remaining PA minus CA, then positional fit.");
    });
    ui.horizontal_wrapped(|ui| {
        ui.label("Minimum familiarity:");
        ui.add(egui::DragValue::new(&mut state.minimum).range(1..=20));
        ui.weak("15 = accomplished; 20 = natural");
        ui.checkbox(&mut state.bench_only, "Alternatives outside XI");
    });
    if state.club.is_none() {
        if let Some(clubs) = &cache.my_clubs {
            ui.label(&clubs.label);
        } else {
            ui.label("Managed clubs unavailable. Select a club by name or UID.");
            return None;
        }
    }
    ui.weak("Current club membership, including youth. Away loans excluded. Cached snapshot: Reload Save to refresh.");
    ui.weak("Fitness, injuries, suspensions and registration eligibility are not checked.");
    if state.lineup.is_none()
        || previous != (state.club, state.formation, state.growth, state.minimum)
    {
        state.lineup = Some(best_xi::calculate(
            schemas,
            cache,
            state.club,
            state.formation,
            state.growth,
            state.minimum,
        ));
    }
    let lineup = state.lineup.as_ref()?;
    let filled = lineup.selected.iter().flatten().count();
    let mode = if state.growth {
        "Growth headroom"
    } else {
        "Current strength"
    };
    ui.strong(format!(
        "{mode} · {filled}/11 positions filled · {} players in scope",
        lineup.pool_size
    ));
    let mut clicked = None;
    egui::ScrollArea::vertical().id_salt("best_xi_content").show(ui, |ui| {
        ui.collapsing("How selection works", |ui| {
            ui.label("The whole XI is optimized together, with each player used once. Fill as many positions as possible, then maximize the total score. Missing or invalid data leaves a gap.");
            ui.label("Current strength uses the editor's heuristic position attribute weights (0–100), multiplied by familiarity / 20. These are not FM match-engine ratings. Left/right central slots share weights; preferred foot and player roles are not modeled.");
            ui.label("Room to grow maximizes remaining PA − CA, then positional fit. This measures capacity for attribute growth, not future ability or how quickly it will be reached. Pro, Det and Amb are context only and do not change the ranking.");
            ui.label("FM24 research links development to age, professionalism and match exposure, with late-bloomer exceptions. These mechanics are unverified in FM27. Match points, development flags and training conditions are unavailable here; no monthly forecast is made.");
            ui.hyperlink_to("Read the FM-Arena development research", "https://fm-arena.com/thread/20900-some-training-and-development-conventional-wisdom-for-fm24/");
        });
        if state.growth { ui.label("Headroom is a capacity estimate. Development speed and late-bloomer status are unknown."); }
        if filled < 11 { ui.colored_label(ui.visuals().warn_fg_color, "Incomplete XI: some positions lack eligible players. Try another tactic or lower minimum familiarity."); }
        if ui.available_width() >= 620.0 { pitch(ui, cache, lineup, state, &mut clicked); }
        ui.add_space(8.0);
        ui.heading("Position depth");
        ui.weak("Alternatives are ranked independently per slot and can appear in several lists. Click a name to open the player.");
        let columns = if ui.available_width() >= 1000.0 { 3 } else if ui.available_width() >= 650.0 { 2 } else { 1 };
        for row in (0..11).collect::<Vec<_>>().chunks(columns) {
            ui.columns(columns, |cols| {
                for (col, &slot) in row.iter().enumerate() {
                    egui::Frame::group(cols[col].style()).show(&mut cols[col], |ui| {
                        ui.set_min_width(ui.available_width());
                        ui.strong(FORMATIONS[state.formation].slots[slot].label);
                        if let Some(c) = &lineup.selected[slot] {
                            player(ui, schemas, cache, c, state.growth, "First choice", &mut clicked);
                        } else { ui.label("No eligible player"); }
                        ui.separator();
                        let alternatives: Vec<_> = lineup.ranked[slot].iter().filter(|c| {
                            lineup.selected[slot].is_none_or(|s| s.player != c.player)
                                && (!state.bench_only || !lineup.selected.iter().flatten().any(|s| s.player == c.player))
                        }).take(3).collect();
                        if alternatives.is_empty() { ui.weak("No eligible alternatives"); }
                        for (rank, c) in alternatives.iter().enumerate() {
                            let elsewhere = lineup.selected.iter().position(|s| s.is_some_and(|s| s.player == c.player));
                            let label = elsewhere.map_or_else(|| format!("Cover {}", rank + 1), |s| format!("Starts at {}", FORMATIONS[state.formation].slots[s].label));
                            player(ui, schemas, cache, c, state.growth, &label, &mut clicked);
                        }
                    });
                }
            });
            ui.add_space(6.0);
        }
    });
    clicked
}

/// Cached name, or the UID when the name could not be read.
fn display_name(p: &crate::cache::CachedPlayer) -> String {
    if p.name.is_empty() {
        format!("Player {}", p.uid)
    } else {
        p.name.clone()
    }
}

fn player(
    ui: &mut egui::Ui,
    schemas: &'static VersionedSchemas,
    cache: &EntityCache,
    c: &Candidate,
    growth: bool,
    label: &str,
    clicked: &mut Option<usize>,
) {
    let p = &cache.players[c.player];
    ui.weak(label);
    if ui
        .add(egui::Link::new(display_name(p)))
        .clicked()
    {
        *clicked = Some(c.player);
    }
    ui.label(format!(
        "Fit {:.1} · attributes {:.1} · familiarity {}/20",
        c.fit, c.rating, c.familiarity
    ));
    let value = |key| best_xi::ability(schemas, p, key).map_or("?".into(), |v| v.to_string());
    let room = c.headroom.map_or("?".into(), |v| format!("+{v}"));
    ui.label(format!(
        "CA {} · PA {} · room {room}",
        value("p_ca"),
        value("p_pa")
    ));
    if growth {
        let trait_value =
            |key| best_xi::attribute(schemas, p, key).map_or("?".into(), |v| v.to_string());
        ui.label(format!("Pro {} · Det {} · Amb {}", trait_value("p_professionalism"), trait_value("determination"), trait_value("p_ambition")))
            .on_hover_text("Professionalism, determination and ambition (1–20). Context for development, not a growth forecast.");
        ui.weak(if p.dob_year > 0 {
            format!("Born {}", p.dob_year)
        } else {
            "Birth year unknown".into()
        });
    }
    ui.add_space(4.0);
}

fn pitch(
    ui: &mut egui::Ui,
    cache: &EntityCache,
    lineup: &Lineup,
    state: &BestXiState,
    clicked: &mut Option<usize>,
) {
    let (rect, _) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), 410.0),
        egui::Sense::hover(),
    );
    let painter = ui.painter();
    painter.rect_filled(rect, 6.0, egui::Color32::from_rgb(24, 65, 48));
    let field = rect.shrink(12.0);
    let stroke = egui::Stroke::new(1.0_f32, egui::Color32::from_rgb(91, 139, 111));
    painter.rect_stroke(field, 0.0, stroke, egui::StrokeKind::Inside);
    painter.line_segment(
        [
            egui::pos2(field.left(), field.center().y),
            egui::pos2(field.right(), field.center().y),
        ],
        stroke,
    );
    painter.circle_stroke(field.center(), 40.0, stroke);
    for (i, slot) in FORMATIONS[state.formation].slots.iter().enumerate() {
        let center = egui::pos2(
            field.left() + field.width() * slot.x,
            field.top() + field.height() * slot.y,
        );
        let card = egui::Rect::from_center_size(center, egui::vec2(field.width() * 0.23, 56.0));
        let text = lineup.selected[i].map_or_else(
            || format!("{}\nUnfilled", slot.label),
            |c| {
                let p = &cache.players[c.player];
                let score = if state.growth {
                    format!("+{} room", c.headroom.unwrap_or(0))
                } else {
                    format!("{:.1} fit", c.fit)
                };
                format!("{} · {score}\n{}", slot.label, display_name(p))
            },
        );
        if ui.put(card, egui::Button::new(text).wrap()).clicked() {
            *clicked = lineup.selected[i].map(|c| c.player);
        }
    }
    // `put` advances the cursor to each positioned card; the last card is at
    // the attacking end. Reserve the complete pitch before laying out depth.
    ui.advance_cursor_after_rect(rect);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replacing_or_removing_cache_discards_the_previous_solution() {
        let ctx = egui::Context::default();
        let s = crate::game_version::GameBuild::Fm27Steam.schemas();
        let empty = || {
            Arc::new(EntityCache {
                my_clubs: Some(crate::cache::MyClubs {
                    club_ids: Default::default(),
                    label: "Test club".into(),
                }),
                players: vec![],
                staff: vec![],
                clubs: vec![],
                nations: vec![],
                club_names: Default::default(),
            })
        };
        let mut state = BestXiState::default();
        let first = empty();
        let second = empty();
        for cache in [Some(&first), Some(&second), None] {
            let _ = ctx.run(Default::default(), |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    show(ui, &mut state, s, cache);
                });
            });
            assert_eq!(state.source.is_some(), cache.is_some());
            if let Some(cache) = cache {
                assert!(Arc::ptr_eq(state.source.as_ref().unwrap(), cache));
            } else {
                assert!(state.lineup.is_none());
            }
        }
    }

    #[test]
    fn unsupported_builds_do_not_reuse_a_lineup() {
        let ctx = egui::Context::default();
        let cache = Arc::new(EntityCache {
            my_clubs: None,
            players: vec![],
            staff: vec![],
            clubs: vec![],
            nations: vec![],
            club_names: Default::default(),
        });
        let mut state = BestXiState::default();
        state.source = Some(cache.clone());
        state.lineup = Some(Lineup {
            pool_size: 0,
            selected: vec![None; 11],
            ranked: vec![vec![]; 11],
        });
        let _ = ctx.run(Default::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                show(
                    ui,
                    &mut state,
                    crate::game_version::GameBuild::Fm27GamePass.schemas(),
                    Some(&cache),
                );
            });
        });
        assert!(state.source.is_none());
        assert!(state.lineup.is_none());
    }

    #[test]
    fn pitch_reserves_full_height_before_depth_cards() {
        let ctx = egui::Context::default();
        let cache = EntityCache {
            my_clubs: None,
            players: vec![],
            staff: vec![],
            clubs: vec![],
            nations: vec![],
            club_names: Default::default(),
        };
        let lineup = Lineup {
            pool_size: 0,
            selected: vec![None; 11],
            ranked: vec![vec![]; 11],
        };
        let _ = ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1000.0, 800.0),
                )),
                ..Default::default()
            },
            |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    let start = ui.cursor().top();
                    pitch(ui, &cache, &lineup, &BestXiState::default(), &mut None);
                    assert!(ui.cursor().top() >= start + 410.0);
                });
            },
        );
    }
}
