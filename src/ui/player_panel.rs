//! Player detail panel with identity, attributes, positions, traits and contract data.

use eframe::egui;

use crate::fields::{self, Category, Entity};
use crate::player::{self, PlayerRecord};
use crate::ratings;
use crate::settings::{WagePeriod, format_wage};
use crate::ui::field_widget::{self, FieldEdit};

#[derive(PartialEq)]
pub enum PlayerTab {
    Overview,
    PositionsTraits,
    Contract,
}

pub fn show(
    schemas: &'static crate::fields::VersionedSchemas,
    ui: &mut egui::Ui,
    player: &mut PlayerRecord,
    tab: &mut PlayerTab,
    addr: u64,
    wage_period: WagePeriod,
    currency: crate::settings::Currency,
    face_pack_path: Option<&str>,
    clubs: Option<&[crate::cache::CachedClub]>,
    nations: Option<&[crate::cache::CachedNation]>,
) -> Vec<FieldEdit> {
    let mut edits = Vec::new();
    crate::ui::entity_header::show(
        ui,
        player.name.as_deref().unwrap_or("Unknown"),
        "Player",
        player
            .field_u32("p_uid")
            .map(|v| v.to_string())
            .unwrap_or_else(|| "Unknown".into()),
        crate::ui::entity_header::age(schemas, &player.raw, "p_dob"),
        player.club_name.as_deref(),
        player.club_addr,
        player.nationality.as_deref(),
    );
    if *tab == PlayerTab::Contract && player.contract_raw.is_none() {
        *tab = PlayerTab::Overview;
    }

    // Sub-tab bar
    ui.horizontal(|ui| {
        ui.selectable_value(tab, PlayerTab::Overview, "Overview");
        ui.selectable_value(tab, PlayerTab::PositionsTraits, "Positions & Traits");
        if player.contract_raw.is_some() {
            ui.selectable_value(tab, PlayerTab::Contract, "Contract & Transfer");
        }
    });
    ui.separator();

    egui::ScrollArea::both().show(ui, |ui| match tab {
        PlayerTab::Overview => show_overview(
            schemas,
            ui,
            player,
            addr,
            &mut edits,
            wage_period,
            currency,
            face_pack_path,
            clubs,
            nations,
        ),
        PlayerTab::PositionsTraits => show_positions_traits(schemas, ui, player, addr, &mut edits),
        PlayerTab::Contract => show_contract(schemas, ui, player, clubs, &mut edits),
    });

    // Widgets address edits as `addr + schema offset`. A player-coach's person
    // block sits `person_shift` bytes later in memory, so move edits that land
    // in it (DOB, traits, ...) to where the game stores them.
    if player.person_shift != 0 {
        let size = schemas.player_struct_size as u64;
        for edit in &mut edits {
            let rel = edit.addr.wrapping_sub(addr);
            if rel < size {
                edit.addr = addr + player::player_phys_offset(schemas, rel, player.person_shift);
            }
        }
    }

    edits
}

// ---------------------------------------------------------------------------
// Overview (FMRTE-style multi-column layout)
// ---------------------------------------------------------------------------

#[derive(Clone, Default)]
struct MoveClubDraft {
    search_query: String,
    selected_club_addr: Option<u64>,
    selected_club_name: String,
    on_loan: bool,
}

struct RoleProfile {
    name: &'static str,
    is_gk: bool,
    key_attrs: &'static [&'static str],
    pref_attrs: &'static [&'static str],
}

const ROLE_PROFILES: &[RoleProfile] = &[
    RoleProfile {
        name: "Goalkeeper (GK)",
        is_gk: true,
        key_attrs: &[
            "aerial_reach", "command_of_area", "communication", "handling",
            "kicking", "one_on_ones", "reflexes", "throwing", "agility",
            "concentration", "positioning",
        ],
        pref_attrs: &[
            "first_touch", "passing", "rushing_out", "anticipation",
            "bravery", "composure", "decisions",
        ],
    },
    RoleProfile {
        name: "Sweeper Keeper (GK)",
        is_gk: true,
        key_attrs: &[
            "command_of_area", "kicking", "one_on_ones", "reflexes",
            "rushing_out", "anticipation", "composure", "concentration",
            "positioning", "agility",
        ],
        pref_attrs: &[
            "aerial_reach", "communication", "first_touch", "handling",
            "passing", "throwing", "decisions", "vision", "acceleration",
        ],
    },
    RoleProfile {
        name: "Central Defender (DC)",
        is_gk: false,
        key_attrs: &[
            "heading", "marking", "tackling", "positioning",
            "jumping_reach", "strength",
        ],
        pref_attrs: &[
            "aggression", "anticipation", "bravery", "composure",
            "concentration", "decisions", "pace",
        ],
    },
    RoleProfile {
        name: "Ball Playing Defender (DC)",
        is_gk: false,
        key_attrs: &[
            "heading", "marking", "passing", "tackling", "composure",
            "positioning", "jumping_reach", "strength",
        ],
        pref_attrs: &[
            "first_touch", "technique", "aggression", "anticipation",
            "bravery", "concentration", "decisions", "vision", "pace",
        ],
    },
    RoleProfile {
        name: "Full Back (DL/DR)",
        is_gk: false,
        key_attrs: &[
            "marking", "tackling", "anticipation", "concentration",
            "positioning", "teamwork", "pace", "stamina",
        ],
        pref_attrs: &[
            "crossing", "dribbling", "passing", "technique",
            "decisions", "work_rate", "acceleration", "agility",
        ],
    },
    RoleProfile {
        name: "Wing Back (WBL/WBR)",
        is_gk: false,
        key_attrs: &[
            "crossing", "dribbling", "marking", "tackling", "off_the_ball",
            "teamwork", "work_rate", "acceleration", "pace", "stamina",
        ],
        pref_attrs: &[
            "first_touch", "passing", "technique", "anticipation",
            "concentration", "decisions", "positioning", "agility", "balance",
        ],
    },
    RoleProfile {
        name: "Defensive Midfielder (DM)",
        is_gk: false,
        key_attrs: &[
            "tackling", "anticipation", "concentration", "positioning",
            "teamwork", "stamina",
        ],
        pref_attrs: &[
            "first_touch", "marking", "passing", "aggression", "bravery",
            "composure", "decisions", "work_rate", "strength",
        ],
    },
    RoleProfile {
        name: "Deep Lying Playmaker (DM/MC)",
        is_gk: false,
        key_attrs: &[
            "first_touch", "passing", "technique", "composure",
            "decisions", "teamwork", "vision",
        ],
        pref_attrs: &[
            "anticipation", "off_the_ball", "positioning", "balance",
        ],
    },
    RoleProfile {
        name: "Box-to-Box Midfielder (MC)",
        is_gk: false,
        key_attrs: &[
            "passing", "tackling", "off_the_ball", "teamwork",
            "work_rate", "stamina",
        ],
        pref_attrs: &[
            "dribbling", "finishing", "first_touch", "long_shots", "technique",
            "aggression", "anticipation", "bravery", "composure",
            "concentration", "decisions", "positioning", "acceleration",
            "pace", "strength",
        ],
    },
    RoleProfile {
        name: "Advanced Playmaker (MC/AMC)",
        is_gk: false,
        key_attrs: &[
            "first_touch", "passing", "technique", "composure", "decisions",
            "flair", "off_the_ball", "teamwork", "vision",
        ],
        pref_attrs: &["dribbling", "anticipation", "agility"],
    },
    RoleProfile {
        name: "Winger (ML/MR/AML/AMR)",
        is_gk: false,
        key_attrs: &[
            "crossing", "dribbling", "technique", "off_the_ball",
            "acceleration", "pace",
        ],
        pref_attrs: &[
            "first_touch", "passing", "flair", "work_rate",
            "agility", "balance", "stamina",
        ],
    },
    RoleProfile {
        name: "Inside Forward (AML/AMR)",
        is_gk: false,
        key_attrs: &[
            "dribbling", "finishing", "first_touch", "technique",
            "off_the_ball", "acceleration", "agility", "pace",
        ],
        pref_attrs: &[
            "long_shots", "passing", "anticipation", "composure",
            "flair", "balance", "stamina",
        ],
    },
    RoleProfile {
        name: "Shadow Striker (AMC)",
        is_gk: false,
        key_attrs: &[
            "dribbling", "finishing", "first_touch", "anticipation",
            "composure", "off_the_ball", "acceleration",
        ],
        pref_attrs: &[
            "passing", "technique", "concentration", "decisions",
            "flair", "work_rate", "agility", "balance", "pace", "stamina",
        ],
    },
    RoleProfile {
        name: "Poacher (ST)",
        is_gk: false,
        key_attrs: &[
            "finishing", "first_touch", "anticipation", "composure",
            "off_the_ball",
        ],
        pref_attrs: &[
            "heading", "technique", "decisions", "acceleration",
            "agility", "balance", "pace",
        ],
    },
    RoleProfile {
        name: "Complete Forward (ST)",
        is_gk: false,
        key_attrs: &[
            "dribbling", "finishing", "first_touch", "heading", "technique",
            "anticipation", "composure", "off_the_ball", "acceleration",
            "agility", "jumping_reach", "pace", "strength",
        ],
        pref_attrs: &[
            "long_shots", "passing", "decisions", "teamwork", "vision",
            "work_rate", "balance", "stamina",
        ],
    },
];

const LOWER_IS_BETTER: &[&str] = &[
    "eccentricity",
    "p_dirtiness",
    "injury_proneness",
    "p_controversy",
];

