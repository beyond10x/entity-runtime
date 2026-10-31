//! Fragment expansion belongs to the YAML adapter, not the kernel's definition model.

use entity_core::{CoreError, Registry, Runtime};
use serde_json::json;

const FRAGMENTS: &str = r#"
$defs:
  money:
    type: integer
    required: true
  audit_fields:
    fields:
      actor: { type: string, required: true }
      amount: { $ref: '#/$defs/money' }
"#;

fn expanded(entity: &str) -> String {
    format!(
        r#"
entity: {entity}
version: 1
schema:
  fields:
    actor: {{ type: string, required: true }}
    amount: {{ type: integer, required: true }}
lifecycle: {{ initial: open, states: [open] }}
operations: {{}}
"#
    )
}

fn assert_equivalent(authored: &str, entity: &str) {
    let actual = entity_yaml::from_str(authored).expect("resolve fragments before kernel parsing");
    let expected = entity_yaml::from_str(&expanded(entity)).expect("parse expanded definition");
    assert_eq!(
        serde_json::to_value(&actual).expect("serialize resolved definition"),
        serde_json::to_value(&expected).expect("serialize expanded definition"),
        "adapter expansion must produce the unchanged kernel model"
    );

    let mut actual_registry = Registry::new();
    actual_registry.register(actual).expect("validate resolved");
    let mut expected_registry = Registry::new();
    expected_registry
        .register(expected)
        .expect("validate expanded");
    let actual_runtime = Runtime::new(&actual_registry);
    let expected_runtime = Runtime::new(&expected_registry);

    let fields = json!({"actor": "alice", "amount": 1200});
    let actual_decision = actual_runtime
        .create(entity, 1, "item_1", fields.clone())
        .expect("create using resolved schema");
    let expected_decision = expected_runtime
        .create(entity, 1, "item_1", fields)
        .expect("create using expanded schema");
    assert_eq!(
        serde_json::to_value(actual_decision).expect("serialize resolved decision"),
        serde_json::to_value(expected_decision).expect("serialize expanded decision")
    );

    for fields in [json!({}), json!({"actor": 42, "amount": "wrong"})] {
        let actual_error = actual_runtime
            .create(entity, 1, "item_1", fields.clone())
            .expect_err("fragment constraints must be enforced");
        let expected_error = expected_runtime
            .create(entity, 1, "item_1", fields)
            .expect_err("expanded constraints must be enforced");
        assert!(matches!(&actual_error, CoreError::Validation(_)));
        assert_eq!(actual_error, expected_error);
    }
}

#[test]
fn two_definitions_sharing_fragments_validate_like_their_expanded_forms() {
    for entity in ["invoice", "payment"] {
        let authored = format!(
            r#"{FRAGMENTS}
entity: {entity}
version: 1
schema: {{ $ref: '#/$defs/audit_fields' }}
lifecycle: {{ initial: open, states: [open] }}
operations: {{}}
"#
        );
        assert_equivalent(&authored, entity);
    }
}

#[test]
fn two_definitions_inherit_a_base_without_changing_the_kernel_model() {
    for entity in ["invoice", "payment"] {
        let authored = format!(
            r#"{FRAGMENTS}
  base:
    version: 1
    schema: {{ $ref: '#/$defs/audit_fields' }}
    lifecycle: {{ initial: open, states: [open] }}
    operations: {{}}
$ref: '#/$defs/base'
entity: {entity}
"#
        );
        assert_equivalent(&authored, entity);
    }
}
