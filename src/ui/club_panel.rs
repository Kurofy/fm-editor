//! Club detail panel: Overview, Facilities, Finance, Stadium tabs.

use eframe::egui;

use crate::bridge::ClubView;
use crate::fields::{self, FieldLocation};
use crate::ui::field_widget::{self, FieldEdit};
use crate::ui::{entity_header, navigation};

#[derive(PartialEq)]
pub enum ClubTab {
    Overview,
    Facilities,
    Finance,
    Stadium,
}

pub fn show(
    schemas: &'static crate::fields::VersionedSchemas,
    ui: &mut egui::Ui,
    club: &mut ClubView,
    tab: &mut ClubTab,
    currency: &mut crate::settings::Currency,
) -> Vec<FieldEdit> {
    let mut edits = Vec::new();

    entity_header::show(
        ui,
        &club.full_name,
        "Club",
        club.uid,
        None,
        None,
        None,
        None,
    );
    ui.horizontal(|ui| {
        ui.label("Ground:");
        if let Some(name) = club
            .stadium_name
            .as_deref()
            .filter(|name| !name.trim().is_empty())
        {
            if ui
                .link(name)
                .on_hover_text("Open stadium details")
                .clicked()
            {
                *tab = ClubTab::Stadium;
            }
        } else {
            ui.label("Unknown");
        }
    });

    ui.horizontal(|ui| {
        ui.selectable_value(tab, ClubTab::Overview, "Overview");
        ui.selectable_value(tab, ClubTab::Facilities, "Facilities");
        ui.selectable_value(tab, ClubTab::Finance, "Finance");
        ui.selectable_value(tab, ClubTab::Stadium, "Stadium (read-only)");
    });
    ui.separator();

    egui::ScrollArea::vertical().show(ui, |ui| match tab {
        ClubTab::Overview => show_overview(schemas, ui, club, &mut edits),
        ClubTab::Facilities => show_facilities(schemas, ui, club, &mut edits),
        ClubTab::Finance => show_finance(schemas, ui, club, currency, &mut edits),
        ClubTab::Stadium => show_stadium(schemas, ui, club),
    });

    edits
}

fn show_overview(
    schemas: &'static crate::fields::VersionedSchemas,
    ui: &mut egui::Ui,
    club: &mut ClubView,
    edits: &mut Vec<FieldEdit>,
) {
    ui.strong("Identity");
    field_widget::info_label(ui, "Full Name", &club.full_name);
    field_widget::info_label(ui, "Short Name", &club.short_name);
    field_widget::info_label(ui, "UID", club.uid);
    if let Some(ref nick) = club.nickname {
        field_widget::info_label(ui, "Nickname", nick);
    }

    // Gender byte — direct club offset, read via c_gender field.
    let gender = schemas
        .field("c_gender")
        .and_then(|f| f.offset)
        .and_then(|l| club.raw.get(l.offset() as usize).copied());
    field_widget::info_label(
        ui,
        "Gender",
        match gender {
            Some(0) => "Male",
            Some(_) => "Female",
            None => "Unknown",
        },
    );
    // Reputation lives in a sub-record (FM27: the senior team object); edit
    // that record's bytes at its own address.
    if let Some(f) = schemas.field("c_reputation") {
        if let Some((container, _)) = sub_location(schemas, "c_reputation") {
            if let Some((buf, base)) = club.sub_record_mut(container) {
                edits.extend(field_widget::raw_u16_edit(ui, f, buf, 10000, base));
            }
        }
    }

    ui.add_space(8.0);
    ui.strong("Club Details");
    // Year founded is a u16; on FM27 the following u16 is not always zero.
    if let Some(v) = schemas
        .field("c_year_founded")
        .and_then(|f| fields::read_field_u16(club, &club.raw, &f))
    {
        field_widget::info_label(ui, "Year Founded", v);
    }
    if let Some(v) = schemas
        .field("c_owner_status")
        .and_then(|f| fields::read_field_u16(club, &club.raw, &f))
    {
        field_widget::info_label(ui, "Owner Status", v);
    }
    if let Some(v) = schemas
        .field("c_season_tickets")
        .and_then(|f| fields::read_field_u32(club, &club.raw, &f))
    {
        field_widget::info_label(ui, "Season Tickets", v);
    }
    if let Some(v) = schemas
        .field("c_social_media")
        .and_then(|f| fields::read_field_u32(club, &club.raw, &f))
    {
        field_widget::info_label(ui, "Social Media", v);
    }
}