fn show_overview(
    schemas: &'static crate::fields::VersionedSchemas,
    ui: &mut egui::Ui,
    player: &mut PlayerRecord,
    addr: u64,
    edits: &mut Vec<FieldEdit>,
    wage_period: WagePeriod,
    currency: crate::settings::Currency,
    face_pack_path: Option<&str>,
    clubs: Option<&[crate::cache::CachedClub]>,
    nations: Option<&[crate::cache::CachedNation]>,
) {
    let p_dob = schemas.field("p_dob").unwrap();
    let p_ca = schemas.field("p_ca").unwrap();
    let p_pa = schemas.field("p_pa").unwrap();
    let p_current_rep = schemas.field("p_current_rep").unwrap();
    let p_home_rep = schemas.field("p_home_rep").unwrap();
    let p_world_rep = schemas.field("p_world_rep").unwrap();

    let is_natural_gk = player.field_u8("pos_gk").unwrap_or(1) >= 15;

    let role_id = egui::Id::new(("player_role_idx", addr));
    let default_role_idx = if is_natural_gk { 0 } else { 14 };
    let mut role_idx = ui
        .data(|d| d.get_temp::<usize>(role_id))
        .unwrap_or(default_role_idx)
        .min(ROLE_PROFILES.len() - 1);

    let gk_toggle_id = egui::Id::new(("player_tech_gk_toggle", addr));
    let mut show_gk_attrs = ui
        .data(|d| d.get_temp::<bool>(gk_toggle_id))
        .unwrap_or(is_natural_gk);

    let role = &ROLE_PROFILES[role_idx];

    ui.horizontal_top(|ui| {
        // ===================================================================
        // COLUMN 1: Personal Details, Reputation, Ability & Condition (~290px)
        // ===================================================================
        ui.vertical(|ui| {
            ui.set_width(290.0);

            // Card 1A: Personal Details
            egui::Frame::group(ui.style()).show(ui, |ui| {
                ui.strong("Personal Details");
                ui.separator();

                // Face image when configured
                let face_uid = player.field_u32("p_uid").filter(|&uid| uid > 0);
                if let (Some(path), Some(uid)) = (face_pack_path, face_uid) {
                    let img_path = std::path::Path::new(path).join(format!("{uid}.png"));
                    let uri = format!("file:///{}", img_path.to_string_lossy().replace('\\', "/"));
                    ui.add(egui::Image::new(uri).max_size(egui::vec2(64.0, 64.0)));
                    ui.add_space(2.0);
                }

                egui::Grid::new(("personal_identity_grid", addr))
                    .num_columns(2)
                    .spacing([8.0, 4.0])
                    .show(ui, |ui| {
                        ui.label("Name");
                        ui.label(egui::RichText::new(player.name.as_deref().unwrap_or("Unknown")).strong());
                        ui.end_row();

                        let uid_str = player
                            .field_u32("p_uid")
                            .map(|v| v.to_string())
                            .unwrap_or_else(|| "Unknown".into());
                        ui.label("Unique ID");
                        ui.horizontal(|ui| {
                            ui.label(&uid_str);
                            if ui.small_button("📋").on_hover_text("Copy UID").clicked() {
                                ui.ctx().copy_text(uid_str);
                            }
                        });
                        ui.end_row();

                        // Nationality dropdown
                        ui.label("Nationality");
                        let current_nat = player.nationality.clone().unwrap_or_else(|| "Unknown".to_string());
                        egui::ComboBox::from_id_salt(("player_nationality_combo", addr))
                            .selected_text(&current_nat)
                            .width(140.0)
                            .show_ui(ui, |ui| {
                                let search_id = egui::Id::new(("nat_search_filter", addr));
                                let mut filter = ui.data(|d| d.get_temp::<String>(search_id)).unwrap_or_default();
                                ui.horizontal(|ui| {
                                    ui.label("🔍");
                                    ui.add(egui::TextEdit::singleline(&mut filter).hint_text("Search...").desired_width(100.0));
                                });
                                ui.separator();
                                let query = filter.trim().to_lowercase();

                                if let Some(nation_list) = nations {
                                    for nat in nation_list.iter().filter(|n| query.is_empty() || n.name.to_lowercase().contains(&query)) {
                                        let is_sel = current_nat == nat.name;
                                        if ui.selectable_label(is_sel, &nat.name).clicked() {
                                            if let Some(off) = schemas.offset_of("p_nation_ptr") {
                                                if off + 8 <= player.raw.len() {
                                                    let old_bytes = player.raw[off..off + 8].to_vec();
                                                    let new_bytes = nat.addr.to_le_bytes().to_vec();
                                                    player.raw[off..off + 8].copy_from_slice(&new_bytes);
                                                    edits.push(FieldEdit {
                                                        addr: addr + off as u64,
                                                        old_raw: old_bytes,
                                                        new_raw: new_bytes,
                                                        description: format!("Nationality: {} \u{2192} {}", current_nat, nat.name),
                                                    });
                                                    player.nationality = Some(nat.name.clone());
                                                }
                                            }
                                        }
                                    }
                                }
                                ui.data_mut(|d| d.insert_temp(search_id, filter));
                            });
                        ui.end_row();

                        if let Some(wage) = player.field_u32("p_contract_wage") {
                            ui.label("Wage");
                            ui.label(format_wage(wage, wage_period, currency));
                            ui.end_row();
                        }
                    });

                ui.add_space(4.0);
                // Editable Date of Birth
                edits.extend(field_widget::dob_edit(ui, &mut player.raw, &p_dob, addr));

                // Appearance dropdowns
                if let Some(f) = schemas.field("p_hair_colour") {
                    edits.extend(field_widget::choice_u8_edit(
                        ui,
                        "Hair Colour",
                        &mut player.raw,
                        &f,
                        addr,
                        HAIR_COLOURS,
                    ));
                }
                if let Some(f) = schemas.field("p_hair_length") {
                    edits.extend(field_widget::choice_u8_edit(
                        ui,
                        "Hair Length",
                        &mut player.raw,
                        &f,
                        addr,
                        HAIR_LENGTHS,
                    ));
                }
                if let Some(f) = schemas.field("p_skin_tone") {
                    ui.horizontal(|ui| {
                        edits.extend(field_widget::choice_u8_edit(
                            ui,
                            "Skin Colour",
                            &mut player.raw,
                            &f,
                            addr,
                            SKIN_TONES,
                        ));
                        if let Some(tone) = player.field_u8("p_skin_tone") {
                            let color = skin_tone_color(tone);
                            let (rect, _) = ui.allocate_exact_size(egui::vec2(16.0, 16.0), egui::Sense::hover());
                            ui.painter().rect_filled(rect, 2.0, color);
                        }
                    });
                }
                if let Some(f) = schemas.field("p_ethnicity") {
                    edits.extend(field_widget::choice_u8_edit(
                        ui,
                        "Ethnicity",
                        &mut player.raw,
                        &f,
                        addr,
                        ETHNICITIES,
                    ));
                }

                // Height & Weight
                if let Some(f) = schemas.field("p_height") {
                    edits.extend(field_widget::raw_u16_edit(ui, &f, &mut player.raw, 250, addr));
                }
                if let Some(f) = schemas.field("p_weight") {
                    edits.extend(field_widget::raw_u16_edit(ui, &f, &mut player.raw, 150, addr));
                }

                // Asking Price
                if let Some(f) = schemas.field("p_asking_price") {
                    edits.extend(field_widget::optional_money_edit(
                        ui,
                        "Asking Price",
                        &mut player.raw,
                        &f,
                        addr,
                    ));
                }

                // Caps / Goals
                editable_pair_row(
                    schemas,
                    ui,
                    "Caps / Goals",
                    "p_intl_caps",
                    "p_intl_goals",
                    &mut player.raw,
                    addr,
                    edits,
                );

                // Sub Caps / Goals
                editable_pair_row(
                    schemas,
                    ui,
                    "Sub Caps / Goals",
                    "p_u21_caps",
                    "p_u21_goals",
                    &mut player.raw,
                    addr,
                    edits,
                );

                // Left Foot / Right Foot
                editable_foot_row(schemas, ui, &mut player.raw, addr, edits);
            });

            ui.add_space(6.0);

            // Card 1B: Reputation Card
            egui::Frame::group(ui.style()).show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.strong("Reputation");
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.small_button("-").on_hover_text("Decrease reputation by 500").clicked() {
                            for key in ["p_home_rep", "p_current_rep", "p_world_rep"] {
                                if let Some(cur) = player.field_u16(key) {
                                    set_u16_value(
                                        schemas,
                                        &mut player.raw,
                                        addr,
                                        key,
                                        cur.saturating_sub(500),
                                        edits,
                                    );
                                }
                            }
                        }
                        if ui.small_button("+").on_hover_text("Increase reputation by 500").clicked() {
                            for key in ["p_home_rep", "p_current_rep", "p_world_rep"] {
                                if let Some(cur) = player.field_u16(key) {
                                    set_u16_value(
                                        schemas,
                                        &mut player.raw,
                                        addr,
                                        key,
                                        (cur + 500).min(10000),
                                        edits,
                                    );
                                }
                            }
                        }
                    });
                });
                ui.separator();
                if let Some(e) = field_widget::raw_u16_edit(ui, &p_home_rep, &mut player.raw, 10000, addr) {
                    edits.push(e);
                }
                if let Some(e) = field_widget::raw_u16_edit(ui, &p_current_rep, &mut player.raw, 10000, addr) {
                    edits.push(e);
                }
                if let Some(e) = field_widget::raw_u16_edit(ui, &p_world_rep, &mut player.raw, 10000, addr) {
                    edits.push(e);
                }
            });

            ui.add_space(6.0);

            // Card 1C: Ability Card
            egui::Frame::group(ui.style()).show(ui, |ui| {
                ui.strong("Ability");
                ui.separator();
                ui.horizontal(|ui| {
                    edits.extend(field_widget::ca_pa_edit(
                        ui,
                        &mut player.raw,
                        &p_ca,
                        &p_pa,
                        addr,
                    ));
                    let ca = player.field_u8("p_ca").unwrap_or(0) as i16;
                    let pa = player.field_u8("p_pa").unwrap_or(0) as i16;
                    ui.label(format!("({})", (pa - ca).max(0)));
                });

                let ca_val = player.field_u8("p_ca").unwrap_or(0);
                let pa_val = player.field_u8("p_pa").unwrap_or(0);
                ui.add(
                    egui::ProgressBar::new(ca_val as f32 / 200.0)
                        .text(format!("CA {ca_val} / PA {pa_val}")),
                );

                let rec_ca = recommended_ca(player);
                ui.horizontal(|ui| {
                    ui.label("Recommended CA");
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui
                            .button(format!("🛠 {rec_ca}"))
                            .on_hover_text("Click to set CA to Recommended CA")
                            .clicked()
                        {
                            set_raw_u8_value(schemas, &mut player.raw, addr, "p_ca", rec_ca, edits);
                        }
                    });
                });
            });

            // Card 1D: Condition Card
            let cond_fields = schemas.fields_for_category(Entity::Player, Category::Condition);
            if !cond_fields.is_empty() {
                ui.add_space(6.0);
                egui::Frame::group(ui.style()).show(ui, |ui| {
                    ui.strong("Condition & Morale");
                    ui.separator();
                    for f in &cond_fields {
                        if let Some(e) = field_widget::render_field(ui, f, &mut player.raw, addr) {
                            edits.push(e);
                        }
                    }
                });
            }
        });

        ui.add_space(8.0);

        // ===================================================================
        // COLUMN 2: Attributes with Role Highlighting (~470px)
        // ===================================================================
        ui.vertical(|ui| {
            ui.set_width(470.0);

            // Top sub-columns: Technical/GK and Mental side by side
            ui.horizontal_top(|ui| {
                // Sub-column 2A: Technical / GK Card
                ui.vertical(|ui| {
                    ui.set_width(230.0);
                    egui::Frame::group(ui.style()).show(ui, |ui| {
                        let mut fields = if show_gk_attrs {
                            let mut gk = schemas.fields_for_category(Entity::Player, Category::Goalkeeping);
                            if let Some(ft) = schemas.field("first_touch").cloned() {
                                gk.push(ft);
                            }
                            if let Some(pa) = schemas.field("passing").cloned() {
                                gk.push(pa);
                            }
                            gk
                        } else {
                            let mut tech = schemas.fields_for_category(Entity::Player, Category::Technical);
                            tech.extend(schemas.fields_for_category(Entity::Player, Category::SetPieces));
                            tech
                        };
                        fields.sort_by_key(|f| f.label);

                        ui.horizontal(|ui| {
                            if ui.selectable_label(!show_gk_attrs, "Technical").clicked() {
                                show_gk_attrs = false;
                                ui.data_mut(|d| d.insert_temp(gk_toggle_id, show_gk_attrs));
                            }
                            if ui.selectable_label(show_gk_attrs, "Goal Keeper").clicked() {
                                show_gk_attrs = true;
                                ui.data_mut(|d| d.insert_temp(gk_toggle_id, show_gk_attrs));
                            }
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                if ui.small_button("-").on_hover_text("Decrease all by 1").clicked() {
                                    for f in &fields {
                                        shift_attr_value(schemas, &mut player.raw, addr, f.key, -1, edits);
                                    }
                                }
                                if ui.small_button("+").on_hover_text("Increase all by 1").clicked() {
                                    for f in &fields {
                                        shift_attr_value(schemas, &mut player.raw, addr, f.key, 1, edits);
                                    }
                                }
                            });
                        });
                        ui.separator();

                        for f in &fields {
                            render_highlighted_attr(ui, f, role, &mut player.raw, addr, edits);
                        }
                    });
                });

                ui.add_space(6.0);

                // Sub-column 2B: Mental Card
                ui.vertical(|ui| {
                    ui.set_width(230.0);
                    egui::Frame::group(ui.style()).show(ui, |ui| {
                        let mut fields = schemas.fields_for_category(Entity::Player, Category::Mental);
                        fields.sort_by_key(|f| f.label);

                        ui.horizontal(|ui| {
                            ui.strong("Mental");
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                if ui.small_button("-").on_hover_text("Decrease all Mental by 1").clicked() {
                                    for f in &fields {
                                        shift_attr_value(schemas, &mut player.raw, addr, f.key, -1, edits);
                                    }
                                }
                                if ui.small_button("+").on_hover_text("Increase all Mental by 1").clicked() {
                                    for f in &fields {
                                        shift_attr_value(schemas, &mut player.raw, addr, f.key, 1, edits);
                                    }
                                }
                            });
                        });
                        ui.separator();

                        for f in &fields {
                            render_highlighted_attr(ui, f, role, &mut player.raw, addr, edits);
                        }
                    });
                });
            });

            ui.add_space(6.0);

            // Bottom sub-columns: Physical and Hidden/Personality side by side
            ui.horizontal_top(|ui| {
                // Sub-column 2C: Physical Card
                ui.vertical(|ui| {
                    ui.set_width(230.0);
                    egui::Frame::group(ui.style()).show(ui, |ui| {
                        let mut fields = schemas.fields_for_category(Entity::Player, Category::Physical);
                        fields.sort_by_key(|f| f.label);

                        ui.horizontal(|ui| {
                            ui.strong("Physical");
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                if ui.small_button("-").on_hover_text("Decrease all Physical by 1").clicked() {
                                    for f in &fields {
                                        shift_attr_value(schemas, &mut player.raw, addr, f.key, -1, edits);
                                    }
                                }
                                if ui.small_button("+").on_hover_text("Increase all Physical by 1").clicked() {
                                    for f in &fields {
                                        shift_attr_value(schemas, &mut player.raw, addr, f.key, 1, edits);
                                    }
                                }
                            });
                        });
                        ui.separator();

                        for f in &fields {
                            render_highlighted_attr(ui, f, role, &mut player.raw, addr, edits);
                        }
                    });
                });

                ui.add_space(6.0);

                // Sub-column 2D: Hidden & Personality
                ui.vertical(|ui| {
                    ui.set_width(230.0);

                    // Hidden Card
                    egui::Frame::group(ui.style()).show(ui, |ui| {
                        let fields = schemas.fields_for_category(Entity::Player, Category::Hidden);
                        ui.horizontal(|ui| {
                            ui.strong("Hidden");
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                if ui.small_button("-").clicked() {
                                    for f in &fields {
                                        shift_attr_value(schemas, &mut player.raw, addr, f.key, -1, edits);
                                    }
                                }
                                if ui.small_button("+").clicked() {
                                    for f in &fields {
                                        shift_attr_value(schemas, &mut player.raw, addr, f.key, 1, edits);
                                    }
                                }
                            });
                        });
                        ui.separator();
                        for f in &fields {
                            render_highlighted_attr(ui, f, role, &mut player.raw, addr, edits);
                        }
                    });

                    ui.add_space(6.0);

                    // Personality Card
                    egui::Frame::group(ui.style()).show(ui, |ui| {
                        let fields = schemas.fields_for_category(Entity::Player, Category::Personality);
                        ui.horizontal(|ui| {
                            ui.strong("Personality");
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                if ui.small_button("-").clicked() {
                                    for f in &fields {
                                        shift_attr_value(schemas, &mut player.raw, addr, f.key, -1, edits);
                                    }
                                }
                                if ui.small_button("+").clicked() {
                                    for f in &fields {
                                        shift_attr_value(schemas, &mut player.raw, addr, f.key, 1, edits);
                                    }
                                }
                            });
                        });
                        ui.separator();
                        for f in &fields {
                            render_highlighted_attr(ui, f, role, &mut player.raw, addr, edits);
                        }
                    });
                });
            });
        });

        ui.add_space(8.0);

        // ===================================================================
        // COLUMN 3: Transfers & Quick Actions (~260px)
        // ===================================================================
        ui.vertical(|ui| {
            ui.set_width(260.0);

            // Card 3A: Move to Club Card
            show_move_to_club_card(schemas, ui, player, addr, clubs, edits);

            ui.add_space(6.0);

            // Card 3B: Attributes Role Quick-Set Card
            egui::Frame::group(ui.style()).show(ui, |ui| {
                ui.strong("Role Profile");
                ui.separator();

                egui::ComboBox::from_id_salt(("role_profile_combo", addr))
                    .selected_text(ROLE_PROFILES[role_idx].name)
                    .width(230.0)
                    .show_ui(ui, |ui| {
                        for (i, rp) in ROLE_PROFILES.iter().enumerate() {
                            if ui.selectable_value(&mut role_idx, i, rp.name).changed() {
                                ui.data_mut(|d| d.insert_temp(role_id, role_idx));
                                show_gk_attrs = rp.is_gk;
                                ui.data_mut(|d| d.insert_temp(gk_toggle_id, show_gk_attrs));
                            }
                        }
                    });

                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.colored_label(egui::Color32::from_rgb(76, 175, 80), "■ Key for role");
                    ui.colored_label(egui::Color32::from_rgb(66, 150, 240), "■ Preferable");
                });

                ui.add_space(4.0);
                ui.label("Set role attributes to:");
                for row_vals in [[20u8, 16, 14], [12u8, 10, 5]] {
                    ui.horizontal(|ui| {
                        for &target in &row_vals {
                            if ui
                                .add_sized([72.0, 22.0], egui::Button::new(target.to_string()))
                                .clicked()
                            {
                                for &k in role.key_attrs.iter().chain(role.pref_attrs.iter()) {
                                    set_attr_value(schemas, &mut player.raw, addr, k, target, edits);
                                }
                            }
                        }
                    });
                }

                ui.horizontal(|ui| {
                    if ui.add_sized([112.0, 22.0], egui::Button::new("- Decrease")).clicked() {
                        for &k in role.key_attrs.iter().chain(role.pref_attrs.iter()) {
                            shift_attr_value(schemas, &mut player.raw, addr, k, -1, edits);
                        }
                    }
                    if ui.add_sized([112.0, 22.0], egui::Button::new("+ Increase")).clicked() {
                        for &k in role.key_attrs.iter().chain(role.pref_attrs.iter()) {
                            shift_attr_value(schemas, &mut player.raw, addr, k, 1, edits);
                        }
                    }
                });

                ui.add_space(4.0);
                if ui
                    .add_sized([230.0, 26.0], egui::Button::new("⭐ Make WonderKid"))
                    .on_hover_text("Set PA to 195, CA to 155+, and boost key role & mental attributes")
                    .clicked()
                {
                    apply_wonderkid(schemas, &mut player.raw, addr, role, edits);
                }
            });

            ui.add_space(6.0);

            // Card 3C: Presets Card
            egui::Frame::group(ui.style()).show(ui, |ui| {
                ui.strong("Player Presets");
                ui.separator();

                for (label, preset) in [
                    ("Heal (100% Fitness)", PlayerPreset::Heal),
                    ("Inspire (20 Morale/Prof)", PlayerPreset::Inspire),
                    ("Destroy (Rival Nerf)", PlayerPreset::Destroy),
                    ("Remove Bans & Dirtiness", PlayerPreset::RemoveBans),
                    ("Register for UEFA", PlayerPreset::RegisterUefa),
                    ("Remove Int. Withdrawal", PlayerPreset::RemoveIntWithdrawal),
                    ("Trained at Club", PlayerPreset::TrainedAtClub),
                    ("Trained at Nation", PlayerPreset::TrainedAtNation),
                    ("Create History Point", PlayerPreset::CreateHistoryPoint),
                    ("Improve Team Cohesion", PlayerPreset::ImproveTeamCohesion),
                    ("Tactical Understanding", PlayerPreset::TacticalUnderstanding),
                    ("All-in-One Super Player", PlayerPreset::AllInOne),
                ] {
                    if ui.add_sized([230.0, 22.0], egui::Button::new(label)).clicked() {
                        apply_player_preset(schemas, player, addr, preset, edits);
                    }
                }
            });
        });
    });
}

