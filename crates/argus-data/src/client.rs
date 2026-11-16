//! Skeleton HTTP client. Endpoints and local key/config loading land here in
//! later phases; for now it just owns a reusable `reqwest` client and base URL.

use argus_core::Layer;

/// A configured external API.
#[derive(Clone, Debug)]
pub struct ApiClient {
    base_url: String,
    http: reqwest::blocking::Client,
}

impl ApiClient {
    /// Build a client from a base URL. Keys are read from local config later.
    pub fn new(base_url: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into(),
            http: reqwest::blocking::Client::builder()
                .timeout(std::time::Duration::from_secs(15))
                .build()
                .expect("build HTTP client"),
        }
    }

    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    /// The shared blocking client, for endpoints added in later phases.
    pub fn http(&self) -> &reqwest::blocking::Client {
        &self.http
    }

    /// Placeholder for the first real endpoint: returns the layers we know
    /// about without touching the network, so callers can be wired up early.
    pub fn known_layers(&self) -> Vec<Layer> {
        vec![Layer::new("flights", "Flights"), Layer::new("news", "News")]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn client_keeps_its_base_url() {
        let client = ApiClient::new("https://example.invalid/v1");
        assert_eq!(client.base_url(), "https://example.invalid/v1");
        assert_eq!(client.known_layers().len(), 2);
    }
}
