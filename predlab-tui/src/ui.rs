//! UI rendering. One `draw` entry point owns the layout; each tab has its own
//! render function. Everything is computed from `App` — no I/O happens here.

use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    symbols,
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Cell, Paragraph, Row, Sparkline, Table, TableState, Wrap},
    Frame,
};

use crate::api::{LeaderRow, Market, Portfolio, Position};
use crate::app::{fmt_age, fmt_money, App, LoadState, Mode, Tab};
use crate::theme;
use predlab_util::truncate;

pub fn draw(f: &mut Frame, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            // Header (PREDLAB + connection)
            Constraint::Length(2),
            // Tabs strip
            Constraint::Length(2),
            // Body
            Constraint::Min(8),
            // Status line
            Constraint::Length(1),
        ])
        .split(f.area());

    render_header(f, app, chunks[0]);
    render_tabs(f, app, chunks[1]);
    match app.tab {
        Tab::Leaderboard => render_leaderboard(f, app, chunks[2]),
        Tab::Markets => render_markets(f, app, chunks[2]),
        Tab::Portfolio => render_portfolio(f, app, chunks[2]),
        Tab::Help => render_help(f, app, chunks[2]),
    }
    render_status_line(f, app, chunks[3]);
}

// ---------------------------------------------------------------------------
// Chrome
// ---------------------------------------------------------------------------

fn render_header(f: &mut Frame, app: &App, area: Rect) {
    let conn = if app.has_key {
        Span::styled("● connected", theme::positive())
    } else {
        Span::styled("○ public mode (no API key)", theme::dim())
    };
    let left = Line::from(vec![
        Span::styled("PREDLAB", theme::primary()),
        Span::styled(" v", theme::dim()),
        Span::styled(env!("CARGO_PKG_VERSION"), theme::dim()),
        Span::styled("  ·  ", theme::dim()),
        Span::styled("paper trading", theme::dim()),
    ]);
    let right = Line::from(vec![conn]).alignment(Alignment::Right);

    let split = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(60), Constraint::Percentage(40)])
        .split(area);
    f.render_widget(Paragraph::new(left), split[0]);
    f.render_widget(Paragraph::new(right), split[1]);
}

fn render_tabs(f: &mut Frame, app: &App, area: Rect) {
    let mut spans: Vec<Span> = Vec::new();
    for (i, t) in Tab::ALL.iter().enumerate() {
        let active = *t == app.tab;
        let num = format!("  {}  ", i + 1);
        let label = format!("{}  ", t.title());
        if active {
            spans.push(Span::styled(num, theme::mode_pill(theme::PRIMARY)));
            spans.push(Span::styled(
                label,
                Style::default()
                    .fg(theme::PRIMARY)
                    .add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
            ));
        } else {
            spans.push(Span::styled(num, theme::dim()));
            spans.push(Span::styled(label, theme::dim()));
        }
    }
    let line1 = Paragraph::new(Line::from(spans));
    // Render a faint horizontal rule beneath the tabs.
    let rule = Paragraph::new(Span::styled(
        "─".repeat(area.width as usize),
        theme::border(),
    ));
    let split = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Length(1)])
        .split(area);
    f.render_widget(line1, split[0]);
    f.render_widget(rule, split[1]);
}

