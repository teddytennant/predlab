//! First-run onboarding wizard: a full-window flow (no sidebar) that stages
//! edits into [`App::staged`] and applies them once on Finish.

use egui::{RichText, Ui};

use super::widgets::{self, ACCENT, GREEN, RED};
use super::App;
use crate::config::Config;
use crate::message::Command;

const STEPS: [&str; 5] = ["Welcome", "Servers", "API key", "Admin", "Done"];

/// Wizard state; the values being edited live in [`App::staged`].
pub(crate) struct Wizard {
    step: usize,
    error: Option<String>,
    /// Whether "Test connection" has been pressed on the servers step.
    test_sent: bool,
}

impl Wizard {
    pub(crate) fn new() -> Self {
        Self {
            step: 0,
            error: None,
            test_sent: false,
        }
    }
}

/// Draw the whole wizard for this frame.
pub(crate) fn show(app: &mut App, ui: &mut Ui) {
    egui::CentralPanel::default().show(ui, |ui| {
        egui::ScrollArea::vertical()
            .auto_shrink([false; 2])
            .show(ui, |ui| {
                ui.vertical_centered(|ui| {
                    ui.set_max_width(680.0);
                    ui.add_space(24.0);
                    ui.label(RichText::new("PredLab").color(ACCENT).strong().size(30.0));
                    ui.label(RichText::new("first-run setup").weak());
                    ui.add_space(10.0);
                    progress(ui, app.wizard.step);
                    ui.add_space(10.0);
                    egui::Frame::group(ui.style())
                        .inner_margin(egui::Margin::same(16))
                        .show(ui, |ui| {
                            ui.set_width(620.0);
                            match app.wizard.step {
                                0 => step_welcome(ui),
                                1 => step_servers(app, ui),
                                2 => step_key(app, ui),
                                3 => step_admin(app, ui),
                                _ => step_done(app, ui),
                            }
                        });
                    ui.add_space(8.0);
                    nav_buttons(app, ui);
                });
            });
    });
}

fn progress(ui: &mut Ui, current: usize) {
    ui.horizontal(|ui| {
        for (i, name) in STEPS.iter().enumerate() {
            let text = format!("{}. {name}", i + 1);
            if i == current {
                ui.label(RichText::new(text).color(ACCENT).strong());
            } else if i < current {
                ui.label(RichText::new(text).color(GREEN));
            } else {
                ui.label(RichText::new(text).weak());
            }
            if i + 1 < STEPS.len() {
                ui.label(RichText::new("›").weak());
            }
        }
    });
}

fn step_welcome(ui: &mut Ui) {
    ui.heading("Welcome");
    ui.add_space(6.0);
    ui.label(
        "PredLab is the Prediction Markets Club's paper-trading environment. \
         You trade Polymarket-style markets with fake money, track your \
         portfolio, and compete on the club leaderboard. The club's hosted \
         server is preconfigured — most members only need to paste their API \
         key. Nothing here touches real markets or real money.",
    );
}

fn step_servers(app: &mut App, ui: &mut Ui) {
    ui.heading("Servers");
    ui.add_space(6.0);
    ui.label(
        "Where the simulator and the public leaderboard run. The defaults \
         are the club's hosted servers — leave them unless you self-host.",
    );
    ui.add_space(6.0);
    egui::Grid::new("onboard-servers")
        .num_columns(2)
        .spacing([12.0, 8.0])
        .show(ui, |ui| {
            ui.label("Simulator URL");
            ui.add(egui::TextEdit::singleline(&mut app.staged.poly_url).desired_width(340.0));
            ui.end_row();
            ui.label("Leaderboard URL");
            ui.add(
                egui::TextEdit::singleline(&mut app.staged.leaderboard_url).desired_width(340.0),
            );
            ui.end_row();
        });
    ui.add_space(6.0);
    if ui.button("Test connection").clicked() {
        app.push_config(&app.staged);
        app.send_command(Command::RefreshAll);
        app.wizard.test_sent = true;
    }
    if app.wizard.test_sent {
        ui.add_space(4.0);
        widgets::conn_row(ui, "Simulator", &app.poly_conn);
        ui.label(
            RichText::new("the result appears within a few seconds — green means connected")
                .small()
                .weak(),
        );
    }
}

