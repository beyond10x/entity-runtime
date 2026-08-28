//! The suite a provider runs against itself.
//!
//! The SPI's value is that a caller can swap what is underneath. That is a claim about
//! **agreement**, and agreement checked against one implementation is not checked at all — so the
//! cases live here, in the crate that owns the traits, and every provider runs the same ones.
//!
//! It is a library function rather than a test module because the providers are not all in this
//! crate: `entity-sqlite` cannot be a dependency of `entity-store` without a cycle, so the suite
//! has to travel to the provider rather than the provider to the suite.
//!
//! # A suite that passes everything tells you nothing
//!
//! [`Broken`] is a provider that ignores the revision it was given. It exists so the suite can be
//! run against something that *should* fail, and `a_broken_provider_is_caught` asserts that it does
//! — because a conformance suite nobody has watched fail is a suite nobody knows the reach of.
//!
//! This is the same move `engineering-protocols` made at `0.2.0-wave-3` and has held since: prove
//! the checker before trusting the check.

use entity_core::{Decision, Registry, Runtime};
use serde_json::json;

use crate::{Expect, StateProvider, Store, StoreError};

/// What one case found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Outcome {
    /// The case's name.
    pub case: &'static str,
    /// What went wrong, or `None` when it held.
    pub failure: Option<String>,
}

impl Outcome {
    /// `true` when the case held.
    #[must_use]
    pub fn held(&self) -> bool {
        self.failure.is_none()
    }
}

/// What running the suite found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Report {
    /// Every case, in the order they ran.
    pub outcomes: Vec<Outcome>,
}

impl Report {
    /// Every case that did not hold.
    #[must_use]
    pub fn failures(&self) -> Vec<&Outcome> {
        self.outcomes.iter().filter(|o| !o.held()).collect()
    }

    /// `true` when every case held.
    #[must_use]
    pub fn is_clean(&self) -> bool {
        self.failures().is_empty()
    }

    /// A line per failing case, for an assertion message.
    #[must_use]
    pub fn summary(&self) -> String {
        self.failures()
            .iter()
            .map(|o| format!("  {}: {}", o.case, o.failure.as_deref().unwrap_or_default()))
            .collect::<Vec<_>>()
            .join("\n")
    }
}

/// The definition every case is driven through.
fn registry() -> Registry {
    let definition = serde_json::from_value(json!({
        "entity": "conformance-ticket",
        "version": 1,
        "schema": { "fields": { "title": { "type": "string", "required": true } } },
        "lifecycle": { "initial": "open", "states": ["open", "closed"] },
        "operations": {
            "close": {
                "transitions": [{ "from": "open", "to": "closed" }],
                "emits": [{ "type": "TicketClosed", "payload": { "ticket": "$id" } }]
            }
        }
    }))
    .expect("the conformance definition parses");
    let mut registry = Registry::new();
    registry
        .register(definition)
        .expect("the conformance definition validates");
    registry
}

/// Runs every case against `store`, under a fresh identity per case so cases cannot interfere.
///
/// The store may be pre-populated; each case uses an id derived from its own name.
pub fn run(store: &mut dyn Store) -> Report {
    let registry = registry();
    let mut outcomes = Vec::new();
    for case in CASES {
        let failure = (case.run)(store, &registry).err();
        outcomes.push(Outcome {
            case: case.name,
            failure,
        });
    }
    Report { outcomes }
}

struct Case {
    name: &'static str,
    run: fn(&mut dyn Store, &Registry) -> Result<(), String>,
}

const CASES: &[Case] = &[
    Case {
        name: "a committed instance reads back",
        run: reads_back,
    },
    Case {
        name: "state and events arrive together",
        run: together,
    },
    Case {
        name: "an observation lands at an unchanged revision, and so does the next",
        run: observations_land,
    },
    Case {
        name: "a stale write is refused",
        run: stale_refused,
    },
    Case {
        name: "a refused commit leaves no trace",
        run: refusal_is_inert,
    },
    Case {
        name: "a second creation of one identity is refused",
        run: second_creation_refused,
    },
    Case {
        name: "an instance nobody stored is absent",
        run: absent_is_not_an_error,
    },
    Case {
        name: "what a store holds is listed, sorted, and only that",
        run: holdings_are_listed,
    },
    Case {
        name: "an entity type nobody stored under lists nothing, not an error",
        run: nothing_stored_lists_nothing,
    },
    Case {
        name: "a refused commit adds nothing to the listing",
        run: refusal_lists_nothing,
    },
];

