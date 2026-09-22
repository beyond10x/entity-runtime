//! The logical typed identity, its storage address, and declared relations.

use entity_core::{
    create, identity::address, replay, CoreError, DefinitionError, DefinitionErrors,
    EntityDefinition, FieldKind, Observed, Registry, ValidatedDefinition,
};
use serde_json::{json, Value};

fn definition(value: Value) -> EntityDefinition {
    serde_json::from_value(value).expect("the fixture is a definition document")
}

fn refused(value: Value) -> DefinitionErrors {
    ValidatedDefinition::new(definition(value)).expect_err("the fixture is refused")
}

fn carries(defects: &DefinitionErrors, expected: &DefinitionError) -> bool {
    defects.iter().any(|defect| defect == expected)
}

/// A `service/1` definition whose identity is one field of the given kind.
fn keyed(field: Value) -> Value {
    json!({
        "entity": "keyed",
        "version": 1,
        "semantics": "service/1",
        "identity": { "field": "key" },
        "schema": { "fields": { "key": field } },
        "lifecycle": { "initial": "held", "states": ["held"] },
        "create": { "emit": { "type": "Keyed", "payload": { "key": "$fields.key" } } }
    })
}

/// Creates one instance at `id` with one identity value, and answers what happened.
fn keyed_at(field: Value, id: &str, value: Value) -> Result<entity_core::Decision, CoreError> {
    let validated = ValidatedDefinition::new(definition(keyed(field))).expect("registers");
    create(&validated, id.to_owned(), json!({ "key": value }))
}

// --- § 7.3: the address function ------------------------------------------------------------------

#[test]
fn an_integer_identity_addresses_by_its_canonical_decimal_and_replays() {
    let field = json!({ "type": "integer", "required": true });
    let decision = keyed_at(field.clone(), "42", json!(42)).expect("42 addresses as `42`");
    assert_eq!(decision.instance.id, "42");
    assert_eq!(replay(&[decision.record]).expect("replays").id, "42");
    assert_eq!(
        keyed_at(field, "s:42", json!(42)).expect_err("a text address is not a numeric one"),
        CoreError::IdentityMismatch {
            field: "key".to_owned(),
            id: "s:42".to_owned(),
            value: "'42'".to_owned(),
        }
    );
}

#[test]
fn a_binary64_identity_addresses_by_its_canonical_text_and_negative_zero_is_one_address() {
    let field = json!({ "type": "binary64", "required": true });
    let negative: Value = serde_json::from_str("-0.0").expect("a number");
    assert!(keyed_at(field.clone(), "0", negative.clone()).is_ok());
    assert!(keyed_at(field.clone(), "0", json!(0)).is_ok());
    // The field's own bytes keep the sign; the address does not distinguish the two.
    let decision = keyed_at(field.clone(), "0", negative).expect("creates");
    assert_eq!(
        serde_json::to_string(&decision.instance.fields).expect("serializes"),
        "{\"key\":-0.0}"
    );
    // The shortest decimal that round-trips, and never `f64`'s own `Display` of a whole value.
    let decision = keyed_at(field, "1.5", json!(1.5)).expect("creates");
    assert_eq!(decision.instance.id, "1.5");
}

#[test]
fn a_binary64_identity_whose_value_is_not_finite_is_refused_at_the_field_kind() {
    let validated = ValidatedDefinition::new(definition(keyed(
        json!({ "type": "binary64", "required": true }),
    )))
    .expect("registers");
    let fields: Value = serde_json::from_str("{\"key\": 1e400}").expect("a document");
    let Err(CoreError::Validation(errors)) = create(&validated, "1e400".to_owned(), fields) else {
        panic!("the admitted domain is a finite binary64 and nothing else");
    };
    assert_eq!(errors[0].path, "fields.key");
    assert!(errors[0].message.contains("source observation domain"));
    // And the address function says the same about an unvalidated value, rather than inventing one.
    let huge: Value = serde_json::from_str("1e400").expect("a number");
    assert!(address(FieldKind::Binary64, &huge).is_err());
}