#[derive(Clone, Copy)]
enum PlayerPreset {
    Heal,
    Inspire,
    Destroy,
    RemoveBans,
    RegisterUefa,
    RemoveIntWithdrawal,
    TrainedAtClub,
    TrainedAtNation,
    CreateHistoryPoint,
    ImproveTeamCohesion,
    TacticalUnderstanding,
    AllInOne,
}

fn apply_wonderkid(
    schemas: &'static crate::fields::VersionedSchemas,
    raw: &mut [u8],
    addr: u64,
    role: &RoleProfile,
    edits: &mut Vec<FieldEdit>,
) {
    set_raw_u8_value(schemas, raw, addr, "p_pa", 195, edits);
    let cur_ca = schemas
        .offset_of("p_ca")
        .and_then(|o| raw.get(o).copied())
        .unwrap_or(100);
    if cur_ca < 155 {
        set_raw_u8_value(schemas, raw, addr, "p_ca", 155, edits);
    }
    for &k in role.key_attrs {
        set_attr_value(schemas, raw, addr, k, 17, edits);
    }
    for &k in role.pref_attrs {
        set_attr_value(schemas, raw, addr, k, 15, edits);
    }
    for k in [
        "determination",
        "p_ambition",
        "p_professionalism",
        "p_loyalty",
        "p_pressure",
        "p_temperament",
        "consistency",
        "important_matches",
    ] {
        set_attr_value(schemas, raw, addr, k, 18, edits);
    }
    set_attr_value(schemas, raw, addr, "injury_proneness", 2, edits);
}

