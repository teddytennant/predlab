//! PredLab TUI — a terminal client for the PredLab paper-trading sim.
//!
//! Mirrors what the website (`predlab.teddytennant.com`) shows, but in your
//! shell with vim navigation. Three live views (leaderboard, markets,
//! portfolio) plus a help screen. Public views work with no setup; the
//! Portfolio tab needs your `POLY_API_KEY` (admin issues it).
//!
//! Architecture: a single tokio runtime drives both the terminal event poller
//! (blocking, in `spawn_blocking`) and the async HTTP fetches. Both stream
//! into one `mpsc` channel that the main loop consumes — so the UI never
//! blocks on a network request and tab switches feel instant.

mod api;
mod app;
mod theme;
mod ui;

use std::io;
use std::time::Duration;

use anyhow::{Context, Result};
use crossterm::{
    event::{self, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};
use tokio::sync::mpsc::{self, UnboundedSender};

use crate::api::Api;
use crate::app::{App, Event, FetchMsg, KeyOutcome, Tab};

/// Number of markets to pull per refresh. Tunable via `PREDLAB_MARKET_LIMIT`.
const DEFAULT_MARKET_LIMIT: usize = 50;
/// Auto-refresh the leaderboard / markets on this cadence so a long-running
/// session stays fresh without the user pressing `r`.
const AUTO_REFRESH: Duration = Duration::from_secs(30);

fn market_limit() -> usize {
    std::env::var("PREDLAB_MARKET_LIMIT")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(DEFAULT_MARKET_LIMIT)
}

#[tokio::main(flavor = "multi_thread", worker_threads = 2)]
async fn main() -> Result<()> {
    // Allow `--help` / `--version` from the shell without entering the TUI —
    // useful for the install script the website hands out.
    if let Some(arg) = std::env::args().nth(1) {
        match arg.as_str() {
            "-h" | "--help" => {
                print_help();
                return Ok(());
            }
            "-V" | "--version" => {
                println!("predlab-tui {}", env!("CARGO_PKG_VERSION"));
                return Ok(());
            }
            _ => {}
        }
    }

    let api = Api::from_env();
    let mut app = App::new(api.has_key());

    enable_raw_mode().context("entering raw mode")?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen).context("entering alt screen")?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend).context("building terminal")?;

    let res = run(&mut terminal, &mut app, &api).await;

    disable_raw_mode().ok();
    execute!(terminal.backend_mut(), LeaveAlternateScreen).ok();
    terminal.show_cursor().ok();

    if let Err(e) = res {
        eprintln!("predlab-tui: {e:?}");
    }
    Ok(())
}

async fn run<B: ratatui::backend::Backend>(
    terminal: &mut Terminal<B>,
    app: &mut App,
    api: &Api,
) -> Result<()> {
    let (tx, mut rx) = mpsc::unbounded_channel::<Event>();

    // Input reader — crossterm's event polling is blocking, so it runs on the
    // blocking pool.
    spawn_input_reader(tx.clone());
    // Periodic UI tick: 250 ms keeps the loading spinner animated and ages
    // ("12s ago") current without flooding the channel.
    spawn_ticker(tx.clone(), Duration::from_millis(250));
    // Auto-refresh the public tabs, plus the portfolio when a key is present —
    // the latter keeps the "NET WORTH (session)" sparkline accumulating
    // snapshots without the member having to press `r`.
    spawn_auto_refresh(api.clone(), tx.clone());

    // Kick off the initial fetches so every tab is ready by the time the user
    // navigates to it.
    refresh(api, &tx, RefreshScope::All);
    for t in [Tab::Leaderboard, Tab::Markets, Tab::Portfolio] {
        if t != Tab::Portfolio || api.has_key() {
            app.mark_loading(t);
        }
    }

    loop {
        terminal.draw(|f| ui::draw(f, app))?;

        let Some(ev) = rx.recv().await else { break };
        match ev {
            Event::Key(key) => {
                let outcome = app.handle_key(key);
                match outcome {
                    KeyOutcome::None => {}
                    KeyOutcome::RefreshTab(t) => {
                        app.mark_loading(t);
                        refresh(api, &tx, RefreshScope::Tab(t));
                    }
                    KeyOutcome::RefreshAll => {
                        for t in Tab::ALL {
                            if t == Tab::Portfolio && !api.has_key() {
                                continue;
                            }
                            app.mark_loading(t);
                        }
                        refresh(api, &tx, RefreshScope::All);
                    }
                }
            }
            Event::Resize | Event::Tick => {}
            Event::Fetch(m) => app.apply_fetch(m),
        }

        if app.should_quit {
            break;
        }
    }
    Ok(())
}

