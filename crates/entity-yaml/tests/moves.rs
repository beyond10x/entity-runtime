//! A `service/1` outcome whose effect is `moves`, read from YAML.
//!
//! `OutcomeEffect` is an externally tagged enum and `moves` is its one variant that carries data.
//! JSON spells it as a mapping with one key, `{"moves": {"from": …, "to": …}}`. The YAML reader took
//! a data-carrying enum only as a `!tag`, so the same mapping written in YAML was refused with
//! *expected a YAML tag starting with '!'*. These tests hold the YAML mapping to the JSON reading
//! of the same document, run it through the kernel, and hold every document the reader accepted
//! before to the definition it read before.

use std::{
    fs,
    path::{Path, PathBuf},
};

use entity_core::{
    identity, CoreError, EntityDefinition, FieldKind, OneOrMany, OutcomeEffect, Registry, Runtime,
};
use serde_json::{json, Value};

/// A `service/1` invoice with two `moves` outcomes, one in block and one in flow style, beside a
/// `creates` and an `updates` outcome.
const INVOICE: &str = include_str!("fixtures/invoice.yaml");

/// The same document read by the JSON front door: YAML text to a plain JSON value, then
/// `serde_json` into the definition, which is how a JSON definition is read in Rust.
fn read_as_json(text: &str) -> EntityDefinition {
    let document: Value = serde_yaml_ng::from_str(text).expect("the fixture is YAML");
    serde_json::from_value(document).expect("the JSON spelling is a definition")
}

fn effect_of(definition: &EntityDefinition, operation: &str, outcome: &str) -> OutcomeEffect {
    definition.operations[operation]
        .outcomes
        .iter()
        .find(|branch| branch.name == outcome)
        .unwrap_or_else(|| panic!("{operation} declares outcome {outcome}"))
        .effect
        .clone()
}

#[test]
fn a_moves_outcome_written_as_a_yaml_mapping_reads_as_its_json_spelling_does() {
    let from_json = read_as_json(INVOICE);
    from_json
        .validate()
        .expect("the JSON reading is a valid service/1 definition");

    let from_yaml = entity_yaml::from_str(INVOICE).expect("the YAML mapping form loads");
    assert_eq!(from_yaml, from_json);

    assert_eq!(
        effect_of(&from_yaml, "pay", "paid"),
        OutcomeEffect::Moves {
            to: "paid".to_owned(),
            from: OneOrMany::One("open".to_owned()),
        }
    );
    assert_eq!(
        effect_of(&from_yaml, "void", "voided"),
        OutcomeEffect::Moves {
            to: "void".to_owned(),
            from: OneOrMany::Many(vec!["open".to_owned(), "paid".to_owned()]),
        }
    );
    assert_eq!(
        effect_of(&from_yaml, "adjust", "adjusted"),
        OutcomeEffect::Updates
    );
    assert_eq!(from_yaml.create.outcomes[0].effect, OutcomeEffect::Creates);
}

#[test]
fn a_moves_outcome_read_from_yaml_moves_the_instance_and_its_wrong_state_branch_refuses() {
    let definition = entity_yaml::from_str(INVOICE).expect("the YAML mapping form loads");
    let mut registry = Registry::new();
    registry.register(definition).expect("a valid definition");
    let runtime = Runtime::new(&registry);
    let id = identity::address(FieldKind::String, &json!("INV-1")).expect("a text address");

    let opened = runtime
        .create(
            "invoice",
            1,
            id,
            json!({ "invoice_id": "INV-1", "amount": 120 }),
        )
        .expect("create")
        .instance;
    assert_eq!(opened.lifecycle_state, "open");

    let paid = runtime.execute(&opened, "pay", json!({})).expect("pay");
    assert_eq!(paid.instance.lifecycle_state, "paid");
    assert_eq!(paid.record.from_state.as_deref(), Some("open"));
    assert_eq!(paid.record.outcome.as_deref(), Some("paid"));
    assert_eq!(paid.events[0].event_type, "InvoicePaid");

    let error = runtime
        .execute(&paid.instance, "pay", json!({}))
        .expect_err("an invoice is paid once");
    assert!(
        matches!(&error, CoreError::Refused { outcome, error, .. } if outcome == "not-open" && error == "InvoiceNotOpen"),
        "{error}"
    );

    let voided = runtime
        .execute(&paid.instance, "void", json!({}))
        .expect("a paid invoice may be voided");
    assert_eq!(voided.instance.lifecycle_state, "void");
}

