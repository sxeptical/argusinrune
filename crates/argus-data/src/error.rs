//! Error types ported from `lib/errors.ts`.
//!
//! The TS route helper mapped these onto HTTP status codes. Here the app is a
//! desktop binary, so the same variants carry enough information for the UI to
//! distinguish an outage from a misconfiguration; `status()` mirrors the old
//! HTTP code for callers that want it.

use std::fmt;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error {
    /// An upstream API failed or answered with a non-2xx status.
    ExternalApi {
        service: String,
        status: u16,
        message: String,
    },
    /// An upstream body was not JSON or did not match its expected shape.
    SchemaParse { service: String, message: String },
    /// The request outlived its deadline.
    Timeout { service: String, message: String },
    /// A well-formed request whose parameters failed validation.
    BadRequest { message: String },
    /// A valid request the visitor is not allowed to make.
    Forbidden { message: String },
    /// A required API key is missing or still a placeholder value.
    NotConfigured { service: String, message: String },
}

impl Error {
    pub fn external(service: impl Into<String>, status: u16, message: impl Into<String>) -> Self {
        Error::ExternalApi {
            service: service.into(),
            status,
            message: message.into(),
        }
    }

    pub fn service(&self) -> &str {
        match self {
            Error::ExternalApi { service, .. }
            | Error::SchemaParse { service, .. }
            | Error::Timeout { service, .. }
            | Error::NotConfigured { service, .. } => service,
            Error::BadRequest { .. } | Error::Forbidden { .. } => "request",
        }
    }

    /// The HTTP code the old Next.js route would have returned.
    pub fn status(&self) -> u16 {
        match self {
            Error::ExternalApi { status, .. } => *status,
            Error::SchemaParse { .. } => 502,
            Error::Timeout { .. } => 504,
            Error::BadRequest { .. } => 400,
            Error::Forbidden { .. } => 403,
            Error::NotConfigured { .. } => 500,
        }
    }

    pub fn message(&self) -> &str {
        match self {
            Error::ExternalApi { message, .. }
            | Error::SchemaParse { message, .. }
            | Error::Timeout { message, .. }
            | Error::BadRequest { message, .. }
            | Error::Forbidden { message, .. }
            | Error::NotConfigured { message, .. } => message,
        }
    }

    /// True for a 404 — callers such as the camera and pagination clients
    /// treat that as "nothing here", not an outage.
    pub fn is_not_found(&self) -> bool {
        matches!(self, Error::ExternalApi { status: 404, .. })
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.service(), self.message())
    }
}

impl std::error::Error for Error {}

pub type Result<T> = std::result::Result<T, Error>;