fn render_status_line(f: &mut Frame, app: &App, area: Rect) {
    let (label, bg) = match app.mode {
        Mode::Normal => (" NORMAL ", theme::MODE_NORMAL),
        Mode::Command => (" COMMAND ", theme::MODE_COMMAND),
        Mode::Search => (" SEARCH ", theme::MODE_SEARCH),
    };

    let mut spans = vec![
        Span::styled(label, theme::mode_pill(bg)),
        Span::raw(" "),
    ];

    match app.mode {
        Mode::Normal => {
            spans.push(Span::styled(app.status.clone(), theme::fg()));
            if !app.last_search.is_empty() {
                spans.push(Span::styled(
                    format!("    /{}", app.last_search),
                    theme::dim(),
                ));
            }
        }
        Mode::Command => {
            spans.push(Span::styled(":", theme::primary()));
            spans.push(Span::styled(app.command.clone(), theme::fg()));
            spans.push(Span::styled("▏", theme::primary()));
        }
        Mode::Search => {
            spans.push(Span::styled("/", theme::primary()));
            spans.push(Span::styled(app.search.clone(), theme::fg()));
            spans.push(Span::styled("▏", theme::primary()));
        }
    }

    let line = Line::from(spans);
    let hint = Line::from(vec![
        Span::styled("? help  ", theme::dim()),
        Span::styled(": cmd  ", theme::dim()),
        Span::styled("/ find  ", theme::dim()),
        Span::styled("q quit", theme::dim()),
    ])
    .alignment(Alignment::Right);

    let split = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(65), Constraint::Percentage(35)])
        .split(area);
    f.render_widget(Paragraph::new(line), split[0]);
    f.render_widget(Paragraph::new(hint), split[1]);
}

// ---------------------------------------------------------------------------
// Tab: Leaderboard
// ---------------------------------------------------------------------------

fn render_leaderboard(f: &mut Frame, app: &App, area: Rect) {
    let block = panel_block(app, Tab::Leaderboard, &leaderboard_subtitle(app));
    let inner = block.inner(area);
    f.render_widget(block, area);

    match &app.leaderboard {
        LoadState::Idle | LoadState::Loading => {
            render_loading(f, "Loading club standings…", inner);
            return;
        }
        LoadState::Error(e) => {
            render_error(f, e, inner);
            return;
        }
        LoadState::Loaded { .. } => {}
    }

    let rows = match app.filtered_leaderboard() {
        Some(r) => r,
        None => return,
    };
    if rows.is_empty() {
        render_empty(f, "no members yet", inner);
        return;
    }

    let header = Row::new(["#", "MEMBER", "NET WORTH", ""]).style(theme::primary());
    let body: Vec<Row> = rows
        .iter()
        .enumerate()
        .map(|(i, row)| leaderboard_row(i, row))
        .collect();

    let widths = [
        Constraint::Length(5),
        Constraint::Min(20),
        Constraint::Length(16),
        Constraint::Length(4),
    ];
    let table = Table::new(body, widths)
        .header(header)
        .row_highlight_style(theme::row_highlight())
        .highlight_symbol(" ▶ ");

    let mut state = TableState::default();
    state.select(Some(app.leaderboard_sel.min(rows.len() - 1)));
    f.render_stateful_widget(table, inner, &mut state);
}

fn leaderboard_row(idx: usize, l: &LeaderRow) -> Row<'static> {
    let rank = idx + 1;
    let crown = if rank == 1 { " ★" } else { "" };
    let style = if rank == 1 {
        theme::accent()
    } else if rank <= 3 {
        Style::default().fg(theme::FG).add_modifier(Modifier::BOLD)
    } else {
        theme::fg()
    };
    Row::new(vec![
        Cell::from(format!("{rank}")),
        Cell::from(l.username.clone()),
        Cell::from(fmt_money(l.net_worth)),
        Cell::from(crown.to_string()),
    ])
    .style(style)
}

fn leaderboard_subtitle(app: &App) -> String {
    let count = app
        .leaderboard
        .data()
        .map(|v| v.len())
        .unwrap_or(0);
    let mut s = format!("{count} members");
    if let Some(age) = app.elapsed_since_load(Tab::Leaderboard) {
        s.push_str(&format!(" · {}", fmt_age(age)));
    }
    s
}

// ---------------------------------------------------------------------------
// Tab: Markets
// ---------------------------------------------------------------------------

