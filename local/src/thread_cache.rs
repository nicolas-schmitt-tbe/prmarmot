//! TTL cache for the bounded thread follow-up only. Board searches stay live.
use prmarmot_core::{
    attention::SnapshotNamespace,
    github::{query::TURNS_QUERY, GhError, GithubTransport},
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    path::Path,
    sync::{Arc, Mutex},
    time::{SystemTime, UNIX_EPOCH},
};

const MAX_BYTES: u64 = 8 * 1024 * 1024;
const MAX_ENTRIES: usize = 512;

#[derive(Serialize, Deserialize)]
struct Entry {
    fetched_at: u64,
    node: Value,
}
#[derive(Serialize, Deserialize)]
struct Cache {
    version: u8,
    namespace: SnapshotNamespace,
    entries: BTreeMap<String, Entry>,
}

pub struct ThreadCacheTransport {
    inner: Arc<dyn GithubTransport>,
    namespace: SnapshotNamespace,
    lock: Mutex<()>,
}

impl ThreadCacheTransport {
    pub fn new(inner: Arc<dyn GithubTransport>, namespace: SnapshotNamespace) -> Self {
        Self {
            inner,
            namespace,
            lock: Mutex::new(()),
        }
    }

    fn cached(
        &self,
        variables: &[(&str, &str)],
        ids: &[String],
        path: &Path,
        ttl: u64,
        now: u64,
    ) -> Result<Value, GhError> {
        // Queue switches can overlap. Serialize follow-ups so one refresh can
        // use another's result, and atomic writers cannot race on the same file.
        let _guard = self.lock.lock().unwrap_or_else(|error| error.into_inner());
        let mut cache = std::fs::metadata(path)
            .ok()
            .filter(|meta| meta.len() <= MAX_BYTES)
            .and_then(|_| std::fs::read(path).ok())
            .and_then(|bytes| serde_json::from_slice::<Cache>(&bytes).ok())
            .filter(|cache| cache.version == 1 && cache.namespace == self.namespace)
            .unwrap_or_else(|| Cache {
                version: 1,
                namespace: self.namespace.clone(),
                entries: BTreeMap::new(),
            });
        cache.entries.retain(|_, entry| {
            now.checked_sub(entry.fetched_at)
                .is_some_and(|age| age < ttl)
        });
        let missing: Vec<String> = ids
            .iter()
            .filter(|id| !cache.entries.contains_key(*id))
            .cloned()
            .collect();
        let mut rate = Value::Null;
        let mut fresh = BTreeMap::new();
        if !missing.is_empty() {
            let body = self
                .inner
                .graphql_with_ids(TURNS_QUERY, variables, &missing)?;
            if body
                .get("errors")
                .and_then(Value::as_array)
                .is_some_and(|errors| !errors.is_empty())
            {
                return Ok(body); // Never cache errors or partial error responses.
            }
            let Some(nodes) = body.pointer("/data/turns").and_then(Value::as_array) else {
                return Ok(body);
            };
            rate = body
                .pointer("/data/rateLimit")
                .cloned()
                .unwrap_or(Value::Null);
            for node in nodes {
                if let Some(id) = node
                    .get("id")
                    .and_then(Value::as_str)
                    .filter(|id| missing.iter().any(|requested| requested == id))
                {
                    if node
                        .pointer("/reviewThreads/nodes")
                        .and_then(Value::as_array)
                        .is_some()
                    {
                        fresh.insert(id.to_owned(), node.clone());
                        cache.entries.insert(
                            id.to_owned(),
                            Entry {
                                fetched_at: now,
                                node: node.clone(),
                            },
                        );
                    }
                }
            }
        }
        let nodes: Vec<Value> = ids
            .iter()
            .map(|id| {
                fresh
                    .get(id)
                    .cloned()
                    .or_else(|| cache.entries.get(id).map(|entry| entry.node.clone()))
                    .unwrap_or(Value::Null)
            })
            .collect();
        if !missing.is_empty() {
            while cache.entries.len() > MAX_ENTRIES {
                let oldest = cache
                    .entries
                    .iter()
                    .min_by_key(|(_, entry)| entry.fetched_at)
                    .map(|(id, _)| id.clone())
                    .unwrap();
                cache.entries.remove(&oldest);
            }
            if let Ok(bytes) = serde_json::to_vec(&cache) {
                if bytes.len() as u64 <= MAX_BYTES {
                    // Cache failure must not turn a successful GitHub request into an error.
                    let _ = crate::attention_state::atomic_write(path, &bytes);
                }
            }
        }
        // Cached responses never replay an old rate-limit balance or cost.
        Ok(json!({"data": {"turns": nodes, "rateLimit": rate}}))
    }
}

