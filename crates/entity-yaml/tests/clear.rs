//! `examples/reminder.yaml`, whose `set:` clears are plain YAML mappings, loaded through the YAML
//! adapter and executed by the kernel (R-165).
//!
//! Like an increment, a clear needs no `!tag`: block and flow mappings alike reach the kernel as
//! `{cleared: true}`.

use entity_core::{CoreError, Registry, Runtime, SetAssignment};
use serde_json::json;

const REMINDER: &str = include_str!("../../../examples/reminder.yaml");

fn spelled<T: serde::Serialize>(value: &T) -> String {
    serde_json::to_string(value).expect("a value serializes")
}

#[test]
fn a_clear_written_as_a_plain_yaml_mapping_loads_and_executes() {
    let definition = entity_yaml::from_str(REMINDER).expect("valid yaml");
    for (operation, field) in [("wake", "snoozed_until"), ("forget_note", "note")] {
        let set = &definition.operations[operation].set;
        assert!(
            matches!(SetAssignment::of(&set[field]), SetAssignment::Cleared(flag) if *flag == json!(true)),
            "{operation}.set.{field} is {}",
            set[field]
        );
    }

    let mut registry = Registry::new();
    registry.register(definition).expect("a valid definition");
    let runtime = Runtime::new(&registry);
    let opened = runtime
        .create(
            "reminder",
            1,
            "r-1",
            json!({ "title": "call back", "note": "ask about the invoice" }),
        )
        .expect("create")
        .instance;
    let snoozed = runtime
        .execute(
            &opened,
            "snooze",
            json!({ "until": "2026-10-08T09:00:00Z" }),
        )
        .expect("snooze")
        .instance;

    let woken = runtime.execute(&snoozed, "wake", json!({})).expect("wake");
    assert_eq!(
        spelled(&woken.instance.fields),
        r#"{"note":"ask about the invoice","title":"call back"}"#
    );
    assert_eq!(spelled(&woken.events[0].removed), r#"["snoozed_until"]"#);
    assert_eq!(
        spelled(&woken.events[0].payload),
        r#"{"now":{"note":"ask about the invoice","title":"call back"},"was":"2026-10-08T09:00:00Z"}"#
    );

    let forgotten = runtime
        .execute(&woken.instance, "forget_note", json!({}))
        .expect("forget the note")
        .instance;
    assert_eq!(spelled(&forgotten.fields), r#"{"title":"call back"}"#);

    // `wake` is declared from `snoozed` only; the clear changes nothing about the transitions.
    let error = runtime
        .execute(&forgotten, "wake", json!({}))
        .expect_err("not snoozed");
    assert!(
        matches!(&error, CoreError::InvalidTransition { operation, state } if operation == "wake" && state == "active"),
        "{error}"
    );
}