fn apply_player_preset(
    schemas: &'static crate::fields::VersionedSchemas,
    player: &mut PlayerRecord,
    addr: u64,
    preset: PlayerPreset,
    edits: &mut Vec<FieldEdit>,
) {
    let raw = &mut player.raw;
    match preset {
        PlayerPreset::Heal => {
            set_u16_value(schemas, raw, addr, "p_fitness", 10000, edits);
            set_u16_value(schemas, raw, addr, "p_sharpness", 10000, edits);
            set_u16_value(schemas, raw, addr, "p_fatigue", 0, edits);
            set_attr_value(schemas, raw, addr, "injury_proneness", 1, edits);
            set_attr_value(schemas, raw, addr, "natural_fitness", 20, edits);
            set_attr_value(schemas, raw, addr, "stamina", 20, edits);
        }
        PlayerPreset::Inspire => {
            set_attr_value(schemas, raw, addr, "p_morale", 20, edits);
            for k in [
                "determination",
                "p_adaptability",
                "p_ambition",
                "p_loyalty",
                "p_pressure",
                "p_professionalism",
                "p_sportsmanship",
                "p_temperament",
            ] {
                set_attr_value(schemas, raw, addr, k, 20, edits);
            }
            set_attr_value(schemas, raw, addr, "p_controversy", 1, edits);
        }
        PlayerPreset::Destroy => {
            set_u16_value(schemas, raw, addr, "p_fitness", 1000, edits);
            set_u16_value(schemas, raw, addr, "p_sharpness", 1000, edits);
            set_u16_value(schemas, raw, addr, "p_fatigue", 9000, edits);
            set_attr_value(schemas, raw, addr, "p_morale", 1, edits);
            for k in ["stamina", "pace", "acceleration", "natural_fitness", "strength", "concentration"] {
                set_attr_value(schemas, raw, addr, k, 1, edits);
            }
        }
        PlayerPreset::RemoveBans => {
            set_attr_value(schemas, raw, addr, "p_dirtiness", 1, edits);
            set_attr_value(schemas, raw, addr, "p_controversy", 1, edits);
            set_attr_value(schemas, raw, addr, "p_temperament", 20, edits);
        }
        PlayerPreset::RegisterUefa => {
            if let (Some(cr), Some(caddr)) = (player.contract_raw.as_mut(), player.contract_addr) {
                if let Some(off) = schemas.offset_of("p_squad_status") {
                    if let Some(&old) = cr.get(off) {
                        if old != 1 {
                            cr[off] = 1;
                            edits.push(FieldEdit {
                                addr: caddr + off as u64,
                                old_raw: vec![old],
                                new_raw: vec![1],
                                description: "Register for UEFA (Squad Status \u{2192} Star Player)".into(),
                            });
                        }
                    }
                }
            }
        }
        PlayerPreset::RemoveIntWithdrawal => {
            set_attr_value(schemas, raw, addr, "p_ambition", 20, edits);
            set_attr_value(schemas, raw, addr, "p_loyalty", 20, edits);
            set_attr_value(schemas, raw, addr, "p_morale", 20, edits);
        }
        PlayerPreset::TrainedAtClub | PlayerPreset::TrainedAtNation => {
            set_attr_value(schemas, raw, addr, "p_loyalty", 20, edits);
            set_attr_value(schemas, raw, addr, "p_adaptability", 20, edits);
        }
        PlayerPreset::CreateHistoryPoint => {
            if let Some(ca) = schemas.offset_of("p_ca").and_then(|o| raw.get(o).copied()) {
                edits.push(FieldEdit {
                    addr: addr + schemas.offset_of("p_ca").unwrap() as u64,
                    old_raw: vec![ca],
                    new_raw: vec![ca],
                    description: "History snapshot point".into(),
                });
            }
        }
        PlayerPreset::ImproveTeamCohesion => {
            for k in ["teamwork", "work_rate", "leadership", "p_loyalty", "p_sportsmanship", "p_morale"] {
                set_attr_value(schemas, raw, addr, k, 20, edits);
            }
        }
        PlayerPreset::TacticalUnderstanding => {
            for k in [
                "decisions",
                "positioning",
                "off_the_ball",
                "anticipation",
                "teamwork",
                "vision",
                "concentration",
                "p_versatility",
            ] {
                set_attr_value(schemas, raw, addr, k, 20, edits);
            }
        }
        PlayerPreset::AllInOne => {
            apply_player_preset(schemas, player, addr, PlayerPreset::Heal, edits);
            apply_player_preset(schemas, player, addr, PlayerPreset::Inspire, edits);
            apply_player_preset(schemas, player, addr, PlayerPreset::TacticalUnderstanding, edits);
            set_attr_value(schemas, &mut player.raw, addr, "consistency", 20, edits);
            set_attr_value(schemas, &mut player.raw, addr, "important_matches", 20, edits);
        }
    }
}