fn render_markets(f: &mut Frame, app: &App, area: Rect) {
    let block = panel_block(app, Tab::Markets, &markets_subtitle(app));
    let inner = block.inner(area);
    f.render_widget(block, area);

    match &app.markets {
        LoadState::Idle | LoadState::Loading => {
            render_loading(f, "Loading markets…", inner);
            return;
        }
        LoadState::Error(e) => {
            render_error(f, e, inner);
            return;
        }
        LoadState::Loaded { .. } => {}
    }

    let rows = match app.filtered_markets() {
        Some(r) => r,
        None => return,
    };
    if rows.is_empty() {
        render_empty(f, "no markets match the filter", inner);
        return;
    }

    let header = Row::new(["QUESTION", "CATEGORY", "YES", "NO", "VOLUME"]).style(theme::primary());
    let body: Vec<Row> = rows.iter().map(market_row).collect();

    let widths = [
        Constraint::Percentage(50),
        Constraint::Length(14),
        Constraint::Length(8),
        Constraint::Length(8),
        Constraint::Length(12),
    ];
    let table = Table::new(body, widths)
        .header(header)
        .row_highlight_style(theme::row_highlight())
        .highlight_symbol(" ▶ ");

    let mut state = TableState::default();
    state.select(Some(app.markets_sel.min(rows.len() - 1)));
    f.render_stateful_widget(table, inner, &mut state);
}

fn market_row(m: &Market) -> Row<'static> {
    let yes = m.mid().map(price_cents).unwrap_or_else(|| "—".into());
    let no = m
        .mid()
        .map(|p| price_cents(1.0 - p))
        .unwrap_or_else(|| "—".into());
    let volume = m
        .volume
        .map(|v| {
            if v >= 1_000_000.0 {
                format!("${:.1}M", v / 1_000_000.0)
            } else if v >= 1_000.0 {
                format!("${:.1}K", v / 1_000.0)
            } else {
                fmt_money(v)
            }
        })
        .unwrap_or_else(|| "—".into());
    let category = m.category.clone().unwrap_or_else(|| "—".into());
    Row::new(vec![
        Cell::from(truncate(&m.question, 80)),
        Cell::from(truncate(&category, 13)),
        Cell::from(yes).style(theme::positive()),
        Cell::from(no).style(theme::negative()),
        Cell::from(volume).style(theme::dim()),
    ])
}

/// Price 0.0..=1.0 → "55¢" (probability shorthand).
fn price_cents(p: f64) -> String {
    let pct = (p.clamp(0.0, 1.0) * 100.0).round() as i64;
    format!("{pct}¢")
}

fn markets_subtitle(app: &App) -> String {
    let count = app
        .markets
        .data()
        .map(|v| v.len())
        .unwrap_or(0);
    let mut s = format!("{count} markets");
    if let Some(age) = app.elapsed_since_load(Tab::Markets) {
        s.push_str(&format!(" · {}", fmt_age(age)));
    }
    s
}

// ---------------------------------------------------------------------------
// Tab: Portfolio
// ---------------------------------------------------------------------------

fn render_portfolio(f: &mut Frame, app: &App, area: Rect) {
    let block = panel_block(app, Tab::Portfolio, &portfolio_subtitle(app));
    let inner = block.inner(area);
    f.render_widget(block, area);

    if !app.has_key {
        render_no_key(f, inner);
        return;
    }
    if let LoadState::Error(e) = &app.portfolio {
        render_error(f, e, inner);
        return;
    }
    if app.portfolio.is_loading() {
        render_loading(f, "Loading your portfolio…", inner);
        return;
    }

    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(5),   // KPI strip
            Constraint::Length(8),   // sparkline
            Constraint::Min(4),      // positions
        ])
        .split(inner);

    if let Some(p) = app.portfolio.data() {
        render_kpis(f, p, layout[0]);
    }
    render_sparkline(f, app, layout[1]);
    render_positions(f, app, layout[2]);
}

