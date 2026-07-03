//! Server API adapters: shared HTTP plumbing plus one module for the
//! Polymarket sim and one for the public leaderboard site. Everything here
//! is synchronous (`ureq`) and only ever called from the engine thread.

pub mod leaderboard;
pub mod polymarket;

use std::time::Duration;

use serde::Serialize;
use serde::de::DeserializeOwned;
use thiserror::Error;

/// Request timeout applied to every simulator call.
const TIMEOUT: Duration = Duration::from_secs(5);

/// What went wrong talking to a server.
#[derive(Debug, Error)]
pub enum ApiError {
    /// Could not reach the server (DNS, refused, timeout, TLS, ...).
    #[error("transport: {0}")]
    Transport(String),
    /// The server answered with a non-success status.
    #[error("HTTP {status}: {body}")]
    Status { status: u16, body: String },
    /// The response body did not match the expected shape.
    #[error("decode: {0}")]
    Decode(String),
}

impl ApiError {
    /// The HTTP status code, when the failure was a non-2xx response.
    pub fn status(&self) -> Option<u16> {
        match self {
            ApiError::Status { status, .. } => Some(*status),
            _ => None,
        }
    }
}

/// `GET /health` shape.
#[derive(Debug, Clone, Default, serde::Deserialize)]
#[serde(default)]
pub struct Health {
    pub status: String,
    pub version: String,
    pub environment: String,
}

/// Header list passed to [`Http`] helpers.
pub type Headers = Vec<(String, String)>;

/// Query-parameter list passed to [`Http`] helpers (values are URL-encoded
/// by ureq).
pub type Query<'a> = &'a [(&'a str, String)];

/// Thin blocking-HTTP helper shared by both adapters: one agent, fixed
/// timeouts, JSON in/out, non-2xx statuses surfaced as [`ApiError::Status`]
/// with the response body preserved.
pub struct Http {
    agent: ureq::Agent,
}

impl Http {
    /// Build an agent with the standard timeout; statuses are handled
    /// manually so error bodies can be captured.
    pub fn new() -> Self {
        let config = ureq::Agent::config_builder()
            .timeout_global(Some(TIMEOUT))
            .http_status_as_error(false)
            .build();
        Self {
            agent: config.new_agent(),
        }
    }

    /// `GET url?query` with `headers`, decoding a JSON response.
    pub fn get_json<T: DeserializeOwned>(
        &self,
        url: &str,
        query: Query,
        headers: &Headers,
    ) -> Result<T, ApiError> {
        let mut req = self.agent.get(url);
        for (k, v) in query {
            req = req.query(*k, v);
        }
        for (k, v) in headers {
            req = req.header(k.as_str(), v.as_str());
        }
        read_response(req.call())
    }

    /// `POST url?query` with a JSON body, decoding a JSON response.
    pub fn post_json<B: Serialize, T: DeserializeOwned>(
        &self,
        url: &str,
        query: Query,
        headers: &Headers,
        body: &B,
    ) -> Result<T, ApiError> {
        let mut req = self.agent.post(url);
        for (k, v) in query {
            req = req.query(*k, v);
        }
        for (k, v) in headers {
            req = req.header(k.as_str(), v.as_str());
        }
        read_response(req.send_json(body))
    }

    /// `POST url?query` with an empty body, decoding a JSON response.
    pub fn post_empty<T: DeserializeOwned>(
        &self,
        url: &str,
        query: Query,
        headers: &Headers,
    ) -> Result<T, ApiError> {
        let mut req = self.agent.post(url);
        for (k, v) in query {
            req = req.query(*k, v);
        }
        for (k, v) in headers {
            req = req.header(k.as_str(), v.as_str());
        }
        read_response(req.send_empty())
    }

    /// `DELETE url` with a JSON body (the sim cancels orders this way).
    pub fn delete_json<B: Serialize, T: DeserializeOwned>(
        &self,
        url: &str,
        headers: &Headers,
        body: &B,
    ) -> Result<T, ApiError> {
        let mut req = self.agent.delete(url);
        for (k, v) in headers {
            req = req.header(k.as_str(), v.as_str());
        }
        read_response(req.force_send_body().send_json(body))
    }
}

impl Default for Http {
    fn default() -> Self {
        Self::new()
    }
}

fn read_response<T: DeserializeOwned>(
    result: Result<ureq::http::Response<ureq::Body>, ureq::Error>,
) -> Result<T, ApiError> {
    let mut res = result.map_err(|e| ApiError::Transport(e.to_string()))?;
    if !res.status().is_success() {
        let status = res.status().as_u16();
        let body = res.body_mut().read_to_string().unwrap_or_default();
        return Err(ApiError::Status { status, body });
    }
    res.body_mut()
        .read_json::<T>()
        .map_err(|e| ApiError::Decode(e.to_string()))
}
