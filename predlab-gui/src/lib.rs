//! PredLab desktop GUI core.
//!
//! Two-thread architecture:
//! - **UI thread (main)**: owns the egui interface via [`ui::run`];
//!   never does HTTP.
//! - **Engine thread**: [`engine::EngineManager::run`] owns all HTTP, drains
//!   [`message::EngineMessage`]s, polls the sim + leaderboard site on a
//!   tick, and writes into the shared [`data::Snapshot`].
//!
//! Communication is `std::sync::mpsc` both ways; config flows UI -> engine
//! as a full [`config::Config`] push, and the engine never reads UI state.

pub mod config;
pub mod data;
pub mod domain;
pub mod engine;
pub mod message;
pub mod ui;