fn step_key(app: &mut App, ui: &mut Ui) {
    ui.heading("Your API key");
    ui.add_space(6.0);
    ui.label(
        "Trading, your portfolio and the leaderboard score all hang off one \
         key. Keys are issued by club admins — ask your admin for yours.",
    );
    ui.add_space(6.0);
    ui.label("API key");
    ui.add(
        egui::TextEdit::singleline(&mut app.staged.poly_api_key)
            .hint_text("pm_paper_...")
            .font(egui::TextStyle::Monospace)
            .desired_width(f32::INFINITY),
    );
    ui.label(
        RichText::new(
            "no key yet? you can still browse markets and the leaderboard — \
             add the key later in Settings",
        )
        .small()
        .weak(),
    );
}

fn step_admin(app: &mut App, ui: &mut Ui) {
    ui.heading("Admin access (optional)");
    ui.add_space(6.0);
    ui.label(
        "Do you run the club? Paste the server's master admin secret to \
         unlock key issuance, balance resets and the roster.",
    );
    ui.label(
        RichText::new(
            "an admin-role API key on the previous step also unlocks the Admin \
             panel — the secret is only needed for owner-rank bootstrap",
        )
        .small()
        .weak(),
    );
    ui.label(
        RichText::new("leave blank if you're a member — you can add it later in Settings")
            .small()
            .weak(),
    );
    ui.add_space(6.0);
    egui::Grid::new("onboard-admin")
        .num_columns(2)
        .spacing([12.0, 8.0])
        .show(ui, |ui| {
            ui.label("Master secret");
            ui.add(
                egui::TextEdit::singleline(&mut app.staged.poly_admin_secret)
                    .password(true)
                    .hint_text("X-Admin-Secret")
                    .desired_width(300.0),
            );
            ui.end_row();
        });
}

fn step_done(app: &mut App, ui: &mut Ui) {
    ui.heading("All set");
    ui.add_space(6.0);
    let staged = &app.staged;
    summary_row(ui, "Simulator", &staged.poly_url);
    summary_row(ui, "Leaderboard", &staged.leaderboard_url);
    summary_row(
        ui,
        "API key",
        if staged.poly_api_key.is_empty() {
            "not set — market data only"
        } else {
            "configured"
        },
    );
    summary_row(
        ui,
        "Admin access",
        if staged.poly_admin_secret.is_empty() {
            "no master secret (an admin-role key still unlocks Admin)"
        } else {
            "master secret configured"
        },
    );
    ui.add_space(6.0);
    ui.label(
        RichText::new(format!(
            "Finish saves this to {} and starts polling.",
            Config::path().display()
        ))
        .small()
        .weak(),
    );
}

fn summary_row(ui: &mut Ui, label: &str, value: &str) {
    ui.horizontal(|ui| {
        ui.label(RichText::new(format!("{label}:")).weak());
        ui.monospace(value);
    });
}

fn nav_buttons(app: &mut App, ui: &mut Ui) {
    ui.horizontal(|ui| {
        if app.wizard.step > 0 && ui.button("← Back").clicked() {
            app.wizard.step -= 1;
            app.wizard.error = None;
        }
        let last = app.wizard.step + 1 == STEPS.len();
        let label = if last { "Finish" } else { "Next →" };
        if ui.button(label).clicked() {
            if let Some(error) = validate(app) {
                app.wizard.error = Some(error);
            } else if last {
                finish(app);
            } else {
                app.wizard.step += 1;
                app.wizard.error = None;
            }
        }
    });
    if let Some(error) = &app.wizard.error {
        ui.label(RichText::new(error).color(RED));
    }
}

/// Loose per-step validation: only empty URLs hard-block.
fn validate(app: &App) -> Option<String> {
    if app.wizard.step == 1
        && (app.staged.poly_url.trim().is_empty() || app.staged.leaderboard_url.trim().is_empty())
    {
        return Some("both server URLs are required".to_string());
    }
    None
}

fn finish(app: &mut App) {
    app.staged.onboarded = true;
    // Saves the config; on failure the toast explains and the wizard stays.
    app.apply_staged("setup complete — welcome to PredLab");
}