fn show_facilities(
    schemas: &'static crate::fields::VersionedSchemas,
    ui: &mut egui::Ui,
    club: &mut ClubView,
    edits: &mut Vec<FieldEdit>,
) {
    ui.strong("Facilities");
    // Facility rows edit whichever sub-record the schema puts them in. Rows
    // with a `<key>_fin` mirror (FM27 academy coaching / youth recruitment,
    // which FM's Facilities screen reads from the finance object) write the
    // same value to the mirror too, so the change shows in game.
    for (label, key) in [
        ("Training", "c_training"),
        ("Youth Facilities", "c_youth_fac"),
        ("Youth Improvement", "c_youth_improve"),
        ("Youth Coaching", "c_youth_coach"),
        ("Youth Recruitment", "c_youth_recruit"),
        ("Corporate", "c_corporate"),
    ] {
        let Some((container, offset)) = sub_location(schemas, key) else {
            continue;
        };
        let before = edits.len();
        if let Some((buf, base)) = club.sub_record_mut(container) {
            u8_drag_1_20(ui, label, buf, offset, base, edits);
        }
        if edits.len() > before {
            let new_val = edits[edits.len() - 1].new_raw[0];
            if let Some(mirror) =
                mirror_u8_edit(schemas, club, &format!("{key}_fin"), label, new_val)
            {
                edits.push(mirror);
            }
        }
    }

    if club.info_raw.is_some() {
        ui.add_space(8.0);
        ui.strong("Supporter Profile");
        for (label, key) in [
            ("Hardcore %", "c_sup_hardcore"),
            ("Core %", "c_sup_core"),
            ("Family %", "c_sup_family"),
            ("Fair Weather %", "c_sup_fair_weather"),
            ("Casual %", "c_sup_casual"),
            ("Other %", "c_sup_other"),
        ] {
            if let Some(f) = schemas.field(key) {
                if let Some(v) = fields::read_field_u8(club, &club.raw, &f) {
                    field_widget::info_label(ui, label, format!("{v}%"));
                }
            }
        }

        ui.add_space(8.0);
        ui.strong("Supporter Attributes");
        for (label, key) in [
            ("Loyalty", "c_sup_loyalty"),
            ("Passion", "c_sup_passion"),
            ("Patience", "c_sup_patience"),
            ("Affluence", "c_sup_affluence"),
            ("Temperament", "c_sup_temperament"),
            ("Expectations", "c_sup_expectations"),
        ] {
            if let Some(f) = schemas.field(key) {
                if let Some(v) = fields::read_field_u8(club, &club.raw, &f) {
                    field_widget::info_label(ui, label, v);
                }
            }
        }
    } else {
        ui.label("Info sub-record not available.");
    }
}

fn show_finance(
    schemas: &'static crate::fields::VersionedSchemas,
    ui: &mut egui::Ui,
    club: &mut ClubView,
    currency: &mut crate::settings::Currency,
    edits: &mut Vec<FieldEdit>,
) {
    ui.horizontal(|ui| {
        ui.strong("Finance");
        ui.add_space(16.0);
        ui.label("Currency:");
        for c in [
            crate::settings::Currency::Eur,
            crate::settings::Currency::Usd,
            crate::settings::Currency::Gbp,
            crate::settings::Currency::Idr,
        ] {
            if ui
                .selectable_label(*currency == c, c.label())
                .clicked()
            {
                *currency = c;
            }
        }
    });
    ui.add_space(4.0);

    let mut shown = false;

    // Primary editable finances: Balance, Transfer Budget, Transfer Remaining, Wage Budget
    for key in [
        "c_balance",
        "c_transfer_budget",
        "c_transfer_remaining",
        "c_wage_budget",
    ] {
        let Some(field) = schemas.field(key) else { continue };
        let label = if key == "c_wage_budget" {
            format!("{} (p/w)", field.label)
        } else {
            field.label.to_string()
        };

        if let Some(row_edits) = money_field_row(ui, &label, key, club, schemas, *currency) {
            edits.extend(row_edits);
            shown = true;
        }
    }

    // Additional info / derived totals
    for (label, key) in [
        ("Currently Spending (p/w)", "c_wage_spending"),
        ("Next Season Min Transfer Budget", "c_transfer_next_season"),
        ("Total Wage Budget (p/w)", "c_wage_total"),
        ("Youth Grant Income", "c_youth_grant"),
    ] {
        if let Some(v) = schemas
            .field(key)
            .and_then(|f| fields::read_field_u32(club, &club.raw, &f))
        {
            field_widget::info_label(ui, label, currency.format_gbp(v as i64));
            shown = true;
        }
    }

    // Ticket prices
    for (label, key) in [
        ("Avg Ticket Price", "c_avg_ticket"),
        ("Avg Season Ticket", "c_avg_season_ticket"),
    ] {
        let Some((container, offset)) = sub_location(schemas, key) else {
            continue;
        };
        if let Some((buf, base)) = club.sub_record_mut(container) {
            f32_drag(ui, label, buf, offset, base, edits);
            shown = true;
        }
    }

    if !shown {
        ui.label("Finance data not available.");
    }
}

