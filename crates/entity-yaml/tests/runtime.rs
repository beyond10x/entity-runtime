//! The worked example, `examples/order.yaml`, driven through the YAML adapter and the kernel: the
//! seven scenarios the runtime was first specified against.

use entity_core::{CoreError, DefinitionError, Registry, Runtime};
use serde_json::json;

const ORDER: &str = include_str!("../../../examples/order.yaml");

fn runtime() -> (Registry, entity_core::EntityDefinition) {
    let definition = entity_yaml::from_str(ORDER).expect("valid yaml");
    let mut registry = Registry::new();
    registry
        .register(definition.clone())
        .expect("valid definition");
    (registry, definition)
}

#[test]
fn yaml_definition_drives_lifecycle_and_events() {
    let (registry, _) = runtime();
    let runtime = Runtime::new(&registry);

    let created = runtime
        .create(
            "order",
            1,
            "ord_1",
            json!({
                "customer_id": "cus_1",
                "total_cents": 1200
            }),
        )
        .expect("create");

    assert_eq!(created.instance.lifecycle_state, "draft");
    assert_eq!(created.instance.fields["priority"], json!("normal"));
    assert_eq!(created.events[0].event_type, "OrderCreated");

    let submitted = runtime
        .execute(&created.instance, "submit", json!({ "actor": "alice" }))
        .expect("submit");

    assert_eq!(submitted.instance.lifecycle_state, "submitted");
    assert_eq!(submitted.events[0].event_type, "OrderSubmitted");
    assert_eq!(submitted.events[0].payload["actor"], json!("alice"));
}

#[test]
fn invalid_transition_is_rejected_without_a_new_decision() {
    let (registry, _) = runtime();
    let runtime = Runtime::new(&registry);

    let created = runtime
        .create(
            "order",
            1,
            "ord_1",
            json!({
                "customer_id": "cus_1",
                "total_cents": 1200
            }),
        )
        .expect("create");

    let error = runtime
        .execute(
            &created.instance,
            "fulfill",
            json!({ "tracking_number": "TRACK-1" }),
        )
        .expect_err("draft -> fulfill must be invalid");

    assert!(matches!(
        error,
        CoreError::InvalidTransition { operation, state }
            if operation == "fulfill" && state == "draft"
    ));
}

#[test]
fn operation_arguments_are_schema_validated() {
    let (registry, _) = runtime();
    let runtime = Runtime::new(&registry);

    let created = runtime
        .create(
            "order",
            1,
            "ord_1",
            json!({
                "customer_id": "cus_1",
                "total_cents": 1200
            }),
        )
        .expect("create");

    let error = runtime
        .execute(&created.instance, "submit", json!({}))
        .expect_err("actor is required");

    assert!(matches!(error, CoreError::Validation(_)));
}

#[test]
fn operation_precondition_blocks_mutation_and_event_emission() {
    let (registry, _) = runtime();
    let runtime = Runtime::new(&registry);

    let created = runtime
        .create(
            "order",
            1,
            "ord_zero",
            json!({
                "customer_id": "cus_1",
                "total_cents": 0
            }),
        )
        .expect("create");

    let submitted = runtime
        .execute(&created.instance, "submit", json!({ "actor": "alice" }))
        .expect("submit");

    let error = runtime
        .execute(
            &submitted.instance,
            "approve",
            json!({ "actor": "manager" }),
        )
        .expect_err("zero-value order cannot be approved");

    assert!(matches!(
        error,
        CoreError::PreconditionFailed { operation, rule, .. }
            if operation == "approve" && rule.as_deref() == Some("positive_total")
    ));

    // The caller still owns the unchanged submitted instance. The pure core
    // never partially mutates caller-owned state when a rule fails.
    assert_eq!(submitted.instance.lifecycle_state, "submitted");
    assert_eq!(submitted.instance.revision, 2);
}

#[test]
fn entity_invariant_is_checked_after_create() {
    let yaml = r#"
entity: account
version: 1
schema:
  fields:
    balance:
      type: integer
      required: true
lifecycle:
  initial: open
  states: [open]
invariants:
  - name: positive_balance
    assert:
      gt: [$fields.balance, 0]
    message: balance must be positive
operations: {}
"#;

    let definition = entity_yaml::from_str(yaml).expect("parse");
    let mut registry = Registry::new();
    registry.register(definition).expect("register");
    let runtime = Runtime::new(&registry);

    let error = runtime
        .create("account", 1, "acc_1", json!({ "balance": 0 }))
        .expect_err("invariant must reject materialized state");

    assert!(matches!(
        error,
        CoreError::InvariantViolation { rule, .. }
            if rule.as_deref() == Some("positive_balance")
    ));
}

#[test]
fn definition_rejects_operation_only_reference_in_invariant() {
    let yaml = r#"
entity: broken
version: 1
schema:
  fields:
    value:
      type: integer
lifecycle:
  initial: open
  states: [open]
invariants:
  - assert:
      eq: [$args.value, 1]
operations: {}
"#;

    let definition = entity_yaml::from_str(yaml).expect("parse");
    let mut registry = Registry::new();
    let error = registry
        .register(definition)
        .expect_err("invariant cannot depend on command arguments");

    assert!(matches!(error, DefinitionError::InvalidRule { .. }));
}

#[test]
fn entity_invariant_is_checked_after_operation_before_events_escape() {
    let yaml = r#"
entity: ticket
version: 1
schema:
  fields:
    resolution:
      type: string
lifecycle:
  initial: open
  states: [open, closed]
invariants:
  - name: closed_requires_resolution
    assert:
      any:
        - ne: [$state, closed]
        - exists: $fields.resolution
    message: closed tickets need a resolution
operations:
  close:
    transitions:
      - from: open
        to: closed
    emits:
      - type: TicketClosed
        payload:
          id: $id
"#;

    let definition = entity_yaml::from_str(yaml).expect("parse");
    let mut registry = Registry::new();
    registry.register(definition).expect("register");
    let runtime = Runtime::new(&registry);

    let created = runtime
        .create("ticket", 1, "ticket_1", json!({}))
        .expect("create");

    let error = runtime
        .execute(&created.instance, "close", json!({}))
        .expect_err("post-operation invariant must fail");

    assert!(matches!(
        error,
        CoreError::InvariantViolation { rule, .. }
            if rule.as_deref() == Some("closed_requires_resolution")
    ));

    assert_eq!(created.instance.lifecycle_state, "open");
    assert_eq!(created.instance.revision, 1);
}