fn show_move_to_club_card(
    schemas: &'static crate::fields::VersionedSchemas,
    ui: &mut egui::Ui,
    player: &mut PlayerRecord,
    addr: u64,
    clubs: Option<&[crate::cache::CachedClub]>,
    edits: &mut Vec<FieldEdit>,
) {
    egui::Frame::group(ui.style()).show(ui, |ui| {
        ui.strong("Move to club");
        ui.separator();

        let move_id = egui::Id::new(("move_club_draft", addr));
        let mut draft = ui
            .data(|d| d.get_temp::<MoveClubDraft>(move_id))
            .unwrap_or_default();

        egui::Grid::new(("move_to_club_grid", addr))
            .num_columns(2)
            .spacing([8.0, 4.0])
            .show(ui, |ui| {
                ui.label("Move");
                ui.label(player.name.as_deref().unwrap_or("Unknown"));
                ui.end_row();

                ui.label("From");
                ui.label(player.club_name.as_deref().unwrap_or("Free Agent"));
                ui.end_row();

                ui.label("To");
                ui.horizontal(|ui| {
                    ui.label("🔍");
                    ui.add(
                        egui::TextEdit::singleline(&mut draft.search_query)
                            .hint_text("Search club...")
                            .desired_width(130.0),
                    );
                });
                ui.end_row();

                ui.label("On Loan");
                ui.checkbox(&mut draft.on_loan, "");
                ui.end_row();
            });

        if let Some(club_list) = clubs {
            let query = draft.search_query.trim().to_lowercase();
            if !query.is_empty() {
                egui::ScrollArea::vertical()
                    .id_salt(("move_club_results", addr))
                    .max_height(100.0)
                    .show(ui, |ui| {
                        for c in club_list
                            .iter()
                            .filter(|c| c.name.to_lowercase().contains(&query))
                            .take(25)
                        {
                            let is_sel = draft.selected_club_addr == Some(c.addr);
                            if ui.selectable_label(is_sel, &c.name).clicked() {
                                draft.selected_club_addr = Some(c.addr);
                                draft.selected_club_name = c.name.clone();
                                draft.search_query = c.name.clone();
                            }
                        }
                    });
            }
        }

        if let Some(target_addr) = draft.selected_club_addr {
            let target_club = clubs.and_then(|list| list.iter().find(|c| c.addr == target_addr));
            if ui
                .add_enabled(
                    crate::ui::navigation::editing_enabled(ui),
                    egui::Button::new(format!("➡ Move to {}", draft.selected_club_name)),
                )
                .clicked()
            {
                // Write senior team pointer (0x0130 in FM24) so FM's squad and tactics recognize the transfer
                if let Some(st_addr) = target_club.and_then(|c| c.senior_team_addr) {
                    if let Some(team_off) = schemas.offset_of("p_team_ptr") {
                        if team_off + 8 <= player.raw.len() {
                            let old_bytes = player.raw[team_off..team_off + 8].to_vec();
                            let new_bytes = st_addr.to_le_bytes().to_vec();
                            player.raw[team_off..team_off + 8].copy_from_slice(&new_bytes);
                            edits.push(FieldEdit {
                                addr: addr + team_off as u64,
                                old_raw: old_bytes,
                                new_raw: new_bytes,
                                description: format!(
                                    "Move {} senior team to {}",
                                    player.name.as_deref().unwrap_or("Player"),
                                    draft.selected_club_name
                                ),
                            });
                        }
                    }
                }

                // Write club pointer (0x03B0 in FM24)
                if let Some(off) = schemas.offset_of("p_club_ptr") {
                    if off + 8 <= player.raw.len() {
                        let old_bytes = player.raw[off..off + 8].to_vec();
                        let new_bytes = target_addr.to_le_bytes().to_vec();
                        player.raw[off..off + 8].copy_from_slice(&new_bytes);
                        edits.push(FieldEdit {
                            addr: addr + off as u64,
                            old_raw: old_bytes,
                            new_raw: new_bytes,
                            description: format!(
                                "Move {} club to {}{}",
                                player.name.as_deref().unwrap_or("Player"),
                                draft.selected_club_name,
                                if draft.on_loan { " (On Loan)" } else { "" }
                            ),
                        });
                    }
                }
                if draft.on_loan {
                    if let (Some(cr), Some(caddr)) = (player.contract_raw.as_mut(), player.contract_addr) {
                        if let Some(off) = schemas.offset_of("p_squad_status") {
                            if let Some(&old) = cr.get(off) {
                                cr[off] = 24; // Out On Loan
                                edits.push(FieldEdit {
                                    addr: caddr + off as u64,
                                    old_raw: vec![old],
                                    new_raw: vec![24],
                                    description: "Squad Status \u{2192} Out On Loan".into(),
                                });
                            }
                        }
                    }
                }
                player.club_addr = Some(target_addr);
                player.club_name = Some(draft.selected_club_name.clone());
            }
        }

        ui.data_mut(|d| d.insert_temp(move_id, draft));
    });
}

fn render_highlighted_attr(
    ui: &mut egui::Ui,
    field: &fields::Field,
    role: &RoleProfile,
    raw: &mut [u8],
    addr: u64,
    edits: &mut Vec<FieldEdit>,
) {
    let bg = if role.key_attrs.contains(&field.key) {
        Some(egui::Color32::from_rgba_unmultiplied(46, 125, 50, 55))
    } else if role.pref_attrs.contains(&field.key) {
        Some(egui::Color32::from_rgba_unmultiplied(30, 100, 180, 55))
    } else if LOWER_IS_BETTER.contains(&field.key) {
        Some(egui::Color32::from_rgba_unmultiplied(140, 45, 60, 45))
    } else {
        None
    };

    if let Some(fill) = bg {
        egui::Frame::default()
            .fill(fill)
            .inner_margin(2.0)
            .show(ui, |ui| {
                if let Some(e) = field_widget::render_field(ui, field, raw, addr) {
                    edits.push(e);
                }
            });
    } else if let Some(e) = field_widget::render_field(ui, field, raw, addr) {
        edits.push(e);
    }
}

fn set_attr_value(
    schemas: &'static crate::fields::VersionedSchemas,
    raw: &mut [u8],
    entity_addr: u64,
    key: &str,
    target_displayed: u8,
    edits: &mut Vec<FieldEdit>,
) {
    let Some(field) = schemas.field(key) else { return };
    let Some(loc) = field.offset.as_ref() else { return };
    let offset = loc.offset() as usize;
    if offset >= raw.len() {
        return;
    }
    let old_raw = raw[offset];
    let old_displayed = loc.decode_u8(old_raw).clamp(1, 20);
    let new_displayed = target_displayed.clamp(1, 20);
    if new_displayed == old_displayed {
        return;
    }
    let new_raw = loc.encode_u8(new_displayed);
    raw[offset] = new_raw;
    edits.push(FieldEdit {
        addr: entity_addr + offset as u64,
        old_raw: vec![old_raw],
        new_raw: vec![new_raw],
        description: format!("{}: {} \u{2192} {}", field.label, old_displayed, new_displayed),
    });
}

fn shift_attr_value(
    schemas: &'static crate::fields::VersionedSchemas,
    raw: &mut [u8],
    entity_addr: u64,
    key: &str,
    delta: i8,
    edits: &mut Vec<FieldEdit>,
) {
    let Some(field) = schemas.field(key) else { return };
    let Some(loc) = field.offset.as_ref() else { return };
    let offset = loc.offset() as usize;
    if offset >= raw.len() {
        return;
    }
    let old_raw = raw[offset];
    let old_displayed = loc.decode_u8(old_raw).clamp(1, 20);
    let new_displayed = (old_displayed as i16 + delta as i16).clamp(1, 20) as u8;
    if new_displayed == old_displayed {
        return;
    }
    let new_raw = loc.encode_u8(new_displayed);
    raw[offset] = new_raw;
    edits.push(FieldEdit {
        addr: entity_addr + offset as u64,
        old_raw: vec![old_raw],
        new_raw: vec![new_raw],
        description: format!("{}: {} \u{2192} {}", field.label, old_displayed, new_displayed),
    });
}

fn set_u16_value(
    schemas: &'static crate::fields::VersionedSchemas,
    raw: &mut [u8],
    entity_addr: u64,
    key: &str,
    target: u16,
    edits: &mut Vec<FieldEdit>,
) {
    let Some(field) = schemas.field(key) else { return };
    let Some(loc) = field.offset.as_ref() else { return };
    let offset = loc.offset() as usize;
    if offset + 1 >= raw.len() {
        return;
    }
    let old_bytes = [raw[offset], raw[offset + 1]];
    let old_val = u16::from_le_bytes(old_bytes);
    if old_val == target {
        return;
    }
    let new_bytes = target.to_le_bytes();
    raw[offset..offset + 2].copy_from_slice(&new_bytes);
    edits.push(FieldEdit {
        addr: entity_addr + offset as u64,
        old_raw: old_bytes.to_vec(),
        new_raw: new_bytes.to_vec(),
        description: format!("{}: {} \u{2192} {}", field.label, old_val, target),
    });
}

fn set_raw_u8_value(
    schemas: &'static crate::fields::VersionedSchemas,
    raw: &mut [u8],
    entity_addr: u64,
    key: &str,
    target: u8,
    edits: &mut Vec<FieldEdit>,
) {
    let Some(field) = schemas.field(key) else { return };
    let Some(loc) = field.offset.as_ref() else { return };
    let offset = loc.offset() as usize;
    if offset >= raw.len() {
        return;
    }
    let old_val = raw[offset];
    if old_val == target {
        return;
    }
    raw[offset] = target;
    edits.push(FieldEdit {
        addr: entity_addr + offset as u64,
        old_raw: vec![old_val],
        new_raw: vec![target],
        description: format!("{}: {} \u{2192} {}", field.label, old_val, target),
    });
}

fn recommended_ca(player: &PlayerRecord) -> u8 {
    let pos_ratings = ratings::all_position_ratings(player);
    let best_pos_rating = pos_ratings.iter().map(|(_, _, _, r)| *r).max().unwrap_or(50) as u16;
    ((best_pos_rating * 2).saturating_sub(2)).clamp(1, 200) as u8
}

fn editable_pair_row(
    schemas: &'static crate::fields::VersionedSchemas,
    ui: &mut egui::Ui,
    label: &str,
    key1: &str,
    key2: &str,
    raw: &mut [u8],
    addr: u64,
    edits: &mut Vec<FieldEdit>,
) {
    let (Some(f1), Some(f2)) = (schemas.field(key1), schemas.field(key2)) else {
        return;
    };
    let (Some(loc1), Some(loc2)) = (f1.offset.as_ref(), f2.offset.as_ref()) else {
        return;
    };
    let (off1, off2) = (loc1.offset() as usize, loc2.offset() as usize);
    if off1 >= raw.len() || off2 >= raw.len() {
        return;
    }

    let enabled = crate::ui::navigation::editing_enabled(ui);
    ui.horizontal(|ui| {
        ui.label(label);
        edit_int_slot(ui, enabled, f1.label, loc1.encoding(), off1, raw, addr, edits);
        ui.label("/");
        edit_int_slot(ui, enabled, f2.label, loc2.encoding(), off2, raw, addr, edits);
    });
}

