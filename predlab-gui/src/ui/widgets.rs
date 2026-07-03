//! Shared widgets (order-book ladder, connection dot, copyable secrets,
//! toasts, history chart) and pure formatting helpers used across the views.

use std::time::{Duration, Instant};

use chrono::{DateTime, Utc};
use egui::{Align2, Color32, FontId, RichText, Sense, Ui, vec2};

use crate::data::{BookLevel, OrderBook};
use crate::domain::polymarket::HistoryPoint;
use crate::message::ConnState;

/// Restrained accent used for the app name, selection and highlights.
pub const ACCENT: Color32 = Color32::from_rgb(0x6c, 0x9e, 0xff);
/// Positive numbers / bid side / success.
pub const GREEN: Color32 = Color32::from_rgb(0x4c, 0xb8, 0x7a);
/// Negative numbers / ask side / failure.
pub const RED: Color32 = Color32::from_rgb(0xe0, 0x5c, 0x6a);
/// Warnings that should stand out without reading as an error.
pub const AMBER: Color32 = Color32::from_rgb(0xe8, 0xb4, 0x4f);

// ---------------------------------------------------------------------------
// Toasts

/// How long a toast stays on screen.
const TOAST_TTL: Duration = Duration::from_secs(5);
/// At most this many toasts are stacked at once.
const TOAST_MAX: usize = 6;

/// One transient notification, born from an
/// [`crate::message::UiMessage::ActionResult`] or a local UI event.
pub struct Toast {
    ok: bool,
    text: String,
    born: Instant,
}

impl Toast {
    pub fn new(ok: bool, text: String) -> Self {
        Self {
            ok,
            text,
            born: Instant::now(),
        }
    }
}

/// Draw the toast stack in the bottom-right corner and drop expired entries.
pub fn show_toasts(ctx: &egui::Context, toasts: &mut Vec<Toast>) {
    toasts.retain(|t| t.born.elapsed() < TOAST_TTL);
    if toasts.is_empty() {
        return;
    }
    egui::Area::new(egui::Id::new("predlab-toasts"))
        .anchor(Align2::RIGHT_BOTTOM, vec2(-12.0, -12.0))
        .order(egui::Order::Foreground)
        .show(ctx, |ui| {
            let skip = toasts.len().saturating_sub(TOAST_MAX);
            for toast in toasts.iter().skip(skip) {
                let color = if toast.ok { GREEN } else { RED };
                egui::Frame::popup(ui.style())
                    .stroke(egui::Stroke::new(1.0, color))
                    .show(ui, |ui| {
                        ui.set_max_width(360.0);
                        ui.label(RichText::new(&toast.text).color(color));
                    });
            }
        });
    // Keep repainting so toasts expire even while the mouse is idle.
    ctx.request_repaint_after(Duration::from_millis(250));
}

// ---------------------------------------------------------------------------
// Small shared pieces

/// A "name ●" connection row; the dot color reflects [`ConnState`] and the
/// details (version / error) live in the hover tooltip.
pub fn conn_row(ui: &mut Ui, name: &str, state: &ConnState) {
    let (color, tip) = match state {
        ConnState::Unknown => (Color32::GRAY, "not checked yet".to_string()),
        ConnState::Connected(version) => (GREEN, format!("connected (v{version})")),
        ConnState::Error(e) => (RED, e.clone()),
    };
    ui.horizontal(|ui| {
        ui.label(RichText::new("●").color(color));
        ui.label(name);
    })
    .response
    .on_hover_text(tip);
}

/// A small button that copies `value` to the clipboard.
pub fn copy_button(ui: &mut Ui, value: &str) {
    if ui
        .small_button("copy")
        .on_hover_text("Copy to clipboard")
        .clicked()
    {
        ui.ctx().copy_text(value.to_string());
    }
}

/// Monospace value with a copy button next to it.
pub fn mono_copy(ui: &mut Ui, value: &str) {
    ui.horizontal(|ui| {
        ui.monospace(value);
        copy_button(ui, value);
    });
}

/// Password-style text field with a show/hide toggle.
pub fn secret_field(ui: &mut Ui, value: &mut String, reveal: &mut bool) {
    ui.horizontal(|ui| {
        ui.add(
            egui::TextEdit::singleline(value)
                .password(!*reveal)
                .desired_width(280.0),
        );
        let label = if *reveal { "hide" } else { "show" };
        if ui.small_button(label).clicked() {
            *reveal = !*reveal;
        }
    });
}

/// Inline per-section error from [`crate::data::SectionErrors`].
pub fn section_error(ui: &mut Ui, error: &Option<String>) {
    if let Some(e) = error {
        ui.label(RichText::new(e).color(RED).small());
    }
}

/// One stat card for the portfolio header row.
pub fn stat_card(ui: &mut Ui, title: &str, value: RichText, hover: Option<&str>) {
    let response = egui::Frame::group(ui.style())
        .inner_margin(egui::Margin::same(12))
        .show(ui, |ui| {
            ui.set_min_width(180.0);
            ui.label(RichText::new(title).small().weak());
            ui.label(value.size(20.0).monospace());
        })
        .response;
    if let Some(text) = hover {
        response.on_hover_text(text);
    }
}

