//! Admin view: issue paper keys (with roles), manage the server-side roster
//! (`/admin/leaderboard`), force-resolve markets, and the confirm-guarded
//! danger zone. Locked until the config has the master secret or the user's
//! own key probes as admin/owner rank.

use std::time::{Duration, Instant};

use egui::{RichText, Ui};
use egui_extras::{Column, TableBuilder};
use predlab_util::fmt_money;

use crate::data::Snapshot;
use crate::message::{Command, Role};
use crate::ui::widgets::{self, AMBER, RED};
use crate::ui::{App, View};

/// How long the "click again to confirm" arm lasts.
const CONFIRM_WINDOW: Duration = Duration::from_secs(5);

/// One-time key material from [`crate::message::UiMessage::KeyIssued`].
pub(crate) struct IssuedKey {
    pub(crate) username: String,
    pub(crate) role: String,
    pub(crate) api_key: String,
}

#[derive(Default)]
pub(crate) struct AdminState {
    username: String,
    display_name: String,
    role: Role,
    pub(crate) issued: Option<IssuedKey>,
    /// Username whose profile pane is open, if any.
    viewing: Option<String>,
    confirm_delete: Option<(String, Instant)>,
    confirm_reset_all: Option<Instant>,
    resolve_market_id: String,
    resolve_yes: bool,
}

/// Whether the Admin view is usable: master secret configured, or the
/// engine's `/admin/leaderboard` probe succeeded with the user's own key.
pub(crate) fn unlocked(app: &App, snap: &Snapshot) -> bool {
    !app.config.poly_admin_secret.is_empty() || snap.admin_ok == Some(true)
}

pub(crate) fn show(app: &mut App, ui: &mut Ui, snap: &Snapshot) {
    ui.heading("Admin");
    ui.add_space(4.0);

    if !unlocked(app, snap) {
        locked_panel(app, ui, snap);
        return;
    }

    if let Some(username) = app.admin.viewing.clone() {
        egui::Panel::bottom("admin-profile")
            .resizable(true)
            .default_size(300.0)
            .show(ui, |ui| {
                egui::ScrollArea::vertical()
                    .auto_shrink([false; 2])
                    .show(ui, |ui| {
                        if widgets::profile_panel(ui, &username, snap) {
                            app.admin.viewing = None;
                        }
                    });
            });
    }

    egui::CentralPanel::default().show(ui, |ui| {
        egui::ScrollArea::vertical()
            .auto_shrink([false; 2])
            .show(ui, |ui| {
                if app.admin.issued.is_some() {
                    issued_panel(app, ui);
                    ui.add_space(10.0);
                }
                issue_key_card(app, ui);
                ui.add_space(10.0);
                roster_section(app, ui, snap);
                ui.add_space(10.0);
                force_resolve_card(app, ui);
                ui.add_space(10.0);
                danger_zone(app, ui);
            });
    });
}

fn locked_panel(app: &mut App, ui: &mut Ui, snap: &Snapshot) {
    egui::Frame::group(ui.style())
        .inner_margin(egui::Margin::same(16))
        .show(ui, |ui| {
            ui.strong("Admin tools are locked");
            ui.label(
                "Issuing member keys, resetting balances and the club roster need admin \
                 rank on the simulator. There are two ways in:",
            );
            ui.label("• your own API key has the admin or owner role (the app checks automatically), or");
            ui.label("• you paste the server's master admin secret (X-Admin-Secret) in Settings.");
            ui.add_space(4.0);
            match snap.admin_ok {
                Some(false) => {
                    ui.label(
                        RichText::new(
                            "your current key was checked and has member rank — ask the club \
                             owner for an admin-role key or the master secret",
                        )
                        .color(AMBER)
                        .small(),
                    );
                }
                None => {
                    ui.label(RichText::new("checking your key's role…").small().weak());
                }
                Some(true) => {}
            }
            ui.add_space(6.0);
            if ui.button("Open Settings").clicked() {
                app.view = View::Settings;
            }
        });
}