fn edit_int_slot(
    ui: &mut egui::Ui,
    enabled: bool,
    label: &str,
    encoding: fields::Encoding,
    offset: usize,
    raw: &mut [u8],
    addr: u64,
    edits: &mut Vec<FieldEdit>,
) {
    match encoding {
        fields::Encoding::Direct => {
            let old = raw[offset];
            let mut val = old as i32;
            if ui
                .add_enabled(
                    enabled,
                    egui::DragValue::new(&mut val).range(0..=255).speed(0.5),
                )
                .changed()
                && enabled
            {
                let new_val = val.clamp(0, 255) as u8;
                if new_val != old {
                    raw[offset] = new_val;
                    edits.push(FieldEdit {
                        addr: addr + offset as u64,
                        old_raw: vec![old],
                        new_raw: vec![new_val],
                        description: format!("{label}: {old} \u{2192} {new_val}"),
                    });
                }
            }
        }
        fields::Encoding::Raw | fields::Encoding::RawCapped(_) if offset + 1 < raw.len() => {
            let old_bytes = [raw[offset], raw[offset + 1]];
            let old = u16::from_le_bytes(old_bytes);
            let mut val = old as i32;
            if ui
                .add_enabled(
                    enabled,
                    egui::DragValue::new(&mut val).range(0..=999).speed(0.5),
                )
                .changed()
                && enabled
            {
                let new_val = val.clamp(0, 999) as u16;
                if new_val != old {
                    let new_bytes = new_val.to_le_bytes();
                    raw[offset..offset + 2].copy_from_slice(&new_bytes);
                    edits.push(FieldEdit {
                        addr: addr + offset as u64,
                        old_raw: old_bytes.to_vec(),
                        new_raw: new_bytes.to_vec(),
                        description: format!("{label}: {old} \u{2192} {new_val}"),
                    });
                }
            }
        }
        _ => {}
    }
}

fn editable_foot_row(
    schemas: &'static crate::fields::VersionedSchemas,
    ui: &mut egui::Ui,
    raw: &mut [u8],
    addr: u64,
    edits: &mut Vec<FieldEdit>,
) {
    let (Some(lf), Some(rf)) = (schemas.field("left_foot"), schemas.field("right_foot")) else {
        return;
    };
    let (Some(loc_l), Some(loc_r)) = (lf.offset.as_ref(), rf.offset.as_ref()) else {
        return;
    };
    let (off_l, off_r) = (loc_l.offset() as usize, loc_r.offset() as usize);
    if off_l >= raw.len() || off_r >= raw.len() {
        return;
    }

    let enabled = crate::ui::navigation::editing_enabled(ui);
    ui.horizontal(|ui| {
        ui.label("Left Foot / Right Foot");
        for (field, loc, off) in [(lf, loc_l, off_l), (rf, loc_r, off_r)] {
            let old_raw = raw[off];
            let disp = loc.decode_u8(old_raw).clamp(1, 20);
            let mut val = disp as i32;
            let color = field_widget::attr_color(disp, ui);
            ui.visuals_mut().override_text_color = Some(color);
            if ui
                .add_enabled(
                    enabled,
                    egui::DragValue::new(&mut val).range(1..=20).speed(0.2),
                )
                .changed()
                && enabled
            {
                let new_disp = val.clamp(1, 20) as u8;
                if new_disp != disp {
                    let new_raw = loc.encode_u8(new_disp);
                    raw[off] = new_raw;
                    edits.push(FieldEdit {
                        addr: addr + off as u64,
                        old_raw: vec![old_raw],
                        new_raw: vec![new_raw],
                        description: format!("{}: {} \u{2192} {}", field.label, disp, new_disp),
                    });
                }
            }
            ui.visuals_mut().override_text_color = None;
        }
    });
}

fn skin_tone_color(tone: u8) -> egui::Color32 {
    let t = (tone.min(19) as f32) / 19.0;
    let r = (252.0 - t * 185.0) as u8;
    let g = (226.0 - t * 178.0) as u8;
    let b = (196.0 - t * 158.0) as u8;
    egui::Color32::from_rgb(r, g, b)
}

// ---------------------------------------------------------------------------
// Positions & Traits
// ---------------------------------------------------------------------------

fn show_positions_traits(
    schemas: &'static crate::fields::VersionedSchemas,
    ui: &mut egui::Ui,
    player: &mut PlayerRecord,
    addr: u64,
    edits: &mut Vec<FieldEdit>,
) {
    // Stack the formation above traits on narrow panels rather than forcing overflow.
    ui.strong("Positions");
    show_positions_formation(schemas, ui, player, addr, edits);
    ui.add_space(12.0);
    ui.strong("Traits");
    show_traits_checkboxes(schemas, ui, player, addr, edits);

    ui.add_space(16.0);
    ui.separator();

    // PA projection — Genie Scout-style, read-only preview. Hidden behind
    // a checkbox so it doesn't clutter the default view. Pure display —
    // never emits FieldEdits / writes to memory.
    let checkbox_id = egui::Id::new("pa_extrap_toggle");
    let mut show_extrap = ui.data_mut(|d| d.get_temp::<bool>(checkbox_id).unwrap_or(false));
    if ui
        .checkbox(&mut show_extrap, "Show PA extrapolation")
        .changed()
    {
        ui.data_mut(|d| d.insert_temp(checkbox_id, show_extrap));
    }

    if show_extrap {
        let ca = player.field_u8("p_ca").unwrap_or(0);
        let pa = player.field_u8("p_pa").unwrap_or(0);
        let projection = ratings::extrapolate_player(player);
        if projection.is_empty() {
            ui.label(
                egui::RichText::new(if pa <= ca {
                    "No growth \u{2014} CA is already at PA."
                } else {
                    "No attributes would grow under the current tier table."
                })
                .small()
                .color(egui::Color32::from_rgb(150, 150, 150)),
            );
        } else {
            ui.strong(format!("PA Projection (CA={ca} \u{2192} PA={pa})"));
            ui.label(
                egui::RichText::new("Read-only preview \u{2014} no changes written to memory")
                    .small()
                    .color(egui::Color32::from_rgb(150, 150, 150)),
            );
            for cat in [
                Category::Technical,
                Category::SetPieces,
                Category::Mental,
                Category::Physical,
                Category::Goalkeeping,
                Category::Hidden,
            ] {
                let cat_rows: Vec<_> = schemas
                    .fields_for_category(Entity::Player, cat)
                    .into_iter()
                    .filter_map(|f| {
                        projection
                            .iter()
                            .find(|(k, _, _)| *k == f.key)
                            .map(|(_, old, new)| (f.label, *old, *new))
                    })
                    .collect();
                if cat_rows.is_empty() {
                    continue;
                }
                ui.horizontal_wrapped(|ui| {
                    ui.label(egui::RichText::new(format!("{cat:?}:")).strong().small());
                    for (label, old, new) in cat_rows {
                        ui.label(
                            egui::RichText::new(format!("{label} {old}\u{2192}{new}"))
                                .small()
                                .color(egui::Color32::LIGHT_GREEN),
                        );
                    }
                });
            }
        }
    }
}

/// Render the 15 position familiarities in a football-formation layout.
/// Layout (3 columns per row):
///   row 0: .   ST  .
///   row 1: AML AMC AMR
///   row 2: ML  MC  MR
///   row 3: WBL DM  WBR
///   row 4: DL  DC  DR
///   row 5: .   SW  .
///   row 6: .   GK  .
fn show_positions_formation(
    schemas: &'static crate::fields::VersionedSchemas,
    ui: &mut egui::Ui,
    player: &mut PlayerRecord,
    addr: u64,
    edits: &mut Vec<FieldEdit>,
) {
    // Rows of (left, center, right) position keys. None = empty cell.
    let rows: &[(Option<&str>, Option<&str>, Option<&str>)] = &[
        (None, Some("pos_st"), None),
        (Some("pos_aml"), Some("pos_amc"), Some("pos_amr")),
        (Some("pos_ml"), Some("pos_mc"), Some("pos_mr")),
        (Some("pos_wbl"), Some("pos_dm"), Some("pos_wbr")),
        (Some("pos_dl"), Some("pos_dc"), Some("pos_dr")),
        (None, Some("pos_sw"), None),
        (None, Some("pos_gk"), None),
    ];

    egui::Grid::new("pos_formation")
        .num_columns(3)
        .spacing([14.0, 6.0])
        .min_col_width(56.0)
        .show(ui, |ui| {
            for (left, center, right) in rows {
                pos_cell(schemas, ui, *left, player, addr, edits);
                pos_cell(schemas, ui, *center, player, addr, edits);
                pos_cell(schemas, ui, *right, player, addr, edits);
                ui.end_row();
            }
        });
}

fn pos_cell(
    schemas: &'static crate::fields::VersionedSchemas,
    ui: &mut egui::Ui,
    key: Option<&str>,
    player: &mut PlayerRecord,
    addr: u64,
    edits: &mut Vec<FieldEdit>,
) {
    match key {
        Some(k) => {
            if let Some(f) = schemas.field(k) {
                if let Some(edit) = field_widget::attr_cell_compact(ui, &f, &mut player.raw, addr) {
                    edits.push(edit);
                }
            }
        }
        None => {
            // Empty cell — reserve space so the grid stays aligned.
            ui.allocate_space(egui::vec2(56.0, 38.0));
        }
    }
}

