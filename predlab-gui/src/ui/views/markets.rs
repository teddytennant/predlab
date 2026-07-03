//! Markets view: browse the market list (search + offset paging), inspect
//! an order book in the detail pane, and jump into a pre-filled trade ticket.

use std::time::{Duration, Instant};

use egui::{RichText, Ui};
use egui_extras::{Column, TableBuilder};
use predlab_util::truncate;

use crate::data::Snapshot;
use crate::domain::polymarket::PolyMarket;
use crate::message::{Command, OrderSide};
use crate::ui::widgets::{self, GREEN, RED};
use crate::ui::{App, View};

/// Delay between the last search keystroke and the engine request.
const SEARCH_DEBOUNCE: Duration = Duration::from_millis(400);
/// Levels drawn per side of the detail-pane ladder.
const LADDER_ROWS: usize = 8;

/// The market the user picked, with its outcome/token pairs.
#[derive(Debug, Clone)]
pub(crate) struct Selection {
    pub(crate) market_id: String,
    pub(crate) question: String,
    /// `(outcome name, clob token id)` pairs, parallel arrays zipped —
    /// index 0 is YES, index 1 is NO.
    pub(crate) tokens: Vec<(String, String)>,
    pub(crate) outcome_idx: usize,
}

impl Selection {
    fn token_id(&self) -> Option<&str> {
        self.tokens.get(self.outcome_idx).map(|(_, t)| t.as_str())
    }

    fn outcome_name(&self) -> &str {
        self.tokens
            .get(self.outcome_idx)
            .map(|(o, _)| o.as_str())
            .unwrap_or("?")
    }
}

#[derive(Default)]
pub(crate) struct MarketsState {
    search: String,
    sent_search: String,
    pending_since: Option<Instant>,
    /// Current `/markets` pagination offset (page size = config market_limit).
    offset: u32,
    pub(crate) selected: Option<Selection>,
}

impl MarketsState {
    /// Debounced search: returns the query to send once [`SEARCH_DEBOUNCE`]
    /// has passed since the last keystroke and the query actually changed.
    pub(crate) fn due_search(&mut self) -> Option<String> {
        if self
            .pending_since
            .is_none_or(|t| t.elapsed() < SEARCH_DEBOUNCE)
        {
            return None;
        }
        self.pending_since = None;
        if self.search == self.sent_search {
            return None;
        }
        self.sent_search = self.search.clone();
        self.offset = 0;
        Some(self.search.clone())
    }

    pub(crate) fn search_pending(&self) -> bool {
        self.pending_since.is_some()
    }
}

pub(crate) fn show(app: &mut App, ui: &mut Ui, snap: &Snapshot) {
    ui.heading("Markets");
    ui.add_space(4.0);

    if app.markets.selected.is_some() {
        egui::Panel::bottom("markets-detail")
            .resizable(true)
            .default_size(260.0)
            .show(ui, |ui| detail_pane(app, ui, snap));
    }
    egui::CentralPanel::default().show(ui, |ui| markets_table(app, ui, snap));
}

fn markets_table(app: &mut App, ui: &mut Ui, snap: &Snapshot) {
    let page = app.config.market_limit;
    ui.horizontal(|ui| {
        ui.label("Search");
        let response = ui.add(
            egui::TextEdit::singleline(&mut app.markets.search)
                .hint_text("filter questions…")
                .desired_width(280.0),
        );
        if response.changed() {
            app.markets.pending_since = Some(Instant::now());
        }
        if app.markets.search_pending() {
            ui.spinner();
        }
        ui.separator();
        // Offset paging: Prev/Next move by one page (the config's limit).
        let offset = app.markets.offset;
        if ui.add_enabled(offset > 0, egui::Button::new("◀ Prev")).clicked() {
            app.markets.offset = offset.saturating_sub(page);
            app.send_command(Command::SetMarketsOffset(app.markets.offset));
        }
        ui.label(
            RichText::new(format!("{}–{}", offset + 1, offset + page))
                .small()
                .weak(),
        );
        // A short page means the catalog ended; disable Next.
        let full_page = snap.markets.len() as u32 >= page;
        if ui.add_enabled(full_page, egui::Button::new("Next ▶")).clicked() {
            app.markets.offset = offset + page;
            app.send_command(Command::SetMarketsOffset(app.markets.offset));
        }
    });
    widgets::section_error(ui, &snap.errors.markets);
    if snap.markets.is_empty() {
        ui.label(RichText::new("no markets loaded yet").weak());
        return;
    }

    let selected_id = app.markets.selected.as_ref().map(|s| s.market_id.clone());
    let mut clicked: Option<&PolyMarket> = None;
    TableBuilder::new(ui)
        .id_salt("markets")
        .striped(true)
        .sense(egui::Sense::click())
        .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
        .column(Column::remainder().at_least(240.0))
        .column(Column::auto().at_least(60.0))
        .column(Column::auto().at_least(60.0))
        .column(Column::auto().at_least(50.0))
        .column(Column::auto().at_least(50.0))
        .column(Column::auto().at_least(70.0))
        .column(Column::auto().at_least(60.0))
        .header(20.0, |mut header| {
            for title in ["Question", "Yes", "No", "Bid", "Ask", "Volume", "Status"] {
                header.col(|ui| {
                    ui.strong(title);
                });
            }
        })
        .body(|mut body| {
            for market in &snap.markets {
                body.row(22.0, |mut row| {
                    row.set_selected(selected_id.as_deref() == Some(market.id.as_str()));
                    row.col(|ui| {
                        ui.label(truncate(&market.question, 64))
                            .on_hover_text(&market.question);
                    });
                    // outcomePrices parallels outcomes: [YES, NO].
                    row.col(|ui| {
                        ui.monospace(outcome_price(market, 0));
                    });
                    row.col(|ui| {
                        ui.monospace(outcome_price(market, 1));
                    });
                    row.col(|ui| {
                        ui.monospace(widgets::fmt_opt_price(market.best_bid));
                    });
                    row.col(|ui| {
                        ui.monospace(widgets::fmt_opt_price(market.best_ask));
                    });
                    row.col(|ui| {
                        ui.monospace(widgets::fmt_volume(market.volume));
                    });
                    row.col(|ui| {
                        status_badge(ui, market.active, market.closed);
                    });
                    if row.response().clicked() {
                        clicked = Some(market);
                    }
                });
            }
        });
    if let Some(market) = clicked {
        select_market(app, market);
    }
}

