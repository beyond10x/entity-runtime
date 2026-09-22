//! Pass-two regression cases for operation-field fulfillment.

use std::collections::BTreeSet;

use entity_core::{rehydrate, Registry, Runtime};
use serde_json::json;

#[test]
fn a_kernel_event_cannot_invent_service_3_removal_evidence() {
    let definition = serde_json::from_value(json!({
        "entity": "ticket",
        "version": 1,
        "schema": {
            "fields": {
                "title": { "type": "string", "required": true },
                "note": { "type": "string" }
            }
        },
        "lifecycle": { "initial": "open", "states": ["open", "closed"] },
        "create": { "emit": { "type": "TicketOpened", "payload": { "ticket": "$id" } } },
        "operations": {
            "close": {
                "transitions": [{ "from": "open", "to": "closed" }],
                "emits": [{ "type": "TicketClosed", "payload": { "ticket": "$id" } }]
            }
        }
    }))
    .expect("definition parses");
    let mut registry = Registry::new();
    registry.register(definition).expect("definition validates");
    let runtime = Runtime::new(&registry);
    let created = runtime
        .create(
            "ticket",
            1,
            "t-1",
            json!({"title": "review", "note": "must remain"}),
        )
        .expect("creation succeeds");
    let closed = runtime
        .execute(&created.instance, "close", json!({}))
        .expect("legacy operation succeeds");
    assert!(closed.events[0].removed.is_empty());

    let mut forged = closed.events[0].clone();
    forged.removed = BTreeSet::from(["note".to_owned()]);
    let history = [created.events[0].clone(), forged];
    let definition = registry.get("ticket", 1).expect("registered");

    rehydrate(definition, &history)
        .expect_err("kernel/1 has no action that can produce removal evidence");
}
