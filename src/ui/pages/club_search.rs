//! Cache-driven club search with validated live filters and sortable results.

use std::cmp::Ordering;

use eframe::egui;

use crate::cache::CachedClub;

#[derive(Default)]
pub struct ClubSearchState {
    pub query: String,
    pub female: bool,
}

impl ClubSearchState {
    fn reset(&mut self) {
        *self = Self::default();
    }
}

/// A numeric-only query is an exact UID, never a silently ignored bad number.
fn query_uid(query: &str) -> Result<Option<u32>, &'static str> {
    let query = query.trim();
    let digits = query.trim_start_matches(['+', '-']);
    if !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit()) {
        return query
            .parse::<u32>()
            .map(Some)
            .map_err(|_| "UID must be a whole number from 0 to 4294967295.");
    }
    Ok(None)
}

/// All active filters are combined with AND; malformed input matches no clubs.
pub fn filter<'a>(cache: &'a [CachedClub], state: &ClubSearchState) -> Vec<&'a CachedClub> {
    let Ok(uid) = query_uid(&state.query) else {
        return Vec::new();
    };
    let query = state.query.to_lowercase();
    let words: Vec<_> = query.split_whitespace().collect();
    cache
        .iter()
        .filter(|club| {
            if state.female && club.gender != 1 {
                return false;
            }
            if let Some(uid) = uid {
                if club.uid != uid {
                    return false;
                }
            } else {
                let name = club.name.to_lowercase();
                if !words.iter().all(|word| name.contains(*word)) {
                    return false;
                }
            }
            true
        })
        .collect()
}

/// Render the Name/UID input first, then a wrapping toolbar of live filters.
pub fn filter_row(ui: &mut egui::Ui, state: &mut ClubSearchState) {
    ui.horizontal_wrapped(|ui| {
        ui.label("Name/UID:");
        ui.add(egui::TextEdit::singleline(&mut state.query).desired_width(220.0).hint_text("Name or exact UID"))
            .on_hover_text("Names match all words, case-insensitively, in any order. Numeric-only input matches an exact UID. All filters combine with AND.");
    });
    if let Err(error) = query_uid(&state.query) {
        ui.colored_label(ui.visuals().error_fg_color, error);
    }
    ui.horizontal_wrapped(|ui| {
        ui.checkbox(&mut state.female, "Female")
            .on_hover_text("Only female clubs; unchecked includes all clubs.");
        if ui
            .button("Reset all")
            .on_hover_text("Clear every club filter.")
            .clicked()
        {
            state.reset();
        }
    });
}

#[derive(Clone, Copy, Default, PartialEq, Eq)]
enum SortColumn {
    #[default]
    Name,
    Uid,
    Budget,
}

#[derive(Clone, Default)]
struct GridState {
    column: SortColumn,
    descending: bool,
    selected: Option<u64>,
}

impl GridState {
    fn sort_by(&mut self, column: SortColumn) {
        if self.column == column {
            self.descending = !self.descending;
        } else {
            self.column = column;
            self.descending = false;
        }
    }
}

fn sorted_indices(results: &[&CachedClub], state: &GridState) -> Vec<usize> {
    let mut indices: Vec<_> = (0..results.len()).collect();
    indices.sort_by(|&a, &b| {
        let (a, b) = (results[a], results[b]);
        // Unknown budgets remain last in either direction.
        if state.column == SortColumn::Budget {
            match (a.transfer_budget, b.transfer_budget) {
                (None, Some(_)) => return Ordering::Greater,
                (Some(_), None) => return Ordering::Less,
                _ => {}
            }
        }
        let order = match state.column {
            SortColumn::Name => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
            SortColumn::Uid => a.uid.cmp(&b.uid),
            SortColumn::Budget => a.transfer_budget.cmp(&b.transfer_budget),
        };
        let order = if state.descending {
            order.reverse()
        } else {
            order
        };
        order
            .then_with(|| a.uid.cmp(&b.uid))
            .then_with(|| a.addr.cmp(&b.addr))
    });
    indices
}

