//! eframe/egui application root: window bootstrap, per-frame message drain,
//! sidebar navigation, and dispatch into the per-view modules.

mod onboarding;
mod views;
mod widgets;

use std::sync::mpsc::{Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::config::Config;
use crate::data::Snapshot;
use crate::message::{ActionKind, Command, ConnState, EngineMessage, UiMessage};

/// Everything the UI thread owns: the loaded config, both channel ends, and
/// the shared snapshot the engine keeps fresh.
pub struct UiContext {
    /// Config as loaded at startup; the UI edits a copy and pushes it via
    /// [`EngineMessage::ConfigChanged`].
    pub config: Config,
    /// Send commands / config / shutdown to the engine.
    pub engine_tx: Sender<EngineMessage>,
    /// Receive status, action results, and one-time key material.
    pub ui_rx: Receiver<UiMessage>,
    /// Read-side of the display snapshot; lock briefly, clone, release.
    pub snapshot: Arc<Mutex<Snapshot>>,
}

/// Run the UI until the user quits. The caller stops the engine after this
/// returns, so the app itself never sends [`EngineMessage::Shutdown`].
pub fn run(ctx: UiContext) -> anyhow::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("PredLab")
            .with_inner_size([1280.0, 800.0])
            .with_min_inner_size([960.0, 620.0]),
        ..Default::default()
    };
    eframe::run_native(
        "PredLab",
        options,
        Box::new(move |cc| Ok(Box::new(App::new(cc, ctx)))),
    )
    .map_err(|e| anyhow::anyhow!("eframe: {e}"))
}

/// Sidebar navigation target.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum View {
    #[default]
    Markets,
    Trade,
    Portfolio,
    Leaderboard,
    Admin,
    Settings,
}

/// The egui application: last-applied + staged config, per-view state, and
/// the channel/snapshot plumbing from [`UiContext`].
pub(crate) struct App {
    /// Config the engine is currently running with.
    pub(crate) config: Config,
    /// Staged edits (settings form + onboarding wizard), applied on demand.
    pub(crate) staged: Config,
    engine_tx: Sender<EngineMessage>,
    ui_rx: Receiver<UiMessage>,
    snapshot: Arc<Mutex<Snapshot>>,
    pub(crate) view: View,
    pub(crate) poly_conn: ConnState,
    toasts: Vec<widgets::Toast>,
    pub(crate) wizard: onboarding::Wizard,
    pub(crate) markets: views::markets::MarketsState,
    pub(crate) trade: views::trade::TradeState,
    pub(crate) leaderboard: views::leaderboard::LeaderboardState,
    pub(crate) admin: views::admin::AdminState,
    pub(crate) settings: views::settings::SettingsState,
    /// Latest place/cancel outcome, shown inline in the trade ticket.
    pub(crate) last_order: Option<(bool, String)>,
}

impl App {
    fn new(cc: &eframe::CreationContext<'_>, uictx: UiContext) -> Self {
        cc.egui_ctx.set_theme(egui::Theme::Dark);
        cc.egui_ctx.all_styles_mut(|style| {
            style.spacing.item_spacing = egui::vec2(8.0, 6.0);
            style.visuals.selection.bg_fill = widgets::ACCENT.gamma_multiply(0.4);
            style.visuals.hyperlink_color = widgets::ACCENT;
        });
        let wizard = onboarding::Wizard::new();
        Self {
            staged: uictx.config.clone(),
            config: uictx.config,
            engine_tx: uictx.engine_tx,
            ui_rx: uictx.ui_rx,
            snapshot: uictx.snapshot,
            view: View::default(),
            poly_conn: ConnState::Unknown,
            toasts: Vec::new(),
            wizard,
            markets: Default::default(),
            trade: Default::default(),
            leaderboard: Default::default(),
            admin: Default::default(),
            settings: Default::default(),
            last_order: None,
        }
    }

    /// Fire-and-forget command send; a closed channel means shutdown is
    /// already in progress.
    pub(crate) fn send_command(&self, command: Command) {
        let _ = self.engine_tx.send(EngineMessage::Command(command));
    }

    /// Push a full config to the engine without persisting it.
    pub(crate) fn push_config(&self, config: &Config) {
        let _ = self.engine_tx.send(EngineMessage::ConfigChanged(config.clone()));
    }

    /// Persist the staged config, adopt it as applied, and tell the engine.
    /// On save failure nothing is adopted and the error surfaces as a toast.
    pub(crate) fn apply_staged(&mut self, toast_text: &str) {
        self.staged.clamp();
        match self.staged.save() {
            Ok(()) => {
                self.config = self.staged.clone();
                self.push_config(&self.config);
                self.send_command(Command::RefreshAll);
                self.toast(true, toast_text.to_string());
            }
            Err(e) => self.toast(false, format!("could not save config: {e:#}")),
        }
    }

    pub(crate) fn toast(&mut self, ok: bool, text: String) {
        self.toasts.push(widgets::Toast::new(ok, text));
    }