/// Price string for outcome `idx` ("—" when the sim omits it).
fn outcome_price(market: &PolyMarket, idx: usize) -> String {
    market
        .outcome_prices
        .get(idx)
        .cloned()
        .unwrap_or_else(|| "—".to_string())
}

fn status_badge(ui: &mut Ui, active: bool, closed: bool) {
    let (text, color) = if closed {
        ("closed", RED)
    } else if active {
        ("active", GREEN)
    } else {
        ("inactive", ui.visuals().weak_text_color())
    };
    ui.label(RichText::new(text).color(color).small());
}

fn select_market(app: &mut App, market: &PolyMarket) {
    let token_ids = market.clob_token_ids.clone().unwrap_or_default();
    let mut tokens: Vec<(String, String)> = market
        .outcomes
        .iter()
        .cloned()
        .zip(token_ids.iter().cloned())
        .collect();
    if tokens.is_empty() {
        // Tokens without named outcomes: fall back to numbering.
        tokens = token_ids
            .iter()
            .enumerate()
            .map(|(i, t)| (format!("Outcome {}", i + 1), t.clone()))
            .collect();
    }
    let selection = Selection {
        market_id: market.id.clone(),
        question: market.question.clone(),
        tokens,
        outcome_idx: 0,
    };
    if let Some(token_id) = selection.token_id() {
        app.send_command(Command::SelectMarket {
            token_id: token_id.to_string(),
        });
    }
    app.markets.selected = Some(selection);
}

fn detail_pane(app: &mut App, ui: &mut Ui, snap: &Snapshot) {
    let Some(sel) = app.markets.selected.clone() else {
        return;
    };
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        ui.strong(truncate(&sel.question, 80))
            .on_hover_text(&sel.question);
        if sel.tokens.len() > 1 {
            let mut idx = sel.outcome_idx;
            egui::ComboBox::from_id_salt("outcome")
                .selected_text(sel.outcome_name())
                .show_ui(ui, |ui| {
                    for (i, (name, _)) in sel.tokens.iter().enumerate() {
                        ui.selectable_value(&mut idx, i, name);
                    }
                });
            if idx != sel.outcome_idx {
                if let Some(s) = app.markets.selected.as_mut() {
                    s.outcome_idx = idx;
                }
                if let Some((_, token_id)) = sel.tokens.get(idx) {
                    app.send_command(Command::SelectMarket {
                        token_id: token_id.clone(),
                    });
                }
            }
        }
        if let Some(token_id) = sel.token_id() {
            let label = format!(
                "{} — {}",
                truncate(&sel.question, 60),
                sel.outcome_name()
            );
            if ui.button("Trade this market").clicked() {
                let price = snap
                    .selected_book
                    .as_ref()
                    .and_then(|b| b.asks.first())
                    .map(|ask| ask.price)
                    .unwrap_or(app.trade.price);
                let size = app.trade.size;
                app.trade
                    .prefill(token_id, label.clone(), OrderSide::Buy, price, size);
                app.view = View::Trade;
            }
            // Held here? Offer to unwind the position from the same pane.
            if let Some(pos) = snap
                .positions
                .iter()
                .find(|p| p.clob_token_id == token_id && p.size != 0.0)
            {
                let (verb, side) = if pos.size > 0.0 {
                    ("Sell", OrderSide::Sell)
                } else {
                    ("Buy back", OrderSide::Buy)
                };
                if ui
                    .button(format!("{verb} {:.0}", pos.size.abs()))
                    .on_hover_text("Pre-fill the trade ticket to unwind your position")
                    .clicked()
                {
                    app.trade
                        .prefill(token_id, label, side, pos.current_price, pos.size.abs());
                    app.view = View::Trade;
                }
            }
        }
        if ui.small_button("✕").on_hover_text("Close detail").clicked() {
            app.markets.selected = None;
        }
    });
    if sel.tokens.is_empty() {
        ui.label(
            RichText::new("this market exposes no clob token ids — no order book available")
                .color(RED)
                .small(),
        );
        return;
    }
    ui.horizontal(|ui| {
        if let Some(token_id) = sel.token_id() {
            ui.label(
                RichText::new(format!("token {token_id}"))
                    .monospace()
                    .small()
                    .weak(),
            );
        }
        if let Some(quotes) = &snap.selected_quotes {
            ui.separator();
            ui.label(RichText::new(format!("mid {}", quotes.midpoint)).monospace().small());
            ui.label(
                RichText::new(format!("spread {}", quotes.spread))
                    .monospace()
                    .small()
                    .weak(),
            );
        }
    });
    widgets::section_error(ui, &snap.errors.book);
    match &snap.selected_book {
        Some(book) => widgets::orderbook_ladder(ui, book, LADDER_ROWS),
        None => {
            ui.label(RichText::new("order book loading…").weak());
        }
    }
}
