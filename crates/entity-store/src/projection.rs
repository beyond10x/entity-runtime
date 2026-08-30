//! Read models, built by the shell from what it holds.
//!
//! A definition *declares* its projections and performs none of them. That is the same split as
//! everything else here: a projection reads across instances, and the kernel is handed one — so it
//! could not evaluate one even in principle, whatever the purity rules said.
//!
//! # What a projection is, and what it deliberately is not
//!
//! Group instances by something they hold, optionally over one lifecycle state. `by_status` is
//! `key: $state`; `open_per_customer` is `key: $fields.customer` with `in_state: open`. That is what
//! a read model is for, and it is the shape a store can build an index for.
//!
//! No filters beyond the state, no joins, no aggregates. The condition language grows operator by
//! operator and never into a language, and the same restraint applies here: a projection needing
//! arithmetic is a consumer's job, over what this hands it.
//!
//! # Ordering
//!
//! `BTreeMap` and `BTreeSet` throughout, so two runs over the same instances produce the same bytes.
//! A read model that reordered between runs would make every diff of one unreadable.

use std::collections::{BTreeMap, BTreeSet};

use entity_core::{EntityDefinition, EntityInstance};

/// One read model: a key, and the instance identities under it.
pub type Grouping = BTreeMap<String, BTreeSet<String>>;

/// Every read model a definition declares, by name.
pub type Projections = BTreeMap<String, Grouping>;

/// Builds every projection the definition declares over `instances`.
///
/// An instance whose key resolves to nothing — a field it does not carry — is **left out** rather
/// than filed under an empty key. Absent is not a group: a bucket of instances that share only the
/// property of not having been classified is a bucket nobody can act on.
#[must_use]
pub fn project<'a>(
    definition: &EntityDefinition,
    instances: impl IntoIterator<Item = &'a EntityInstance>,
) -> Projections {
    let instances: Vec<&EntityInstance> = instances.into_iter().collect();
    let mut out = Projections::new();

    for (name, projection) in &definition.projections {
        let mut grouping = Grouping::new();
        for instance in &instances {
            if instance.entity != definition.entity || instance.version != definition.version {
                continue;
            }
            if let Some(state) = &projection.in_state {
                if &instance.lifecycle_state != state {
                    continue;
                }
            }
            if let Some(key) = key_of(&projection.key, instance) {
                grouping.entry(key).or_default().insert(instance.id.clone());
            }
        }
        out.insert(name.clone(), grouping);
    }
    out
}

/// Resolves a projection key against one instance.
///
/// Only the references a projection may name: `$state`, `$id`, `$entity`, `$version` and
/// `$fields.<name>` — the same set an invariant may read, which is what `validate_reference`
/// already refuses anything outside of at registration.
fn key_of(reference: &str, instance: &EntityInstance) -> Option<String> {
    let value = match reference {
        "$state" => instance.lifecycle_state.clone(),
        "$id" => instance.id.clone(),
        "$entity" => instance.entity.clone(),
        "$version" => instance.version.to_string(),
        other => {
            let path = other.strip_prefix("$fields.")?;
            let mut parts = path.split('.');
            let first = parts.next()?;
            let mut value = instance.fields.get(first)?;
            for part in parts {
                value = value.as_object()?.get(part)?;
            }
            match value {
                serde_json::Value::String(text) => text.clone(),
                serde_json::Value::Null => return None,
                // Numbers and booleans group perfectly well; anything structural does not have one
                // obvious spelling, so it is left out rather than given an arbitrary one.
                value @ (serde_json::Value::Number(_) | serde_json::Value::Bool(_)) => {
                    value.to_string()
                }
                _ => return None,
            }
        }
    };
    Some(value)
}
