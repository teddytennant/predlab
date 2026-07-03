//! Leaderboard view: the public club ranking from the leaderboard site,
//! with a per-member profile pane (net worth, positions, history chart) on
//! row click.

use egui::{RichText, Ui};
use egui_extras::{Column, TableBuilder};
use predlab_util::fmt_money;

use crate::data::Snapshot;
use crate::message::Command;
use crate::ui::widgets;
use crate::ui::App;

#[derive(Default)]
pub(crate) struct LeaderboardState {
    /// Username whose profile pane is open, if any.
    viewing: Option<String>,
}

pub(crate) fn show(app: &mut App, ui: &mut Ui, snap: &Snapshot) {
    ui.horizontal(|ui| {
        ui.heading("Leaderboard");
        if ui.button("Refresh").clicked() {
            app.send_command(Command::RefreshLeaderboard);
        }
    });
    ui.label(
        RichText::new(
            "public club standings by paper net worth — click a row for the member's profile",
        )
        .small()
        .weak(),
    );
    widgets::section_error(ui, &snap.errors.leaderboard);
    ui.add_space(6.0);

    if let Some(username) = app.leaderboard.viewing.clone() {
        egui::Panel::bottom("leaderboard-profile")
            .resizable(true)
            .default_size(300.0)
            .show(ui, |ui| {
                egui::ScrollArea::vertical()
                    .auto_shrink([false; 2])
                    .show(ui, |ui| {
                        if widgets::profile_panel(ui, &username, snap) {
                            app.leaderboard.viewing = None;
                        }
                    });
            });
    }

    egui::CentralPanel::default().show(ui, |ui| {
        if snap.leaderboard.is_empty() {
            ui.label(RichText::new("no standings yet").weak());
            return;
        }
        standings_table(app, ui, snap);
    });
}

fn standings_table(app: &mut App, ui: &mut Ui, snap: &Snapshot) {
    let viewing = app.leaderboard.viewing.clone();
    let mut clicked: Option<String> = None;
    TableBuilder::new(ui)
        .id_salt("leaderboard")
        .striped(true)
        .sense(egui::Sense::click())
        .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
        .column(Column::auto().at_least(70.0))
        .column(Column::remainder().at_least(180.0))
        .column(Column::auto().at_least(120.0))
        .header(20.0, |mut header| {
            for title in ["Rank", "Member", "Net worth"] {
                header.col(|ui| {
                    ui.strong(title);
                });
            }
        })
        .body(|mut body| {
            for row_data in &snap.leaderboard {
                body.row(24.0, |mut row| {
                    row.set_selected(viewing.as_deref() == Some(row_data.username.as_str()));
                    row.col(|ui| {
                        ui.label(format!("{}{}", medal(row_data.rank), row_data.rank));
                    });
                    row.col(|ui| {
                        ui.label(&row_data.username);
                    });
                    row.col(|ui| {
                        ui.monospace(fmt_money(row_data.net_worth));
                    });
                    if row.response().clicked() {
                        clicked = Some(row_data.username.clone());
                    }
                });
            }
        });
    if let Some(username) = clicked {
        app.leaderboard.viewing = Some(username.clone());
        app.send_command(Command::FetchProfile { username });
    }
}

fn medal(rank: usize) -> &'static str {
    match rank {
        1 => "🥇 ",
        2 => "🥈 ",
        3 => "🥉 ",
        _ => "",
    }
}