    fn drain_messages(&mut self) {
        while let Ok(msg) = self.ui_rx.try_recv() {
            match msg {
                UiMessage::Status { poly } => {
                    self.poly_conn = poly;
                }
                UiMessage::ActionResult { kind, ok, detail } => {
                    self.on_action_result(kind, ok, detail);
                }
                UiMessage::KeyIssued {
                    username,
                    role,
                    api_key,
                } => {
                    self.admin.issued = Some(views::admin::IssuedKey {
                        username,
                        role,
                        api_key,
                    });
                }
            }
        }
    }

    fn on_action_result(&mut self, kind: ActionKind, ok: bool, detail: String) {
        if matches!(kind, ActionKind::PlaceOrder | ActionKind::CancelOrder) {
            self.last_order = Some((ok, detail.clone()));
        }
        // Navigation-ish actions succeed constantly; only their failures
        // are worth a toast.
        let quiet_when_ok = matches!(
            kind,
            ActionKind::SetSearch
                | ActionKind::SetMarketsOffset
                | ActionKind::SelectMarket
                | ActionKind::FetchProfile
                | ActionKind::FetchAdminUser
        );
        if ok && quiet_when_ok {
            return;
        }
        self.toast(ok, format!("{}: {detail}", kind_label(kind)));
    }

    fn show_sidebar(&mut self, ui: &mut egui::Ui, snap: &Snapshot) {
        egui::Panel::left("nav")
            .resizable(false)
            .exact_size(190.0)
            .show(ui, |ui| {
                ui.add_space(10.0);
                ui.label(
                    egui::RichText::new("PredLab")
                        .color(widgets::ACCENT)
                        .strong()
                        .size(24.0),
                );
                ui.label(
                    egui::RichText::new("Prediction Markets Club")
                        .small()
                        .weak(),
                );
                ui.add_space(14.0);
                let admin_label = if views::admin::unlocked(self, snap) {
                    "Admin"
                } else {
                    "Admin 🔒"
                };
                for (view, label) in [
                    (View::Markets, "Markets"),
                    (View::Trade, "Trade"),
                    (View::Portfolio, "Portfolio"),
                    (View::Leaderboard, "Leaderboard"),
                    (View::Admin, admin_label),
                    (View::Settings, "Settings"),
                ] {
                    if ui.selectable_label(self.view == view, label).clicked() {
                        self.view = view;
                    }
                }
                ui.with_layout(egui::Layout::bottom_up(egui::Align::Min), |ui| {
                    ui.add_space(8.0);
                    ui.label(
                        egui::RichText::new(widgets::fmt_last_updated(snap.last_updated))
                            .small()
                            .weak(),
                    );
                    widgets::conn_row(ui, "Simulator", &self.poly_conn);
                });
            });
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.drain_messages();

        // Debounced market search (fires ~400ms after the last keystroke).
        if let Some(query) = self.markets.due_search() {
            self.send_command(Command::SetSearch(query));
        }
        if self.markets.search_pending() {
            ctx.request_repaint_after(Duration::from_millis(120));
        }

        // Clone the snapshot under a short lock; it is small by design.
        let snap = self
            .snapshot
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone();

        if !self.config.onboarded {
            onboarding::show(self, ui);
        } else {
            self.show_sidebar(ui, &snap);
            egui::CentralPanel::default().show(ui, |ui| match self.view {
                View::Markets => views::markets::show(self, ui, &snap),
                View::Trade => views::trade::show(self, ui, &snap),
                View::Portfolio => views::portfolio::show(self, ui, &snap),
                View::Leaderboard => views::leaderboard::show(self, ui, &snap),
                View::Admin => views::admin::show(self, ui, &snap),
                View::Settings => views::settings::show(self, ui),
            });
        }

        widgets::show_toasts(&ctx, &mut self.toasts);
        // Poll results should appear without mouse movement.
        ctx.request_repaint_after(Duration::from_millis(500));
    }
}

/// Short human label for an [`ActionKind`], used as the toast prefix.
fn kind_label(kind: ActionKind) -> &'static str {
    match kind {
        ActionKind::RefreshAll => "refresh",
        ActionKind::SetSearch => "search",
        ActionKind::SetMarketsOffset => "page",
        ActionKind::SelectMarket => "select market",
        ActionKind::PlaceOrder => "order",
        ActionKind::CancelOrder => "cancel",
        ActionKind::FetchProfile => "profile",
        ActionKind::FetchAdminUser => "member detail",
        ActionKind::IssueKey => "issue key",
        ActionKind::SetRole => "set role",
        ActionKind::RevokeKey => "revoke key",
        ActionKind::ResetBalance => "reset balance",
        ActionKind::DeleteUser => "delete user",
        ActionKind::ForceResolve => "force resolve",
        ActionKind::RefreshLeaderboard => "leaderboard",
    }
}