/// Returns the original results-slice index, even when the display is sorted.
pub fn results_grid(
    ui: &mut egui::Ui,
    results: &[&CachedClub],
    currency: crate::settings::Currency,
) -> Option<usize> {
    const ROW_HEIGHT: f32 = 22.0;
    const COLUMNS: [(SortColumn, &str, f32); 3] = [
        (SortColumn::Name, "Club Name", 260.0),
        (SortColumn::Uid, "UID", 110.0),
        (SortColumn::Budget, "Transfer budget", 130.0),
    ];
    let id = ui.make_persistent_id("club_results_state");
    let mut state = ui
        .data_mut(|data| data.get_temp::<GridState>(id))
        .unwrap_or_default();
    ui.horizontal(|ui| {
        for (column, label, width) in COLUMNS {
            let label = if state.column == column {
                format!("{label} {}", if state.descending { "▼" } else { "▲" })
            } else {
                label.to_owned()
            };
            if ui
                .add_sized([width, ROW_HEIGHT], egui::Button::new(label).frame(false))
                .clicked()
            {
                state.sort_by(column);
            }
        }
    });
    ui.separator();
    let order = sorted_indices(results, &state);
    let mut opened = None;
    let mut moved = None;
    if results.is_empty() {
        ui.label("No results. Adjust filters or Reset all.");
    } else if crate::ui::navigation::result_keyboard_enabled(ui) && !ui.ctx().wants_keyboard_input()
    {
        let selected = order
            .iter()
            .position(|&i| Some(results[i].addr) == state.selected);
        let (up, down, enter) = ui.input(|input| {
            (
                input.key_pressed(egui::Key::ArrowUp),
                input.key_pressed(egui::Key::ArrowDown),
                input.key_pressed(egui::Key::Enter),
            )
        });
        if up || down {
            let position = match selected {
                Some(position) if down => (position + 1).min(order.len() - 1),
                Some(position) => position.saturating_sub(1),
                None if up => order.len() - 1,
                None => 0,
            };
            state.selected = Some(results[order[position]].addr);
            moved = Some(position);
        }
        if enter {
            opened = order
                .iter()
                .copied()
                .find(|&i| Some(results[i].addr) == state.selected);
        }
    }
    let mut scroll = egui::ScrollArea::vertical().id_salt("club_results_scroll");
    if let Some(position) = moved {
        scroll = scroll
            .vertical_scroll_offset(position as f32 * (ROW_HEIGHT + ui.spacing().item_spacing.y));
    }
    scroll.show_rows(ui, ROW_HEIGHT, order.len(), |ui, row_range| {
        for position in row_range {
            let index = order[position];
            let club = results[index];
            ui.push_id(club.addr, |ui| {
                let row = ui.horizontal(|ui| {
                    let glyph = if club.gender == 1 { "♀" } else { "♂" };
                    let budget = club
                        .transfer_budget
                        .map(|value| currency.format_gbp(value as i64))
                        .unwrap_or_else(|| "Unknown".into());
                    for (text, width) in [
                        (format!("{glyph} {}", club.name), COLUMNS[0].2),
                        (club.uid.to_string(), COLUMNS[1].2),
                        (budget, COLUMNS[2].2),
                    ] {
                        ui.add_sized([width, ROW_HEIGHT], egui::Label::new(text));
                    }
                });
                let response =
                    ui.interact(row.response.rect, ui.id().with("row"), egui::Sense::click());
                if state.selected == Some(club.addr) || response.hovered() {
                    let color = if state.selected == Some(club.addr) {
                        ui.visuals().selection.bg_fill.linear_multiply(0.4)
                    } else {
                        ui.visuals().widgets.hovered.bg_fill.linear_multiply(0.4)
                    };
                    ui.painter()
                        .rect_filled(response.rect, egui::CornerRadius::ZERO, color);
                }
                if response.clicked() {
                    state.selected = Some(club.addr);
                    opened = Some(index);
                }
            });
        }
    });
    ui.data_mut(|data| data.insert_temp(id, state));
    opened
}

#[cfg(test)]
mod tests {
    use super::*;

    fn club(addr: u64, uid: u32, name: &str, budget: Option<u32>) -> CachedClub {
        CachedClub {
            addr,
            index: 0,
            uid,
            name: name.into(),
            gender: 0,
            transfer_budget: budget,
            nation_id: None,
            status: None,
            reputation: None,
        }
    }

    #[test]
    fn invalid_numeric_inputs_match_nothing() {
        let clubs = [club(1, 12, "United", Some(100))];
        for query in ["4294967296", "-12", "++12"] {
            let state = ClubSearchState {
                query: query.into(),
                ..Default::default()
            };
            assert!(filter(&clubs, &state).is_empty());
        }
    }

    #[test]
    fn uid_bounds_and_number_prefixed_names_are_supported() {
        assert_eq!(query_uid(" 0 "), Ok(Some(0)));
        assert_eq!(query_uid("4294967295"), Ok(Some(u32::MAX)));
        assert_eq!(query_uid("1860 Munich"), Ok(None));
        assert_eq!(query_uid("   "), Ok(None));
        let clubs = [club(1, 12, "1860 Munich", None)];
        let state = ClubSearchState {
            query: "1860 Munich".into(),
            ..Default::default()
        };
        assert_eq!(filter(&clubs, &state).len(), 1);
    }

    #[test]
    fn name_uid_and_gender_filters_combine() {
        let clubs = [
            club(1, 12, "United City", Some(100)),
            club(2, 13, "Other City", None),
        ];
        let mut state = ClubSearchState {
            query: " CITY united ".into(),
            ..Default::default()
        };
        assert_eq!(
            filter(&clubs, &state)
                .iter()
                .map(|c| c.uid)
                .collect::<Vec<_>>(),
            [12]
        );
        state.query = " 12 ".into();
        assert_eq!(filter(&clubs, &state).len(), 1);
        state.female = true;
        assert!(filter(&clubs, &state).is_empty());
    }

    #[test]
    fn reset_clears_all_filters() {
        let mut state = ClubSearchState {
            query: "12".into(),
            female: true,
        };
        state.reset();
        assert!(state.query.is_empty());
        assert!(!state.female);
    }

    #[test]
    fn sorting_defaults_to_name_and_preserves_original_indices() {
        let clubs = [
            club(1, 3, "Zulu", None),
            club(2, 2, "alpha", Some(50)),
            club(3, 1, "Alpha", Some(100)),
        ];
        let results: Vec<_> = clubs.iter().collect();
        let mut state = GridState::default();
        assert_eq!(sorted_indices(&results, &state), [2, 1, 0]);
        state.sort_by(SortColumn::Uid);
        assert_eq!(sorted_indices(&results, &state), [2, 1, 0]);
        state.sort_by(SortColumn::Uid);
        assert_eq!(sorted_indices(&results, &state), [0, 1, 2]);
        state.sort_by(SortColumn::Budget);
        assert!(!state.descending);
        assert_eq!(sorted_indices(&results, &state), [1, 2, 0]);
        state.sort_by(SortColumn::Budget);
        assert_eq!(sorted_indices(&results, &state), [2, 1, 0]);
    }
}