// ---------------------------------------------------------------------------
// Order-book ladder

/// Bids (green) and asks (red) side by side, sizes drawn as horizontal
/// depth bars behind the numbers.
pub fn orderbook_ladder(ui: &mut Ui, book: &OrderBook, rows: usize) {
    let max_size = book
        .bids
        .iter()
        .take(rows)
        .chain(book.asks.iter().take(rows))
        .map(|l| l.size)
        .fold(0.0_f64, f64::max);
    ui.columns(2, |cols| {
        ladder_side(&mut cols[0], "Bids", &book.bids, rows, max_size, GREEN);
        ladder_side(&mut cols[1], "Asks", &book.asks, rows, max_size, RED);
    });
}

fn ladder_side(
    ui: &mut Ui,
    title: &str,
    levels: &[BookLevel],
    rows: usize,
    max_size: f64,
    color: Color32,
) {
    ui.label(RichText::new(title).small().weak());
    if levels.is_empty() {
        ui.label(RichText::new("no liquidity").small().weak());
        return;
    }
    let width = ui.available_width();
    let text_color = ui.visuals().text_color();
    for level in levels.iter().take(rows) {
        let (rect, _) = ui.allocate_exact_size(vec2(width, 18.0), Sense::hover());
        if !ui.is_rect_visible(rect) {
            continue;
        }
        let frac = if max_size > 0.0 {
            (level.size / max_size).clamp(0.0, 1.0) as f32
        } else {
            0.0
        };
        let painter = ui.painter();
        let bar = egui::Rect::from_min_max(
            egui::pos2(rect.right() - rect.width() * frac, rect.top() + 1.0),
            egui::pos2(rect.right(), rect.bottom() - 1.0),
        );
        painter.rect_filled(bar, egui::CornerRadius::same(2), color.gamma_multiply(0.2));
        painter.text(
            egui::pos2(rect.left() + 4.0, rect.center().y),
            Align2::LEFT_CENTER,
            &level.price_raw,
            FontId::monospace(13.0),
            color,
        );
        painter.text(
            egui::pos2(rect.right() - 4.0, rect.center().y),
            Align2::RIGHT_CENTER,
            &level.size_raw,
            FontId::monospace(12.0),
            text_color,
        );
    }
}

// ---------------------------------------------------------------------------
// Net-worth history line chart (hand-painted; no plotting dependency)

/// Simple polyline of a member's net-worth history with min/max labels.
pub fn history_chart(ui: &mut Ui, history: &[HistoryPoint], height: f32) {
    if history.len() < 2 {
        ui.label(RichText::new("not enough history yet — check back later").small().weak());
        return;
    }
    let (min, max) = history.iter().fold((f64::MAX, f64::MIN), |(lo, hi), p| {
        (lo.min(p.net_worth), hi.max(p.net_worth))
    });
    // Flat lines still deserve a visible middle-of-the-chart stroke.
    let span = if (max - min).abs() < f64::EPSILON { 1.0 } else { max - min };
    let pad = span * 0.08;
    let (lo, hi) = (min - pad, max + pad);

    let width = ui.available_width();
    let (rect, response) = ui.allocate_exact_size(vec2(width, height), Sense::hover());
    if !ui.is_rect_visible(rect) {
        return;
    }
    let painter = ui.painter();
    painter.rect_filled(
        rect,
        egui::CornerRadius::same(4),
        ui.visuals().extreme_bg_color,
    );

    let n = history.len();
    let points: Vec<egui::Pos2> = history
        .iter()
        .enumerate()
        .map(|(i, p)| {
            let x = rect.left() + rect.width() * (i as f32 / (n - 1) as f32);
            let frac = ((p.net_worth - lo) / (hi - lo)) as f32;
            let y = rect.bottom() - rect.height() * frac;
            egui::pos2(x, y)
        })
        .collect();
    painter.add(egui::Shape::line(points.clone(), egui::Stroke::new(1.6, ACCENT)));

    // Min / max / latest annotations.
    let text_color = ui.visuals().weak_text_color();
    painter.text(
        rect.left_top() + vec2(6.0, 4.0),
        Align2::LEFT_TOP,
        predlab_util::fmt_money(max),
        FontId::monospace(11.0),
        text_color,
    );
    painter.text(
        rect.left_bottom() + vec2(6.0, -4.0),
        Align2::LEFT_BOTTOM,
        predlab_util::fmt_money(min),
        FontId::monospace(11.0),
        text_color,
    );

    // Hover: nearest point's timestamp + value.
    if let Some(pointer) = response.hover_pos() {
        let idx = (((pointer.x - rect.left()) / rect.width()) * (n - 1) as f32)
            .round()
            .clamp(0.0, (n - 1) as f32) as usize;
        let p = &history[idx];
        painter.circle_filled(points[idx], 3.0, ACCENT);
        response.on_hover_text(format!(
            "{} — {}",
            p.t,
            predlab_util::fmt_money(p.net_worth)
        ));
    }
}

