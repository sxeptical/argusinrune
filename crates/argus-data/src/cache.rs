//! In-memory TTL cache with per-key producer deduplication and short negative
//! caching for failures — a port of `lib/cache.ts` minus the Workers L2 (there
//! is no isolate boundary in a desktop app).
//!
//! A caller that arrives while a producer is in flight waits on the condvar
//! and reuses its result, so a cold feed is fetched once even when several
//! Rune `Latest` closures ask for it at the same time.

use std::{
    collections::HashMap,
    sync::{Condvar, Mutex, MutexGuard},
    time::{Duration, Instant},
};

use serde::{Serialize, de::DeserializeOwned};

use crate::error::{Error, Result};

const MAX_ENTRIES: usize = 500;
const NEGATIVE_TTL: Duration = Duration::from_secs(10);
const NEGATIVE_TTL_MAX: Duration = Duration::from_secs(60);

struct Entry {
    state: Result<serde_json::Value>,
    timestamp: Instant,
    ttl: Duration,
}

#[derive(Default)]
struct Inner {
    entries: HashMap<String, Entry>,
    in_flight: std::collections::HashSet<String>,
}

/// Clears the in-flight flag even if the producer panics.
struct InFlightGuard<'a> {
    inner: &'a Mutex<Inner>,
    cv: &'a Condvar,
    key: String,
}

impl Drop for InFlightGuard<'_> {
    fn drop(&mut self) {
        let mut inner = self.inner.lock().unwrap();
        inner.in_flight.remove(&self.key);
        self.cv.notify_all();
    }
}

#[derive(Default)]
pub struct Cache {
    inner: Mutex<Inner>,
    cv: Condvar,
}

impl Cache {
    pub fn new() -> Self {
        Self::default()
    }

    fn negative_ttl(error: &Error, override_ttl: Option<Duration>) -> Duration {
        let ttl = override_ttl.unwrap_or({
            if matches!(error, Error::ExternalApi { status: 429, .. }) {
                NEGATIVE_TTL_MAX
            } else {
                NEGATIVE_TTL
            }
        });
        ttl.min(NEGATIVE_TTL_MAX)
    }

    fn store(&self, inner: &mut MutexGuard<'_, Inner>, key: &str, entry: Entry) {
        if inner.entries.len() >= MAX_ENTRIES && !inner.entries.contains_key(key) {
            let oldest = inner
                .entries
                .iter()
                .min_by_key(|(_, e)| e.timestamp)
                .map(|(k, _)| k.clone());
            if let Some(oldest) = oldest {
                inner.entries.remove(&oldest);
            }
        }
        inner.entries.insert(key.to_string(), entry);
    }

    /// Return a fresh value, or run `producer` once and cache the result.
    pub fn get<T>(
        &self,
        key: &str,
        ttl: Duration,
        producer: impl FnOnce() -> Result<T>,
    ) -> Result<T>
    where
        T: Serialize + DeserializeOwned,
    {
        self.get_with(key, ttl, |_| ttl, producer)
    }

    /// Like `get`, but the successful entry's TTL can depend on the value
    /// (weather uses a shorter TTL for partial snapshots).
    pub fn get_with<T, F>(
        &self,
        key: &str,
        ttl: Duration,
        ttl_for_value: F,
        producer: impl FnOnce() -> Result<T>,
    ) -> Result<T>
    where
        T: Serialize + DeserializeOwned,
        F: Fn(&T) -> Duration,
    {
        loop {
            let mut inner = self.inner.lock().unwrap();
            if let Some(entry) = inner.entries.get(key)
                && entry.timestamp.elapsed() < entry.ttl.min(ttl)
            {
                return match &entry.state {
                    Ok(value) => {
                        serde_json::from_value(value.clone()).map_err(|e| Error::SchemaParse {
                            service: "cache".into(),
                            message: format!("decode cached {key}: {e}"),
                        })
                    }
                    Err(err) => Err(err.clone()),
                };
            }
            if inner.in_flight.contains(key) {
                // Another worker is producing this key; wait for its answer.
                inner = self.cv.wait(inner).unwrap();
                continue;
            }
            inner.in_flight.insert(key.to_string());
            drop(inner);
            let _guard = InFlightGuard {
                inner: &self.inner,
                cv: &self.cv,
                key: key.to_string(),
            };

            let produced = producer();
            let mut inner = self.inner.lock().unwrap();
            let entry = match &produced {
                Ok(value) => {
                    let entry_ttl = ttl_for_value(value);
                    // Serialise to a JSON value for type-erased storage.
                    match serde_json::to_value(value) {
                        Ok(json) => Entry {
                            state: Ok(json),
                            timestamp: Instant::now(),
                            ttl: entry_ttl,
                        },
                        Err(_) => return produced,
                    }
                }
                Err(err) => Entry {
                    state: Err(err.clone()),
                    timestamp: Instant::now(),
                    ttl: Self::negative_ttl(err, None),
                },
            };
            self.store(&mut inner, key, entry);
            return produced;
        }
    }

