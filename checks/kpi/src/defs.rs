//! `kpis.yaml`: what each KPI measures, its unit, which way is better and how much change is
//! noise. Timing ids may name `{store}` and list the providers they expand over.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// Which way a value should move.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Direction {
    /// Smaller is better.
    Lower,
    /// Larger is better.
    Higher,
    /// Tracked and reported; a change is shown but never fails the gate.
    None,
}

/// One counter as written in the file.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Counter {
    /// Stable id.
    pub id: String,
    /// The review finding the counter follows, when it follows one.
    #[serde(default)]
    pub finding: Option<String>,
    /// What is counted.
    pub measures: String,
    /// Unit.
    pub unit: String,
    /// Better direction.
    pub direction: Direction,
    /// Fixture and size.
    pub scenario: String,
    /// Allowed relative change before a verdict (0.02 is 2 %).
    #[serde(default)]
    pub tolerance: f64,
    /// Allowed absolute change on top of the relative one.
    #[serde(default)]
    pub slack: u64,
    /// Present only with `ENTITY_POSTGRES_URL`.
    #[serde(default)]
    pub postgres: bool,
    /// Present only in a build with the `counters` feature.
    #[serde(default)]
    pub hooked: bool,
}

/// One timing KPI (or a family over providers) as written in the file.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Timing {
    /// Stable id; `{store}` expands over `stores`.
    pub id: String,
    /// The review findings whose paths this times.
    #[serde(default)]
    pub findings: Vec<String>,
    /// What is timed.
    pub measures: String,
    /// Unit.
    pub unit: String,
    /// Better direction.
    pub direction: Direction,
    /// Fixture and size.
    pub scenario: String,
    /// Providers `{store}` expands over.
    #[serde(default)]
    pub stores: Vec<String>,
    /// Whether `ab` fails on it.
    pub gate: bool,
    /// The worst head/base ratio (in the bad direction) `ab` accepts.
    #[serde(default = "default_ratio")]
    pub max_ratio: f64,
}

fn default_ratio() -> f64 {
    2.0
}

/// The whole file.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Definitions {
    /// Format version.
    pub version: u32,
    /// Deterministic counters.
    pub counters: Vec<Counter>,
    /// Timing KPIs.
    pub timing: Vec<Timing>,
}

/// Timing KPIs with `{store}` expanded.
pub struct Expanded {
    /// Counters by id.
    pub counters: BTreeMap<String, Counter>,
    /// Timing by id.
    pub timing: BTreeMap<String, Timing>,
}

/// The committed definitions.
pub const SOURCE: &str = include_str!("../kpis.yaml");

/// Parses and expands the committed definitions.
pub fn load() -> Expanded {
    let definitions: Definitions = serde_yaml_ng::from_str(SOURCE).expect("kpis.yaml parses");
    assert_eq!(definitions.version, 1, "kpis.yaml version");
    let mut counters = BTreeMap::new();
    for counter in definitions.counters {
        let previous = counters.insert(counter.id.clone(), counter);
        assert!(previous.is_none(), "kpis.yaml names a counter twice");
    }
    let mut timing = BTreeMap::new();
    for family in definitions.timing {
        let ids: Vec<(String, Option<&String>)> = if family.id.contains("{store}") {
            family
                .stores
                .iter()
                .map(|store| (family.id.replace("{store}", store), Some(store)))
                .collect()
        } else {
            vec![(family.id.clone(), None)]
        };
        for (id, store) in ids {
            let mut entry = family.clone();
            entry.id.clone_from(&id);
            if store == Some(&"postgres".to_owned()) {
                entry.gate = entry.gate && std::env::var_os("ENTITY_POSTGRES_URL").is_some();
            }
            let previous = timing.insert(id, entry);
            assert!(previous.is_none(), "kpis.yaml names a timing KPI twice");
        }
    }
    Expanded { counters, timing }
}
