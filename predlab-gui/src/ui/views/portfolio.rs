//! Portfolio view: `/portfolio` stat cards plus positions and open orders
//! (with cancel).

use egui::{RichText, Ui};
use egui_extras::{Column, TableBuilder};
use predlab_util::{fmt_money, truncate};

use crate::data::Snapshot;
use crate::message::Command;
use crate::ui::widgets::{self, AMBER};
use crate::ui::App;

pub(crate) fn show(app: &mut App, ui: &mut Ui, snap: &Snapshot) {
    ui.heading("Portfolio");
    ui.add_space(4.0);
    egui::ScrollArea::vertical()
        .auto_shrink([false; 2])
        .show(ui, |ui| {
            if !app.config.has_poly_creds() {
                ui.label(
                    RichText::new(
                        "no API key configured — ask your club admin for one, then add it in Settings",
                    )
                    .color(AMBER),
                );
                ui.add_space(6.0);
            }
            widgets::section_error(ui, &snap.errors.portfolio);
            stat_row(ui, snap);
            ui.add_space(10.0);

            ui.strong("Positions");
            positions_table(ui, snap);
            ui.add_space(10.0);

            ui.strong("Open orders");
            orders_table(app, ui, snap);
        });
}

fn stat_row(ui: &mut Ui, snap: &Snapshot) {
    let fmt = |v: Option<f64>| v.map(fmt_money).unwrap_or_else(|| "—".to_string());
    let p = snap.portfolio.as_ref();
    ui.horizontal(|ui| {
        widgets::stat_card(
            ui,
            "Cash",
            RichText::new(fmt(p.map(|p| p.cash))),
            Some("Free paper cash, not counting escrow."),
        );
        widgets::stat_card(
            ui,
            "Positions value",
            RichText::new(fmt(p.map(|p| p.positions_value))),
            Some("Your shares marked at the current market price."),
        );
        widgets::stat_card(
            ui,
            "Open-order escrow",
            RichText::new(fmt(p.map(|p| p.open_orders_value))),
            Some("Cash reserved in resting buy orders; released on cancel."),
        );
        widgets::stat_card(
            ui,
            "Net worth ⓘ",
            RichText::new(fmt(p.map(|p| p.net_worth))).color(widgets::ACCENT),
            Some(
                "Cash + positions value + open-order escrow. This is your \
                 leaderboard score.",
            ),
        );
    });
}

fn positions_table(ui: &mut Ui, snap: &Snapshot) {
    if snap.positions.is_empty() {
        ui.label(RichText::new("no positions").weak());
        return;
    }
    TableBuilder::new(ui)
        .id_salt("positions")
        .striped(true)
        .vscroll(false)
        .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
        .column(Column::remainder().at_least(240.0))
        .column(Column::auto().at_least(60.0))
        .column(Column::auto().at_least(70.0))
        .column(Column::auto().at_least(70.0))
        .column(Column::auto().at_least(90.0))
        .header(20.0, |mut header| {
            for title in ["Question", "Size", "Avg entry", "Current", "Unrealized PnL"] {
                header.col(|ui| {
                    ui.strong(title);
                });
            }
        })
        .body(|mut body| {
            for pos in &snap.positions {
                body.row(22.0, |mut row| {
                    row.col(|ui| {
                        let question = pos
                            .market_question
                            .clone()
                            .unwrap_or_else(|| pos.clob_token_id.clone());
                        ui.label(truncate(&question, 64)).on_hover_text(question);
                    });
                    row.col(|ui| {
                        ui.monospace(format!("{:.0}", pos.size));
                    });
                    row.col(|ui| {
                        ui.monospace(widgets::fmt_opt_price(pos.avg_entry_price));
                    });
                    row.col(|ui| {
                        ui.monospace(format!("{:.2}", pos.current_price));
                    });
                    row.col(|ui| {
                        ui.label(
                            RichText::new(widgets::fmt_signed(pos.unrealized_pnl))
                                .monospace()
                                .color(widgets::signed_color(pos.unrealized_pnl)),
                        );
                    });
                });
            }
        });
}

fn orders_table(app: &mut App, ui: &mut Ui, snap: &Snapshot) {
    if snap.orders.is_empty() {
        ui.label(RichText::new("no open orders").weak());
        return;
    }
    let mut cancel: Option<String> = None;
    TableBuilder::new(ui)
        .id_salt("portfolio-orders")
        .striped(true)
        .vscroll(false)
        .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
        .column(Column::auto().at_least(50.0))
        .column(Column::remainder().at_least(200.0))
        .column(Column::auto().at_least(50.0))
        .column(Column::auto().at_least(60.0))
        .column(Column::auto().at_least(60.0))
        .column(Column::auto().at_least(80.0))
        .column(Column::auto().at_least(70.0))
        .header(20.0, |mut header| {
            for title in ["Id", "Market", "Side", "Price", "Size", "Status", ""] {
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
                        ui.label(truncate(&order.market_id, 40));
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
                        ui.label(RichText::new(&order.status).small());
                    });
                    row.col(|ui| {
                        if ui.small_button("Cancel").clicked() {
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
