//! Schema-checked HTTP GETs with timeouts and normalised errors.
//!
//! Port of `lib/api-clients/http.ts`. The `Transport` seam lets the parsing /
//! fallback tests run against in-memory fixture responses (the TS tests stubbed
//! `globalThis.fetch`) while production uses `reqwest::blocking` with rustls and
//! a descriptive User-Agent.

use std::{sync::Arc, time::Duration};

use serde::de::DeserializeOwned;

use crate::error::{Error, Result};

pub const DEFAULT_TIMEOUT_MS: u64 = 10_000;
pub const USER_AGENT: &str = "Argus/0.1 (Singapore OSINT dashboard; reqwest/rustls; +https://github.com/curioputterings/argus)";

#[derive(Clone, Debug)]
pub struct HttpRequest {
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub timeout_ms: u64,
    /// If set, the transport must refuse bodies larger than this (RSS cap).
    pub max_bytes: Option<usize>,
}

#[derive(Clone, Debug)]
pub struct HttpResponse {
    pub status: u16,
    pub body: Vec<u8>,
}

/// Abstraction over "do a HTTP request", so tests can substitute fixture
/// responses. `post` is only used by the Neon feedback driver.
pub trait Transport: Send + Sync {
    fn get(&self, request: &HttpRequest) -> Result<HttpResponse>;

    fn post(&self, _request: &HttpRequest, _body: &[u8]) -> Result<HttpResponse> {
        Err(Error::external(
            "http",
            502,
            "POST is not supported by this transport",
        ))
    }
}

/// Production transport: `reqwest::blocking` with rustls (no OpenSSL).
#[derive(Clone)]
pub struct ReqwestTransport {
    client: reqwest::blocking::Client,
}

impl Default for ReqwestTransport {
    fn default() -> Self {
        Self {
            client: reqwest::blocking::Client::builder()
                .user_agent(USER_AGENT)
                .timeout(Duration::from_millis(DEFAULT_TIMEOUT_MS))
                .build()
                .expect("build reqwest blocking client"),
        }
    }
}

impl Transport for ReqwestTransport {
    fn get(&self, request: &HttpRequest) -> Result<HttpResponse> {
        let mut builder = self.client.get(&request.url);
        for (name, value) in &request.headers {
            builder = builder.header(name, value);
        }
        let response = builder
            .timeout(Duration::from_millis(request.timeout_ms))
            .send()
            .map_err(|e| {
                if e.is_timeout() {
                    Error::Timeout {
                        service: "http".into(),
                        message: format!("request timed out after {}ms", request.timeout_ms),
                    }
                } else {
                    Error::external("http", 502, format!("request failed: {e}"))
                }
            })?;

        let status = response.status().as_u16();
        if let Some(max) = request.max_bytes
            && let Some(len) = response.content_length()
            && len as usize > max
        {
            return Ok(HttpResponse {
                status,
                body: Vec::new(),
            });
        }
        let bytes = response.bytes().map_err(|e| {
            Error::external("http", 502, format!("failed to read response body: {e}"))
        })?;
        if request.max_bytes.is_some_and(|max| bytes.len() > max) {
            return Ok(HttpResponse {
                status,
                body: Vec::new(),
            });
        }
        Ok(HttpResponse {
            status,
            body: bytes.to_vec(),
        })
    }

    fn post(&self, request: &HttpRequest, body: &[u8]) -> Result<HttpResponse> {
        let mut builder = self.client.post(&request.url);
        for (name, value) in &request.headers {
            builder = builder.header(name, value);
        }
        let response = builder
            .timeout(Duration::from_millis(request.timeout_ms))
            .body(body.to_vec())
            .send()
            .map_err(|e| {
                if e.is_timeout() {
                    Error::Timeout {
                        service: "http".into(),
                        message: format!("request timed out after {}ms", request.timeout_ms),
                    }
                } else {
                    Error::external("http", 502, format!("request failed: {e}"))
                }
            })?;
        Ok(HttpResponse {
            status: response.status().as_u16(),
            body: response.bytes().map(|b| b.to_vec()).unwrap_or_default(),
        })
    }
}

#[derive(Clone)]
pub struct HttpClient {
    transport: Arc<dyn Transport>,
}

impl Default for HttpClient {
    fn default() -> Self {
        Self::new()
    }
}

impl HttpClient {
    pub fn new() -> Self {
        Self {
            transport: Arc::new(ReqwestTransport::default()),
        }
    }

    pub fn with_transport(transport: Arc<dyn Transport>) -> Self {
        Self { transport }
    }

    fn check_status(&self, service: &str, status: u16) -> Result<()> {
        if (200..300).contains(&status) {
            Ok(())
        } else {
            Err(Error::external(
                service,
                status,
                format!("{service} request failed ({status})"),
            ))
        }
    }