/// Freshly issued key; it exists only here and in the member's hands.
fn issued_panel(app: &mut App, ui: &mut Ui) {
    let mut dismiss = false;
    if let Some(issued) = &app.admin.issued {
        egui::Frame::group(ui.style())
            .inner_margin(egui::Margin::same(12))
            .stroke(egui::Stroke::new(1.0, AMBER))
            .show(ui, |ui| {
                ui.strong(format!(
                    "Key issued for {} (role: {})",
                    issued.username, issued.role
                ));
                ui.horizontal(|ui| {
                    ui.label("API key:");
                    widgets::mono_copy(ui, &issued.api_key);
                });
                ui.label(
                    RichText::new(
                        "Hand this key to the member NOW — it is shown once and the app \
                         does not store it anywhere.",
                    )
                    .color(AMBER)
                    .strong(),
                );
                if ui.button("Dismiss").clicked() {
                    dismiss = true;
                }
            });
    }
    if dismiss {
        app.admin.issued = None;
    }
}

fn issue_key_card(app: &mut App, ui: &mut Ui) {
    egui::Frame::group(ui.style()).show(ui, |ui| {
        ui.strong("Issue member key");
        egui::Grid::new("issue-key")
            .num_columns(2)
            .spacing([12.0, 8.0])
            .show(ui, |ui| {
                ui.label("Username");
                ui.add(
                    egui::TextEdit::singleline(&mut app.admin.username)
                        .hint_text("e.g. jsmith")
                        .desired_width(220.0),
                );
                ui.end_row();
                ui.label("Display name");
                ui.add(
                    egui::TextEdit::singleline(&mut app.admin.display_name)
                        .hint_text("e.g. Jane Smith")
                        .desired_width(220.0),
                );
                ui.end_row();
                ui.label("Role");
                ui.horizontal(|ui| {
                    for role in [Role::Member, Role::Admin, Role::Owner] {
                        ui.selectable_value(&mut app.admin.role, role, role.label());
                    }
                });
                ui.end_row();
            });
        if app.admin.role != Role::Member {
            ui.label(
                RichText::new(
                    "granting admin or owner requires owner rank (the master secret, or \
                     an owner-role key)",
                )
                .color(AMBER)
                .small(),
            );
        }
        let username = app.admin.username.trim().to_string();
        if ui
            .add_enabled(!username.is_empty(), egui::Button::new("Issue key"))
            .clicked()
        {
            let display_name = app.admin.display_name.trim().to_string();
            let display_name = if display_name.is_empty() {
                username.clone()
            } else {
                display_name
            };
            app.send_command(Command::IssueKey {
                username,
                display_name,
                role: app.admin.role,
            });
        }
    });
}

