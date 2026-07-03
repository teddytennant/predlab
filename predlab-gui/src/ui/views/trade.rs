//! Trade view: one order ticket plus the open-order table with cancel
//! actions.

use egui::{RichText, Ui};
use egui_extras::{Column, TableBuilder};

use crate::data::Snapshot;
use crate::message::{Command, OrderSide};
use crate::ui::widgets::{self, AMBER, GREEN, RED};
use crate::ui::App;

/// The ticket's editable state; pre-filled by the Markets detail pane.
pub(crate) struct TradeState {
    pub(crate) token_id: String,
    /// "question — outcome" carried over from Markets, display only.
    pub(crate) label: String,
    pub(crate) side: OrderSide,
    pub(crate) market_order: bool,
    pub(crate) price: f64,
    pub(crate) size: f64,
    error: Option<String>,
}

impl Default for TradeState {
    fn default() -> Self {
        Self {
            token_id: String::new(),
            label: String::new(),
            side: OrderSide::Buy,
            market_order: false,
            price: 0.50,
            size: 10.0,
            error: None,
        }
    }
}

impl TradeState {
    /// Round a price onto the ticket's 1¢ tick and clamp it into the valid
    /// limit range (0.01..=0.99).
    pub(crate) fn clamp_to_tick(price: f64) -> f64 {
        ((price * 100.0).round() / 100.0).clamp(0.01, 0.99)
    }

    /// The single entry point every "open the ticket pre-filled" flow uses
    /// (Markets "Trade this market", Portfolio "Sell" / "Buy back"). Only
    /// stages the ticket — nothing is submitted until the user clicks.
    pub(crate) fn prefill(
        &mut self,
        token_id: &str,
        label: String,
        side: OrderSide,
        price: f64,
        size: f64,
    ) {
        self.token_id = token_id.to_string();
        self.label = label;
        self.side = side;
        self.market_order = false;
        self.price = Self::clamp_to_tick(price);
        self.size = size;
        self.error = None;
    }
}

/// Estimate line for a limit order: buys cost money, sells raise proceeds.
fn estimate_line(side: OrderSide, price: f64, size: f64) -> String {
    let amount = predlab_util::fmt_money(price * size);
    match side {
        OrderSide::Buy => format!("≈ {amount} cost"),
        OrderSide::Sell => format!("≈ {amount} proceeds"),
    }
}

/// True when an order in this status can still be cancelled.
fn cancellable(status: &str) -> bool {
    let s = status.to_ascii_lowercase();
    s.contains("open") || s.contains("partial") || s.contains("resting") || s.contains("pending")
}

pub(crate) fn show(app: &mut App, ui: &mut Ui, snap: &Snapshot) {
    ui.heading("Trade");
    ui.add_space(4.0);
    egui::ScrollArea::vertical()
        .auto_shrink([false; 2])
        .show(ui, |ui| {
            ticket(app, ui);
            ui.add_space(8.0);
            ui.strong("Open orders");
            widgets::section_error(ui, &snap.errors.portfolio);
            orders_table(app, ui, snap);
        });
}

fn ticket(app: &mut App, ui: &mut Ui) {
    egui::Frame::group(ui.style()).show(ui, |ui| {
        ui.strong("Order ticket");
        if !app.trade.label.is_empty() {
            ui.label(RichText::new(&app.trade.label).weak());
        }
        egui::Grid::new("ticket")
            .num_columns(2)
            .spacing([12.0, 8.0])
            .show(ui, |ui| {
                ui.label("Token id");
                ui.add(
                    egui::TextEdit::singleline(&mut app.trade.token_id)
                        .font(egui::TextStyle::Monospace)
                        .hint_text("pick a market in Markets, or paste a token id")
                        .desired_width(380.0),
                );
                ui.end_row();

                ui.label("Side");
                ui.horizontal(|ui| {
                    ui.selectable_value(&mut app.trade.side, OrderSide::Buy, "Buy");
                    ui.selectable_value(&mut app.trade.side, OrderSide::Sell, "Sell");
                });
                ui.end_row();

                ui.label("Limit price");
                ui.horizontal(|ui| {
                    ui.add_enabled(
                        !app.trade.market_order,
                        egui::DragValue::new(&mut app.trade.price)
                            .range(0.01..=0.99)
                            .speed(0.01)
                            .fixed_decimals(2),
                    );
                    ui.checkbox(&mut app.trade.market_order, "market order");
                });
                ui.end_row();

                ui.label("Size");
                ui.add(
                    egui::DragValue::new(&mut app.trade.size)
                        .range(0.0..=1_000_000.0)
                        .speed(1.0)
                        .fixed_decimals(0),
                );
                ui.end_row();

                ui.label("Estimate");
                if app.trade.market_order {
                    ui.label(RichText::new("market price × size").weak());
                } else {
                    ui.monospace(estimate_line(
                        app.trade.side,
                        app.trade.price,
                        app.trade.size,
                    ));
                }
                ui.end_row();
            });
        ui.label(
            RichText::new(
                "prices are 0.01–0.99 per share; each winning share pays $1.00 at resolution",
            )
            .small()
            .weak(),
        );
        if ui.button("Submit order").clicked() {
            submit(app);
        }
        if let Some(error) = &app.trade.error {
            ui.label(RichText::new(error).color(RED).small());
        }
        if !app.config.has_poly_creds() {
            ui.label(
                RichText::new("no API key configured — orders will be rejected; add one in Settings")
                    .color(AMBER)
                    .small(),
            );
        }
        if let Some((ok, detail)) = &app.last_order {
            let color = if *ok { GREEN } else { RED };
            ui.label(RichText::new(format!("last result: {detail}")).color(color).small());
        }
    });
}

