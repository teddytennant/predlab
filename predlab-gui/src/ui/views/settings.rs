//! Settings view: staged config editing with Apply / Revert, plus a way to
//! re-run onboarding.

use egui::{RichText, Ui};

use crate::config::{Config, MARKET_LIMIT_MAX, MARKET_LIMIT_MIN, TICK_SECONDS_MAX, TICK_SECONDS_MIN};
use crate::ui::onboarding::Wizard;
use crate::ui::widgets::{self, AMBER};
use crate::ui::App;

#[derive(Default)]
pub(crate) struct SettingsState {
    reveal_admin: bool,
}

pub(crate) fn show(app: &mut App, ui: &mut Ui) {
    ui.heading("Settings");
    ui.add_space(4.0);
    egui::ScrollArea::vertical()
        .auto_shrink([false; 2])
        .show(ui, |ui| {
            servers_section(app, ui);
            ui.add_space(8.0);
            key_section(app, ui);
            ui.add_space(8.0);
            admin_section(app, ui);
            ui.add_space(8.0);
            polling_section(app, ui);
            ui.add_space(10.0);
            buttons_row(app, ui);
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                ui.label(RichText::new("Config file:").small().weak());
                ui.label(
                    RichText::new(Config::path().display().to_string())
                        .small()
                        .monospace(),
                );
            });
        });
}

fn servers_section(app: &mut App, ui: &mut Ui) {
    egui::Frame::group(ui.style()).show(ui, |ui| {
        ui.strong("Servers");
        egui::Grid::new("settings-servers")
            .num_columns(2)
            .spacing([12.0, 8.0])
            .show(ui, |ui| {
                ui.label("Simulator URL");
                ui.add(egui::TextEdit::singleline(&mut app.staged.poly_url).desired_width(340.0));
                ui.end_row();
                ui.label("Leaderboard URL");
                ui.add(
                    egui::TextEdit::singleline(&mut app.staged.leaderboard_url)
                        .desired_width(340.0),
                );
                ui.end_row();
            });
        ui.label(
            RichText::new(
                "the defaults point at the club's hosted servers; change them only if \
                 you self-host (e.g. http://localhost:8001 and http://localhost:8003)",
            )
            .small()
            .weak(),
        );
    });
}

fn key_section(app: &mut App, ui: &mut Ui) {
    egui::Frame::group(ui.style()).show(ui, |ui| {
        ui.strong("Trading key");
        egui::Grid::new("settings-key")
            .num_columns(2)
            .spacing([12.0, 8.0])
            .show(ui, |ui| {
                ui.label("API key");
                ui.add(
                    egui::TextEdit::singleline(&mut app.staged.poly_api_key)
                        .hint_text("pm_paper_...")
                        .font(egui::TextStyle::Monospace)
                        .desired_width(340.0),
                );
                ui.end_row();
            });
        ui.label(
            RichText::new("sent as the POLY_API_KEY header — ask your club admin for one")
                .small()
                .weak(),
        );
    });
}

fn admin_section(app: &mut App, ui: &mut Ui) {
    egui::Frame::group(ui.style()).show(ui, |ui| {
        ui.strong("Admin access");
        egui::Grid::new("settings-admin")
            .num_columns(2)
            .spacing([12.0, 8.0])
            .show(ui, |ui| {
                ui.label("Master secret (X-Admin-Secret)");
                widgets::secret_field(
                    ui,
                    &mut app.staged.poly_admin_secret,
                    &mut app.settings.reveal_admin,
                );
                ui.end_row();
            });
        ui.label(
            RichText::new(
                "optional: the server's owner-rank master secret. An admin-role API key \
                 above also unlocks the Admin panel — leave this empty in that case.",
            )
            .small()
            .weak(),
        );
    });
}

fn polling_section(app: &mut App, ui: &mut Ui) {
    egui::Frame::group(ui.style()).show(ui, |ui| {
        ui.strong("Polling");
        egui::Grid::new("settings-polling")
            .num_columns(2)
            .spacing([12.0, 8.0])
            .show(ui, |ui| {
                ui.label("Refresh interval");
                ui.add(
                    egui::Slider::new(
                        &mut app.staged.tick_seconds,
                        TICK_SECONDS_MIN..=TICK_SECONDS_MAX,
                    )
                    .suffix(" s"),
                );
                ui.end_row();
                ui.label("Markets per page");
                ui.add(
                    egui::DragValue::new(&mut app.staged.market_limit)
                        .range(MARKET_LIMIT_MIN..=MARKET_LIMIT_MAX),
                );
                ui.end_row();
            });
        ui.label(
            RichText::new("the sim caps market pages at 500")
                .small()
                .weak(),
        );
    });
}

fn buttons_row(app: &mut App, ui: &mut Ui) {
    ui.horizontal(|ui| {
        let dirty = app.staged != app.config;
        if ui.add_enabled(dirty, egui::Button::new("Apply")).clicked() {
            app.apply_staged("settings applied");
        }
        if ui.add_enabled(dirty, egui::Button::new("Revert")).clicked() {
            app.staged = app.config.clone();
        }
        if ui.button("Re-run onboarding").clicked() {
            app.staged.onboarded = false;
            app.apply_staged("onboarding will run again");
            app.wizard = Wizard::new();
        }
        if dirty {
            ui.label(RichText::new("unapplied changes").color(AMBER).small());
        }
    });
}