fn render_kpis(f: &mut Frame, p: &Portfolio, area: Rect) {
    let cells = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(25),
            Constraint::Percentage(25),
            Constraint::Percentage(25),
            Constraint::Percentage(25),
        ])
        .split(area);

    kpi(f, cells[0], "CASH", &fmt_money(p.cash), theme::fg());
    kpi(
        f,
        cells[1],
        "POSITIONS",
        &fmt_money(p.positions_value),
        theme::fg(),
    );
    kpi(
        f,
        cells[2],
        "OPEN ORDERS",
        &fmt_money(p.open_orders_value),
        theme::dim(),
    );
    let nw_style = if p.net_worth >= 25_000.0 {
        theme::positive()
    } else {
        theme::negative()
    };
    kpi(
        f,
        cells[3],
        "NET WORTH",
        &fmt_money(p.net_worth),
        nw_style.add_modifier(Modifier::BOLD),
    );
}

fn kpi(f: &mut Frame, area: Rect, label: &str, value: &str, value_style: Style) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(theme::border())
        .title(Span::styled(format!(" {label} "), theme::dim()));
    let inner = block.inner(area);
    f.render_widget(block, area);
    let para = Paragraph::new(Line::from(Span::styled(value.to_string(), value_style)))
        .alignment(Alignment::Center);
    let v_split = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1)])
        .split(inner);
    f.render_widget(para, v_split[0]);
}

fn render_sparkline(f: &mut Frame, app: &App, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(theme::border())
        .title(Span::styled(" NET WORTH (session) ", theme::dim()));
    let inner = block.inner(area);
    f.render_widget(block, area);

    let history: Vec<u64> = app
        .net_worth_history
        .iter()
        .map(|v| (*v as i64).max(0) as u64)
        .collect();

    if history.len() < 2 {
        let msg = Paragraph::new(vec![
            Line::from(""),
            Line::from(Span::styled(
                "  collecting net-worth snapshots — your line will appear here after a couple of refreshes",
                theme::dim(),
            )),
            Line::from(""),
            Line::from(Span::styled(
                "  press R to refresh now",
                theme::dim(),
            )),
        ]);
        f.render_widget(msg, inner);
        return;
    }
    let max = *history.iter().max().unwrap_or(&1);
    let min = *history.iter().min().unwrap_or(&0);
    let last = *history.last().unwrap_or(&0);
    let first = *history.first().unwrap_or(&0);
    let trend = if last >= first {
        Span::styled(
            format!("  ▲ {}", fmt_money((last as i64 - first as i64) as f64)),
            theme::positive(),
        )
    } else {
        Span::styled(
            format!("  ▼ {}", fmt_money((last as i64 - first as i64) as f64)),
            theme::negative(),
        )
    };
    let header = Line::from(vec![
        Span::styled(format!("min {}", fmt_money(min as f64)), theme::dim()),
        Span::raw("  "),
        Span::styled(format!("max {}", fmt_money(max as f64)), theme::dim()),
        trend,
    ]);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(1)])
        .split(inner);
    f.render_widget(Paragraph::new(header), chunks[0]);
    // Sparkline scales 0..max, so net-worth values clustered far above zero
    // would all render as near-full bars. Rebase to `min` (with a small floor
    // so a dead-flat series still shows a baseline) to make the trend visible.
    let normalized: Vec<u64> = history
        .iter()
        .map(|v| v.saturating_sub(min) + 1)
        .collect();
    let sparkline = Sparkline::default()
        .data(&normalized)
        .style(Style::default().fg(theme::PRIMARY))
        .bar_set(symbols::bar::NINE_LEVELS);
    f.render_widget(sparkline, chunks[1]);
}