/// Creates `id` in `store` and returns the stored instance.
fn opened(
    store: &mut dyn Store,
    registry: &Registry,
    id: &str,
) -> Result<entity_core::EntityInstance, String> {
    let created = Runtime::new(registry)
        .create("conformance-ticket", 1, id, json!({ "title": "A ticket" }))
        .map_err(|error| format!("the kernel refused a creation: {error}"))?;
    store
        .commit(&created, Expect::Absent)
        .map_err(|error| format!("a creation into an empty store must be accepted: {error}"))?;
    Ok(created.instance)
}

fn reads_back(store: &mut dyn Store, registry: &Registry) -> Result<(), String> {
    let created = opened(store, registry, "reads-back")?;
    let loaded = store
        .load("conformance-ticket", "reads-back")
        .map_err(|error| format!("load failed: {error}"))?
        .ok_or("the instance was committed and is not there")?;
    if loaded != created {
        return Err("what was read back is not what was committed".to_owned());
    }
    Ok(())
}

fn together(store: &mut dyn Store, registry: &Registry) -> Result<(), String> {
    let created = opened(store, registry, "together")?;
    let closed = Runtime::new(registry)
        .execute(&created, "close", json!({}))
        .map_err(|error| format!("the kernel refused: {error}"))?;
    store
        .commit(&closed, Expect::Revision(1))
        .map_err(|error| format!("commit failed: {error}"))?;

    let loaded = store
        .load("conformance-ticket", "together")
        .map_err(|error| format!("load failed: {error}"))?
        .ok_or("the instance vanished")?;
    let events = store
        .events("conformance-ticket", "together")
        .map_err(|error| format!("event read failed: {error}"))?;

    if loaded.lifecycle_state != "closed" || loaded.revision != 2 {
        return Err(format!(
            "the state did not land: {} at revision {}",
            loaded.lifecycle_state, loaded.revision
        ));
    }
    if events.len() != 1 {
        return Err(format!(
            "the state moved but {} event(s) landed; state and events must arrive together",
            events.len()
        ));
    }
    Ok(())
}

/// An observation is a fact *about* an instance, not a change *to* it: the instance as it stands,
/// with one more event at its current revision. A store that keyed events by revision alone, or
/// dropped an event at a revision it had reached, would lose the second observation — or the
/// first — and a shell counting what was observed (evidence about a plan's story, say) would count
/// short with nothing saying so.
fn observations_land(store: &mut dyn Store, registry: &Registry) -> Result<(), String> {
    let created = opened(store, registry, "observed")?;
    let observed = |what: &str| {
        let args = serde_json::Map::from_iter([("what".to_owned(), json!(what))]);
        Decision {
            instance: created.clone(),
            events: vec![entity_core::DomainEvent {
                entity: created.entity.clone(),
                version: created.version,
                id: created.id.clone(),
                revision: created.revision,
                event_type: "TicketObserved".to_owned(),
                from_state: Some(created.lifecycle_state.clone()),
                to_state: created.lifecycle_state.clone(),
                changed: serde_json::Map::new(),
                args,
                payload: json!({ "what": what }),
            }],
        }
    };
    for what in ["first", "second"] {
        store
            .commit(&observed(what), Expect::Revision(created.revision))
            .map_err(|error| format!("the {what} observation was refused: {error}"))?;
    }

    let loaded = store
        .load("conformance-ticket", "observed")
        .map_err(|error| format!("load failed: {error}"))?
        .ok_or("the instance vanished")?;
    if loaded.revision != created.revision {
        return Err(format!(
            "an observation moved the revision to {}; it must stay at {}",
            loaded.revision, created.revision
        ));
    }
    let events = store
        .events("conformance-ticket", "observed")
        .map_err(|error| format!("event read failed: {error}"))?;
    let seen: Vec<&str> = events
        .iter()
        .filter(|event| event.event_type == "TicketObserved")
        .filter_map(|event| event.args.get("what").and_then(|what| what.as_str()))
        .collect();
    if seen != ["first", "second"] {
        return Err(format!(
            "two observations were committed and the log holds {seen:?}; an observation at a \
             revision the log has reached must still land, and in order"
        ));
    }
    Ok(())
}

