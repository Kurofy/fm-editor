//! Club detail panel: Overview, Facilities, Finance, Stadium tabs.

use eframe::egui;

use crate::bridge::ClubView;
use crate::fields::{self, FieldLocation};
use crate::ui::field_widget::{self, FieldEdit};
use crate::ui::{entity_header, navigation};

#[derive(PartialEq, Clone, Copy)]
pub enum ClubTab {
    Overview,
    Facilities,
    Finance,
    Stadium,
    Affiliations,
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
        ui.selectable_value(tab, ClubTab::Affiliations, "Affiliations");
    });
    ui.separator();

    egui::ScrollArea::vertical().show(ui, |ui| match tab {
        ClubTab::Overview => show_overview(schemas, ui, club, &mut edits),
        ClubTab::Facilities => show_facilities(schemas, ui, club, &mut edits),
        ClubTab::Finance => show_finance(schemas, ui, club, currency, &mut edits),
        ClubTab::Stadium => show_stadium(schemas, ui, club),
        ClubTab::Affiliations => show_affiliations(schemas, ui, club, *currency, &mut edits),
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

fn show_affiliations(
    _schemas: &'static crate::fields::VersionedSchemas,
    ui: &mut egui::Ui,
    club: &mut ClubView,
    currency: crate::settings::Currency,
    edits: &mut Vec<FieldEdit>,
) {
    ui.horizontal(|ui| {
        ui.strong("Affiliations");
        ui.add_space(16.0);
        if ui.button("➕ Add Affiliation").clicked() {
            let new_aff = crate::affiliation::AffiliationView::new_default(club.uid, &club.full_name);
            club.affiliations.push(new_aff);
        }
    });
    ui.separator();

    if club.affiliations.is_empty() {
        ui.label("No affiliations found for this club.");
        return;
    }

    let selected_idx_id = ui.make_persistent_id("club_aff_selected");
    let mut selected_idx = ui.data(|d| d.get_temp::<usize>(selected_idx_id)).unwrap_or(0);
    if selected_idx >= club.affiliations.len() {
        selected_idx = club.affiliations.len().saturating_sub(1);
    }

    ui.horizontal_top(|ui| {
        // Left Column: Affiliations List
        egui::Frame::group(ui.style()).show(ui, |ui| {
            ui.set_width(320.0);
            ui.horizontal(|ui| {
                ui.strong("Teams");
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.strong("Type");
                });
            });
            ui.separator();

            let mut to_remove = None;
            for (i, aff) in club.affiliations.iter().enumerate() {
                let is_selected = i == selected_idx;
                let display_name = if aff.parent_club_name == club.full_name {
                    &aff.feeder_club_name
                } else {
                    &aff.parent_club_name
                };

                let label_text = format!("{:<20}  {}", display_name, aff.affiliation_type.label());
                let resp = ui.selectable_label(is_selected, label_text);
                if resp.clicked() {
                    selected_idx = i;
                    ui.data_mut(|d| d.insert_temp(selected_idx_id, selected_idx));
                }
            }

            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if ui.button("➕ Add").clicked() {
                    let new_aff = crate::affiliation::AffiliationView::new_default(club.uid, &club.full_name);
                    club.affiliations.push(new_aff);
                    selected_idx = club.affiliations.len() - 1;
                    ui.data_mut(|d| d.insert_temp(selected_idx_id, selected_idx));
                }
                if !club.affiliations.is_empty() && ui.button("🗑 Remove").clicked() {
                    to_remove = Some(selected_idx);
                }
            });

            if let Some(idx) = to_remove {
                if idx < club.affiliations.len() {
                    club.affiliations.remove(idx);
                    if selected_idx >= club.affiliations.len() {
                        selected_idx = club.affiliations.len().saturating_sub(1);
                    }
                    ui.data_mut(|d| d.insert_temp(selected_idx_id, selected_idx));
                }
            }
        });

        ui.add_space(12.0);

        // Right Column: Details of Selected Affiliation
        if let Some(aff) = club.affiliations.get_mut(selected_idx) {
            ui.vertical(|ui| {
                // Card 1: Information
                egui::Frame::group(ui.style()).show(ui, |ui| {
                    ui.strong("Information");
                    ui.separator();

                    egui::Grid::new("aff_info_grid")
                        .num_columns(2)
                        .spacing([12.0, 6.0])
                        .show(ui, |ui| {
                            // Parent Club
                            ui.label("Parent Club");
                            ui.text_edit_singleline(&mut aff.parent_club_name);
                            ui.end_row();

                            // Feeder Club
                            ui.label("Feeder Club");
                            ui.text_edit_singleline(&mut aff.feeder_club_name);
                            ui.end_row();

                            // Start Date
                            ui.label("Start Date");
                            ui.horizontal(|ui| {
                                let (mut d, mut m, mut y) = aff.start_date_dmy();
                                let prev_d = d;
                                let prev_m = m;
                                let prev_y = y;
                                ui.add(egui::DragValue::new(&mut d).range(1..=31).prefix("D: "));
                                ui.add(egui::DragValue::new(&mut m).range(1..=12).prefix("M: "));
                                ui.add(egui::DragValue::new(&mut y).range(1900..=2100).prefix("Y: "));
                                if d != prev_d || m != prev_m || y != prev_y {
                                    let old_doy = aff.start_doy;
                                    let old_yr = aff.start_year;
                                    aff.set_start_date_dmy(d, m, y);
                                    if aff.addr > 0 && (aff.start_doy != old_doy || aff.start_year != old_yr) {
                                        let mut old_bytes = [0u8; 4];
                                        old_bytes[0..2].copy_from_slice(&old_doy.to_le_bytes());
                                        old_bytes[2..4].copy_from_slice(&old_yr.to_le_bytes());
                                        let mut new_bytes = [0u8; 4];
                                        new_bytes[0..2].copy_from_slice(&aff.start_doy.to_le_bytes());
                                        new_bytes[2..4].copy_from_slice(&aff.start_year.to_le_bytes());
                                        if aff.raw.len() >= 0x1C {
                                            aff.raw[0x18..0x1C].copy_from_slice(&new_bytes);
                                        }
                                        edits.push(FieldEdit {
                                            addr: aff.addr + 0x18,
                                            old_raw: old_bytes.to_vec(),
                                            new_raw: new_bytes.to_vec(),
                                            description: format!(
                                                "Start Date: {:02}/{:02}/{} \u{2192} {:02}/{:02}/{}",
                                                prev_d, prev_m, prev_y, d, m, y
                                            ),
                                        });
                                    }
                                }
                            });
                            ui.end_row();

                            // End Date
                            ui.label("End Date");
                            ui.horizontal(|ui| {
                                let (mut d, mut m, mut y) = aff.end_date_dmy();
                                let prev_d = d;
                                let prev_m = m;
                                let prev_y = y;
                                ui.add(egui::DragValue::new(&mut d).range(1..=31).prefix("D: "));
                                ui.add(egui::DragValue::new(&mut m).range(1..=12).prefix("M: "));
                                ui.add(egui::DragValue::new(&mut y).range(1900..=2100).prefix("Y: "));
                                let is_perm = aff.end_year == 1900;
                                if ui.selectable_label(is_perm, "Permanent").clicked() {
                                    d = 1;
                                    m = 1;
                                    y = 1900;
                                }
                                if d != prev_d || m != prev_m || y != prev_y {
                                    let old_doy = aff.end_doy;
                                    let old_yr = aff.end_year;
                                    aff.set_end_date_dmy(d, m, y);
                                    if aff.addr > 0 && (aff.end_doy != old_doy || aff.end_year != old_yr) {
                                        let mut old_bytes = [0u8; 4];
                                        old_bytes[0..2].copy_from_slice(&old_doy.to_le_bytes());
                                        old_bytes[2..4].copy_from_slice(&old_yr.to_le_bytes());
                                        let mut new_bytes = [0u8; 4];
                                        new_bytes[0..2].copy_from_slice(&aff.end_doy.to_le_bytes());
                                        new_bytes[2..4].copy_from_slice(&aff.end_year.to_le_bytes());
                                        if aff.raw.len() >= 0x20 {
                                            aff.raw[0x1C..0x20].copy_from_slice(&new_bytes);
                                        }
                                        edits.push(FieldEdit {
                                            addr: aff.addr + 0x1C,
                                            old_raw: old_bytes.to_vec(),
                                            new_raw: new_bytes.to_vec(),
                                            description: format!(
                                                "End Date: {:02}/{:02}/{} \u{2192} {:02}/{:02}/{}",
                                                prev_d, prev_m, prev_y, d, m, y
                                            ),
                                        });
                                    }
                                }
                            });
                            ui.end_row();

                            // Affiliation Type
                            ui.label("Affiliation Type");
                            egui::ComboBox::from_id_salt("aff_type_combo")
                                .selected_text(aff.affiliation_type.label())
                                .show_ui(ui, |ui| {
                                    for &t in crate::affiliation::AffiliationType::ALL {
                                        if ui
                                            .selectable_value(&mut aff.affiliation_type, t, t.label())
                                            .changed()
                                        {
                                            if aff.addr > 0 {
                                                let old_val = if aff.raw.len() >= 0x24 {
                                                    u32::from_le_bytes(aff.raw[0x20..0x24].try_into().unwrap())
                                                } else {
                                                    0
                                                };
                                                let new_val = t.to_u32();
                                                if aff.raw.len() >= 0x24 {
                                                    aff.raw[0x20..0x24].copy_from_slice(&new_val.to_le_bytes());
                                                }
                                                edits.push(FieldEdit {
                                                    addr: aff.addr + 0x20,
                                                    old_raw: old_val.to_le_bytes().to_vec(),
                                                    new_raw: new_val.to_le_bytes().to_vec(),
                                                    description: format!(
                                                        "Affiliation Type: {} \u{2192} {}",
                                                        crate::affiliation::AffiliationType::from_u32(old_val).label(),
                                                        t.label()
                                                    ),
                                                });
                                            }
                                        }
                                    }
                                });
                            ui.end_row();

                            // Annual Fee
                            ui.label("Annual Fee");
                            let mut fee = aff.annual_fee;
                            if ui
                                .add(egui::DragValue::new(&mut fee).prefix(currency.symbol()).speed(1000))
                                .changed()
                            {
                                let old_fee = aff.annual_fee;
                                aff.annual_fee = fee;
                                if aff.addr > 0 {
                                    if aff.raw.len() >= 0x28 {
                                        aff.raw[0x24..0x28].copy_from_slice(&fee.to_le_bytes());
                                    }
                                    edits.push(FieldEdit {
                                        addr: aff.addr + 0x24,
                                        old_raw: old_fee.to_le_bytes().to_vec(),
                                        new_raw: fee.to_le_bytes().to_vec(),
                                        description: format!(
                                            "Annual Fee: {}{} \u{2192} {}{}",
                                            currency.symbol(),
                                            old_fee,
                                            currency.symbol(),
                                            fee
                                        ),
                                    });
                                }
                            }
                            ui.end_row();

                            // % Chance Of Friendly
                            ui.label("% Chance Of Friendly");
                            ui.horizontal(|ui| {
                                let mut enabled = aff.friendly_chance_enabled;
                                if ui.checkbox(&mut enabled, "").changed() {
                                    aff.friendly_chance_enabled = enabled;
                                    let val = if enabled { aff.friendly_chance.max(1) } else { 0 };
                                    write_aff_u8(aff, 0x28, val, "% Chance Of Friendly", edits);
                                }
                                let mut val = aff.friendly_chance;
                                if ui
                                    .add_enabled(
                                        enabled,
                                        egui::DragValue::new(&mut val).range(0..=100),
                                    )
                                    .changed()
                                {
                                    aff.friendly_chance = val;
                                    if enabled {
                                        write_aff_u8(aff, 0x28, val, "% Chance Of Friendly", edits);
                                    }
                                }
                            });
                            ui.end_row();

                            // Maximum No. Players Loaned
                            ui.label("Maximum No. Players Loaned");
                            ui.horizontal(|ui| {
                                let mut enabled = aff.max_loaned_enabled;
                                if ui.checkbox(&mut enabled, "").changed() {
                                    aff.max_loaned_enabled = enabled;
                                    let val = if enabled { aff.max_loaned } else { 0 };
                                    write_aff_u8(aff, 0x29, val, "Max Players Loaned", edits);
                                }
                                let mut val = aff.max_loaned;
                                if ui
                                    .add_enabled(
                                        enabled,
                                        egui::DragValue::new(&mut val).range(0..=99),
                                    )
                                    .changed()
                                {
                                    aff.max_loaned = val;
                                    if enabled {
                                        write_aff_u8(aff, 0x29, val, "Max Players Loaned", edits);
                                    }
                                }
                            });
                            ui.end_row();

                            // Percentage of loan player wage paid by parent club
                            ui.label("Percentage of loan player wage paid by parent club");
                            ui.horizontal(|ui| {
                                let mut enabled = aff.wage_percentage_enabled;
                                if ui.checkbox(&mut enabled, "").changed() {
                                    aff.wage_percentage_enabled = enabled;
                                    let val = if enabled { aff.wage_percentage.max(1) } else { 0 };
                                    write_aff_u8(aff, 0x2A, val, "Wage % Paid", edits);
                                }
                                let mut val = aff.wage_percentage;
                                if ui
                                    .add_enabled(
                                        enabled,
                                        egui::DragValue::new(&mut val).range(0..=100),
                                    )
                                    .changed()
                                {
                                    aff.wage_percentage = val;
                                    if enabled {
                                        write_aff_u8(aff, 0x2A, val, "Wage % Paid", edits);
                                    }
                                }
                            });
                            ui.end_row();
                        });
                });

                ui.add_space(8.0);

                // Card 2: Friendship Benefits
                egui::Frame::group(ui.style()).show(ui, |ui| {
                    ui.strong("Friendship Benefits");
                    ui.separator();

                    use crate::affiliation::FriendshipBenefits;

                    let col1_benefits = [
                        ("Players Move Freely", FriendshipBenefits::PLAYERS_MOVE_FREELY),
                        ("First Option To Buy", FriendshipBenefits::FIRST_OPTION_TO_BUY),
                        ("Youth Players Training", FriendshipBenefits::YOUTH_PLAYERS_TRAINING),
                        ("Same Board", FriendshipBenefits::SAME_BOARD),
                        ("Help With Facilities", FriendshipBenefits::HELP_WITH_FACILITIES),
                        ("Marketing Relations", FriendshipBenefits::MARKETING_RELATIONS),
                        ("Cannot Play In Same Division", FriendshipBenefits::CANNOT_PLAY_IN_SAME_DIVISION),
                        ("Loan Players from Sub Team", FriendshipBenefits::LOAN_PLAYERS_FROM_SUB_TEAM),
                        ("Uses Same Kit", FriendshipBenefits::USES_SAME_KIT),
                    ];

                    let col2_benefits = [
                        ("Players Go On Loan", FriendshipBenefits::PLAYERS_GO_ON_LOAN),
                        ("Play Friendlies", FriendshipBenefits::PLAY_FRIENDLIES),
                        ("Permanent Deal", FriendshipBenefits::PERMANENT_DEAL),
                        ("Financial Help", FriendshipBenefits::FINANCIAL_HELP),
                        ("Renewable", FriendshipBenefits::RENEWABLE),
                        ("Scouting Knowledge Shared", FriendshipBenefits::SCOUTING_KNOWLEDGE_SHARED),
                        ("Uses All Parent Club Facilities", FriendshipBenefits::USES_ALL_PARENT_FACILITIES),
                        ("Uses sub team training", FriendshipBenefits::USES_SUB_TEAM_TRAINING),
                    ];

                    ui.columns(2, |cols| {
                        for (label, flag) in col1_benefits {
                            benefit_checkbox(&mut cols[0], aff, label, flag, edits);
                        }
                        for (label, flag) in col2_benefits {
                            benefit_checkbox(&mut cols[1], aff, label, flag, edits);
                        }
                    });
                });
            });
        }
    });
}

