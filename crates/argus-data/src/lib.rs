//! HTTP clients and local config for Argus.
//!
//! Rune keeps the UI thread free with `rune::Latest<T>`: a handler calls
//! `request(key, |q| ...)` and the closure runs on one of Rune's shared worker
//! threads; the answer is picked up in `AppSpec::poll` on the UI thread and a
//! `Waker` (given to `AppSpec::setup`) asks Rune to call `poll` again. That
//! closure is synchronous, so these clients use `reqwest::blocking` — no async
//! runtime has to cross the Rune boundary.
//!
//! The entry point is [`Client`]. Each source module adds methods to it:
//! `get_bus_stops`, `get_weather`, `get_flights`, `submit_feedback`, …. All
//! requests go through a shared [`HttpClient`], an in-memory TTL [`Cache`], and
//! a [`HealthRegistry`] that the UI reads via `Client::source_health`.
//! See the "argus-data API" section of `RUST_REWRITE.md`.

use std::sync::Arc;

pub mod bus_routes;
pub mod cache;
pub mod client;
pub mod config;
pub mod error;
pub mod feedback;
pub mod flights;
pub mod health;
pub mod http;
pub mod lta;
pub mod news;
pub mod time;
pub mod util;
pub mod weather;
pub mod wire;

pub use cache::Cache;
pub use config::ArgusConfig;
pub use error::{Error, Result};
pub use feedback::FeedbackSubmission;
pub use health::{HealthRegistry, HealthStatus, Source, SourceHealth};
pub use http::{HttpClient, HttpRequest, HttpResponse, ReqwestTransport, Transport};

/// The configured data layer: config + HTTP + cache + source health.
pub struct Client {
    config: ArgusConfig,
    http: HttpClient,
    cache: Cache,
    health: HealthRegistry,
}

impl Default for Client {
    fn default() -> Self {
        Self::new()
    }
}

impl Client {
    /// Load keys from the environment / `.env` / `~/.config/argus/config.toml`.
    pub fn new() -> Self {
        Self::with_config(ArgusConfig::load())
    }

    pub fn with_config(config: ArgusConfig) -> Self {
        Self {
            config,
            http: HttpClient::new(),
            cache: Cache::new(),
            health: HealthRegistry::new(),
        }
    }

    /// Inject a transport (used by offline tests with fixture responses).
    pub fn with_transport(config: ArgusConfig, transport: Arc<dyn Transport>) -> Self {
        Self {
            config,
            http: HttpClient::with_transport(transport),
            cache: Cache::new(),
            health: HealthRegistry::new(),
        }
    }

    pub fn config(&self) -> &ArgusConfig {
        &self.config
    }

    pub fn http(&self) -> &HttpClient {
        &self.http
    }

    pub fn cache(&self) -> &Cache {
        &self.cache
    }

    pub fn health(&self) -> &HealthRegistry {
        &self.health
    }

    /// Snapshot for the source-health panel, in display order.
    pub fn source_health(&self) -> Vec<SourceHealth> {
        self.health.list()
    }

    pub fn source(&self, source: Source) -> SourceHealth {
        self.health.get(source)
    }

    /// Mark a finished request's source ok/error, then pass the result on.
    pub(crate) fn finish<T>(&self, source: Source, result: Result<T>) -> Result<T> {
        match &result {
            Ok(_) => self.health.mark_ok(source),
            Err(e) => self.health.mark_error(source, e),
        }
        result
    }
}

/// 5-digit LTA bus stop code (`BUS_STOP_ID_RE` in `lib/route-utils.ts`).
pub fn validate_bus_stop_id(id: &str) -> Result<()> {
    let trimmed = id.trim();
    if trimmed.len() == 5 && trimmed.bytes().all(|b| b.is_ascii_digit()) {
        Ok(())
    } else {
        Err(Error::BadRequest {
            message: "stopId must be a 5-digit bus stop code".into(),
        })
    }
}

/// LTA service numbers: 1-8 alphanumeric chars (`BUS_SERVICE_NO_RE`).
pub fn validate_service_no(value: &str) -> Result<()> {
    let trimmed = value.trim();
    if (1..=8).contains(&trimmed.len()) && trimmed.bytes().all(|b| b.is_ascii_alphanumeric()) {
        Ok(())
    } else {
        Err(Error::BadRequest {
            message: "serviceNo must be 1-8 alphanumeric characters".into(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn client_exposes_config_cache_and_health() {
        let config = ArgusConfig::new(Some("key".into()), None, None);
        let client = Client::with_config(config);
        assert_eq!(client.config().lta_api_key().unwrap(), "key");
        assert!(client.cache().is_empty());
        assert_eq!(client.source(Source::News).status, HealthStatus::Loading);
        assert_eq!(client.source_health().len(), Source::ALL.len());
    }

    #[test]
    fn route_parameter_validation() {
        assert!(validate_bus_stop_id("01013").is_ok());
        assert!(validate_bus_stop_id("1234").is_err());
        assert!(validate_bus_stop_id("abcde").is_err());
        assert!(validate_service_no("12e").is_ok());
        assert!(validate_service_no("NR1").is_ok());
        assert!(validate_service_no("too-long-service").is_err());
    }
}