fn submit(app: &mut App) {
    let token_id = app.trade.token_id.trim().to_string();
    if token_id.is_empty() {
        app.trade.error =
            Some("enter a token id — pick a market in the Markets tab".to_string());
        return;
    }
    if app.trade.size <= 0.0 {
        app.trade.error = Some("size must be greater than zero".to_string());
        return;
    }
    app.trade.error = None;
    let price = if app.trade.market_order {
        None
    } else {
        Some(app.trade.price)
    };
    app.send_command(Command::PlaceOrder {
        token_id,
        side: app.trade.side,
        price,
        size: app.trade.size,
    });
}

fn orders_table(app: &mut App, ui: &mut Ui, snap: &Snapshot) {
    if snap.orders.is_empty() {
        ui.label(RichText::new("no open orders").weak());
        return;
    }
    let mut cancel: Option<String> = None;
    TableBuilder::new(ui)
        .id_salt("orders")
        .striped(true)
        .vscroll(false)
        .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
        .column(Column::auto().at_least(50.0))
        .column(Column::auto().at_least(50.0))
        .column(Column::auto().at_least(60.0))
        .column(Column::auto().at_least(60.0))
        .column(Column::auto().at_least(60.0))
        .column(Column::auto().at_least(80.0))
        .column(Column::remainder().at_least(70.0))
        .header(20.0, |mut header| {
            for title in ["Id", "Side", "Price", "Size", "Filled", "Status", ""] {
                header.col(|ui| {
                    ui.strong(title);
                });
            }
        })
        .body(|mut body| {
            for order in &snap.orders {
                body.row(22.0, |mut row| {
                    row.col(|ui| {
                        ui.monospace(order.id.to_string());
                    });
                    row.col(|ui| {
                        ui.label(&order.side);
                    });
                    row.col(|ui| {
                        ui.monospace(widgets::fmt_opt_price(order.price));
                    });
                    row.col(|ui| {
                        ui.monospace(format!("{:.0}", order.size));
                    });
                    row.col(|ui| {
                        ui.monospace(format!("{:.0}", order.filled_size));
                    });
                    row.col(|ui| {
                        ui.label(RichText::new(&order.status).small());
                    });
                    row.col(|ui| {
                        if cancellable(&order.status) && ui.small_button("Cancel").clicked() {
                            cancel = Some(order.id.to_string());
                        }
                    });
                });
            }
        });
    if let Some(order_id) = cancel {
        app.send_command(Command::CancelOrder { order_id });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn open_like_statuses_are_cancellable() {
        for s in ["open", "OPEN", "partially_filled", "partial", "resting", "pending"] {
            assert!(cancellable(s), "{s} should be cancellable");
        }
    }

    #[test]
    fn terminal_statuses_are_not_cancellable() {
        for s in ["filled", "cancelled", "canceled", "expired", "rejected", ""] {
            assert!(!cancellable(s), "{s} should not be cancellable");
        }
    }

    #[test]
    fn clamp_to_tick_rounds_and_clamps() {
        assert_eq!(TradeState::clamp_to_tick(0.5),   0.50);
        assert_eq!(TradeState::clamp_to_tick(0.123), 0.12, "rounds to the 1¢ tick");
        assert_eq!(TradeState::clamp_to_tick(0.567), 0.57);
        assert_eq!(TradeState::clamp_to_tick(0.005), 0.01, "floor of the range");
        assert_eq!(TradeState::clamp_to_tick(0.0),   0.01);
        assert_eq!(TradeState::clamp_to_tick(-3.0),  0.01);
        assert_eq!(TradeState::clamp_to_tick(0.999), 0.99, "ceiling of the range");
        assert_eq!(TradeState::clamp_to_tick(42.0),  0.99);
    }

    #[test]
    fn estimate_reads_as_cost_for_buys_and_proceeds_for_sells() {
        assert_eq!(estimate_line(OrderSide::Buy, 0.55, 10.0), "≈ $5.50 cost");
        assert_eq!(estimate_line(OrderSide::Sell, 0.55, 10.0), "≈ $5.50 proceeds");
        assert_eq!(estimate_line(OrderSide::Sell, 0.30, 3.0), "≈ $0.90 proceeds");
    }

    #[test]
    fn prefill_stages_a_reviewable_sell_ticket() {
        let mut state = TradeState {
            market_order: true,
            error: Some("stale".to_string()),
            ..TradeState::default()
        };
        state.prefill("tok-yes", "Will it rain? — Yes".to_string(), OrderSide::Sell, 0.6234, 5.0);
        assert_eq!(state.token_id, "tok-yes");
        assert_eq!(state.label, "Will it rain? — Yes");
        assert_eq!(state.side, OrderSide::Sell, "side toggle reflects the prefilled SELL");
        assert_eq!(state.price, 0.62, "price snapped to the ticket tick");
        assert_eq!(state.size, 5.0);
        assert!(!state.market_order, "prefill always stages a limit order");
        assert!(state.error.is_none(), "stale validation errors cleared");
    }
}