fn benefit_checkbox(
    ui: &mut egui::Ui,
    aff: &mut crate::affiliation::AffiliationView,
    label: &str,
    flag: u32,
    edits: &mut Vec<FieldEdit>,
) {
    let mut checked = aff.benefits.has(flag);
    if ui.checkbox(&mut checked, label).changed() {
        let old_flags = aff.benefits.flags;
        aff.benefits.set(flag, checked);
        if aff.addr > 0 && aff.benefits.flags != old_flags {
            let old_bytes = old_flags.to_le_bytes().to_vec();
            let new_bytes = aff.benefits.flags.to_le_bytes().to_vec();
            if aff.raw.len() >= 0x30 {
                aff.raw[0x2C..0x30].copy_from_slice(&new_bytes);
            }
            edits.push(FieldEdit {
                addr: aff.addr + 0x2C,
                old_raw: old_bytes,
                new_raw: new_bytes,
                description: format!("Friendship Benefit '{}' set to {}", label, checked),
            });
        }
    }
}

fn write_aff_u8(
    aff: &mut crate::affiliation::AffiliationView,
    offset: u64,
    new_val: u8,
    label: &str,
    edits: &mut Vec<FieldEdit>,
) {
    let old_val = if aff.raw.len() > offset as usize {
        aff.raw[offset as usize]
    } else {
        0
    };
    if old_val == new_val {
        return;
    }
    if aff.raw.len() > offset as usize {
        aff.raw[offset as usize] = new_val;
    }
    if aff.addr > 0 {
        edits.push(FieldEdit {
            addr: aff.addr + offset,
            old_raw: vec![old_val],
            new_raw: vec![new_val],
            description: format!("{label}: {old_val} \u{2192} {new_val}"),
        });
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
            nickname: None,
            stadium_entity_raw: None,
            stadium_name: None,
            stadium_name2: None,
            affiliations: Vec::new(),
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

    #[test]
    fn test_affiliations_benefit_toggle() {
        let mut aff = crate::affiliation::AffiliationView::new_default(100, "Porto");
        aff.addr = 0x5000;
        let old_flags = aff.benefits.flags;
        aff.benefits.set(crate::affiliation::FriendshipBenefits::FIRST_OPTION_TO_BUY, true);
        assert!(aff.benefits.has(crate::affiliation::FriendshipBenefits::FIRST_OPTION_TO_BUY));
        assert_ne!(aff.benefits.flags, old_flags);
    }

    #[test]
    fn test_write_aff_u8_generates_edit() {
        let mut aff = crate::affiliation::AffiliationView::new_default(100, "Porto");
        aff.addr = 0x5000;
        let mut edits = Vec::new();
        write_aff_u8(&mut aff, 0x28, 75, "% Chance Of Friendly", &mut edits);
        assert_eq!(edits.len(), 1);
        assert_eq!(edits[0].addr, 0x5000 + 0x28);
        assert_eq!(edits[0].new_raw, vec![75]);
        assert_eq!(aff.raw[0x28], 75);
    }
}