fn stale_refused(store: &mut dyn Store, registry: &Registry) -> Result<(), String> {
    let created = opened(store, registry, "stale")?;
    let runtime = Runtime::new(registry);
    let first = runtime
        .execute(&created, "close", json!({}))
        .map_err(|error| error.to_string())?;
    let second = runtime
        .execute(&created, "close", json!({}))
        .map_err(|error| error.to_string())?;

    store
        .commit(&first, Expect::Revision(1))
        .map_err(|error| format!("the first writer must win: {error}"))?;

    match store.commit(&second, Expect::Revision(1)) {
        Err(StoreError::RevisionConflict { found, .. }) => {
            if found != Some(2) {
                return Err(format!(
                    "the conflict reported {found:?}, not the stored revision 2"
                ));
            }
            Ok(())
        }
        Ok(()) => {
            Err("a write from a stale revision was accepted; the first write is lost".to_owned())
        }
        Err(other) => Err(format!("expected a revision conflict, got {other}")),
    }
}

fn refusal_is_inert(store: &mut dyn Store, registry: &Registry) -> Result<(), String> {
    let created = opened(store, registry, "inert")?;
    let closed = Runtime::new(registry)
        .execute(&created, "close", json!({}))
        .map_err(|error| error.to_string())?;

    if store.commit(&closed, Expect::Revision(99)).is_ok() {
        return Err("a commit expecting revision 99 was accepted".to_owned());
    }

    let after = store
        .load("conformance-ticket", "inert")
        .map_err(|error| error.to_string())?
        .ok_or("the instance vanished after a refusal")?;
    if after.revision != 1 || after.lifecycle_state != "open" {
        return Err("a refused commit moved the state".to_owned());
    }
    let events = store
        .events("conformance-ticket", "inert")
        .map_err(|error| error.to_string())?;
    if !events.is_empty() {
        return Err(format!(
            "a refused commit appended {} event(s)",
            events.len()
        ));
    }
    Ok(())
}

fn second_creation_refused(store: &mut dyn Store, registry: &Registry) -> Result<(), String> {
    opened(store, registry, "twice")?;
    let again = Runtime::new(registry)
        .create(
            "conformance-ticket",
            1,
            "twice",
            json!({ "title": "Again" }),
        )
        .map_err(|error| error.to_string())?;
    match store.commit(&again, Expect::Absent) {
        Err(StoreError::RevisionConflict { .. }) => Ok(()),
        Ok(()) => Err("a second creation of one identity replaced the first".to_owned()),
        Err(other) => Err(format!("expected a revision conflict, got {other}")),
    }
}

fn absent_is_not_an_error(store: &mut dyn Store, _registry: &Registry) -> Result<(), String> {
    match store.load("conformance-ticket", "nobody-stored-this") {
        Ok(None) => {}
        Ok(Some(_)) => {
            return Err("something was returned for an identity nobody stored".to_owned())
        }
        Err(error) => return Err(format!("absent must be an answer, not a failure: {error}")),
    }
    match store.events("conformance-ticket", "nobody-stored-this") {
        Ok(events) if events.is_empty() => Ok(()),
        Ok(events) => Err(format!(
            "{} event(s) for an identity nobody stored",
            events.len()
        )),
        Err(error) => Err(format!("absent must be an answer, not a failure: {error}")),
    }
}