fn roster_section(app: &mut App, ui: &mut Ui, snap: &Snapshot) {
    ui.strong(format!("Roster ({})", snap.roster.len()));
    ui.label(
        RichText::new("server-side member list from /admin/leaderboard, ranked by net worth")
            .small()
            .weak(),
    );
    widgets::section_error(ui, &snap.errors.roster);
    if snap.roster.is_empty() {
        ui.label(RichText::new("no members yet — issue a key above to add one").weak());
        return;
    }
    let mut action: Option<Command> = None;
    let mut view_user: Option<String> = None;
    let mut arm_delete: Option<String> = None;
    let delete_armed = app
        .admin
        .confirm_delete
        .as_ref()
        .filter(|(_, t)| t.elapsed() < CONFIRM_WINDOW)
        .map(|(u, _)| u.clone());
    TableBuilder::new(ui)
        .id_salt("roster")
        .striped(true)
        .vscroll(false)
        .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
        .column(Column::auto().at_least(120.0))
        .column(Column::auto().at_least(70.0))
        .column(Column::auto().at_least(100.0))
        .column(Column::auto().at_least(100.0))
        .column(Column::remainder().at_least(330.0))
        .header(20.0, |mut header| {
            for title in ["Username", "Role", "Cash", "Net worth", "Actions"] {
                header.col(|ui| {
                    ui.strong(title);
                });
            }
        })
        .body(|mut body| {
            for member in &snap.roster {
                body.row(24.0, |mut row| {
                    row.col(|ui| {
                        ui.monospace(&member.username);
                    });
                    row.col(|ui| {
                        let color = match member.role.as_str() {
                            "owner" => AMBER,
                            "admin" => widgets::ACCENT,
                            _ => ui.visuals().text_color(),
                        };
                        ui.label(RichText::new(&member.role).color(color).small());
                    });
                    row.col(|ui| {
                        ui.monospace(fmt_money(member.cash));
                    });
                    row.col(|ui| {
                        ui.monospace(fmt_money(member.net_worth));
                    });
                    row.col(|ui| {
                        if ui.small_button("View").on_hover_text("Profile + history").clicked() {
                            view_user = Some(member.username.clone());
                        }
                        if ui
                            .small_button("Reset")
                            .on_hover_text("Reset balance to the starting amount")
                            .clicked()
                        {
                            action = Some(Command::ResetBalance {
                                username: Some(member.username.clone()),
                            });
                        }
                        if ui
                            .small_button("Revoke")
                            .on_hover_text("Deactivate all of this member's API keys")
                            .clicked()
                        {
                            action = Some(Command::RevokeKey {
                                username: member.username.clone(),
                            });
                        }
                        ui.menu_button("Role", |ui| {
                            ui.label(RichText::new("owner only").small().weak());
                            for role in [Role::Member, Role::Admin, Role::Owner] {
                                if ui.button(role.label()).clicked() {
                                    action = Some(Command::SetRole {
                                        username: member.username.clone(),
                                        role,
                                    });
                                    ui.close();
                                }
                            }
                        });
                        let armed = delete_armed.as_deref() == Some(member.username.as_str());
                        let label = if armed {
                            RichText::new("Confirm?").color(RED).strong()
                        } else {
                            RichText::new("Delete").color(RED)
                        };
                        if ui
                            .small_button(label)
                            .on_hover_text("Permanently remove this member and all their data")
                            .clicked()
                        {
                            if armed {
                                action = Some(Command::DeleteUser {
                                    username: member.username.clone(),
                                });
                            } else {
                                arm_delete = Some(member.username.clone());
                            }
                        }
                    });
                });
            }
        });
    if let Some(username) = view_user {
        app.admin.viewing = Some(username.clone());
        app.send_command(Command::FetchAdminUser { username });
    }
    if let Some(username) = arm_delete {
        app.admin.confirm_delete = Some((username, Instant::now()));
    }
    if let Some(command) = action {
        if matches!(command, Command::DeleteUser { .. }) {
            app.admin.confirm_delete = None;
        }
        app.send_command(command);
    }
}

fn force_resolve_card(app: &mut App, ui: &mut Ui) {
    egui::Frame::group(ui.style()).show(ui, |ui| {
        ui.strong("Force-resolve a market");
        ui.label(
            RichText::new("owner only — settles the market and pays out winners at $1.00")
                .small()
                .weak(),
        );
        ui.horizontal(|ui| {
            ui.label("Market id");
            ui.add(
                egui::TextEdit::singleline(&mut app.admin.resolve_market_id)
                    .font(egui::TextStyle::Monospace)
                    .hint_text("from the market row / API")
                    .desired_width(260.0),
            );
            ui.selectable_value(&mut app.admin.resolve_yes, true, "YES");
            ui.selectable_value(&mut app.admin.resolve_yes, false, "NO");
            let market_id = app.admin.resolve_market_id.trim().to_string();
            if ui
                .add_enabled(!market_id.is_empty(), egui::Button::new("Resolve"))
                .clicked()
            {
                app.send_command(Command::ForceResolve {
                    market_id,
                    resolution: if app.admin.resolve_yes { "yes" } else { "no" }.to_string(),
                });
            }
        });
    });
}

fn danger_zone(app: &mut App, ui: &mut Ui) {
    egui::CollapsingHeader::new(RichText::new("Danger zone").color(RED))
        .id_salt("danger-zone")
        .show(ui, |ui| {
            let armed = app
                .admin
                .confirm_reset_all
                .is_some_and(|t| t.elapsed() < CONFIRM_WINDOW);
            let label = if armed {
                RichText::new("Click again to confirm").color(RED).strong()
            } else {
                RichText::new("Reset ALL balances").color(RED)
            };
            if ui.button(label).clicked() {
                if armed {
                    app.admin.confirm_reset_all = None;
                    app.send_command(Command::ResetBalance { username: None });
                } else {
                    app.admin.confirm_reset_all = Some(Instant::now());
                }
            }
            ui.label(
                RichText::new(
                    "returns every member to the starting balance, cancelling their \
                     orders and clearing their positions — e.g. before a new competition",
                )
                .small()
                .weak(),
            );
        });
}