/// The tag form `!moves { … }` is what the YAML library writes for a data-carrying variant, and it
/// is not a spelling of a definition: the duplicate-key pass refuses a tagged node anywhere in the
/// document, as it did before mappings were read as effects. One spelling is documented, the
/// mapping JSON also uses; reading the tag as well would take a reader that forwards tags at every
/// depth, which the mapping reader does not.
#[test]
fn a_moves_effect_written_as_a_yaml_tag_is_refused_at_its_path() {
    let tagged = INVOICE.replace(
        "effect: { moves: { from: [open, paid], to: void } }",
        "effect: !moves { from: [open, paid], to: void }",
    );
    assert_ne!(tagged, INVOICE, "the fixture still spells the flow mapping");

    let error = entity_yaml::from_str(&tagged).expect_err("a tag is not a definition spelling");
    let message = error.to_string();
    assert!(
        message.contains("operations.void.outcomes[0].effect: invalid type: enum"),
        "{message}"
    );
}

#[test]
fn a_repeated_key_inside_a_moves_effect_is_still_refused() {
    let repeated = INVOICE.replace(
        "effect: { moves: { from: [open, paid], to: void } }",
        "effect: { moves: { from: [open, paid], to: void, to: open } }",
    );
    assert_ne!(
        repeated, INVOICE,
        "the fixture still spells the flow mapping"
    );
    let error = entity_yaml::from_str(&repeated).expect_err("last-key-wins is ambiguous");
    assert!(
        error.to_string().contains("duplicate mapping key"),
        "{error}"
    );
}

#[test]
fn an_effect_mapping_naming_two_effects_is_refused() {
    let two = INVOICE.replace(
        "effect: { moves: { from: [open, paid], to: void } }",
        "effect: { moves: { from: [open, paid], to: void }, updates: null }",
    );
    assert_ne!(two, INVOICE, "the fixture still spells the flow mapping");
    let error = entity_yaml::from_str(&two).expect_err("one effect per outcome");
    assert!(
        error.to_string().contains("map with a single key"),
        "{error}"
    );
}

/// The `service/1` invoice of the service-semantics page, which declares only `creates` and
/// `updates` effects, a `when` condition and a declared refusal: a document the reader accepted
/// before mapping effects did.
const UNIT_EFFECTS: &str = r#"
entity: invoice
version: 1
semantics: service/1
identity: { field: invoice_id }
schema:
  fields:
    invoice_id: { type: string, required: true }
    total: { type: integer, required: true }
lifecycle:
  initial: open
  states: [open]
create:
  arguments:
    fields:
      invoice_id: { type: string, required: true }
      amount: { type: integer, required: true }
  outcomes:
    - name: accepted
      effect: creates
      set:
        invoice_id: $args.invoice_id
        total: $args.amount
      emits:
        - type: InvoiceCreated
          payload: { invoice_id: $fields.invoice_id }
operations:
  adjust:
    arguments:
      fields:
        amount: { type: integer, required: true }
    response:
      fields:
        new_total: { type: integer, required: true }
    outcomes:
      - name: adjusted
        when: { compare: { left: $args.amount, op: gt, right: 0 } }
        effect: updates
        set: { total: $args.amount }
        emits:
          - type: InvoiceAdjusted
            payload: { total: $fields.total }
        responds: { new_total: $fields.total }
      - name: rejected
        refuses:
          error: AmountNotPositive
          message: an invoice total is positive
"#;

/// Every YAML document under `examples/`, by path.
fn shipped_documents() -> Vec<(PathBuf, String)> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples");
    let mut found = Vec::new();
    let mut directories = vec![root];
    while let Some(directory) = directories.pop() {
        for entry in fs::read_dir(&directory).expect("examples/ is readable") {
            let path = entry.expect("a directory entry").path();
            if path.is_dir() {
                directories.push(path);
            } else if path
                .extension()
                .is_some_and(|extension| extension == "yaml")
            {
                let text = fs::read_to_string(&path).expect("readable");
                found.push((path, text));
            }
        }
    }
    found.sort();
    found
}

/// Reading a mapping as an enum must not move any spelling the reader already took: a `Condition`,
/// a one-or-many state list, an `{increment: n}` or `{cleared: true}` assignment, a unit effect.
/// The reference is `serde_yaml_ng::from_str`, the reader's second pass before this change.
#[test]
fn every_document_the_reader_accepted_before_reads_to_the_definition_it_read_before() {
    let mut documents = shipped_documents();
    assert!(
        documents.len() >= 15,
        "only {} shipped documents were read",
        documents.len()
    );
    documents.push((PathBuf::from("UNIT_EFFECTS"), UNIT_EFFECTS.to_owned()));

    for (path, text) in documents {
        let before: EntityDefinition = serde_yaml_ng::from_str(&text)
            .unwrap_or_else(|error| panic!("{} read before: {error}", path.display()));
        let now = entity_yaml::from_str(&text)
            .unwrap_or_else(|error| panic!("{} reads now: {error}", path.display()));
        assert_eq!(now, before, "{} reads differently", path.display());
    }
}
