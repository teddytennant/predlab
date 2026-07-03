//! PredLab desktop GUI bootstrap.
//!
//! Two-thread architecture:
//! - **UI thread (main)**: owns the egui interface; never does HTTP.
//! - **Engine thread**: owns all HTTP, drains [`message::EngineMessage`]s,
//!   polls the sim + leaderboard site on a tick, and writes into the shared
//!   [`data::Snapshot`].

use std::sync::mpsc;
use std::sync::{Arc, Mutex};

use anyhow::Context;

use predlab_gui::config::Config;
use predlab_gui::data::Snapshot;
use predlab_gui::engine::EngineManager;
use predlab_gui::message::{self, EngineMessage};
use predlab_gui::ui;

fn main() -> anyhow::Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    let config = Config::load().context("load config")?;
    log::info!(
        "config loaded: poly={} leaderboard={} tick={}s",
        config.poly_url,
        config.leaderboard_url,
        config.tick_seconds
    );

    let (engine_tx, engine_rx) = mpsc::channel::<EngineMessage>();
    let (ui_tx, ui_rx) = mpsc::channel::<message::UiMessage>();
    let snapshot = Arc::new(Mutex::new(Snapshot::default()));

    let engine_snapshot = Arc::clone(&snapshot);
    let engine_config = config.clone();
    let engine_thread = std::thread::Builder::new()
        .name("predlab-engine".to_string())
        .spawn(move || EngineManager::run(engine_rx, ui_tx, engine_snapshot, engine_config))
        .context("spawn engine thread")?;

    let ui_result = ui::run(ui::UiContext {
        config,
        engine_tx: engine_tx.clone(),
        ui_rx,
        snapshot,
    });

    // UI is done (or failed): stop the engine either way, then surface the
    // UI result.
    let _ = engine_tx.send(EngineMessage::Shutdown);
    engine_thread
        .join()
        .map_err(|_| anyhow::anyhow!("engine thread panicked"))?;
    ui_result
}