fn show_stadium(
    schemas: &'static crate::fields::VersionedSchemas,
    ui: &mut egui::Ui,
    club: &ClubView,
) {
    ui.strong("Attendance");
    if club.stadium_raw.is_some() {
        for key in ["c_avg_attendance", "c_min_attendance", "c_max_attendance"] {
            if let Some(f) = schemas.field(key) {
                if let Some(v) = fields::read_field_u32(club, &club.raw, &f) {
                    field_widget::info_label(ui, f.label, v);
                }
            }
        }
    } else {
        ui.label("Attendance sub-record not available.");
    }

    if let Some(se) = &club.stadium_entity_raw {
        ui.separator();
        ui.strong("Stadium");
        if let Some(name) = &club.stadium_name {
            field_widget::info_label(ui, "Name", name);
        }
        if let Some(name2) = &club.stadium_name2 {
            field_widget::info_label(ui, "Official Name", name2);
        }
        for key in [
            "st_capacity",
            "st_seated",
            "st_used",
            "st_all_seater",
            "st_expansion",
        ] {
            if let Some(f) = schemas.field(key) {
                if let Some(v) = fields::read_field_u32(&(), se, &f) {
                    field_widget::info_label(ui, f.label, v);
                }
            }
        }
        for key in ["st_pitch_length", "st_pitch_width"] {
            if let Some(f) = schemas.field(key) {
                if let Some(v) = fields::read_field_u16(&(), se, &f) {
                    field_widget::info_label(ui, f.label, format!("{:.1}m", v as f32 / 10.0));
                }
            }
        }
        for key in ["st_environment", "st_condition"] {
            if let Some(f) = schemas.field(key) {
                if let Some(v) = fields::read_field_u8(&(), se, &f) {
                    field_widget::info_label(ui, f.label, v);
                }
            }
        }
    }
}

/// Container and byte offset of a sub-record field, from its schema key.
fn sub_location(
    schemas: &'static crate::fields::VersionedSchemas,
    key: &str,
) -> Option<(fields::SubRecordId, usize)> {
    match schemas.field(key)?.offset? {
        FieldLocation::SubRecord {
            container, offset, ..
        } => Some((container, offset as usize)),
        FieldLocation::Direct { .. } => None,
    }
}

/// Edit writing `new_val` to the mirror field `key` (a second copy of a
/// facility rating that FM keeps in sync), or `None` when the schema has no
/// such mirror or its value already matches.
fn mirror_u8_edit(
    schemas: &'static crate::fields::VersionedSchemas,
    club: &mut ClubView,
    key: &str,
    label: &str,
    new_val: u8,
) -> Option<FieldEdit> {
    let (container, offset) = sub_location(schemas, key)?;
    let (buf, base) = club.sub_record_mut(container)?;
    let old = *buf.get(offset)?;
    if old == new_val {
        return None;
    }
    buf[offset] = new_val;
    Some(FieldEdit {
        addr: base + offset as u64,
        old_raw: vec![old],
        new_raw: vec![new_val],
        description: format!("{label} (finance copy): {old} \u{2192} {new_val}"),
    })
}

// ---------------------------------------------------------------------------
// Sub-record edit helpers — offset is relative to the sub-record base, not
// the top-level club struct.
// ---------------------------------------------------------------------------