fn spawn_input_reader(tx: UnboundedSender<Event>) {
    tokio::task::spawn_blocking(move || loop {
        match event::poll(Duration::from_millis(150)) {
            Ok(true) => match event::read() {
                Ok(event::Event::Key(k)) if k.kind == KeyEventKind::Press => {
                    if tx.send(Event::Key(k)).is_err() {
                        break;
                    }
                }
                Ok(event::Event::Resize(_, _)) => {
                    if tx.send(Event::Resize).is_err() {
                        break;
                    }
                }
                Ok(_) => {}
                Err(_) => break,
            },
            Ok(false) => {}
            Err(_) => break,
        }
    });
}

fn spawn_ticker(tx: UnboundedSender<Event>, every: Duration) {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(every);
        // The first tick fires immediately; skip it so the initial draw isn't
        // immediately retriggered.
        interval.tick().await;
        loop {
            interval.tick().await;
            if tx.send(Event::Tick).is_err() {
                break;
            }
        }
    });
}

fn spawn_auto_refresh(api: Api, tx: UnboundedSender<Event>) {
    tokio::spawn(async move {
        // With a key, also poll the portfolio so the session sparkline fills in.
        let scope = if api.has_key() {
            RefreshScope::PublicAndPortfolio
        } else {
            RefreshScope::Public
        };
        let mut interval = tokio::time::interval(AUTO_REFRESH);
        interval.tick().await;
        loop {
            interval.tick().await;
            refresh(&api, &tx, scope);
        }
    });
}

#[derive(Debug, Clone, Copy)]
enum RefreshScope {
    /// All four tabs (Help is a no-op).
    All,
    /// Just one tab.
    Tab(Tab),
    /// Auto-refresh: the public, key-less endpoints only.
    Public,
    /// Auto-refresh for keyed members: public endpoints plus the portfolio.
    PublicAndPortfolio,
}

fn refresh(api: &Api, tx: &UnboundedSender<Event>, scope: RefreshScope) {
    let tabs: Vec<Tab> = match scope {
        RefreshScope::All => Tab::ALL.to_vec(),
        RefreshScope::Tab(t) => vec![t],
        RefreshScope::Public => vec![Tab::Leaderboard, Tab::Markets],
        RefreshScope::PublicAndPortfolio => {
            vec![Tab::Leaderboard, Tab::Markets, Tab::Portfolio]
        }
    };
    for t in tabs {
        let api = api.clone();
        let tx = tx.clone();
        tokio::spawn(async move {
            let msg = match t {
                Tab::Leaderboard => FetchMsg::Leaderboard(
                    api.leaderboard().await.map_err(|e| e.to_string()),
                ),
                Tab::Markets => {
                    FetchMsg::Markets(api.markets(market_limit()).await.map_err(|e| e.to_string()))
                }
                Tab::Portfolio => {
                    if !api.has_key() {
                        return;
                    }
                    let portfolio = api.portfolio().await.map_err(|e| e.to_string());
                    let positions = api.positions().await.map_err(|e| e.to_string());
                    FetchMsg::Portfolio { portfolio, positions }
                }
                Tab::Help => return,
            };
            let _ = tx.send(Event::Fetch(msg));
        });
    }
}

fn print_help() {
    println!(
        "predlab-tui {ver}
PredLab paper-trading TUI.

USAGE:
    predlab-tui

ENVIRONMENT:
    POLY_API_KEY        your `pm_paper_…` key (admin issues it)
    POLY_BASE           sim base URL  (default {poly})
    LEADERBOARD_BASE    leaderboard host (default {lb})
    PREDLAB_MARKET_LIMIT  how many markets to fetch per refresh ({mlim})

KEYS:
    h/l, 1-4    switch tabs
    j/k         move selection
    gg / G      jump to top / bottom
    r / R       refresh tab / refresh all
    /needle     filter list
    :cmd        ex command (try `:help`)
    q, Ctrl-c   quit

GET YOUR KEY:
    {lb}/start
",
        ver = env!("CARGO_PKG_VERSION"),
        poly = api::DEFAULT_POLY_BASE,
        lb = api::DEFAULT_LEADERBOARD_BASE,
        mlim = DEFAULT_MARKET_LIMIT,
    );
}
