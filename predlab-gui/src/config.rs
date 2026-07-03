//! GUI configuration, persisted as TOML at `~/.predlab/gui.toml`
//! (override with the `PREDLAB_GUI_CONFIG` env var).

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

/// Allowed poll interval bounds in seconds.
pub const TICK_SECONDS_MIN: u64 = 1;
/// Allowed poll interval bounds in seconds.
pub const TICK_SECONDS_MAX: u64 = 60;
/// Allowed market list size bounds.
pub const MARKET_LIMIT_MIN: u32 = 1;
/// Allowed market list size bounds (the sim clamps to 500 server-side).
pub const MARKET_LIMIT_MAX: u32 = 500;

/// Default Polymarket simulator base — the club's hosted instance.
pub const DEFAULT_POLY_URL: &str = "https://poly.teddytennant.com";
/// Default leaderboard server base — serves the public ranking JSON.
pub const DEFAULT_LEADERBOARD_URL: &str = "https://predlab.teddytennant.com";

/// Everything the GUI and engine need to talk to the simulator and the
/// public leaderboard server.
///
/// The UI edits a copy and pushes the whole struct to the engine via
/// [`crate::message::EngineMessage::ConfigChanged`]; the engine never reads
/// UI state directly.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    /// Whether the first-run onboarding flow has completed.
    pub onboarded: bool,
    /// Base URL of the Polymarket simulator.
    pub poly_url: String,
    /// Base URL of the public leaderboard site.
    pub leaderboard_url: String,
    /// Paper API key (`pm_paper_...`), sent as `POLY_API_KEY`.
    pub poly_api_key: String,
    /// Sim master admin secret (`X-Admin-Secret`, owner rank). Empty =>
    /// admin access depends on the API key's own role.
    pub poly_admin_secret: String,
    /// Poll interval in seconds, clamped to `1..=60`.
    pub tick_seconds: u64,
    /// Max markets fetched per poll (sim caps at 500).
    pub market_limit: u32,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            onboarded: false,
            poly_url: DEFAULT_POLY_URL.to_string(),
            leaderboard_url: DEFAULT_LEADERBOARD_URL.to_string(),
            poly_api_key: String::new(),
            poly_admin_secret: String::new(),
            tick_seconds: 3,
            market_limit: 50,
        }
    }
}