/// Render traits as checkbox groups arranged in 4 columns to mirror the FM26 UI.
fn show_traits_checkboxes(
    schemas: &'static crate::fields::VersionedSchemas,
    ui: &mut egui::Ui,
    player: &mut PlayerRecord,
    addr: u64,
    edits: &mut Vec<FieldEdit>,
) {
    let location = schemas.field("p_traits").and_then(|field| field.offset);
    let Some((_, old_mask)) = trait_value(&player.raw, location) else {
        ui.label("Traits unavailable for this record or game version.");
        return;
    };
    let mut new_mask = old_mask;

    // Column 1
    const COL1: &[(&str, &[u8])] = &[
        ("General", &[25, 14, 18, 17, 42, 45, 19]),
        ("Defending", &[16, 38, 39, 34, 61, 33]),
        ("Ball Control", &[35, 31, 12, 15, 23, 27]),
    ];
    // Column 2
    const COL2: &[(&str, &[u8])] = &[
        ("Shooting", &[10, 9, 11, 43, 22, 24, 57]),
        ("Movement", &[5, 4, 6, 32, 20, 41, 55]),
        ("Set Pieces", &[51, 56, 37, 46, 62, 63]),
    ];
    // Column 3
    const COL3: &[(&str, &[u8])] = &[(
        "Dribbling / Running",
        &[52, 53, 2, 1, 3, 28, 36, 47, 29, 30, 40, 59, 58, 13],
    )];
    // Column 4
    const COL4: &[(&str, &[u8])] = &[("Passing", &[7, 26, 8, 48, 21, 44, 60, 64])];

    let groups = [COL1, COL2, COL3, COL4];
    crate::ui::entity_header::columns(ui, |index, col| {
        render_trait_column(col, groups[index], &mut new_mask);
    });

    if crate::ui::navigation::editing_enabled(ui) && new_mask != old_mask {
        // Re-resolve the mapping and validate the whole range before copying bytes.
        let location = schemas.field("p_traits").and_then(|field| field.offset);
        let Some((traits_offset, current_mask)) = trait_value(&player.raw, location) else {
            return;
        };
        if current_mask != old_mask {
            return;
        }
        let old_bytes = old_mask.to_le_bytes();
        let new_bytes = new_mask.to_le_bytes();
        let Some(bytes) = player.raw.get_mut(traits_offset..traits_offset + 8) else {
            return;
        };
        bytes.copy_from_slice(&new_bytes);

        let mut added = Vec::new();
        let mut removed = Vec::new();
        for bit in 0..64u8 {
            let o = (old_mask >> bit) & 1;
            let n = (new_mask >> bit) & 1;
            if o != n {
                let name = player::ppm_name(bit + 1);
                if n == 1 {
                    added.push(name);
                } else {
                    removed.push(name);
                }
            }
        }
        let desc = match (added.as_slice(), removed.as_slice()) {
            ([a], []) => format!("Added trait: {a}"),
            ([], [r]) => format!("Removed trait: {r}"),
            (a, r) => format!("Traits: +{} -{}", a.len(), r.len()),
        };

        edits.push(FieldEdit {
            addr: addr + traits_offset as u64,
            old_raw: old_bytes.to_vec(),
            new_raw: new_bytes.to_vec(),
            description: desc,
        });
    }
}

fn render_trait_column(ui: &mut egui::Ui, groups: &[(&str, &[u8])], mask: &mut u64) {
    for (heading, ppm_ids) in groups {
        ui.strong(*heading);
        for &id in *ppm_ids {
            let bit = 1u64 << (id - 1);
            let mut checked = (*mask & bit) != 0;
            if ui
                .add_enabled(
                    crate::ui::navigation::editing_enabled(ui),
                    egui::Checkbox::new(&mut checked, player::ppm_name(id)),
                )
                .changed()
            {
                if checked {
                    *mask |= bit;
                } else {
                    *mask &= !bit;
                }
            }
        }
        ui.add_space(6.0);
    }
}

// ---------------------------------------------------------------------------
// Contract / Relations (placeholder)
// ---------------------------------------------------------------------------

fn show_contract(
    schemas: &'static crate::fields::VersionedSchemas,
    ui: &mut egui::Ui,
    player: &mut PlayerRecord,
    clubs: Option<&[crate::cache::CachedClub]>,
    edits: &mut Vec<FieldEdit>,
) {
    let addr = player.addr;
    ui.horizontal_top(|ui| {
        egui::Frame::group(ui.style()).show(ui, |ui| {
            ui.set_width(340.0);
            ui.strong("Contract");
            ui.separator();

            if let (Some(cr), Some(caddr)) = (player.contract_raw.as_mut(), player.contract_addr) {
                if let Some(f) = schemas.field("p_contract_wage") {
                    edits.extend(field_widget::raw_u32_edit(ui, &f, cr, caddr));
                }
                if let Some(f) = schemas.field("p_contract_expiry") {
                    edits.extend(field_widget::date_edit(
                        ui,
                        "Expiry",
                        cr,
                        &f,
                        caddr,
                        2000..=2060,
                    ));
                }
                if let Some(f) = schemas.field("p_squad_status") {
                    let code = schemas
                        .offset_of("p_squad_status")
                        .and_then(|o| cr.get(o).copied());
                    match code {
                        Some(c) if !SQUAD_STATUS_OFFERED.iter().any(|(o, _)| *o == c) => {
                            let name = squad_status_name(c)
                                .map_or(format!("code {c}"), |n| format!("{n} ({c})"));
                            field_widget::info_label(ui, "Squad Status", name);
                        }
                        _ => edits.extend(field_widget::choice_u8_edit(
                            ui,
                            "Squad Status",
                            cr,
                            &f,
                            caddr,
                            SQUAD_STATUS_OFFERED,
                        )),
                    }
                }
                if let Some(f) = schemas.field("p_squad_number") {
                    edits.extend(field_widget::optional_u8_edit(
                        ui,
                        "Squad Number",
                        cr,
                        &f,
                        caddr,
                        99,
                        255,
                    ));
                }
                if let Some(off) = schemas.offset_of("p_list_flags") {
                    if let Some(&flags) = cr.get(off) {
                        let enabled = crate::ui::navigation::editing_enabled(ui);
                        let mut transfer_listed = (flags & 1) != 0;
                        let mut loan_listed = (flags & 2) != 0;
                        let mut new_flags = flags;
                        if ui
                            .add_enabled(
                                enabled,
                                egui::Checkbox::new(&mut transfer_listed, "Transfer Listed"),
                            )
                            .changed()
                        {
                            if transfer_listed {
                                new_flags |= 1;
                            } else {
                                new_flags &= !1;
                            }
                        }
                        if ui
                            .add_enabled(
                                enabled,
                                egui::Checkbox::new(&mut loan_listed, "Loan Listed"),
                            )
                            .changed()
                        {
                            if loan_listed {
                                new_flags |= 2;
                            } else {
                                new_flags &= !2;
                            }
                        }
                        if new_flags != flags && enabled {
                            cr[off] = new_flags;
                            edits.push(FieldEdit {
                                addr: caddr + off as u64,
                                old_raw: vec![flags],
                                new_raw: vec![new_flags],
                                description: format!("Listing Flags: {flags} \u{2192} {new_flags}"),
                            });
                        }
                    }
                }
                if let Some(s) = schemas
                    .offset_of("p_contract_signed")
                    .and_then(|o| fmt_date_at(cr, o))
                {
                    field_widget::info_label(ui, "Signed", s);
                }
            } else {
                ui.label("Contract sub-record unavailable.");
            }
            if let Some(f) = schemas.field("p_asking_price") {
                edits.extend(field_widget::optional_money_edit(
                    ui,
                    "Asking Price",
                    &mut player.raw,
                    &f,
                    addr,
                ));
            }

            ui.add_space(4.0);

            if let Some(apps) = player.field_u16("p_senior_apps") {
                field_widget::info_label(ui, "Senior Apps", apps);
            }

            for (key, label) in [
                ("p_contract_start", "Contract Start"),
                ("p_join_date", "Joined Club"),
                ("p_debut_date", "Senior Debut"),
            ] {
                if let Some(s) = fmt_date_field(schemas, &player.raw, key) {
                    field_widget::info_label(ui, label, s);
                }
            }
        });

        ui.add_space(12.0);
        show_move_to_club_card(schemas, ui, player, addr, clubs, edits);
    });
}

/// Squad statuses offered for editing: one code per distinct label. Codes
/// 6, 8, 19 and 21 duplicate a label; 0 ("-"), 23 (Scouting Required) and
/// 24 (Out On Loan) are FM's own and not offered. A stored code outside
/// this list is shown read-only.
const SQUAD_STATUS_OFFERED: &[(u8, &str)] = &[
    (1, "Star Player"),
    (2, "Important Player"),
    (3, "Regular Starter"),
    (4, "Squad Player"),
    (5, "Impact Sub"),
    (7, "Fringe Player"),
    (9, "Emergency Backup"),
    (10, "Breakthrough Prospect"),
    (11, "Future Prospect"),
    (12, "Hot Prospect"),
    (13, "Youngster"),
    (14, "Regular"),
    (15, "First-Choice Goalkeeper"),
    (16, "Cup Goalkeeper"),
    (17, "Domestic Cup Goalkeeper"),
    (18, "Continental Cup Goalkeeper"),
    (20, "Backup"),
    (22, "Surplus to Requirements"),
];

/// FM's ethnicity codes (`p_ethnicity`).
const ETHNICITIES: &[(u8, &str)] = &[
    (0, "Northern European"),
    (1, "Mediterranean/Hispanic"),
    (2, "North African/Middle Eastern"),
    (3, "African/Caribbean"),
    (4, "Asian"),
    (5, "South East Asian"),
    (6, "Pacific Islander"),
    (7, "Native American"),
    (8, "Native Australian"),
    (9, "Mixed Race"),
    (10, "East Asian"),
];

const HAIR_COLOURS: &[(u8, &str)] = &[
    (0, "Black"),
    (1, "Dark Brown 1"),
    (2, "Dark Brown 2"),
    (3, "Dark Brown 3"),
    (4, "Brown 1"),
    (5, "Brown 2"),
    (6, "Light Brown 1"),
    (7, "Light Brown 2"),
    (8, "Blonde 1"),
    (9, "Blonde 2"),
    (10, "Blonde 3"),
    (11, "Red 1"),
    (12, "Red 2"),
    (13, "Grey 1"),
    (14, "Grey 2"),
    (15, "White"),
    (16, "Bald"),
    (17, "Dyed"),
    (18, "Other"),
];

const HAIR_LENGTHS: &[(u8, &str)] = &[
    (0, "Bald"),
    (1, "Buzzcut"),
    (2, "Short"),
    (3, "Medium"),
    (4, "Long"),
    (5, "Curly"),
    (6, "Braids"),
];

const SKIN_TONES: &[(u8, &str)] = &[
    (0, "Skin Tone 1"),
    (1, "Skin Tone 2"),
    (2, "Skin Tone 3"),
    (3, "Skin Tone 4"),
    (4, "Skin Tone 5"),
    (5, "Skin Tone 6"),
    (6, "Skin Tone 7"),
    (7, "Skin Tone 8"),
    (8, "Skin Tone 9"),
    (9, "Skin Tone 10"),
    (10, "Skin Tone 11"),
    (11, "Skin Tone 12"),
    (12, "Skin Tone 13"),
    (13, "Skin Tone 14"),
    (14, "Skin Tone 15"),
    (15, "Skin Tone 16"),
    (16, "Skin Tone 17"),
    (17, "Skin Tone 18"),
    (18, "Skin Tone 19"),
    (19, "Skin Tone 20"),
];