impl GithubTransport for ThreadCacheTransport {
    fn graphql(&self, query: &str, variables: &[(&str, &str)]) -> Result<Value, GhError> {
        self.inner.graphql(query, variables)
    }
    fn graphql_with_ids(
        &self,
        query: &str,
        variables: &[(&str, &str)],
        ids: &[String],
    ) -> Result<Value, GhError> {
        if query != TURNS_QUERY {
            return self.inner.graphql_with_ids(query, variables, ids);
        }
        let file = crate::config::load();
        let ttl = file.thread_refresh_secs.unwrap_or(600).max(30);
        let name = crate::attention_state::namespaced_path("threads", &self.namespace);
        let path = crate::config::cache_root().join(name.file_name().unwrap());
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        self.cached(variables, ids, &path, ttl, now)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Fake(Mutex<Vec<Vec<String>>>);
    impl GithubTransport for Fake {
        fn graphql(&self, _: &str, _: &[(&str, &str)]) -> Result<Value, GhError> {
            panic!("unexpected query")
        }
        fn graphql_with_ids(
            &self,
            _: &str,
            _: &[(&str, &str)],
            ids: &[String],
        ) -> Result<Value, GhError> {
            self.0.lock().unwrap().push(ids.to_vec());
            Ok(
                json!({"data":{"turns": ids.iter().map(|id| json!({"id":id,"reviewThreads":{"nodes":[]}})).collect::<Vec<_>>(), "rateLimit":{"remaining":123,"cost":2}}}),
            )
        }
    }
    #[test]
    fn hits_partial_misses_expiry_and_account_isolation() {
        let path = std::env::temp_dir().join(format!(
            "prmarmot-thread-cache-test-{}.json",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&path);
        let fake = Arc::new(Fake(Mutex::new(vec![])));
        let cache =
            ThreadCacheTransport::new(fake.clone(), SnapshotNamespace::new("github.com", "one"));
        cache.cached(&[], &["A".into()], &path, 60, 100).unwrap();
        let hit = cache.cached(&[], &["A".into()], &path, 60, 110).unwrap();
        assert!(hit["data"]["rateLimit"].is_null());
        cache
            .cached(&[], &["A".into(), "B".into()], &path, 60, 120)
            .unwrap();
        cache.cached(&[], &["A".into()], &path, 60, 160).unwrap();
        let other =
            ThreadCacheTransport::new(fake.clone(), SnapshotNamespace::new("github.com", "two"));
        other.cached(&[], &["A".into()], &path, 60, 161).unwrap();
        assert_eq!(
            *fake.0.lock().unwrap(),
            vec![vec!["A"], vec!["B"], vec!["A"], vec!["A"]]
        );
        std::fs::remove_file(path).unwrap();
    }
    #[test]
    fn failures_are_not_cached_and_corrupt_files_are_ignored() {
        struct Failing(Mutex<usize>);
        impl GithubTransport for Failing {
            fn graphql(&self, _: &str, _: &[(&str, &str)]) -> Result<Value, GhError> {
                unreachable!()
            }
            fn graphql_with_ids(
                &self,
                _: &str,
                _: &[(&str, &str)],
                _: &[String],
            ) -> Result<Value, GhError> {
                *self.0.lock().unwrap() += 1;
                Ok(json!({"errors":[{"message":"timeout"}], "data":{"turns":[]}}))
            }
        }
        let path = std::env::temp_dir().join(format!(
            "prmarmot-thread-cache-errors-{}.json",
            std::process::id()
        ));
        std::fs::write(&path, b"invalid json").unwrap();
        let fake = Arc::new(Failing(Mutex::new(0)));
        let cache =
            ThreadCacheTransport::new(fake.clone(), SnapshotNamespace::new("github.com", "one"));
        for now in [100, 110] {
            let body = cache.cached(&[], &["A".into()], &path, 60, now).unwrap();
            assert!(body.get("errors").is_some());
        }
        assert_eq!(*fake.0.lock().unwrap(), 2);
        assert_eq!(std::fs::read(&path).unwrap(), b"invalid json");
        std::fs::remove_file(path).unwrap();
    }
}