fn render_positions(f: &mut Frame, app: &App, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(theme::border())
        .title(Span::styled(" POSITIONS ", theme::dim()));
    let inner = block.inner(area);
    f.render_widget(block, area);

    let rows = match &app.positions {
        LoadState::Loaded { data, .. } => data,
        LoadState::Error(e) => {
            render_error(f, e, inner);
            return;
        }
        LoadState::Loading => {
            render_loading(f, "Loading positions…", inner);
            return;
        }
        LoadState::Idle => {
            render_empty(f, "press r to refresh", inner);
            return;
        }
    };
    if rows.is_empty() {
        render_empty(f, "no open positions — go place an order!", inner);
        return;
    }
    let header = Row::new(["MARKET", "QUESTION", "SIZE", "ENTRY", "MARK", "UNREALIZED"])
        .style(theme::primary());
    let body: Vec<Row> = rows.iter().map(position_row).collect();
    let widths = [
        Constraint::Length(10),
        Constraint::Percentage(40),
        Constraint::Length(10),
        Constraint::Length(8),
        Constraint::Length(8),
        Constraint::Length(14),
    ];
    let table = Table::new(body, widths)
        .header(header)
        .row_highlight_style(theme::row_highlight())
        .highlight_symbol(" ▶ ");
    let mut state = TableState::default();
    state.select(Some(app.positions_sel.min(rows.len().saturating_sub(1))));
    f.render_stateful_widget(table, inner, &mut state);
}

fn position_row(p: &Position) -> Row<'static> {
    let pnl_style = if p.unrealized_pnl >= 0.0 {
        theme::positive()
    } else {
        theme::negative()
    };
    let entry = p
        .avg_entry_price
        .map(|v| format!("{v:.3}"))
        .unwrap_or_else(|| "—".into());
    Row::new(vec![
        Cell::from(truncate(&p.market_id, 9)),
        Cell::from(truncate(
            p.market_question.as_deref().unwrap_or("—"),
            60,
        )),
        Cell::from(format!("{:.2}", p.size)),
        Cell::from(entry),
        Cell::from(format!("{:.3}", p.current_price)),
        Cell::from(fmt_money(p.unrealized_pnl)).style(pnl_style),
    ])
}

fn portfolio_subtitle(app: &App) -> String {
    if !app.has_key {
        return "no API key".into();
    }
    match app.elapsed_since_load(Tab::Portfolio) {
        Some(d) => fmt_age(d),
        None => "press r to load".into(),
    }
}

fn render_no_key(f: &mut Frame, area: Rect) {
    let lines = vec![
        Line::from(""),
        Line::from(Span::styled(
            "  no POLY_API_KEY in your environment",
            theme::accent(),
        )),
        Line::from(""),
        Line::from(Span::styled(
            "  ask a club admin for your `pm_paper_…` key, then:",
            theme::fg(),
        )),
        Line::from(""),
        Line::from(Span::styled(
            "    export POLY_API_KEY=pm_paper_…",
            theme::primary(),
        )),
        Line::from(Span::styled("    predlab-tui", theme::primary())),
        Line::from(""),
        Line::from(Span::styled(
            "  the Leaderboard and Markets tabs work without a key.",
            theme::dim(),
        )),
    ];
    f.render_widget(Paragraph::new(lines), area);
}

// ---------------------------------------------------------------------------
// Tab: Help
// ---------------------------------------------------------------------------

const BANNER: &str = "\
█▀█ █▀█ █▀▀ █▀▄ █   ▄▀█ █▄▄
█▀▀ █▀▄ ██▄ █▄▀ █▄▄ █▀█ █▄█";

