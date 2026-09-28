//! Lossless adapter vocabulary. No expected value is read or computed here.
use ess_conformance::target::TargetError;
use ess_primitives::node::Node;
use serde::Serialize;
use std::collections::BTreeMap;

pub type Fields = BTreeMap<String, Node>;

pub fn unavailable(detail: impl ToString) -> TargetError {
    TargetError::unavailable("ER adapter", detail.to_string())
}

pub fn text<'a>(input: &'a Fields, name: &str) -> Result<&'a str, TargetError> {
    input
        .get(name)
        .and_then(Node::as_text)
        .ok_or_else(|| unavailable(format!("missing string input {name}")))
}

pub fn json<T: Serialize>(value: &T) -> Result<Node, TargetError> {
    // Preserve number tokens. ESS Node's generic JSON reader normalizes decimal values.
    serde_json::to_string(value)
        .map(Node::Text)
        .map_err(unavailable)
}

pub fn fields<const N: usize>(entries: [(&str, Node); N]) -> Fields {
    entries
        .into_iter()
        .map(|(key, value)| (key.to_owned(), value))
        .collect()
}

pub fn string(value: impl Into<String>) -> Node {
    Node::Text(value.into())
}

pub fn integer(value: i64) -> Node {
    Node::Number(value.into())
}

/// A task-owned temporary directory, removed by its owner after the provider is dropped.
pub struct TempRoot(pub std::path::PathBuf);

impl TempRoot {
    pub fn new() -> Result<Self, TargetError> {
        use std::sync::atomic::{AtomicU64, Ordering};
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "er-ess-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&root).map_err(unavailable)?;
        Ok(Self(root))
    }
}

impl Drop for TempRoot {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