    /// Store a value with an explicit TTL (open-ended when `None`).
    pub fn set_with_ttl<T: Serialize>(&self, key: &str, value: &T, ttl: Option<Duration>) {
        let json = match serde_json::to_value(value) {
            Ok(v) => v,
            Err(_) => return,
        };
        let mut inner = self.inner.lock().unwrap();
        self.store(
            &mut inner,
            key,
            Entry {
                state: Ok(json),
                timestamp: Instant::now(),
                ttl: ttl.unwrap_or(Duration::MAX),
            },
        );
    }

    pub fn set<T: Serialize>(&self, key: &str, value: &T) {
        self.set_with_ttl(key, value, None);
    }

    /// Read a fresh value without running a producer.
    pub fn peek<T: DeserializeOwned>(&self, key: &str, max_age: Duration) -> Option<T> {
        let inner = self.inner.lock().unwrap();
        let entry = inner.entries.get(key)?;
        if entry.state.is_err() || entry.timestamp.elapsed() >= max_age {
            return None;
        }
        entry
            .state
            .as_ref()
            .ok()
            .and_then(|v| serde_json::from_value(v.clone()).ok())
    }

    pub fn clear(&self, key: Option<&str>) {
        let mut inner = self.inner.lock().unwrap();
        match key {
            Some(key) => {
                inner.entries.remove(key);
                inner.in_flight.remove(key);
            }
            None => {
                inner.entries.clear();
                inner.in_flight.clear();
            }
        }
        self.cv.notify_all();
    }

    pub fn len(&self) -> usize {
        self.inner.lock().unwrap().entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };

    #[test]
    fn caches_success_and_evicts_after_ttl() {
        let cache = Cache::new();
        let calls = AtomicUsize::new(0);
        let get = || {
            cache.get("k", Duration::from_millis(60), || {
                calls.fetch_add(1, Ordering::SeqCst);
                Ok::<_, Error>(42)
            })
        };
        assert_eq!(get().unwrap(), 42);
        assert_eq!(get().unwrap(), 42);
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        std::thread::sleep(Duration::from_millis(80));
        assert_eq!(get().unwrap(), 42);
        assert_eq!(calls.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn negative_errors_are_cached_briefly() {
        let cache = Cache::new();
        let calls = AtomicUsize::new(0);
        let get = || {
            cache.get("k", Duration::from_secs(60), || {
                calls.fetch_add(1, Ordering::SeqCst);
                Err::<(), _>(Error::external("x", 503, "down"))
            })
        };
        assert!(get().is_err());
        assert!(get().is_err());
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        cache.clear(None);
        assert!(get().is_err());
        assert_eq!(calls.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn value_dependent_ttl_and_snapshot_peek() {
        let cache = Cache::new();
        cache.set("snapshot", &vec![1, 2, 3]);
        assert_eq!(
            cache.peek::<Vec<i32>>("snapshot", Duration::from_secs(1)),
            Some(vec![1, 2, 3])
        );

        let got = cache
            .get_with(
                "weather",
                Duration::from_millis(80),
                |partial: &bool| {
                    if *partial {
                        Duration::from_millis(30)
                    } else {
                        Duration::from_millis(80)
                    }
                },
                || Ok::<_, Error>(true),
            )
            .unwrap();
        assert!(got);
        std::thread::sleep(Duration::from_millis(45));
        let calls = AtomicUsize::new(0);
        let _ = cache.get("weather", Duration::from_millis(80), || {
            calls.fetch_add(1, Ordering::SeqCst);
            Ok::<_, Error>(true)
        });
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn concurrent_callers_share_one_producer() {
        let cache = Arc::new(Cache::new());
        let calls = Arc::new(AtomicUsize::new(0));
        let mut handles = vec![];
        for _ in 0..6 {
            let cache = Arc::clone(&cache);
            let calls = Arc::clone(&calls);
            handles.push(std::thread::spawn(move || {
                cache
                    .get("shared", Duration::from_secs(5), || {
                        calls.fetch_add(1, Ordering::SeqCst);
                        std::thread::sleep(Duration::from_millis(20));
                        Ok::<_, Error>(7)
                    })
                    .unwrap()
            }));
        }
        for h in handles {
            assert_eq!(h.join().unwrap(), 7);
        }
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }
}