fn holdings_are_listed(store: &mut dyn Store, registry: &Registry) -> Result<(), String> {
    // Two, created in the order a sort would *not* produce, so "sorted" is tested rather than
    // inherited from insertion order.
    opened(store, registry, "listed-b")?;
    opened(store, registry, "listed-a")?;
    let ids = store
        .ids("conformance-ticket")
        .map_err(|error| format!("listing failed: {error}"))?;

    let mut sorted = ids.clone();
    sorted.sort();
    if ids != sorted {
        return Err(format!(
            "the listing is not sorted: {ids:?}; two calls and two providers must agree byte for byte"
        ));
    }
    for wanted in ["listed-a", "listed-b"] {
        if !ids.iter().any(|id| id == wanted) {
            return Err(format!(
                "`{wanted}` was committed and is not listed: {ids:?}"
            ));
        }
    }
    // Every id listed must be one `load` answers for. A provider that lists what it does not hold
    // sends a hydrating shell to fetch instances that are not there.
    for id in &ids {
        if store
            .load("conformance-ticket", id)
            .map_err(|error| format!("load failed: {error}"))?
            .is_none()
        {
            return Err(format!(
                "`{id}` is listed and `load` answers absent; a listing must name only what is held"
            ));
        }
    }
    Ok(())
}

fn nothing_stored_lists_nothing(store: &mut dyn Store, _registry: &Registry) -> Result<(), String> {
    match store.ids("nobody-stored-this-type") {
        Ok(ids) if ids.is_empty() => Ok(()),
        Ok(ids) => Err(format!(
            "{} id(s) listed for an entity type nobody stored under: {ids:?}",
            ids.len()
        )),
        Err(error) => Err(format!(
            "nothing stored must be an answer, not a failure: {error}"
        )),
    }
}

fn refusal_lists_nothing(store: &mut dyn Store, registry: &Registry) -> Result<(), String> {
    let created = Runtime::new(registry)
        .create(
            "conformance-ticket",
            1,
            "listed-refused",
            json!({ "title": "Never landed" }),
        )
        .map_err(|error| format!("the kernel refused a creation: {error}"))?;
    if store.commit(&created, Expect::Revision(7)).is_ok() {
        return Err("a creation expecting revision 7 was accepted".to_owned());
    }
    let ids = store
        .ids("conformance-ticket")
        .map_err(|error| format!("listing failed: {error}"))?;
    if ids.iter().any(|id| id == "listed-refused") {
        return Err("a refused commit is listed as held".to_owned());
    }
    Ok(())
}

/// A provider that ignores the revision it was given, and lists an id it does not hold.
///
/// Deliberately wrong, and wrong in the two ways that matter most: every write is accepted, so a
/// concurrent writer silently replaces another's work; and the listing names a phantom, so a shell
/// hydrating from it goes looking for an instance that is not there. The suite is run against it to
/// prove the suite would catch both — a conformance suite nobody has watched fail is a suite nobody
/// knows the reach of.
#[derive(Debug, Default)]
pub struct Broken {
    inner: crate::MemoryStore,
}

impl StateProvider for Broken {
    fn load(
        &self,
        entity: &str,
        id: &str,
    ) -> Result<Option<entity_core::EntityInstance>, StoreError> {
        self.inner.load(entity, id)
    }

    fn ids(&self, entity: &str) -> Result<Vec<String>, StoreError> {
        // The second defect: an id nobody stored, listed as if it were held.
        let mut ids = self.inner.ids(entity)?;
        ids.push("ghost-nobody-stored".to_owned());
        ids.sort();
        Ok(ids)
    }
}

impl crate::EventProvider for Broken {
    fn events(&self, entity: &str, id: &str) -> Result<Vec<entity_core::DomainEvent>, StoreError> {
        self.inner.events(entity, id)
    }
}

impl Store for Broken {
    fn commit(
        &mut self,
        decision: &entity_core::Decision,
        _expect: Expect,
    ) -> Result<(), StoreError> {
        // The defect: whatever was expected, write anyway.
        let expect = match self
            .inner
            .load(&decision.instance.entity, &decision.instance.id)?
        {
            Some(held) => Expect::Revision(held.revision),
            None => Expect::Absent,
        };
        self.inner.commit(decision, expect)
    }
}