/// Editable money field (Balance, Budgets, Wages).
/// Displays and edits in the chosen `Currency` and converts to/from base GBP in memory.
fn money_field_row(
    ui: &mut egui::Ui,
    label: &str,
    key: &str,
    club: &mut ClubView,
    schemas: &'static crate::fields::VersionedSchemas,
    currency: crate::settings::Currency,
) -> Option<Vec<FieldEdit>> {
    let loc = sub_location(schemas, key);
    let (container, offset) = match loc {
        Some((c, off)) if club.sub_record_mut(c).is_some() => (c, off),
        _ => {
            // Fallback for transfer budget/remaining if defined in budget sub-record but budget_raw is None
            if key == "c_transfer_budget"
                && club.finance_raw.as_ref().map_or(0, |b| b.len()) >= 0x7D0
            {
                ("finance", 0x7CC)
            } else if key == "c_transfer_remaining"
                && club.finance_raw.as_ref().map_or(0, |b| b.len()) >= 0x7D4
            {
                ("finance", 0x7D0)
            } else {
                return None;
            }
        }
    };

    let (raw, base_addr) = club.sub_record_mut(container)?;
    if offset + 4 > raw.len() {
        return None;
    }

    let is_balance = key == "c_balance";
    let old_bytes: [u8; 4] = raw[offset..offset + 4].try_into().unwrap();
    let old_gbp = if is_balance {
        i32::from_le_bytes(old_bytes) as i64
    } else {
        u32::from_le_bytes(old_bytes) as i64
    };

    let mut curr_val = currency.from_gbp(old_gbp);
    let enabled = navigation::editing_enabled(ui);
    let mut changed = false;

    let (range_min, range_max) = if is_balance {
        (-2_000_000_000_000i64, 2_000_000_000_000i64)
    } else {
        (0i64, 2_000_000_000_000i64)
    };

    let speed = match currency {
        crate::settings::Currency::Idr => {
            if key == "c_wage_budget" { 100_000.0 } else { 1_000_000.0 }
        }
        _ => {
            if key == "c_wage_budget" { 500.0 } else { 50_000.0 }
        }
    };

    let prefix = format!("{} ", currency.symbol());

    ui.horizontal(|ui| {
        ui.add_sized([160.0, 22.0], egui::Label::new(label));
        changed = ui
            .add_enabled_ui(enabled, |ui| {
                ui.add_sized(
                    [150.0, 22.0],
                    egui::DragValue::new(&mut curr_val)
                        .prefix(prefix)
                        .update_while_editing(false)
                        .range(range_min..=range_max)
                        .speed(speed),
                )
                .changed()
            })
            .inner;
        ui.label(
            egui::RichText::new(currency.format_amount(curr_val))
                .weak()
                .size(11.0),
        );
    });

    let mut row_edits = Vec::new();
    if enabled && changed {
        let new_gbp = currency.to_gbp(curr_val);
        let (min_gbp, max_gbp) = if is_balance {
            (i32::MIN as i64, i32::MAX as i64)
        } else {
            (0i64, i32::MAX as i64)
        };
        let new_gbp = new_gbp.clamp(min_gbp, max_gbp);
        if new_gbp != old_gbp {
            let new_bytes = (new_gbp as i32).to_le_bytes();
            raw[offset..offset + 4].copy_from_slice(&new_bytes);
            row_edits.push(FieldEdit {
                addr: base_addr + offset as u64,
                old_raw: old_bytes.to_vec(),
                new_raw: new_bytes.to_vec(),
                description: format!(
                    "{label}: {} \u{2192} {}",
                    currency.format_gbp(old_gbp),
                    currency.format_gbp(new_gbp)
                ),
            });

            mirror_budget_finance(
                club,
                key,
                container,
                new_gbp as i32,
                label,
                currency,
                &mut row_edits,
            );
        }
    }

    Some(row_edits)
}

