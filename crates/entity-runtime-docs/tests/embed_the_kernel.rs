//! The program the "Embed the kernel" guide shows, run by the gate so the page cannot drift from
//! what the libraries do.

use entity_core::{CoreError, Registry, Runtime};
use serde_json::json;

const REFUND: &str = include_str!("../../../examples/refund.yaml");

#[test]
fn a_refund_is_drafted_submitted_refused_for_an_agent_and_approved_for_a_human() {
    // Parse, then register: registration is where a definition is validated.
    let definition = entity_yaml::from_str(REFUND).expect("the YAML parses");
    let mut registry = Registry::new();
    registry
        .register(definition)
        .expect("the definition is valid");
    registry
        .validate_all()
        .expect("every referenced type is registered");
    let runtime = Runtime::new(&registry);

    // The caller supplies the identity; the kernel invents none.
    let drafted = runtime
        .create(
            "refund",
            1,
            "refund-104",
            json!({ "order_id": "order-88", "amount_cents": 12_500, "evidence_count": 2 }),
        )
        .expect("creation is accepted");
    let submitted = runtime
        .execute(&drafted.instance, "submit", json!({}))
        .expect("submit is legal from draft");
    assert_eq!(submitted.instance.revision, 2);

    // An agent may propose approval; the rule refuses it, and nothing changes.
    let refused = runtime.execute(
        &submitted.instance,
        "approve",
        json!({ "actor_role": "agent", "reason": "customer supplied delivery evidence" }),
    );
    match refused {
        Err(CoreError::PreconditionFailed { rule, .. }) => {
            assert_eq!(rule.as_deref(), Some("large_refunds_need_a_human"));
        }
        other => panic!("expected a policy refusal, got {other:?}"),
    }
    assert_eq!(submitted.instance.lifecycle_state, "submitted");

    // The trusted shell, not the model, says a human approved.
    let approved = runtime
        .execute(
            &submitted.instance,
            "approve",
            json!({ "actor_role": "human", "reason": "supervisor verified the evidence" }),
        )
        .expect("a human may approve");
    assert_eq!(approved.instance.lifecycle_state, "approved");
    assert_eq!(approved.instance.revision, 3);
    assert_eq!(approved.events[0].event_type, "RefundApproved");
}
