//! `examples/counter.yaml`, whose `set:` increments are plain YAML mappings, loaded through the YAML
//! adapter and executed by the kernel (R-164).
//!
//! The adapter reads an externally tagged enum only in its `!tag` form, so the point here is that an
//! increment needs no tag: block and flow mappings alike reach the kernel as `{increment: n}`.

use entity_core::{CoreError, Registry, Runtime, SetAssignment};
use serde_json::json;

const COUNTER: &str = include_str!("../../../examples/counter.yaml");

fn spelled(value: &serde_json::Value) -> String {
    serde_json::to_string(value).expect("a value serializes")
}

#[test]
fn an_increment_written_as_a_plain_yaml_mapping_loads_and_executes() {
    let definition = entity_yaml::from_str(COUNTER).expect("valid yaml");
    for (operation, field, amount) in [
        ("view", "views", json!(1)),
        ("unview", "views", json!(-1)),
        ("charge", "spent", json!("$args.amount")),
    ] {
        let set = &definition.operations[operation].set;
        assert!(
            matches!(SetAssignment::of(&set[field]), SetAssignment::Increment(read) if *read == amount),
            "{operation}.set.{field} is {}",
            set[field]
        );
    }

    let mut registry = Registry::new();
    registry.register(definition).expect("a valid definition");
    let runtime = Runtime::new(&registry);
    let opened = runtime
        .create("counter", 1, "c-1", json!({ "name": "home" }))
        .expect("create")
        .instance;
    let viewed = runtime.execute(&opened, "view", json!({})).expect("view");
    assert_eq!(spelled(&viewed.instance.fields["views"]), "1");
    assert_eq!(
        spelled(&viewed.events[0].payload),
        r#"{"after":1,"before":0}"#
    );

    let charged = runtime
        .execute(&viewed.instance, "charge", json!({ "amount": 2.5 }))
        .expect("charge")
        .instance;
    let charged = runtime
        .execute(&charged, "charge", json!({ "amount": 0.25 }))
        .expect("charge again")
        .instance;
    assert_eq!(spelled(&charged.fields["spent"]), "2.75");

    // `views` declares `min: 0`, so the decrement past it is refused after `set`.
    let error = runtime
        .execute(&opened, "unview", json!({}))
        .expect_err("below the minimum");
    assert!(
        matches!(&error, CoreError::Validation(errors) if errors.len() == 1 && errors[0].path == "fields.views"),
        "{error}"
    );
}
