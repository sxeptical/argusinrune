//! HTTP clients and local config for Argus.
//!
//! Rune keeps the UI thread free with `rune::Latest<T>`: a handler calls
//! `request(key, |q| ...)` and the closure runs on one of Rune's shared worker
//! threads; the answer is picked up in `AppSpec::poll` on the UI thread and a
//! `Waker` (given to `AppSpec::setup`) asks Rune to call `poll` again. That
//! closure is synchronous, so a blocking HTTP client is sufficient — no async
//! runtime has to cross the Rune boundary. `tokio` is available here for
//! clients that need an async runtime of their own (see RUST_REWRITE.md).
//! API keys live in a local config file, not in the app binary.

pub mod client;

pub use client::ApiClient;