fn render_help(f: &mut Frame, _app: &App, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(theme::border_active())
        .title(Span::styled(" HELP ", theme::primary()))
        .title_alignment(Alignment::Left);
    let inner = block.inner(area);
    f.render_widget(block, area);

    let mut lines: Vec<Line> = Vec::new();
    for ln in BANNER.lines() {
        lines.push(Line::from(Span::styled(ln.to_string(), theme::primary())));
    }
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "  paper trading · terminal edition",
        theme::dim(),
    )));
    lines.push(Line::from(""));
    lines.push(section("NAVIGATION"));
    lines.push(kv("h / l, Tab, Shift-Tab", "previous / next tab"));
    lines.push(kv("1, 2, 3, 4", "jump to tab"));
    lines.push(kv("j / k, Down / Up", "move selection"));
    lines.push(kv("gg / G", "jump to top / bottom"));
    lines.push(kv("Ctrl-d / Ctrl-u", "half-page down / up"));
    lines.push(Line::from(""));
    lines.push(section("ACTIONS"));
    lines.push(kv("r", "refresh current tab"));
    lines.push(kv("R", "refresh everything"));
    lines.push(kv("/needle", "filter the current list"));
    lines.push(kv("Esc", "clear filter (or quit when none)"));
    lines.push(kv("?", "show this help"));
    lines.push(kv("q, Ctrl-c", "quit"));
    lines.push(Line::from(""));
    lines.push(section("COMMANDS  (press : first)"));
    lines.push(kv(":q  :quit", "quit"));
    lines.push(kv(":r  :refresh", "refresh current tab"));
    lines.push(kv(":R  :refresh!", "refresh everything"));
    lines.push(kv(":help  :h", "this page"));
    lines.push(kv(":lb :m :p", "leaderboard / markets / portfolio"));
    lines.push(kv(":tab N", "switch to tab N"));
    lines.push(Line::from(""));
    lines.push(section("ENVIRONMENT"));
    lines.push(kv("POLY_API_KEY", "your `pm_paper_…` key (admin issues it)"));
    lines.push(kv("POLY_BASE", "sim base URL (default poly.teddytennant.com)"));
    lines.push(kv("LEADERBOARD_BASE", "leaderboard host"));
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "  predlab.teddytennant.com  ·  paper money, real practice",
        theme::dim(),
    )));

    let para = Paragraph::new(lines).wrap(Wrap { trim: false });
    f.render_widget(para, inner);
}

fn section(label: &str) -> Line<'static> {
    Line::from(Span::styled(
        format!("  ── {label} ──"),
        Style::default().fg(theme::ACCENT).add_modifier(Modifier::BOLD),
    ))
}

fn kv(key: &str, desc: &str) -> Line<'static> {
    Line::from(vec![
        Span::raw("    "),
        Span::styled(format!("{key:<22}"), Style::default().fg(theme::PRIMARY)),
        Span::styled(format!("  {desc}"), theme::fg()),
    ])
}

// ---------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------

fn panel_block(app: &App, tab: Tab, subtitle: &str) -> Block<'static> {
    let style = if app.tab == tab {
        theme::border_active()
    } else {
        theme::border()
    };
    Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(style)
        .title(Span::styled(format!(" {} ", tab.title()), theme::primary()))
        .title_alignment(Alignment::Left)
        .title_bottom(Line::from(Span::styled(
            format!(" {subtitle} "),
            theme::dim(),
        )))
}

fn render_loading(f: &mut Frame, msg: &str, area: Rect) {
    let dots = match (std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0)
        / 250) % 4
    {
        0 => "   ",
        1 => ".  ",
        2 => ".. ",
        _ => "...",
    };
    let para = Paragraph::new(vec![
        Line::from(""),
        Line::from(Span::styled(
            format!("  {msg}{dots}"),
            theme::dim(),
        )),
    ]);
    f.render_widget(para, area);
}

fn render_error(f: &mut Frame, msg: &str, area: Rect) {
    let para = Paragraph::new(vec![
        Line::from(""),
        Line::from(Span::styled("  request failed", theme::negative())),
        Line::from(Span::styled(format!("  {msg}"), theme::dim())),
        Line::from(""),
        Line::from(Span::styled(
            "  press r to retry",
            theme::fg(),
        )),
    ])
    .wrap(Wrap { trim: false });
    f.render_widget(para, area);
}

fn render_empty(f: &mut Frame, msg: &str, area: Rect) {
    let para = Paragraph::new(vec![
        Line::from(""),
        Line::from(Span::styled(format!("  {msg}"), theme::dim())),
    ]);
    f.render_widget(para, area);
}

