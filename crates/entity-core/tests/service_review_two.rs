//! Independent source examination, pass 2: cases driven from
//! `docs/design/service-semantics-v0.1.md` against the corrected implementation.
//!
//! Pass 1's findings and root's dispositions of them are preserved and not re-litigated here.
//! Each case below names the contract line it is driven from and decides a claim that document
//! makes about a surface pass 1 did not reach.

use entity_core::{
    create, CoreError, DefinitionError, DefinitionErrors, EntityDefinition, Truth,
    ValidatedDefinition,
};
use serde_json::{json, Value};

fn definition(value: Value) -> EntityDefinition {
    serde_json::from_value(value).expect("the fixture is a definition document")
}

fn document(value: &str) -> Value {
    // `serde_json::from_str` retains the authored token; `json!` cannot carry `1e400` at all.
    serde_json::from_str(value).expect("the fixture is a JSON document")
}

/// Whether any defect names the source observation domain, in either variant a definition-admission
/// refusal of an unreadable number can carry.
fn names_the_domain(defects: &DefinitionErrors) -> bool {
    defects.iter().any(|defect| {
        matches!(
            defect,
            DefinitionError::InvalidField { message, .. }
                | DefinitionError::InvalidRule { message, .. }
                if message.contains("source observation domain")
        )
    })
}

/// A `service/1` definition whose declared logical identity is the field `key`.
fn identity_probe(key: Value) -> Value {
    json!({
        "entity": "probe",
        "version": 1,
        "semantics": "service/1",
        "identity": { "field": "key" },
        "schema": { "fields": { "key": key } },
        "lifecycle": { "initial": "held", "states": ["held"] }
    })
}

/// A `service/1` definition whose only declared field is `x`.
fn field_probe(field: Value) -> Value {
    json!({
        "entity": "probe",
        "version": 1,
        "semantics": "service/1",
        "schema": { "fields": { "x": field } },
        "lifecycle": { "initial": "held", "states": ["held"] }
    })
}

/// What one condition answers about one instance, read through the invariant it is written as.
fn answer(schema: Value, condition: Value, fields: Value) -> Truth {
    let validated = ValidatedDefinition::new(definition(json!({
        "entity": "probe",
        "version": 1,
        "semantics": "service/1",
        "schema": schema,
        "lifecycle": { "initial": "held", "states": ["held"] },
        "invariants": [{ "name": "rule", "assert": condition, "message": "the rule" }]
    })))
    .expect("the probe registers");
    match create(&validated, "p-1".to_owned(), fields) {
        Ok(_) => Truth::True,
        Err(CoreError::InvariantViolation { .. }) => Truth::False,
        Err(CoreError::InvariantUnobservable { .. }) => Truth::Unknown,
        Err(other) => panic!("the probe refused for another reason: {other}"),
    }
}

/// § 7.3.3: *"a `json` leaf cannot occur, because `json` is refused as an identity kind **at every
/// depth** by `IdentityFieldNotAddressable`"*, beside § 7.3's *"total"* address function and
/// § 7.4's *"there is no second refusal"*.
///
/// `validate_identity` reads the declared identity field's own kind and nothing below it
/// (`crates/entity-core/src/validation.rs:693-712`), so a composite identity carrying a `json`
/// member registers. A `json` value is validated by nothing (`:2138`), so such a member may hold a
/// `null` or a number the source cannot observe — both of which `address` answers with an error
/// rather than an address (`crates/entity-core/src/identity.rs:142-174`), and step 11 then reports
/// `IdentityMismatch` carrying that sentence in place of an address.
#[test]
fn a_json_member_of_a_composite_identity_is_refused_at_every_depth() {
    // The control: the root row is refused, so the mechanism exists and names the field.
    let root = ValidatedDefinition::new(definition(identity_probe(json!({
        "type": "json",
        "required": true
    }))))
    .expect_err("a json identity has no address function");
    assert!(
        root.iter().any(|defect| matches!(
            defect,
            DefinitionError::IdentityFieldNotAddressable { field, .. } if field == "key"
        )),
        "the root row names the field: {root}"
    );

    let nested = ValidatedDefinition::new(definition(identity_probe(json!({
        "type": "object",
        "required": true,
        "properties": { "blob": { "type": "json", "required": true } }
    }))))
    .expect_err("a json leaf inside a composite identity is the same defect one level down");
    assert!(
        nested.iter().any(|defect| matches!(
            defect,
            DefinitionError::IdentityFieldNotAddressable { field, .. } if field == "key"
        )),
        "a composite identity carrying a json member is addressable by nothing: {nested}"
    );
}

/// § 10.2.1: *"Under `service/1`, numeric schema admission refuses a value outside this
/// source-observation domain with a path-bearing `ValidationError`; **definition admission rejects
/// unobservable numeric literals and bounds**."*
///
/// A declared `default` is read at definition admission and refused there
/// (`crates/entity-core/src/validation.rs:1872-1883`). A declared `min`/`max` is read only when a
/// value arrives (`:2196-2216`), so the definition registers and the bound answers nothing until —
/// and unless — a value for that field is supplied, which is the *unevaluable at every evaluation*
/// case the same sentence exists to refuse.
#[test]
fn an_unobservable_numeric_bound_is_refused_at_definition_admission_like_an_unobservable_default() {
    // The control: the same unreadable number written as a default is refused where it is written.
    let defaulted = ValidatedDefinition::new(definition(field_probe(document(
        r#"{"type": "number", "required": true, "default": 1e400}"#,
    ))))
    .expect_err("an unreadable default is refused at definition admission");
    assert!(
        names_the_domain(&defaulted),
        "the default names the domain: {defaulted}"
    );

    let bounded = ValidatedDefinition::new(definition(field_probe(document(
        r#"{"type": "number", "required": true, "min": 1e400}"#,
    ))))
    .expect_err("an unreadable bound is the other half of the same sentence");
    assert!(
        names_the_domain(&bounded),
        "the bound names the domain: {bounded}"
    );
}

/// § 10.6: *"a `map`'s keys stay unaddressable (§ 10.4), so `{metadata: {count: "7"}}` cannot be
/// read as its own `count` key: the only `count` a `map` has is its size"*, over a `map` declared
/// as a `union` variant — which § 10.1 types with a full `FieldDefinition` like any other field,
/// and which § 10.5 reaches at `$fields.payee.value` under the tag test.
///
/// The run-time walk continues under a union's content key with `field.variants.get(segment)`
/// (`crates/entity-core/src/runtime.rs:2075-2081`), which looks a **variant label** up by the
/// **wire key**. No variant here is called `value`, so the declared `map` is lost and the payload
/// is walked as an untyped object: `count` reads the map's own key instead of its size.
/// Registration admits the address without checking it, because `walk_field_path`'s union arm
/// returns at the content key (`crates/entity-core/src/validation.rs:1105-1117`).
#[test]
fn a_map_declared_as_a_union_variant_still_addresses_its_size_and_not_its_count_key() {
    let schema = json!({ "fields": { "payee": {
        "type": "union",
        "required": true,
        "tag": "kind",
        "variants": { "meta": {
            "type": "map",
            "required": true,
            "key": "string",
            "items": { "type": "string" }
        }}
    }}});
    let condition =
        json!({ "compare": { "left": "$fields.payee.value.count", "op": "eq", "right": 1 } });
    let fields = json!({ "payee": { "kind": "meta", "value": { "count": "7" } } });

    assert_eq!(
        answer(schema, condition, fields),
        Truth::True,
        "a declared map's only `count` is its size, whichever field declares the map"
    );
}