fn mirror_budget_finance(
    club: &mut ClubView,
    key: &str,
    primary_container: &str,
    new_val: i32,
    label: &str,
    currency: crate::settings::Currency,
    edits: &mut Vec<FieldEdit>,
) {
    let new_bytes = new_val.to_le_bytes();
    match key {
        "c_transfer_budget" => {
            if primary_container == "finance" {
                if let (Some(b_raw), Some(b_addr)) = (club.budget_raw.as_mut(), club.budget_addr) {
                    if b_raw.len() >= 8 {
                        let old = b_raw[4..8].to_vec();
                        b_raw[4..8].copy_from_slice(&new_bytes);
                        edits.push(FieldEdit {
                            addr: b_addr + 4,
                            old_raw: old,
                            new_raw: new_bytes.to_vec(),
                            description: format!("{label} (budget copy): {}", currency.format_gbp(new_val as i64)),
                        });
                    }
                }
            } else if primary_container == "budget" {
                if let (Some(f_raw), Some(f_addr)) = (club.finance_raw.as_mut(), club.finance_addr) {
                    if f_raw.len() >= 0x7D0 {
                        let old = f_raw[0x7CC..0x7D0].to_vec();
                        f_raw[0x7CC..0x7D0].copy_from_slice(&new_bytes);
                        edits.push(FieldEdit {
                            addr: f_addr + 0x7CC,
                            old_raw: old,
                            new_raw: new_bytes.to_vec(),
                            description: format!("{label} (finance copy): {}", currency.format_gbp(new_val as i64)),
                        });
                    }
                }
            }
        }
        "c_transfer_remaining" => {
            if primary_container == "finance" {
                if let (Some(b_raw), Some(b_addr)) = (club.budget_raw.as_mut(), club.budget_addr) {
                    if b_raw.len() >= 12 {
                        let old = b_raw[8..12].to_vec();
                        b_raw[8..12].copy_from_slice(&new_bytes);
                        edits.push(FieldEdit {
                            addr: b_addr + 8,
                            old_raw: old,
                            new_raw: new_bytes.to_vec(),
                            description: format!("{label} (budget copy): {}", currency.format_gbp(new_val as i64)),
                        });
                    }
                }
            } else if primary_container == "budget" {
                if let (Some(f_raw), Some(f_addr)) = (club.finance_raw.as_mut(), club.finance_addr) {
                    if f_raw.len() >= 0x7D4 {
                        let old = f_raw[0x7D0..0x7D4].to_vec();
                        f_raw[0x7D0..0x7D4].copy_from_slice(&new_bytes);
                        edits.push(FieldEdit {
                            addr: f_addr + 0x7D0,
                            old_raw: old,
                            new_raw: new_bytes.to_vec(),
                            description: format!("{label} (finance copy): {}", currency.format_gbp(new_val as i64)),
                        });
                    }
                }
            }
        }
        "c_wage_budget" => {
            if primary_container == "finance" {
                if let (Some(b_raw), Some(b_addr)) = (club.budget_raw.as_mut(), club.budget_addr) {
                    if b_raw.len() >= 16 {
                        let old = b_raw[12..16].to_vec();
                        b_raw[12..16].copy_from_slice(&new_bytes);
                        edits.push(FieldEdit {
                            addr: b_addr + 12,
                            old_raw: old,
                            new_raw: new_bytes.to_vec(),
                            description: format!("{label} (budget copy): {}", currency.format_gbp(new_val as i64)),
                        });
                    }
                }
            }
        }
        _ => {}
    }
}

/// Editable f32 field (ticket prices, etc.).
fn f32_drag(
    ui: &mut egui::Ui,
    label: &str,
    raw: &mut [u8],
    offset: usize,
    base_addr: u64,
    edits: &mut Vec<FieldEdit>,
) {
    if offset + 4 > raw.len() {
        return;
    }
    let old_bytes: [u8; 4] = raw[offset..offset + 4].try_into().unwrap();
    let old_val = f32::from_le_bytes(old_bytes);
    if !old_val.is_finite() || !(0.0..=100_000.0).contains(&old_val) {
        let grey = egui::Color32::from_rgb(110, 110, 110);
        ui.horizontal(|ui| {
            ui.label(label);
            ui.label(egui::RichText::new(format!("raw={old_val}")).color(grey))
                .on_hover_text(
                    "Stored value is outside the supported 0..100000 range; shown read-only.",
                );
        });
        return;
    }
    let mut val = old_val;
    let enabled = navigation::editing_enabled(ui);
    let mut changed = false;
    ui.horizontal(|ui| {
        ui.label(label);
        changed = ui
            .add_enabled_ui(enabled, |ui| {
                ui.add_sized(
                    [80.0, 22.0],
                    egui::DragValue::new(&mut val)
                        .update_while_editing(false)
                        .range(0.0f32..=100_000.0f32)
                        .clamp_existing_to_range(false)
                        .speed(0.5),
                )
                .changed()
            })
            .inner;
    });
    if enabled && changed && (val - old_val).abs() > 0.001 {
        let new_bytes = val.to_le_bytes();
        raw[offset..offset + 4].copy_from_slice(&new_bytes);
        edits.push(FieldEdit {
            addr: base_addr + offset as u64,
            old_raw: old_bytes.to_vec(),
            new_raw: new_bytes.to_vec(),
            description: format!("{label}: {old_val:.0} \u{2192} {val:.0}"),
        });
    }
}