impl Config {
    /// Config file location: `$PREDLAB_GUI_CONFIG` if set, else
    /// `~/.predlab/gui.toml`.
    pub fn path() -> PathBuf {
        if let Ok(p) = std::env::var("PREDLAB_GUI_CONFIG")
            && !p.is_empty()
        {
            return PathBuf::from(p);
        }
        dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".predlab")
            .join("gui.toml")
    }

    /// Load from [`Config::path`]. A missing file yields defaults; a file
    /// that exists but fails to parse is an error (the user's file is never
    /// overwritten with defaults).
    pub fn load() -> Result<Self> {
        Self::load_from(&Self::path())
    }

    /// Load from an explicit path (see [`Config::load`] for semantics).
    pub fn load_from(path: &Path) -> Result<Self> {
        let mut cfg = match std::fs::read_to_string(path) {
            Ok(text) => toml::from_str::<Config>(&text)
                .with_context(|| format!("parse config {}", path.display()))?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Config::default(),
            Err(e) => {
                return Err(e).with_context(|| format!("read config {}", path.display()));
            }
        };
        cfg.apply_env_fallbacks();
        cfg.clamp();
        Ok(cfg)
    }

    /// Persist to [`Config::path`], creating the parent directory.
    pub fn save(&self) -> Result<()> {
        self.save_to(&Self::path())
    }

    /// Persist to an explicit path, creating the parent directory.
    pub fn save_to(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("create config dir {}", parent.display()))?;
        }
        let text = toml::to_string_pretty(self).context("serialize config")?;
        std::fs::write(path, text).with_context(|| format!("write config {}", path.display()))?;
        Ok(())
    }

    /// Clamp numeric fields into their allowed ranges.
    pub fn clamp(&mut self) {
        self.tick_seconds = self.tick_seconds.clamp(TICK_SECONDS_MIN, TICK_SECONDS_MAX);
        self.market_limit = self.market_limit.clamp(MARKET_LIMIT_MIN, MARKET_LIMIT_MAX);
    }

    /// True when trading endpoints can be called.
    pub fn has_poly_creds(&self) -> bool {
        !self.poly_api_key.is_empty()
    }

    fn apply_env_fallbacks(&mut self) {
        // Matches the existing admin TUI convention for the sim secret.
        if self.poly_admin_secret.is_empty() {
            for var in ["PREDLAB_ADMIN_SECRET", "ADMIN_SECRET"] {
                if let Ok(v) = std::env::var(var)
                    && !v.is_empty()
                {
                    self.poly_admin_secret = v;
                    break;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Mutex, MutexGuard};

    /// Env vars are process-global; serialize the tests that touch them.
    static ENV_LOCK: Mutex<()> = Mutex::new(());

    fn env_guard() -> MutexGuard<'static, ()> {
        ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("predlab-gui-cfg-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn default_toml_roundtrip() {
        let cfg = Config::default();
        let text = toml::to_string_pretty(&cfg).unwrap();
        let back: Config = toml::from_str(&text).unwrap();
        assert_eq!(cfg, back);
        assert_eq!(back.poly_url, "https://poly.teddytennant.com");
        assert_eq!(back.leaderboard_url, "https://predlab.teddytennant.com");
        assert_eq!(back.tick_seconds, 3);
        assert_eq!(back.market_limit, 50);
        assert!(!back.onboarded);
    }

    #[test]
    fn save_then_load_is_identity() {
        let _guard = env_guard();
        let dir = temp_dir("roundtrip");
        let path = dir.join("gui.toml");
        let cfg = Config {
            onboarded: true,
            poly_api_key: "pm_paper_abc".to_string(),
            poly_url: "http://localhost:8001".to_string(),
            leaderboard_url: "http://localhost:8003".to_string(),
            tick_seconds: 7,
            ..Config::default()
        };
        cfg.save_to(&path).unwrap();
        let back = Config::load_from(&path).unwrap();
        assert_eq!(cfg, back);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn missing_file_yields_defaults() {
        let _guard = env_guard();
        let dir = temp_dir("missing");
        let cfg = Config::load_from(&dir.join("does-not-exist.toml")).unwrap();
        assert_eq!(cfg, Config::default());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn parse_error_is_error_not_defaults() {
        let _guard = env_guard();
        let dir = temp_dir("garbage");
        let path = dir.join("gui.toml");
        std::fs::write(&path, "this is { not toml").unwrap();
        assert!(Config::load_from(&path).is_err());
        // The broken file is left untouched.
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "this is { not toml");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn old_dual_sim_config_still_parses() {
        // Configs written by the previous two-simulator build carry extra
        // kalshi_* keys; serde must ignore them rather than erroring.
        let _guard = env_guard();
        let dir = temp_dir("legacy");
        let path = dir.join("gui.toml");
        std::fs::write(
            &path,
            "onboarded = true\npoly_url = \"http://localhost:8001\"\n\
             kalshi_url = \"http://localhost:8002\"\nkalshi_key_id = \"ks_live_x\"\n\
             poly_api_key = \"pm_paper_y\"\n",
        )
        .unwrap();
        let cfg = Config::load_from(&path).unwrap();
        assert!(cfg.onboarded);
        assert_eq!(cfg.poly_api_key, "pm_paper_y");
        assert_eq!(cfg.leaderboard_url, DEFAULT_LEADERBOARD_URL);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn load_clamps_out_of_range_values() {
        let _guard = env_guard();
        let dir = temp_dir("clamp");
        let path = dir.join("gui.toml");
        std::fs::write(&path, "tick_seconds = 0\nmarket_limit = 99999\n").unwrap();
        let cfg = Config::load_from(&path).unwrap();
        assert_eq!(cfg.tick_seconds, TICK_SECONDS_MIN);
        assert_eq!(cfg.market_limit, MARKET_LIMIT_MAX);

        std::fs::write(&path, "tick_seconds = 999\nmarket_limit = 0\n").unwrap();
        let cfg = Config::load_from(&path).unwrap();
        assert_eq!(cfg.tick_seconds, TICK_SECONDS_MAX);
        assert_eq!(cfg.market_limit, MARKET_LIMIT_MIN);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn env_var_overrides_config_path() {
        let _guard = env_guard();
        let dir = temp_dir("envpath");
        let path = dir.join("custom.toml");
        // SAFETY: env mutation is serialized by ENV_LOCK.
        unsafe { std::env::set_var("PREDLAB_GUI_CONFIG", &path) };
        assert_eq!(Config::path(), path);

        let cfg = Config {
            poly_api_key: "pm_paper_env".to_string(),
            ..Config::default()
        };
        cfg.save().unwrap();
        let back = Config::load().unwrap();
        assert_eq!(back.poly_api_key, "pm_paper_env");

        unsafe { std::env::remove_var("PREDLAB_GUI_CONFIG") };
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn poly_admin_secret_falls_back_to_env() {
        let _guard = env_guard();
        let dir = temp_dir("admin-env");
        let path = dir.join("gui.toml");
        Config::default().save_to(&path).unwrap();

        // SAFETY: env mutation is serialized by ENV_LOCK.
        unsafe { std::env::set_var("PREDLAB_ADMIN_SECRET", "s3cret") };
        let cfg = Config::load_from(&path).unwrap();
        assert_eq!(cfg.poly_admin_secret, "s3cret");
        unsafe { std::env::remove_var("PREDLAB_ADMIN_SECRET") };

        unsafe { std::env::set_var("ADMIN_SECRET", "fallback") };
        let cfg = Config::load_from(&path).unwrap();
        assert_eq!(cfg.poly_admin_secret, "fallback");
        unsafe { std::env::remove_var("ADMIN_SECRET") };

        // A secret already present in the file wins over env.
        let with_secret = Config {
            poly_admin_secret: "from-file".to_string(),
            ..Config::default()
        };
        with_secret.save_to(&path).unwrap();
        unsafe { std::env::set_var("PREDLAB_ADMIN_SECRET", "ignored") };
        let cfg = Config::load_from(&path).unwrap();
        assert_eq!(cfg.poly_admin_secret, "from-file");
        unsafe { std::env::remove_var("PREDLAB_ADMIN_SECRET") };
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn creds_helper() {
        let mut cfg = Config::default();
        assert!(!cfg.has_poly_creds());
        cfg.poly_api_key = "pm_paper_1".to_string();
        assert!(cfg.has_poly_creds());
    }
}