/// FM's label for a squad-status code (contract "agreed playing time").
/// Codes from FM's Squad Status list; several codes share a label.
fn squad_status_name(code: u8) -> Option<&'static str> {
    Some(match code {
        1 => "Star Player",
        2 => "Important Player",
        3 => "Regular Starter",
        4 | 6 => "Squad Player",
        5 => "Impact Sub",
        7 | 8 => "Fringe Player",
        9 | 21 => "Emergency Backup",
        10 => "Breakthrough Prospect",
        11 => "Future Prospect",
        12 => "Hot Prospect",
        13 => "Youngster",
        14 => "Regular",
        15 => "First-Choice Goalkeeper",
        16 | 19 => "Cup Goalkeeper",
        17 => "Domestic Cup Goalkeeper",
        18 => "Continental Cup Goalkeeper",
        20 => "Backup",
        22 => "Surplus to Requirements",
        23 => "Scouting Required",
        24 => "Out On Loan",
        _ => return None,
    })
}

/// Read a DateDayYear field from `raw` by schema key and format as "DD Mon YYYY".
fn fmt_date_field(
    schemas: &'static crate::fields::VersionedSchemas,
    raw: &[u8],
    key: &str,
) -> Option<String> {
    let off = schemas.field(key)?.offset?.offset() as usize;
    fmt_date_at(raw, off)
}

/// Format a DateDayYear value at byte offset `off` in `buf` as "DD Mon YYYY".
pub(crate) fn fmt_date_at(buf: &[u8], off: usize) -> Option<String> {
    if off + 4 > buf.len() {
        return None;
    }
    // Low 9 bits hold the day; FM keeps flags above them.
    let doy = u16::from_le_bytes(buf[off..off + 2].try_into().ok()?) & 0x1FF;
    let year = u16::from_le_bytes(buf[off + 2..off + 4].try_into().ok()?);
    if year == 0 || doy == 0 || doy > 366 || !(1900..=2050).contains(&year) {
        return None;
    }
    let is_leap = (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0);
    let mdays: [u16; 12] = if is_leap {
        [31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    } else {
        [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    };
    let mnames = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    let mut rem = doy;
    for (i, &d) in mdays.iter().enumerate() {
        if rem <= d {
            return Some(format!("{rem} {} {year}", mnames[i]));
        }
        rem -= d;
    }
    None
}

/// Trait masks must be mapped directly into this buffer, never into a sub-record.
fn trait_value(raw: &[u8], location: Option<fields::FieldLocation>) -> Option<(usize, u64)> {
    let fields::FieldLocation::Direct { offset, .. } = location? else {
        return None;
    };
    let offset = usize::try_from(offset).ok()?;
    let bytes = raw.get(offset..offset.checked_add(8)?)?;
    Some((offset, u64::from_le_bytes(bytes.try_into().ok()?)))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn direct(offset: u64) -> Option<fields::FieldLocation> {
        Some(fields::FieldLocation::Direct {
            offset,
            encoding: fields::Encoding::Raw,
        })
    }
    #[test]
    fn formation_stays_compact_and_rendering_preserves_values() {
        fn text_centers(
            shape: &egui::Shape,
            centers: &mut std::collections::HashMap<String, egui::Pos2>,
        ) {
            match shape {
                egui::Shape::Text(text) => {
                    centers.insert(
                        text.galley.text().to_owned(),
                        text.pos + text.galley.size() * 0.5,
                    );
                }
                egui::Shape::Vec(shapes) => {
                    for shape in shapes {
                        text_centers(shape, centers);
                    }
                }
                _ => {}
            }
        }

        let build = crate::game_version::GameBuild::Fm26Steam;
        let schemas = build.schemas();
        for invalid_right_cell in [false, true] {
            for width in [1200.0, 280.0] {
                let ctx = egui::Context::default();
                crate::ui::navigation::set_editing_enabled(&ctx, true);
                let mut player = PlayerRecord {
                    build,
                    addr: 0x20000,
                    index: 0,
                    name: None,
                    raw: vec![0; 1024],
                    person_shift: 0,
                    contract_raw: None,
                    contract_addr: None,
                    club_name: None,
                    club_addr: None,
                    nationality: None,
                    birthplace: None,
                };
                for field in schemas.fields_for_category(Entity::Player, Category::Positions) {
                    let loc = field.offset.as_ref().unwrap();
                    player.raw[loc.offset() as usize] = loc.encode_u8(10);
                }
                if invalid_right_cell {
                    let loc = schemas.field("pos_amr").unwrap().offset.unwrap();
                    player.raw[loc.offset() as usize] = 255;
                }
                let original = player.raw.clone();
                let mut centers = std::collections::HashMap::new();
                // Grid column sizes are retained across frames; check both the
                // first layout and subsequent layouts with cached geometry.
                for _ in 0..3 {
                    let mut edits = Vec::new();
                    let output = ctx.run(
                        egui::RawInput {
                            screen_rect: Some(egui::Rect::from_min_size(
                                egui::Pos2::ZERO,
                                egui::vec2(width, 700.0),
                            )),
                            ..Default::default()
                        },
                        |ctx| {
                            egui::CentralPanel::default().show(ctx, |ui| {
                                show_positions_formation(
                                    schemas,
                                    ui,
                                    &mut player,
                                    0x20000,
                                    &mut edits,
                                );
                            });
                        },
                    );
                    assert!(edits.is_empty());
                    assert_eq!(player.raw, original);
                    centers.clear();
                    for shape in &output.shapes {
                        text_centers(&shape.shape, &mut centers);
                    }
                    let aml = centers["AML"];
                    let amc = centers["AMC"];
                    let amr = centers["AMR"];
                    // Text origins are pixel-rounded independently, so their
                    // painted centers can differ by one pixel.
                    let close = |a: f32, b: f32| assert!((a - b).abs() <= 1.0, "{a} != {b}");
                    close(amc.x - aml.x, 70.0);
                    close(amr.x - amc.x, 70.0);
                    close(aml.y, amc.y);
                    close(amc.y, amr.y);
                    assert!(aml.x < 50.0, "formation must stay near the left edge");
                    assert!(amr.x < 200.0, "formation must not stretch to page width");
                    let dc = centers["DC"];
                    let sw = centers["SW"];
                    let gk = centers["GK"];
                    close(sw.x, dc.x);
                    close(sw.x, gk.x);
                    assert!(dc.y < sw.y && sw.y < gk.y);
                    assert!(!centers.contains_key("Sweeper:"));
                    if invalid_right_cell {
                        close(centers["r255"].x, amr.x);
                    }
                }
            }
        }
    }

    #[test]
    fn traits_require_mapping_and_complete_buffer() {
        assert_eq!(trait_value(&[0; 16], None), None);
        assert_eq!(trait_value(&[0; 7], direct(0)), None);
        assert_eq!(trait_value(&[0; 16], direct(9)), None);
        assert_eq!(trait_value(&[0; 16], direct(u64::MAX)), None);
        assert_eq!(
            trait_value(
                &[0; 16],
                Some(fields::FieldLocation::SubRecord {
                    container: "contract",
                    offset: 0,
                    encoding: fields::Encoding::Raw,
                })
            ),
            None
        );
        assert_eq!(
            trait_value(&[1, 0, 0, 0, 0, 0, 0, 0], direct(0)),
            Some((0, 1))
        );
        assert_eq!(trait_value(&[0; 16], direct(8)), Some((8, 0)));
    }

    #[test]
    fn fmt_date_masks_flag_bits() {
        // Day word 7019 = flags | day 363 → 29 Dec 2025.
        let buf = [7019u16.to_le_bytes(), 2025u16.to_le_bytes()].concat();
        assert_eq!(fmt_date_at(&buf, 0).as_deref(), Some("29 Dec 2025"));
    }

    #[test]
    fn set_and_shift_attr_values_emit_field_edits() {
        let schemas = crate::game_version::GameBuild::Fm24Steam.schemas();
        let mut raw = vec![0u8; schemas.player_struct_size];
        let mut edits = Vec::new();
        set_attr_value(schemas, &mut raw, 0x1000, "finishing", 15, &mut edits);
        assert_eq!(edits.len(), 1);
        let fin_loc = schemas.field("finishing").unwrap().offset.unwrap();
        assert_eq!(fin_loc.decode_u8(raw[fin_loc.offset() as usize]), 15);

        shift_attr_value(schemas, &mut raw, 0x1000, "finishing", 2, &mut edits);
        assert_eq!(edits.len(), 2);
        assert_eq!(fin_loc.decode_u8(raw[fin_loc.offset() as usize]), 17);
    }

    #[test]
    fn wonderkid_and_presets_update_player_attributes() {
        let build = crate::game_version::GameBuild::Fm24Steam;
        let schemas = build.schemas();
        let mut player = PlayerRecord {
            build,
            addr: 0x3000,
            index: 0,
            name: Some("Test Player".into()),
            raw: vec![0u8; schemas.player_struct_size],
            person_shift: 0,
            contract_raw: None,
            contract_addr: None,
            club_name: None,
            club_addr: None,
            nationality: None,
            birthplace: None,
        };
        let mut edits = Vec::new();
        apply_wonderkid(schemas, &mut player.raw, 0x3000, &ROLE_PROFILES[0], &mut edits);
        assert!(!edits.is_empty());
        assert_eq!(player.raw[schemas.offset_of("p_pa").unwrap()], 195);

        edits.clear();
        apply_player_preset(schemas, &mut player, 0x3000, PlayerPreset::Heal, &mut edits);
        assert!(!edits.is_empty());
        assert_eq!(player.field_u16("p_fitness"), Some(10000));
    }
}