/// Editable 1–20 facility rating with FMLE-style color coding.
fn u8_drag_1_20(
    ui: &mut egui::Ui,
    label: &str,
    raw: &mut [u8],
    offset: usize,
    base_addr: u64,
    edits: &mut Vec<FieldEdit>,
) {
    if offset >= raw.len() {
        return;
    }
    let old = raw[offset];
    if !(1..=20).contains(&old) {
        let grey = egui::Color32::from_rgb(110, 110, 110);
        ui.horizontal(|ui| {
            ui.add_sized(
                [150.0, 22.0],
                egui::Label::new(egui::RichText::new(label).color(grey)),
            );
            ui.label(egui::RichText::new(format!("raw={old}")).color(grey));
        });
        return;
    }
    let color = field_widget::value_color(old);
    let mut val = old as i32;
    let enabled = navigation::editing_enabled(ui);
    let mut changed = false;
    ui.horizontal(|ui| {
        ui.add_sized([150.0, 22.0], egui::Label::new(label));
        ui.visuals_mut().override_text_color = Some(color);
        changed = ui
            .add_enabled_ui(enabled, |ui| {
                ui.add_sized(
                    [44.0, 22.0],
                    egui::DragValue::new(&mut val)
                        .update_while_editing(false)
                        .range(1..=20)
                        .speed(0.1),
                )
                .changed()
            })
            .inner;
    });
    if enabled && changed {
        let new_val = val.clamp(1, 20) as u8;
        if new_val != old {
            raw[offset] = new_val;
            edits.push(FieldEdit {
                addr: base_addr + offset as u64,
                old_raw: vec![old],
                new_raw: vec![new_val],
                description: format!("{label}: {old} \u{2192} {new_val}"),
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_f32(value: f32, enabled: bool) -> (Vec<FieldEdit>, [u8; 4]) {
        let ctx = egui::Context::default();
        ctx.data_mut(|d| d.insert_temp(egui::Id::new("editing_enabled"), enabled));
        let mut raw = value.to_le_bytes();
        let mut edits = Vec::new();
        let _ = ctx.run(egui::RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                f32_drag(ui, "Price", &mut raw, 0, 0x1000, &mut edits);
            });
        });
        (edits, raw)
    }

    #[test]
    fn invalid_or_out_of_range_f32_is_not_mutated() {
        for v in [f32::NAN, f32::INFINITY, -5.0, 200_000.0] {
            for enabled in [true, false] {
                let (edits, raw) = run_f32(v, enabled);
                assert!(edits.is_empty());
                assert_eq!(raw, v.to_le_bytes());
            }
        }
    }

    #[test]
    fn disabled_in_range_f32_is_not_mutated() {
        let (edits, raw) = run_f32(50.0, false);
        assert!(edits.is_empty());
        assert_eq!(raw, 50.0f32.to_le_bytes());
    }

    #[test]
    fn mirror_budget_finance_updates_budget_when_finance_edited() {
        let mut club = ClubView {
            addr: 0x1000,
            index: 0,
            raw: vec![0; 0x200],
            uid: 100,
            full_name: "Test Club".to_string(),
            short_name: "Test".to_string(),
            info_raw: None,
            info_addr: None,
            finance_raw: Some(vec![0; 0x830]),
            finance_addr: Some(0x2000),
            stadium_raw: None,
            senior_team_raw: None,
            senior_team_addr: None,
            budget_raw: Some(vec![0; 0x20]),
            budget_addr: Some(0x3000),
            stadium_entity_raw: None,
            stadium_name: None,
            stadium_name2: None,
        };

        let mut edits = Vec::new();
        mirror_budget_finance(
            &mut club,
            "c_transfer_budget",
            "finance",
            25_000_000,
            "Transfer Budget",
            crate::settings::Currency::Eur,
            &mut edits,
        );

        assert_eq!(edits.len(), 1);
        assert_eq!(edits[0].addr, 0x3000 + 4);
        assert_eq!(
            club.budget_raw.as_ref().unwrap()[4..8],
            25_000_000i32.to_le_bytes()
        );
    }
}
