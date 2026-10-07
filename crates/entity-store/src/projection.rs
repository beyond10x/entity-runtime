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

use entity_core::{EntityDefinition, EntityInstance, FieldDefinition, FieldKind, ObjectSchema};
use serde_json::{Map, Value};

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
            if let Some(key) = key_of(&projection.key, instance, definition) {
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
///
/// A `$fields` path is read the way the kernel reads the same address of the same instance, so a
/// key registration admits is one the read model files instances under (R-167): under `kernel/1`
/// through object members only, and under the service rules through the collection address forms
/// as well, with the declarations the definition's schema gives each step.
fn key_of(
    reference: &str,
    instance: &EntityInstance,
    definition: &EntityDefinition,
) -> Option<String> {
    let value = match reference {
        "$state" => instance.lifecycle_state.clone(),
        "$id" => instance.id.clone(),
        "$entity" => instance.entity.clone(),
        "$version" => instance.version.to_string(),
        other => {
            let path = other.strip_prefix("$fields.")?;
            if definition.semantics.has_service_semantics() {
                return match service_lookup(&instance.fields, path, &definition.schema)? {
                    Read::Held(value) => spelled(value),
                    Read::Count(count) => Some(count.to_string()),
                };
            }
            let mut parts = path.split('.');
            let first = parts.next()?;
            let mut value = instance.fields.get(first)?;
            for part in parts {
                value = value.as_object()?.get(part)?;
            }
            return spelled(value);
        }
    };
    Some(value)
}

/// How a resolved value is spelled as a key.
fn spelled(value: &Value) -> Option<String> {
    match value {
        Value::String(text) => Some(text.clone()),
        Value::Null => None,
        // Numbers and booleans group perfectly well; anything structural does not have one obvious
        // spelling, so it is left out rather than given an arbitrary one.
        value @ (Value::Number(_) | Value::Bool(_)) => Some(value.to_string()),
        _ => None,
    }
}

/// What a service address resolves to: a value the instance holds, or the size of a collection,
/// which nothing holds and so is not borrowed from the instance.
enum Read<'a> {
    Held(&'a Value),
    Count(usize),
}

// --- The service rules' address walk --------------------------------------------------------------
//
// A copy of the kernel's (`entity-core` `runtime.rs` `lookup`, `walk`, `collection_address` and
// `selected_variant`), which is private to the kernel. Copied rather than approximated: a key the
// store reads differently from the kernel is a read model that disagrees with every rule over the
// same address. `crates/entity-store/tests/projections.rs` holds the copy to the kernel's answer
// for every form, so a change on either side that the other does not make fails there.
//
// One deliberate difference: the kernel reads a declared text's length, and this walk does not.
// Registration refuses `<text>.count` as a projection key for that reason (R-161), so no
// registered definition reaches it.

/// Resolves one `$fields` path from the instance's fields, with each field's declaration.
fn service_lookup<'a>(
    fields: &'a Map<String, Value>,
    path: &str,
    schema: &'a ObjectSchema,
) -> Option<Read<'a>> {
    let (first, rest) = split(path);
    if first.is_empty() {
        return None;
    }
    let value = fields.get(first)?;
    let field = schema.fields.get(first);
    match rest {
        None => Some(Read::Held(value)),
        Some(rest) => service_walk(value, rest, field, field.is_some()),
    }
}

/// Walks the rest of a path from one value, with the declared field it came from where there is
/// one. `checked` turns false past a union's content key, as in the kernel's walk.
fn service_walk<'a>(
    value: &'a Value,
    path: &str,
    field: Option<&'a FieldDefinition>,
    checked: bool,
) -> Option<Read<'a>> {
    let (segment, rest) = split(path);
    let declared = field.map(|field| field.kind);

    // The kernel reads a checked text's length here and nothing else; this walk reads neither.
    if checked && declared == Some(FieldKind::String) {
        return None;
    }
    match value {
        // A declared map's size, and nothing else it holds. A count has no members.
        Value::Object(members) if declared == Some(FieldKind::Map) && segment == "count" => {
            return rest.is_none().then_some(Read::Count(members.len()));
        }
        Value::Array(values) if segment == "count" => {
            return rest.is_none().then_some(Read::Count(values.len()));
        }
        // `0`, or a digit string with no leading zero; an array has nothing else to address.
        Value::Array(values) => {
            if segment != "0" && segment.starts_with('0') {
                return None;
            }
            let element = values.get(segment.parse::<usize>().ok()?)?;
            let items = field.and_then(|field| field.items.as_deref());
            return match rest {
                None => Some(Read::Held(element)),
                Some(rest) => service_walk(element, rest, items, checked),
            };
        }
        _ => {}
    }
    // A declared map's keys are not addressable, so nothing but its size resolves.
    if declared == Some(FieldKind::Map) {
        return None;
    }

    let next = value.as_object()?.get(segment)?;
    let next_field = field.and_then(|field| match field.kind {
        FieldKind::Object => field.properties.get(segment),
        FieldKind::Union => selected_variant(field, value, segment),
        _ => None,
    });
    let checked = checked && declared != Some(FieldKind::Union);
    match rest {
        None => Some(Read::Held(next)),
        Some(rest) => service_walk(next, rest, next_field, checked),
    }
}

/// The variant a union value's tag selects, where `segment` is the derived content key: `value`,
/// or `content` when the tag is itself `value`.
fn selected_variant<'a>(
    field: &'a FieldDefinition,
    value: &Value,
    segment: &str,
) -> Option<&'a FieldDefinition> {
    let content = if field.tag.as_deref() == Some("value") {
        "content"
    } else {
        "value"
    };
    if segment != content {
        return None;
    }
    let tag = field.tag.as_deref().unwrap_or("kind");
    let label = value.as_object()?.get(tag)?.as_str()?;
    field.variants.get(label)
}

/// The first segment of a path, and the rest after the first `.`, if there is one.
fn split(path: &str) -> (&str, Option<&str>) {
    match path.split_once('.') {
        Some((first, rest)) => (first, Some(rest)),
        None => (path, None),
    }
}