// ---------------------------------------------------------------------------
// Member profile panel (shared by Leaderboard row-click and Admin "view")

/// Draw the profile for `username` from the snapshot (or a loading state
/// while the engine fetch is in flight). Returns `true` when the close
/// button was clicked.
pub fn profile_panel(ui: &mut Ui, username: &str, snap: &crate::data::Snapshot) -> bool {
    let mut close = false;
    egui::Frame::group(ui.style())
        .inner_margin(egui::Margin::same(12))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.strong(RichText::new(username).size(16.0));
                if let Some(loaded) = snap.profile.as_ref().filter(|p| p.username == username)
                    && !loaded.profile.role.is_empty()
                {
                    ui.label(RichText::new(&loaded.profile.role).small().weak());
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.small_button("✕").on_hover_text("Close profile").clicked() {
                        close = true;
                    }
                });
            });
            section_error(ui, &snap.errors.profile);
            let Some(loaded) = snap.profile.as_ref().filter(|p| p.username == username) else {
                ui.label(RichText::new("loading profile…").weak());
                return;
            };
            let p = &loaded.profile;
            ui.horizontal(|ui| {
                stat_card(
                    ui,
                    "Net worth",
                    RichText::new(predlab_util::fmt_money(p.net_worth)).color(ACCENT),
                    Some("The leaderboard score."),
                );
                stat_card(ui, "Cash", RichText::new(predlab_util::fmt_money(p.cash)), None);
                stat_card(
                    ui,
                    "Positions value",
                    RichText::new(predlab_util::fmt_money(p.positions_value)),
                    None,
                );
            });
            ui.add_space(6.0);
            ui.label(RichText::new("Net worth over time").small().weak());
            history_chart(ui, &p.history, 120.0);
            if !p.positions.is_empty() {
                ui.add_space(6.0);
                ui.label(RichText::new(format!("Positions ({})", p.positions.len())).small().weak());
                for pos in p.positions.iter().take(8) {
                    let question = pos
                        .market_question
                        .clone()
                        .unwrap_or_else(|| pos.market_id.clone());
                    ui.horizontal(|ui| {
                        ui.label(predlab_util::truncate(&question, 56))
                            .on_hover_text(question);
                        ui.monospace(format!("{:.0} @ {:.2}", pos.size, pos.current_price));
                        ui.label(
                            RichText::new(fmt_signed(pos.unrealized_pnl))
                                .monospace()
                                .color(signed_color(pos.unrealized_pnl)),
                        );
                    });
                }
            }
        });
    close
}

// ---------------------------------------------------------------------------
// Pure formatting helpers

/// `Some(0.63) -> "0.63"`, `None -> "—"`.
pub fn fmt_opt_price(price: Option<f64>) -> String {
    match price {
        Some(v) => format!("{v:.2}"),
        None => "—".to_string(),
    }
}

/// Explicit sign, 2 decimals: `3.5 -> "+3.50"`, `-0.1 -> "-0.10"`.
pub fn fmt_signed(v: f64) -> String {
    format!("{v:+.2}")
}

/// `Some(12345.6) -> "12346"`, `None -> "—"`.
pub fn fmt_volume(v: Option<f64>) -> String {
    match v {
        Some(v) => format!("{v:.0}"),
        None => "—".to_string(),
    }
}

/// Green when positive, red when negative, gray at exactly zero.
pub fn signed_color(v: f64) -> Color32 {
    if v > 0.0 {
        GREEN
    } else if v < 0.0 {
        RED
    } else {
        Color32::GRAY
    }
}

/// Sidebar footer: when the snapshot last refreshed, in local time.
pub fn fmt_last_updated(t: Option<DateTime<Utc>>) -> String {
    match t {
        Some(t) => format!(
            "updated {}",
            t.with_timezone(&chrono::Local).format("%H:%M:%S")
        ),
        None => "no data yet".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opt_price_formats_or_dashes() {
        assert_eq!(fmt_opt_price(Some(0.631)), "0.63");
        assert_eq!(fmt_opt_price(None), "—");
    }

    #[test]
    fn signed_formatting_and_color() {
        assert_eq!(fmt_signed(3.5), "+3.50");
        assert_eq!(fmt_signed(-0.1), "-0.10");
        assert_eq!(signed_color(1.0), GREEN);
        assert_eq!(signed_color(-1.0), RED);
        assert_eq!(signed_color(0.0), Color32::GRAY);
    }

    #[test]
    fn volume_formats_or_dashes() {
        assert_eq!(fmt_volume(Some(12345.6)), "12346");
        assert_eq!(fmt_volume(None), "—");
    }

    #[test]
    fn last_updated_none_reads_as_no_data() {
        assert_eq!(fmt_last_updated(None), "no data yet");
    }
}
