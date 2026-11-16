//! Visitor feedback persistence via Neon's HTTP SQL endpoint.
//!
//! The TS build used `@neondatabase/serverless`, which is one POST per query:
//! `POST https://<host>/sql` with `Neon-Connection-String: <DATABASE_URL>` and
//! `{"query": ..., "params": [...]}`. That maps cleanly onto `reqwest::blocking`
//! with no Postgres driver, so the desktop app keeps the same table
//! (`scripts/sql/feedback.sql`). With no `DATABASE_URL`, submission returns a
//! clear `NotConfigured` error and the source reports "unconfigured".

use crate::{
    Client, Result,
    error::Error,
    health::Source,
};

pub const FEEDBACK_COUNTRY: &str = "SG";
pub const FEEDBACK_MESSAGE_MAX_LENGTH: usize = 2000;
const FEEDBACK_TIMEOUT_MS: u64 = 10_000;

#[derive(Clone, Debug, PartialEq)]
pub struct FeedbackSubmission {
    pub rating: i32,
    /// Empty comments are stored as SQL NULL, matching the TS transform.
    pub message: Option<String>,
}

impl FeedbackSubmission {
    pub fn validate(rating: i32, message: Option<String>) -> Result<Self> {
        if !(1..=5).contains(&rating) {
            return Err(Error::BadRequest {
                message: "Feedback needs a rating from 1 to 5".into(),
            });
        }
        let message = message.map(|s| s.trim().to_string()).filter(|s| !s.is_empty());
        if message.as_ref().is_some_and(|s| s.chars().count() > FEEDBACK_MESSAGE_MAX_LENGTH) {
            return Err(Error::BadRequest {
                message: "Feedback comment is too long".into(),
            });
        }
        Ok(Self { rating, message })
    }
}

/// Derive Neon's SQL-over-HTTP endpoint from a Postgres connection string.
pub fn neon_sql_endpoint(database_url: &str) -> Option<String> {
    let after_scheme = database_url
        .strip_prefix("postgresql://")
        .or_else(|| database_url.strip_prefix("postgres://"))?;
    let authority = after_scheme.split('/').next()?;
    let host = authority.rsplit('@').next()?;
    if host.is_empty() {
        return None;
    }
    Some(format!("https://{host}/sql"))
}

impl Client {
    pub fn submit_feedback(&self, submission: FeedbackSubmission) -> Result<()> {
        let result = self.submit_feedback_inner(submission);
        match &result {
            Ok(()) => self.health.mark_ok(Source::Feedback),
            Err(e) => self.health.mark_error(Source::Feedback, e),
        }
        result
    }

    fn submit_feedback_inner(&self, submission: FeedbackSubmission) -> Result<()> {
        let url = self.config.database_url()?;
        let endpoint = neon_sql_endpoint(url).ok_or_else(|| {
            Error::external("feedback", 502, "DATABASE_URL is not a Neon Postgres URL")
        })?;
        let body = serde_json::json!({
            "query": "INSERT INTO feedback (rating, message, country) VALUES ($1, $2, $3)",
            "params": [submission.rating, submission.message, FEEDBACK_COUNTRY],
        });
        self.http
            .post_json(
                "feedback",
                endpoint,
                &[
                    ("Content-Type", "application/json"),
                    ("Neon-Connection-String", url),
                    ("Accept", "application/json"),
                ],
                &body,
                FEEDBACK_TIMEOUT_MS,
            )
            .map_err(|e| match e {
                Error::NotConfigured { .. } => e,
                _ => Error::external("feedback", 503, "Feedback insert failed"),
            })?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_rating_and_message() {
        assert!(FeedbackSubmission::validate(0, None).is_err());
        assert!(FeedbackSubmission::validate(6, None).is_err());
        let ok = FeedbackSubmission::validate(5, Some("  great  ".into())).unwrap();
        assert_eq!(ok.message.as_deref(), Some("great"));
        let empty = FeedbackSubmission::validate(1, Some(" ".into())).unwrap();
        assert_eq!(empty.message, None);
    }

    #[test]
    fn neon_endpoint_uses_host_only() {
        let url = "postgresql://user:pass@ep-cool-name.ap-southeast-1.aws.neon.tech/argus?sslmode=require";
        assert_eq!(
            neon_sql_endpoint(url).as_deref(),
            Some("https://ep-cool-name.ap-southeast-1.aws.neon.tech/sql")
        );
        assert!(neon_sql_endpoint("not a url").is_none());
    }
}