#[test]
fn a_struct_identity_addresses_by_canonical_json_with_sorted_keys_and_normalized_numbers() {
    let field = json!({
        "type": "object",
        "required": true,
        "properties": {
            "tenant": { "type": "string", "required": true },
            "seq": { "type": "integer", "required": true }
        }
    });
    let decision = keyed_at(
        field,
        r#"{"seq":7,"tenant":"acme"}"#,
        json!({ "tenant": "acme", "seq": 7 }),
    )
    .expect("a composite addresses as canonical JSON");
    assert_eq!(decision.instance.id, r#"{"seq":7,"tenant":"acme"}"#);
}

#[test]
fn a_composite_identity_containing_a_binary64_member_addresses_recursively() {
    let field = json!({
        "type": "object",
        "required": true,
        "properties": {
            "name": { "type": "string", "required": true },
            "weight": { "type": "binary64", "required": true }
        }
    });
    let value: Value =
        serde_json::from_str(r#"{"name":"a\"b","weight":-0.0}"#).expect("a document");
    let decision = keyed_at(field, r#"{"name":"a\"b","weight":0}"#, value)
        .expect("every numeric leaf takes the canonical text and every string leaf is quoted");
    assert_eq!(decision.instance.id, r#"{"name":"a\"b","weight":0}"#);
}

#[test]
fn a_text_identity_addresses_with_the_s_prefix_and_an_empty_string_identity_is_admitted() {
    let field = json!({ "type": "string", "required": true });
    assert!(keyed_at(field.clone(), "s:INV-1", json!("INV-1")).is_ok());
    // The source admits an empty and a whitespace `String` identity, and the prefix is what makes
    // the rule total over them without a storage address that is empty or whitespace.
    assert!(keyed_at(field.clone(), "s:", json!("")).is_ok());
    assert!(keyed_at(field.clone(), "s: ", json!(" ")).is_ok());
    assert!(keyed_at(field, "s:s:x", json!("s:x")).is_ok());
}

#[test]
fn a_relation_carrier_and_an_event_payload_publish_the_logical_identity_not_the_address() {
    let decision = keyed_at(
        json!({ "type": "string", "required": true }),
        "s:INV-1",
        json!("INV-1"),
    )
    .expect("creates");
    assert_eq!(
        decision.events[0].payload["key"],
        json!("INV-1"),
        "an event publishes the declared logical value"
    );
    assert_eq!(
        decision.instance.id, "s:INV-1",
        "and `$id` stays the address"
    );

    // A relation carrier holds the related instance's logical identity, in its declared kind.
    let mut registry = Registry::new();
    registry
        .register(definition(json!({
            "entity": "account",
            "version": 1,
            "semantics": "service/1",
            "identity": { "field": "account_id" },
            "schema": { "fields": { "account_id": { "type": "string", "required": true } } },
            "lifecycle": { "initial": "open", "states": ["open"] }
        })))
        .expect("registers");
    registry
        .register(definition(json!({
            "entity": "invoice",
            "version": 1,
            "semantics": "service/1",
            "identity": { "field": "invoice_id" },
            "relations": { "account": {
                "kind": "references", "target": "account", "cardinality": "one", "via": "account_id"
            }},
            "schema": { "fields": {
                "invoice_id": { "type": "string", "required": true },
                "account_id": { "type": "string", "required": true }
            }},
            "lifecycle": { "initial": "draft", "states": ["draft"] }
        })))
        .expect("registers");
    registry
        .validate_all()
        .expect("the registry holds together");
    let invoice = registry.get("invoice", 1).expect("registered");
    let decision = create(
        invoice,
        "s:INV-1".to_owned(),
        json!({ "invoice_id": "INV-1", "account_id": "ACC-1" }),
    )
    .expect("creates");
    assert_eq!(
        decision.instance.fields["account_id"],
        json!("ACC-1"),
        "the carrier is the logical value, never the address"
    );
    assert_eq!(
        address(FieldKind::String, &json!("ACC-1")).expect("addresses"),
        "s:ACC-1"
    );
}

#[test]
fn two_numeric_identity_spellings_of_one_value_are_one_address() {
    for token in ["1", "1.0", "1e0"] {
        let value: Value = serde_json::from_str(token).expect("a number");
        assert_eq!(
            address(FieldKind::Number, &value).expect("addresses"),
            "1",
            "{token}"
        );
    }
    for token in ["-0.0", "0", "0.0", "-0"] {
        let value: Value = serde_json::from_str(token).expect("a number");
        assert_eq!(
            address(FieldKind::Binary64, &value).expect("addresses"),
            "0",
            "{token}"
        );
    }
}

/// The property the mirror step depends on: canonical identity equality agrees with the address.
#[test]
fn identity_equality_and_the_address_agree_for_every_numeric_identity_kind() {
    let tokens = [
        "0",
        "-0.0",
        "1",
        "1.0",
        "1e0",
        "1.5",
        "-1.5",
        "9007199254740992",
        "9007199254740993",
        "946.3702156715110866946",
        "0.1",
        "0.30000000000000004",
        "1.0000000000000000001",
    ];
    for left in tokens {
        for right in tokens {
            let left_value: Value = serde_json::from_str(left).expect("a number");
            let right_value: Value = serde_json::from_str(right).expect("a number");
            let equal = Observed::of_number(left_value.as_number().expect("a number"))
                .expect("observable")
                .cmp(
                    Observed::of_number(right_value.as_number().expect("a number"))
                        .expect("observable"),
                )
                .is_eq();
            let same_address = address(FieldKind::Number, &left_value).expect("addresses")
                == address(FieldKind::Number, &right_value).expect("addresses");
            assert_eq!(equal, same_address, "{left} against {right}");
        }
    }
}

#[test]
fn an_identity_field_that_stops_mirroring_the_id_after_set_is_refused() {
    let document = json!({
        "entity": "keyed",
        "version": 1,
        "semantics": "service/1",
        "identity": { "field": "key" },
        "schema": { "fields": { "key": { "type": "string", "required": true } } },
        "lifecycle": { "initial": "held", "states": ["held"] },
        "operations": { "rename": {
            "arguments": { "fields": { "to": { "type": "string", "required": true } } },
            "outcomes": [{ "name": "renamed", "effect": "updates", "set": { "key": "$args.to" } }]
        }}
    });
    let validated = ValidatedDefinition::new(definition(document)).expect("registers");
    let created = create(&validated, "s:A".to_owned(), json!({ "key": "A" })).expect("creates");
    assert_eq!(
        entity_core::execute(
            &validated,
            &created.instance,
            "rename",
            json!({ "to": "B" })
        ),
        Err(CoreError::IdentityMismatch {
            field: "key".to_owned(),
            id: "s:A".to_owned(),
            value: "'s:B'".to_owned(),
        })
    );
    // Writing the same value back is not a mismatch.
    assert!(entity_core::execute(
        &validated,
        &created.instance,
        "rename",
        json!({ "to": "A" })
    )
    .is_ok());
}

// --- § 7.4 and § 2.1: what the identity declaration refuses ---------------------------------------

#[test]
fn an_identity_field_that_is_unknown_or_optional_or_untyped_is_refused_at_registration() {
    let mut document = keyed(json!({ "type": "string", "required": true }));
    document["identity"] = json!({ "field": "nope" });
    assert!(carries(
        &refused(document),
        &DefinitionError::IdentityFieldUnknown {
            field: "nope".to_owned()
        }
    ));

    // The source's type says the value may be absent and the source's runtime says an instance's
    // may not, so a lowering emits a required field and an optional one is refused.
    assert!(carries(
        &refused(keyed(json!({ "type": "string" }))),
        &DefinitionError::IdentityFieldUnknown {
            field: "key".to_owned()
        }
    ));

    assert!(carries(
        &refused(keyed(json!({ "type": "json", "required": true }))),
        &DefinitionError::IdentityFieldNotAddressable {
            field: "key".to_owned(),
            kind: "json",
        }
    ));
}

/// § 7.3.3: *"a `json` leaf cannot occur, because `json` is refused as an identity kind **at every
/// depth** by `IdentityFieldNotAddressable`"*, beside § 7.3's *"total"* address function.
///
/// Reading the declared field's own kind and nothing below it left the refusal true only of the
/// root row. A `json` member is validated by nothing, so a composite carrying one may hold a
/// `null` or a number outside the observation domain — both of which `address` answers with an
/// error rather than an address, and step 11 then reports as an `IdentityMismatch` carrying that
/// sentence where an address belongs.
#[test]
fn a_json_leaf_is_refused_as_an_identity_at_every_depth_of_every_composite() {
    let not_addressable = DefinitionError::IdentityFieldNotAddressable {
        field: "key".to_owned(),
        kind: "json",
    };
    let json_leaf = json!({ "type": "json", "required": true });
    // One row per composite kind the address function recurses through, and one nesting that
    // reaches through three of them, so the recursion is pinned rather than the four arms.
    let shapes = [
        json!({ "type": "object", "required": true, "properties": { "blob": json_leaf } }),
        // An **optional** member is walked like a required one: a leaf the address function has no
        // row for is a leaf whether or not a value is supplied for it.
        json!({ "type": "object", "required": true, "properties": {
            "blob": { "type": "json" }
        }}),
        json!({ "type": "array", "required": true, "items": json_leaf }),
        json!({ "type": "map", "required": true, "key": "string", "items": json_leaf }),
        json!({ "type": "union", "required": true, "tag": "kind", "variants": {
            "raw": json_leaf
        }}),
        json!({ "type": "object", "required": true, "properties": {
            "rows": { "type": "array", "required": true, "items": {
                "type": "map", "required": true, "key": "string", "items": json_leaf
            }}
        }}),
    ];
    for shape in shapes {
        assert!(
            carries(&refused(keyed(shape.clone())), &not_addressable),
            "a json leaf inside {shape} is the same defect one level down"
        );
    }
}

/// The other half of the same rule: **no otherwise admitted composite kind is banned**, and the
/// empty `String` identity § 7.4 admits stays admitted. A refusal that swept up every composite
/// would close twelve of the thirteen identity rows § 7.1 measured the source admitting.
#[test]
fn a_composite_identity_without_a_json_leaf_still_registers_and_addresses() {
    for shape in [
        json!({ "type": "object", "required": true, "properties": {
            "seq": { "type": "integer", "required": true },
            "note": { "type": "string" }
        }}),
        json!({ "type": "array", "required": true, "items": { "type": "string" } }),
        json!({ "type": "map", "required": true, "key": "string", "items": { "type": "string" } }),
        json!({ "type": "union", "required": true, "tag": "kind", "variants": {
            "person": { "type": "string", "required": true },
            "company": { "type": "object", "required": true, "properties": {
                "name": { "type": "string", "required": true }
            }}
        }}),
    ] {
        assert!(
            ValidatedDefinition::new(definition(keyed(shape.clone()))).is_ok(),
            "{shape} is an admitted identity shape"
        );
    }

    // The composite recursion and the empty-string row, measured through the address rather than
    // asserted: `s:` is what makes the text rule total, and it does not appear on a composite's
    // text leaves, where the JSON quoting supplies the injectivity instead.
    let pair = json!({ "type": "object", "required": true, "properties": {
        "seq": { "type": "integer", "required": true },
        "note": { "type": "string", "required": true }
    }});
    let decision = keyed_at(
        pair,
        r#"{"note":"","seq":1}"#,
        json!({ "seq": 1, "note": "" }),
    )
    .expect("a composite identity addresses as canonical JSON");
    assert_eq!(decision.instance.id, r#"{"note":"","seq":1}"#);
    assert_eq!(
        keyed_at(
            json!({ "type": "string", "required": true }),
            "s:",
            json!("")
        )
        .expect("an empty string identity addresses as `s:`")
        .instance
        .id,
        "s:"
    );
}

/// A `json` field is refused as an **identity**, and nowhere else. A `kernel/1` definition cannot
/// declare `identity` at all (§ 2.1), so no `kernel/1` document changes meaning; a `service/1`
/// `json` field that the identity does not name keeps being an ordinary declared field.
#[test]
fn a_json_field_the_identity_does_not_name_is_untouched_by_the_recursion() {
    let mut document = keyed(json!({ "type": "string", "required": true }));
    document["schema"]["fields"]["blob"] = json!({ "type": "json" });
    document["schema"]["fields"]["wrapper"] = json!({
        "type": "object", "properties": { "inner": { "type": "json" } }
    });
    assert!(ValidatedDefinition::new(definition(document)).is_ok());

    let mut kernel = keyed(json!({ "type": "string", "required": true }));
    kernel["semantics"] = json!("kernel/1");
    kernel
        .as_object_mut()
        .expect("an object")
        .remove("identity");
    kernel["schema"]["fields"]["blob"] = json!({
        "type": "object", "properties": { "inner": { "type": "json" } }
    });
    assert!(ValidatedDefinition::new(definition(kernel)).is_ok());
}

#[test]
fn open_object_identity_members_are_untyped_json_at_every_depth() {
    let open = json!({ "type": "object", "additional_properties": true });
    let shapes = [
        open.clone(),
        json!({ "type": "object", "properties": { "nested": open } }),
        json!({ "type": "array", "items": open }),
        json!({ "type": "map", "key": "string", "items": open }),
        json!({ "type": "union", "tag": "kind", "variants": { "open": open } }),
    ];
    for mut shape in shapes {
        shape["required"] = json!(true);
        assert!(carries(
            &refused(keyed(shape)),
            &DefinitionError::IdentityFieldNotAddressable {
                field: "key".to_owned(),
                kind: "json",
            }
        ));
    }

    // Open objects outside the identity keep their existing value domain, including null.
    let mut document = keyed(json!({ "type": "string", "required": true }));
    document["schema"]["fields"]["extra"] = open.clone();
    let validated = ValidatedDefinition::new(definition(document)).expect("nonidentity stays open");
    create(
        &validated,
        "s:key".to_owned(),
        json!({ "key": "key", "extra": { "x": null } }),
    )
    .expect("untyped nonidentity member remains admitted");

    let mut kernel =
        keyed(json!({ "type": "object", "required": true, "additional_properties": true }));
    kernel["semantics"] = json!("kernel/1");
    kernel.as_object_mut().expect("object").remove("identity");
    let validated = ValidatedDefinition::new(definition(kernel)).expect("kernel stays open");
    create(
        &validated,
        "key".to_owned(),
        json!({ "key": { "x": null } }),
    )
    .expect("legacy object values are unchanged");
}

// --- § 8: relations -------------------------------------------------------------------------------

/// The owner and the thing it owns, where the carrier lives on the target and is typed as the
/// **owner's** identity kind.
fn owned_pair(owner_identity: Value, carrier: Value, cardinality: &str) -> Registry {
    let mut registry = Registry::new();
    registry
        .register(definition(json!({
            "entity": "invoice",
            "version": 1,
            "semantics": "service/1",
            "identity": { "field": "invoice_id" },
            "schema": { "fields": {
                "invoice_id": { "type": "string", "required": true },
                "account_id": carrier
            }},
            "lifecycle": { "initial": "draft", "states": ["draft"] }
        })))
        .expect("registers");
    registry
        .register(definition(json!({
            "entity": "account",
            "version": 1,
            "semantics": "service/1",
            "identity": { "field": "account_id" },
            "relations": { "invoices": {
                "kind": "owns", "target": "invoice", "cardinality": cardinality, "via": "account_id"
            }},
            "schema": { "fields": { "account_id": owner_identity } },
            "lifecycle": { "initial": "open", "states": ["open"] }
        })))
        .expect("registers");
    registry
}

#[test]
fn an_owns_relation_carrier_is_typed_as_the_owners_identity_kind_not_always_as_a_ref() {
    // An integer-identified owner is carried by an `integer` field on its target, not by a `ref`.
    let registry = owned_pair(
        json!({ "type": "integer", "required": true }),
        json!({ "type": "integer", "required": true }),
        "many",
    );
    registry
        .validate_all()
        .expect("the integer carrier is the owner's identity kind");

    let wrong = owned_pair(
        json!({ "type": "integer", "required": true }),
        json!({ "type": "string", "required": true }),
        "many",
    );
    let defects = wrong
        .validate_all()
        .expect_err("a string carrier is not an integer identity");
    assert!(defects
        .iter()
        .any(|defect| matches!(defect, DefinitionError::RelationCarrierWrong { .. })));
}

#[test]
fn an_owns_relation_is_validated_against_the_targets_field_and_not_the_declarers() {
    // The declaring definition has no `account_id` field of its own beyond its identity, and the
    // check still finds the carrier — because it looks at the target.
    let mut registry = Registry::new();
    registry
        .register(definition(json!({
            "entity": "invoice",
            "version": 1,
            "semantics": "service/1",
            "identity": { "field": "invoice_id" },
            "schema": { "fields": { "invoice_id": { "type": "string", "required": true } } },
            "lifecycle": { "initial": "draft", "states": ["draft"] }
        })))
        .expect("registers");
    registry
        .register(definition(json!({
            "entity": "account",
            "version": 1,
            "semantics": "service/1",
            "identity": { "field": "account_id" },
            "relations": { "invoices": {
                "kind": "owns", "target": "invoice", "cardinality": "many", "via": "account_id"
            }},
            "schema": { "fields": { "account_id": { "type": "string", "required": true } } },
            "lifecycle": { "initial": "open", "states": ["open"] }
        })))
        .expect("registers");
    let defects = registry
        .validate_all()
        .expect_err("the target declares no carrier");
    assert!(
        defects.iter().any(|defect| matches!(
            defect,
            DefinitionError::RelationCarrierWrong { entity, via, detail, .. }
                if entity == "account" && via == "account_id" && detail.contains("invoice")
        )),
        "{defects}"
    );
}

#[test]
fn a_many_owns_relation_carried_by_an_array_field_is_refused() {
    // `cardinality` says how many invoices one account has, and says nothing about that field,
    // which is one account whether the account has one invoice or a thousand.
    let registry = owned_pair(
        json!({ "type": "string", "required": true }),
        json!({ "type": "array", "required": true, "items": { "type": "string" } }),
        "many",
    );
    let defects = registry
        .validate_all()
        .expect_err("a Many owns carries a scalar");
    assert!(
        defects.iter().any(|defect| matches!(
            defect,
            DefinitionError::RelationCarrierWrong { detail, .. }
                if detail.contains("one value whether the owner has one of these or a thousand")
        )),
        "{defects}"
    );
}

#[test]
fn a_references_many_relation_requires_an_array_of_the_targets_identity_kind() {
    let document = |carrier: Value| {
        json!({
            "entity": "basket",
            "version": 1,
            "semantics": "service/1",
            "relations": { "items": {
                "kind": "references", "target": "item", "cardinality": "many", "via": "item_ids"
            }},
            "schema": { "fields": { "item_ids": carrier } },
            "lifecycle": { "initial": "open", "states": ["open"] }
        })
    };
    assert!(ValidatedDefinition::new(definition(document(
        json!({ "type": "array", "required": true, "items": { "type": "string" } })
    )))
    .is_ok());
    let defects = refused(document(json!({ "type": "string", "required": true })));
    assert!(carries(
        &defects,
        &DefinitionError::RelationViaWrongShape {
            relation: "items".to_owned(),
            via: "item_ids".to_owned(),
            expected: "an array of the target's identity kind".to_owned(),
            found: "a string field".to_owned(),
        }
    ));
}

#[test]
fn an_optional_references_one_carrier_lowers_to_required_false_and_registers() {
    // The source offers `Optional<target identity>` for this row, so a reference that is not yet
    // set is a shape the schema admits rather than one it turns away.
    let document = json!({
        "entity": "invoice",
        "version": 1,
        "semantics": "service/1",
        "relations": { "account": {
            "kind": "references", "target": "account", "cardinality": "one", "via": "account_id"
        }},
        "schema": { "fields": { "account_id": { "type": "string" } } },
        "lifecycle": { "initial": "draft", "states": ["draft"] }
    });
    assert!(ValidatedDefinition::new(definition(document)).is_ok());
}

#[test]
fn a_non_optional_references_one_carrier_lowers_to_required_true() {
    let document = json!({
        "entity": "invoice",
        "version": 1,
        "semantics": "service/1",
        "relations": { "account": {
            "kind": "references", "target": "account", "cardinality": "one", "via": "account_id"
        }},
        "schema": { "fields": { "account_id": { "type": "string", "required": true } } },
        "lifecycle": { "initial": "draft", "states": ["draft"] }
    });
    assert!(ValidatedDefinition::new(definition(document)).is_ok());
}

#[test]
fn an_optional_owns_or_references_many_carrier_is_refused() {
    let defects = refused(json!({
        "entity": "basket",
        "version": 1,
        "semantics": "service/1",
        "relations": { "items": {
            "kind": "references", "target": "item", "cardinality": "many", "via": "item_ids"
        }},
        "schema": { "fields": { "item_ids": { "type": "array", "items": { "type": "string" } } } },
        "lifecycle": { "initial": "open", "states": ["open"] }
    }));
    assert!(carries(
        &defects,
        &DefinitionError::RelationCarrierOptionality {
            relation: "items".to_owned(),
            via: "item_ids".to_owned(),
        }
    ));

    let registry = owned_pair(
        json!({ "type": "string", "required": true }),
        json!({ "type": "string" }),
        "one",
    );
    let defects = registry
        .validate_all()
        .expect_err("an owns carrier is required whatever the cardinality says");
    assert!(carries(
        &defects,
        &DefinitionError::RelationCarrierOptionality {
            relation: "invoices".to_owned(),
            via: "account_id".to_owned(),
        }
    ));
}

#[test]
fn an_unset_optional_reference_answers_unknown_rather_than_false() {
    // An absent optional carrier is *nothing observed*, not a null: the key is absent and a guard
    // reading it answers Unknown.
    let document = json!({
        "entity": "invoice",
        "version": 1,
        "semantics": "service/1",
        "relations": { "account": {
            "kind": "references", "target": "account", "cardinality": "one", "via": "account_id"
        }},
        "schema": { "fields": { "account_id": { "type": "string" } } },
        "lifecycle": { "initial": "draft", "states": ["draft"] },
        "invariants": [{
            "name": "account_is_acme",
            "assert": { "compare": { "left": "$fields.account_id", "op": "eq", "right": "ACC-1" } },
            "message": "the invoice names its account"
        }]
    });
    let validated = ValidatedDefinition::new(definition(document)).expect("registers");
    assert!(matches!(
        create(&validated, "i-1".to_owned(), json!({})),
        Err(CoreError::InvariantUnobservable { .. })
    ));
    assert!(matches!(
        create(
            &validated,
            "i-1".to_owned(),
            json!({ "account_id": "OTHER" })
        ),
        Err(CoreError::InvariantViolation { .. })
    ));
}

#[test]
fn a_second_owner_of_one_entity_is_refused_by_validate_all() {
    let mut registry = owned_pair(
        json!({ "type": "string", "required": true }),
        json!({ "type": "string", "required": true }),
        "many",
    );
    registry
        .register(definition(json!({
            "entity": "tenant",
            "version": 1,
            "semantics": "service/1",
            "identity": { "field": "account_id" },
            "relations": { "invoices": {
                "kind": "owns", "target": "invoice", "cardinality": "many", "via": "account_id"
            }},
            "schema": { "fields": { "account_id": { "type": "string", "required": true } } },
            "lifecycle": { "initial": "open", "states": ["open"] }
        })))
        .expect("registers");
    let defects = registry
        .validate_all()
        .expect_err("one entity has at most one owner");
    assert!(carries(
        &defects,
        &DefinitionError::RelationSecondOwner {
            target: "invoice".to_owned(),
            owner: "account".to_owned(),
            other: "tenant".to_owned(),
        }
    ));
}

#[test]
fn one_field_carrying_two_relations_is_refused_by_validate_all() {
    let mut registry = Registry::new();
    registry
        .register(definition(json!({
            "entity": "account",
            "version": 1,
            "semantics": "service/1",
            "schema": { "fields": { "account_id": { "type": "string", "required": true } } },
            "lifecycle": { "initial": "open", "states": ["open"] }
        })))
        .expect("registers");
    registry
        .register(definition(json!({
            "entity": "invoice",
            "version": 1,
            "semantics": "service/1",
            "relations": {
                "billed_to": {
                    "kind": "references", "target": "account", "cardinality": "one", "via": "account_id"
                },
                "owned_by": {
                    "kind": "references", "target": "account", "cardinality": "one", "via": "account_id"
                }
            },
            "schema": { "fields": { "account_id": { "type": "string", "required": true } } },
            "lifecycle": { "initial": "draft", "states": ["draft"] }
        })))
        .expect("registers");
    let defects = registry
        .validate_all()
        .expect_err("one field carries one relation");
    assert!(carries(
        &defects,
        &DefinitionError::RelationFieldClaimedTwice {
            entity: "invoice".to_owned(),
            field: "account_id".to_owned(),
            relation: "billed_to".to_owned(),
            other: "owned_by".to_owned(),
        }
    ));
}

#[test]
fn an_unregistered_relation_target_is_refused_by_validate_all() {
    let mut registry = Registry::new();
    registry
        .register(definition(json!({
            "entity": "invoice",
            "version": 1,
            "semantics": "service/1",
            "relations": { "account": {
                "kind": "references", "target": "account", "cardinality": "one", "via": "account_id"
            }},
            "schema": { "fields": { "account_id": { "type": "string", "required": true } } },
            "lifecycle": { "initial": "draft", "states": ["draft"] }
        })))
        .expect("registers");
    let defects = registry
        .validate_all()
        .expect_err("nobody registered the target");
    assert!(carries(
        &defects,
        &DefinitionError::RelationTargetMissing {
            entity: "invoice".to_owned(),
            relation: "account".to_owned(),
            target: "account".to_owned(),
        }
    ));
}

/// The correction the coordinator made to the text-carrier sentence: a new text relation carrier
/// uses the identity's declared kind, so an empty logical `String` identity stays valid. The legacy
/// `ref` kind keeps its own non-empty validation, unchanged.
#[test]
fn a_reference_to_an_empty_logical_text_identity_preserves_the_declared_carrier() {
    let mut registry = Registry::new();
    registry
        .register(definition(json!({
            "entity": "account",
            "version": 1,
            "semantics": "service/1",
            "identity": { "field": "account_id" },
            "schema": { "fields": { "account_id": { "type": "string", "required": true } } },
            "lifecycle": { "initial": "open", "states": ["open"] }
        })))
        .expect("registers");
    registry
        .register(definition(json!({
            "entity": "invoice",
            "version": 1,
            "semantics": "service/1",
            "relations": { "account": {
                "kind": "references", "target": "account", "cardinality": "one", "via": "account_id"
            }},
            "schema": { "fields": { "account_id": { "type": "string", "required": true } } },
            "lifecycle": { "initial": "draft", "states": ["draft"] }
        })))
        .expect("registers");
    registry
        .validate_all()
        .expect("the registry holds together");

    // The carrier is a `string`, so it accepts the empty logical identity the source admits.
    let invoice = registry.get("invoice", 1).expect("registered");
    assert!(create(invoice, "i-1".to_owned(), json!({ "account_id": "" })).is_ok());
    // The binding derives the storage address from that logical value through the one function.
    assert_eq!(
        address(FieldKind::String, &json!("")).expect("addresses"),
        "s:"
    );

    // `ref` keeps its existing meaning and validation, which is why it is not the carrier a new
    // lowering emits for arbitrary logical text.
    let legacy = ValidatedDefinition::new(definition(json!({
        "entity": "legacy",
        "version": 1,
        "schema": { "fields": { "account": { "type": "ref", "entity": "account", "required": true } } },
        "lifecycle": { "initial": "draft", "states": ["draft"] }
    })))
    .expect("registers");
    let Err(CoreError::Validation(errors)) =
        create(&legacy, "l-1".to_owned(), json!({ "account": "" }))
    else {
        panic!("a ref is not empty or whitespace");
    };
    assert_eq!(errors[0].message, "a reference is not empty or whitespace");
}

// --- § 8.1: every relation row compares its carrier against the related identity's kind ----------

/// A registry holding `item`, whose logical identity has `identity`, and `basket`, whose
/// `references` carrier `item_ids` is declared as `carrier`.
///
/// Both definitions register: the kind comparison is the registry's, because one definition does
/// not hold the other's identity field.
fn referring_pair(identity: Value, cardinality: &str, carrier: Value) -> Registry {
    let mut registry = Registry::new();
    registry
        .register(definition(json!({
            "entity": "item",
            "version": 1,
            "semantics": "service/1",
            "identity": { "field": "sku" },
            "schema": { "fields": { "sku": identity } },
            "lifecycle": { "initial": "listed", "states": ["listed"] }
        })))
        .expect("the target registers");
    registry
        .register(definition(json!({
            "entity": "basket",
            "version": 1,
            "semantics": "service/1",
            "relations": { "items": {
                "kind": "references", "target": "item", "cardinality": cardinality, "via": "item_ids"
            }},
            "schema": { "fields": { "item_ids": carrier } },
            "lifecycle": { "initial": "open", "states": ["open"] }
        })))
        .expect("the declaring definition registers");
    registry
}

fn carrier_is_wrong(registry: &Registry) -> bool {
    match registry.validate_all() {
        Ok(()) => false,
        Err(defects) => defects.iter().any(|defect| {
            matches!(defect, DefinitionError::RelationViaWrongShape { via, .. } if via == "item_ids")
        }),
    }
}

#[test]
fn every_references_row_compares_its_carrier_against_the_targets_identity_kind() {
    let integer = json!({ "type": "integer", "required": true });
    let text = json!({ "type": "string", "required": true });
    let choice = json!({ "type": "enum", "values": ["a", "b"], "required": true });
    let composite = json!({
        "type": "object",
        "required": true,
        "properties": { "left": { "type": "string", "required": true } }
    });

    // The `references`/`one` row: the carrier's kind **is** the target's identity kind, and this
    // row is the only one the source offers an optional carrier for, so both optionalities are
    // admitted and neither excuses a wrong kind.
    for (identity, carrier, wrong) in [
        (
            integer.clone(),
            json!({ "type": "integer", "required": true }),
            false,
        ),
        (integer.clone(), json!({ "type": "integer" }), false),
        (
            integer.clone(),
            json!({ "type": "boolean", "required": true }),
            true,
        ),
        (
            integer.clone(),
            json!({ "type": "string", "required": true }),
            true,
        ),
        (integer.clone(), json!({ "type": "boolean" }), true),
        (
            text.clone(),
            json!({ "type": "string", "required": true }),
            false,
        ),
        // A `ref` requires non-empty text and is not the `string` kind an arbitrary logical text
        // identity is carried in, which is why § 8.1 says a lowering must not substitute it.
        (
            text.clone(),
            json!({ "type": "ref", "entity": "item", "required": true }),
            true,
        ),
        // A named identity and a composite one are compared by the same rule, not skipped.
        (
            choice.clone(),
            json!({ "type": "enum", "values": ["a", "b"], "required": true }),
            false,
        ),
        (choice, json!({ "type": "string", "required": true }), true),
        (composite.clone(), composite.clone(), false),
        (
            composite,
            json!({ "type": "string", "required": true }),
            true,
        ),
    ] {
        let registry = referring_pair(identity, "one", carrier.clone());
        assert_eq!(carrier_is_wrong(&registry), wrong, "one: {carrier}");
    }

    // The `references`/`many` row: the comparison is against the array's **element** kind, which is
    // what `{type: array, items: <the target's identity kind>}` says.
    for (identity, items, wrong) in [
        (integer.clone(), json!({ "type": "integer" }), false),
        (integer.clone(), json!({ "type": "string" }), true),
        (integer.clone(), json!({ "type": "boolean" }), true),
        (text.clone(), json!({ "type": "string" }), false),
        (text.clone(), json!({ "type": "integer" }), true),
    ] {
        let carrier = json!({ "type": "array", "required": true, "items": items.clone() });
        let registry = referring_pair(identity, "many", carrier);
        assert_eq!(carrier_is_wrong(&registry), wrong, "many: {items}");
    }
}

#[test]
fn a_references_target_that_declares_no_logical_identity_still_carries_whatever_it_declares() {
    // The kind comparison needs a kind to compare against. A target with no `identity` has none, so
    // nothing new is refused — which is the position the `owns` row is already in, and is what
    // keeps every definition admitted today admitted.
    let mut registry = Registry::new();
    registry
        .register(definition(json!({
            "entity": "item",
            "version": 1,
            "semantics": "service/1",
            "schema": { "fields": { "sku": { "type": "integer", "required": true } } },
            "lifecycle": { "initial": "listed", "states": ["listed"] }
        })))
        .expect("the target registers");
    registry
        .register(definition(json!({
            "entity": "basket",
            "version": 1,
            "semantics": "service/1",
            "relations": { "items": {
                "kind": "references", "target": "item", "cardinality": "one", "via": "item_ids"
            }},
            "schema": { "fields": { "item_ids": { "type": "boolean", "required": true } } },
            "lifecycle": { "initial": "open", "states": ["open"] }
        })))
        .expect("the declaring definition registers");
    registry.validate_all().expect("nothing new is refused");
}

#[test]
fn the_declaring_definition_still_answers_the_half_it_can_see_about_a_references_carrier() {
    // The registry answering the kind does not move the shape and optionality rules off the
    // declaring definition: a `many` carrier that is not an array, and one that is optional, are
    // still refused where they are written, before any registry holds the target.
    let document = |carrier: Value| {
        json!({
            "entity": "basket",
            "version": 1,
            "semantics": "service/1",
            "relations": { "items": {
                "kind": "references", "target": "item", "cardinality": "many", "via": "item_ids"
            }},
            "schema": { "fields": { "item_ids": carrier } },
            "lifecycle": { "initial": "open", "states": ["open"] }
        })
    };
    let defects = refused(document(json!({ "type": "string", "required": true })));
    assert!(carries(
        &defects,
        &DefinitionError::RelationViaWrongShape {
            relation: "items".to_owned(),
            via: "item_ids".to_owned(),
            expected: "an array of the target's identity kind".to_owned(),
            found: "a string field".to_owned(),
        }
    ));
    let defects = refused(document(
        json!({ "type": "array", "items": { "type": "string" } }),
    ));
    assert!(carries(
        &defects,
        &DefinitionError::RelationCarrierOptionality {
            relation: "items".to_owned(),
            via: "item_ids".to_owned(),
        }
    ));
}

// --- § 8.1: a list identity is carried by a list, on every row -----------------------------------

/// ER's spelling of ESS `List<T>`: `{type: array, items: <element>}`.
///
/// `required` is the row's optionality, which only the `references`/`one` row may vary. The element
/// is written without `required`, which is how every other array fixture in this file spells one.
fn list_of(element: Value, required: bool) -> Value {
    let mut field = json!({ "type": "array", "items": element });
    if required {
        field["required"] = json!(true);
    }
    field
}

fn element(kind: &str) -> Value {
    json!({ "type": kind })
}

fn owns_carrier_is_wrong(registry: &Registry) -> bool {
    match registry.validate_all() {
        Ok(()) => false,
        Err(defects) => defects.iter().any(|defect| {
            matches!(defect, DefinitionError::RelationCarrierWrong { via, .. } if via == "account_id")
        }),
    }
}

/// § 8.1's worked table for a target identified by `List<String>`, row for row.
///
/// `carried_types` compares the carrier against the whole `TypeRef`, and `TypeRef::List` nests, so
/// an identity that is itself a list is carried by a list — the case the single-document array
/// refusal used to turn away on the `references`/`one` row.
#[test]
fn a_list_identity_is_carried_by_a_list_on_every_row() {
    let list_identity = || list_of(element("string"), true);
    let list_carrier = |required: bool| list_of(element("string"), required);

    // `Owns`, both cardinalities: the owner's identity once, which here is one list.
    for cardinality in ["one", "many"] {
        let registry = owned_pair(list_identity(), list_carrier(true), cardinality);
        registry
            .validate_all()
            .unwrap_or_else(|defects| panic!("owns/{cardinality}: {defects}"));
    }

    // `References`/`One`: the identity's own shape, and `Optional<it>` is the same shape declared
    // `required: false`.
    for required in [true, false] {
        let registry = referring_pair(list_identity(), "one", list_carrier(required));
        assert!(
            !carrier_is_wrong(&registry),
            "references/one, required {required}"
        );
    }

    // `References`/`Many`: `List<List<String>>`, which is an array of arrays.
    let nested = list_of(list_of(element("string"), false), true);
    let registry = referring_pair(list_identity(), "many", nested);
    assert!(!carrier_is_wrong(&registry));
}

/// The outer array of a `references`/`many` carrier is the **cardinality**, so it is one array
/// deeper than the identity — and a carrier that is only as deep as the identity is the wrong type.
#[test]
fn a_references_many_carrier_of_a_list_identity_is_one_array_deeper_than_the_identity() {
    let list_identity = || list_of(element("string"), true);

    // One array of `string` is `List<String>`, which is the identity itself and not a list of it.
    let flat = referring_pair(list_identity(), "many", list_of(element("string"), true));
    assert!(carrier_is_wrong(&flat));

    // Two arrays deep is `List<List<String>>`, which is the row.
    let nested = referring_pair(
        list_identity(),
        "many",
        list_of(list_of(element("string"), false), true),
    );
    assert!(!carrier_is_wrong(&nested));

    // And the depth is not free in the other direction either: a scalar identity is carried by one
    // array, never by two.
    let too_deep = referring_pair(
        json!({ "type": "string", "required": true }),
        "many",
        list_of(list_of(element("string"), false), true),
    );
    assert!(carrier_is_wrong(&too_deep));

    // The element kind still has to match at the bottom of the nesting.
    let wrong_element = referring_pair(
        list_of(element("integer"), true),
        "many",
        list_of(list_of(element("string"), false), true),
    );
    assert!(carrier_is_wrong(&wrong_element));
}

/// `carried_types` has **one** arm for `(Owns, _)` — the owner's identity, once — so the carrier is
/// the same on both cardinalities and `Many` wraps nothing.
#[test]
fn an_owns_carrier_is_the_owners_identity_once_on_both_cardinalities() {
    for cardinality in ["one", "many"] {
        // The refusal that survives: an array carrier where the owner's identity is a scalar is the
        // cardinality mistake, and it keeps its own sentence.
        let scalar_owner = owned_pair(
            json!({ "type": "string", "required": true }),
            list_of(element("string"), true),
            cardinality,
        );
        let defects = scalar_owner
            .validate_all()
            .expect_err("an owner identified by one string is carried once");
        assert!(
            defects.iter().any(|defect| matches!(
                defect,
                DefinitionError::RelationCarrierWrong { detail, .. }
                    if detail.contains("one value whether the owner has one of these or a thousand")
            )),
            "owns/{cardinality}: {defects}"
        );

        // A list owner identity carried by that same list is admitted, on both cardinalities.
        let list_owner = owned_pair(
            list_of(element("string"), true),
            list_of(element("string"), true),
            cardinality,
        );
        assert!(
            !owns_carrier_is_wrong(&list_owner),
            "owns/{cardinality}: a list identity is carried by that list"
        );

        // `Many` still wraps nothing: a list of the identity is not the identity.
        let wrapped = owned_pair(
            list_of(element("string"), true),
            list_of(list_of(element("string"), false), true),
            cardinality,
        );
        assert!(
            owns_carrier_is_wrong(&wrapped),
            "owns/{cardinality}: cardinality introduces no list here"
        );

        // And the element kind is compared, not just the array-ness.
        let wrong_element = owned_pair(
            list_of(element("integer"), true),
            list_of(element("string"), true),
            cardinality,
        );
        assert!(owns_carrier_is_wrong(&wrong_element), "owns/{cardinality}");

        // A scalar carrier for a list identity is the mirror mistake.
        let flattened = owned_pair(
            list_of(element("string"), true),
            json!({ "type": "string", "required": true }),
            cardinality,
        );
        assert!(owns_carrier_is_wrong(&flattened), "owns/{cardinality}");
    }
}