    /// GET a JSON body and deserialise it into the API response type.
    pub fn get_json<T: DeserializeOwned>(
        &self,
        service: &str,
        url: impl Into<String>,
        headers: &[(&str, &str)],
        timeout_ms: u64,
    ) -> Result<T> {
        let request = HttpRequest {
            url: url.into(),
            headers: headers
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
            timeout_ms,
            max_bytes: None,
        };
        let response = self.transport.get(&request)?;
        self.check_status(service, response.status)?;
        serde_json::from_slice(&response.body).map_err(|e| Error::SchemaParse {
            service: service.to_string(),
            message: format!("{service} returned malformed JSON: {e}"),
        })
    }

    /// GET an opaque byte body with a hard cap (images decoded by Rune's
    /// `ImageCache`). The cap is enforced by the transport as well as here, so
    /// a hostile upstream cannot stream an unbounded response into the UI.
    pub fn get_bytes_limited(
        &self,
        service: &str,
        url: impl Into<String>,
        headers: &[(&str, &str)],
        timeout_ms: u64,
        max_bytes: usize,
    ) -> Result<Vec<u8>> {
        let request = HttpRequest {
            url: url.into(),
            headers: headers
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
            timeout_ms,
            max_bytes: Some(max_bytes),
        };
        let response = self.transport.get(&request)?;
        self.check_status(service, response.status)?;
        if response.body.len() > max_bytes {
            return Err(Error::external(
                service,
                502,
                format!("{service} image exceeded {max_bytes} bytes"),
            ));
        }
        Ok(response.body)
    }

    /// GET a text body with a hard byte cap (RSS feeds).
    pub fn get_text_limited(
        &self,
        service: &str,
        url: impl Into<String>,
        headers: &[(&str, &str)],
        timeout_ms: u64,
        max_bytes: usize,
    ) -> Result<String> {
        let request = HttpRequest {
            url: url.into(),
            headers: headers
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
            timeout_ms,
            max_bytes: Some(max_bytes),
        };
        let response = self.transport.get(&request)?;
        self.check_status(service, response.status)?;
        if response.body.len() > max_bytes {
            return Err(Error::external(
                service,
                502,
                format!("{service} response exceeded {max_bytes} bytes"),
            ));
        }
        Ok(String::from_utf8_lossy(&response.body).into_owned())
    }

    /// POST a JSON body and check for a 2xx response (Neon SQL over HTTP).
    pub fn post_json(
        &self,
        service: &str,
        url: impl Into<String>,
        headers: &[(&str, &str)],
        body: &serde_json::Value,
        timeout_ms: u64,
    ) -> Result<HttpResponse> {
        let request = HttpRequest {
            url: url.into(),
            headers: headers
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
            timeout_ms,
            max_bytes: None,
        };
        let payload = serde_json::to_vec(body).map_err(|e| Error::SchemaParse {
            service: service.to_string(),
            message: format!("encode request body: {e}"),
        })?;
        let response = self.transport.post(&request, &payload)?;
        self.check_status(service, response.status)?;
        Ok(response)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct StaticTransport(HttpResponse);
    impl Transport for StaticTransport {
        fn get(&self, _: &HttpRequest) -> Result<HttpResponse> {
            Ok(self.0.clone())
        }
    }

    #[test]
    fn json_success_and_error_mapping() {
        let client = HttpClient::with_transport(Arc::new(StaticTransport(HttpResponse {
            status: 200,
            body: br#"{"ok":true}"#.to_vec(),
        })));
        let value: serde_json::Value = client
            .get_json("test", "https://example.test", &[], DEFAULT_TIMEOUT_MS)
            .unwrap();
        assert_eq!(value["ok"], true);
    }

    #[test]
    fn non_2xx_and_malformed_json_map_to_distinct_errors() {
        let client = HttpClient::with_transport(Arc::new(StaticTransport(HttpResponse {
            status: 503,
            body: vec![],
        })));
        let err = client
            .get_json::<serde_json::Value>("test", "https://example.test", &[], DEFAULT_TIMEOUT_MS)
            .unwrap_err();
        assert_eq!(err.status(), 503);
        assert!(matches!(err, Error::ExternalApi { .. }));

        let client = HttpClient::with_transport(Arc::new(StaticTransport(HttpResponse {
            status: 200,
            body: b"not json".to_vec(),
        })));
        let err = client
            .get_json::<serde_json::Value>("test", "https://example.test", &[], DEFAULT_TIMEOUT_MS)
            .unwrap_err();
        assert!(matches!(err, Error::SchemaParse { .. }));
    }
}
