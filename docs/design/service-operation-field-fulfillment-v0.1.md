# Operation-field fulfillment and replay

Implementation contract for story:service-operation-field-fulfillment, derived from the complete
operation-field correction of ESS lowerer review F1. Approved ESS evolution service acceptance
requires preserving host-owned writes in billing and gatepass. Both lowerer design examinations
are closed; this page projects the corrected target contract, not a third review.

The source correction is docs/design/ess-evolution/entity-runtime-lowering.md in ESS, SHA256
22d0eb4bb0c0847855b75e2458c3ce57d573c3d12e068ddcf360a31001a536ce. The following target
section is copied exactly. The accepted base da5d3687 retains its narrower service/2 behavior.
Implementation and actual compatibility/replay gates must establish service/3; this page does not
claim the behavior exists. SDK policy types below describe the external consumer handoff only.

## Minimal Entity Runtime amendment required by this contract

The required amendment is one operation-only fulfillment phase in `entity-core`; it is not a
stateless engine, registry, host outcome selector, or general property bag. `OutcomeDefinition`
gains one closed, ordered map available only under new `Semantics::Service3`:

```rust
pub struct OutcomeDefinition {
    // existing fields unchanged
    pub fulfills: BTreeMap<String, OperationFieldRequirement>,
}

pub struct OperationFieldRequirement {
    pub actions: OperationFieldActions,
}

pub enum OperationFieldActions {
    Required,
    Optional,
}

pub enum OperationFieldAction {
    Preserve,
    Set { value: serde_json::Value },
    Remove,
}
```

Registration permits `fulfills` only on accepting operation outcomes, rejects a key also present in
`set`, rejects unknown fields and the identity field, and requires `Required` versus `Optional` to
equal the target field's outer presence. `Required` admits `Set` and `Preserve`; `Optional` admits
all three actions. `Set` validates against the exact target `FieldDefinition`; `Remove` on a required
field is a typed refusal. There is no user-supplied field name or schema in the invocation.

`PreparedOperation` retains its existing input-only partial evaluation. Add the following post-load
surface without changing the existing `continue_with` signature or behavior for `kernel/1`,
`service/1`, and `service/2` definitions:

```rust
pub enum LoadedDecision<'a> {
    Complete(Evaluation),
    NeedsFulfillment(PreparedOutcome<'a>),
}

impl PreparedOperation<'_> {
    pub fn select_with<'a>(self, instance: &'a EntityInstance)
        -> Result<LoadedDecision<'a>, CoreError>;
}

impl PreparedOutcome<'_> {
    pub fn outcome(&self) -> &str;
    pub fn requirements(&self) -> &BTreeMap<String, OperationFieldRequirement>;
    pub fn complete(
        self,
        actions: BTreeMap<String, OperationFieldAction>,
    ) -> Result<Evaluation, CoreError>;
}
```

`select_with` performs the existing entity, exact subject, state, ordered outcome selection,
refusal, state admission, and precondition steps. A refusal returns `Complete(Refused)` and never
requests fulfillment. An accepted branch with an empty map follows the existing path. Otherwise it
returns the opaque selected continuation. `complete` requires exactly the advertised field keys,
applies actions to the loaded field map, validates the resulting schema, checks the identity mirror
and invariants, materializes events and response from that one resulting field map, and builds one
decision. The continuation owns the selected branch; the SDK cannot substitute an outcome. The
existing `continue_with` delegates to this flow and returns a typed `FulfillmentRequired` error only
when called on a `service/3` branch that actually needs actions.

This placement handles facts unavailable before load. The SDK invokes no fulfillment policy while
`decide_before_load` can still answer an input-only refusal. After the exact subject is loaded,
`select_with` chooses the branch before the SDK sees `requirements()`. Only then does the SDK call
the bound policies with the normalized command input, verified context, read-only loaded instance,
selected outcome coordinate, and named field requirement. No placeholder action is used to probe a
branch.

The SDK `/4` policy is a closed binding per exact command/outcome/field:

```rust
pub enum OperationFieldPolicy {
    Preserve,
    Remove,
    CommandField { field: String },
    Obligation { name: DefinitionId },
}

pub trait OperationFieldObligation {
    fn fulfill(
        &self,
        context: OperationFieldContext<'_>,
    ) -> Result<OperationFieldAction, UnmetObligation>;
}
```

`Preserve`, `Remove`, and `CommandField` are interpreted directly and type-checked at SDK `/4`
compile time; `Obligation` reuses the SDK's existing versioned `ObligationProviderId` admission and
returns one action per invocation. There is no lookup by an arbitrary runtime string: the compiled
realization plan carries the resolved provider and exact coordinate. Missing, duplicate, extra,
wrong-type, identity-targeting, required-field `Remove`, and undeclared-action bindings are compile
diagnostics. For the accepted fixtures, `IssueInvoice.issued.issued_at` binds a clock obligation that
returns `Set` once after selection, and `AdmitVisitor.admitted.badge` binds
`CommandField { field: "badge" }`. Every other omitted operation field has an explicit
source-backed policy, commonly `Preserve`; the compiler does not synthesize one.

A chosen `Set` value is inserted before event and response materialization. An event or response
explicitly bound to the same semantic entity value reads the post-action `$fields.<field>`; a
command-field binding such as gatepass badge reads the same normalized command value for both the
action and its already declared event payload. `Preserve` exposes the loaded value and `Remove`
exposes absence. Independent event/response obligations remain independent. Thus reuse is explicit
and one invocation never calls a value-producing obligation twice.

Record and replay must preserve the action as well as the result. Under new `service/3`,
`DecisionCommand::Execute` carries an ordered `fulfillments` map, `DecisionRecord` and every
`DomainEvent` carry an ordered `removed` field-name set beside `changed`, and replay supplies the
recorded actions to the same continuation before byte-comparing the complete decision. Folding
removes `removed` members before inserting `changed`; the two sets must be disjoint. This requires
new `er.record/4` and `er.request/4` domains. Existing `kernel/1`, `service/1`, `service/2`,
`er.record/1` through `/3`, `er.request/1` through `/3`, and `er.batch/1` bytes and behavior remain
unchanged; new readers accept all versions, older readers refuse `/4`, and a `fulfills` key under an
earlier service semantics is a registration error. The accepted target pin remains `da5d3687` until
this amendment is actually implemented and accepted.


## Requirement references

R-152 pins the typed operation fulfillment and post-selection continuation contract above.
R-153 pins its durable actions, removal evidence, exact retry/replay and versioned framing.
The copied target contract above remains unchanged; implementation acceptance is recorded
separately after the full gate and independent source examination.
