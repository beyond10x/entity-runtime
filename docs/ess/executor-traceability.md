# Executor contract traceability

`entity.executor` calls the actual `entity_executor::Executor` over a scenario-owned
`MemoryRecordedStore`. The public constructor still selects no clock, ID generator or async
runtime. Recording metadata, dynamic definitions and every request identity are authored inputs.
The adapter uses the crate's existing safe `test_support::{block_on,poll_once}` polling helpers.

Each command returns the same `status`, concrete `error` variant and lossless JSON `value`
observation used by the storage adapter. Rust `AppendOutcome` is represented explicitly as
`Empty`, `Committed { receipt, replayed }`, or `Historical { evidence, assurance }`.
Original receipt fields are copied individually from the actual receipt; no receipt, successful
observation or expected decision is synthesized by the adapter. Read commands inspect the actual
store after execution. `HistoryCoordinates` is a projection of actual entry IDs, kinds,
revisions, events and receipts, not a pass/fail assertion. `LookupReceipt` returns the actual
committed receipt or the imported evidence that carries no receipt.

## Native and supplied capabilities

`Register` and `ClearRegistry` arrange the real registry. `ScriptAppend` selects the native
`MemoryRecordedStore::script_next_append` behavior. `CancelBatch` either drops an unpolled actual
executor future, or polls it once and records whether it returned `Ready` or `Pending` before
dropping it. `Trace`, `ClearTrace`, `SeedImported` and `Tamper` expose the real provider APIs.

`RecordingRefusals` installs a separately supplied implementation of `AsyncRefusalRecorder`.
It records the exact calls the real executor makes and exposes their arguments. This is a port
fixture, not a claim that MemoryRecordedStore implements refusal persistence. The duplicate
refusal storage behavior belongs to that fixture; the contract assertion is that the executor
forwards the actual refusal before returning and excludes malformed requests.

`SupplyLoadFailure` installs an explicitly supplied `Unreachable` or `Forked` reader failure.
`SupplyHistory` installs authored complete lineage evidence on a supplied reader. A supplied
lineage writer captures the real executor's append request and returns
`WriteFailure::NotCommitted(Backend(...))`; **it never reports a successful commit**.
`CapturedMerge` observes the captured merge heads/base and the actual resulting instance.
These scenarios prove executor merge preparation and refusal propagation. They do not claim
native MemoryRecordedStore fork persistence or successful durable merge behavior; that provider
has no lineage capability. The store domain separately executes the pure lineage verifier.

## Requirement register

| Requirements / normative authority | Public API and implementation evidence | Named scenarios |
|---|---|---|
| R-121; `recorded-execution-v0.1.md` additive boundary | `Executor::new`, synchronous kernel with boxed Send storage ports; `entity-executor/Cargo.toml` | Static dependency/MSRV and async-port tests remain in the repository gate |
| R-124, R-130 | `Executor::batch` shape validation before `recover_existing` | `empty-batches-perform-no-io`, `malformed-create-is-rejected-before-io`, `duplicate-identities-refuse-before-io`, `single-key-must-match-member` |
| R-123 | `decide_and_append` transaction-local overlay, one mandatory append | `mixed-batch-orders-decisions-and-observations`, `stale-observation-rolls-back-whole-batch`, `complete-decisions-retain-duplicate-events` |
| R-122 | `recover_existing`, saved definition/request matching and `verify_subject_prefix` | `saved-definition-retry-precedes-current-authority`, `changed-request-conflicts`, `corrupt-history-never-recovers-success`, `service-1-saved-definition-retry`, `service-2-conditional-presence-retry`, `service-3-fulfillment-retry` |
| R-124 | Disjoint single/named keys, global identity and original member receipts | `named-and-single-retries-keep-original-receipts`, `changed-batch-order-conflicts`, `global-record-identities-span-subjects-and-kinds` |
| R-127 | Mandatory append plus uncertain recovery using the same original identities | `uncertain-response-recovers-one-original-receipt`, `dropped-pending-response-is-not-rollback`, `dropping-before-poll-dispatches-nothing` |
| R-104 and R-122 | Supplied reader refusal propagates through the real executor | `unreachable-is-never-absent` |
| R-126 | Exact imported matching and named unverifiability | `imported-exact-retry-is-historical`, `historical-observation-has-no-committed-receipt`, `imported-missing-definition-cannot-prove-retry` |
| R-157, R-159 | `Executor::recording_refusals`, actual `RecordedRefusal` arguments | `optional-refusal-recorder-sees-conflicts-not-malformed-input`, `ordinary-forked-write-records-refusal`, `kernel-refusal-is-recorded` |
| Declared refusal precedes success fulfillment validation | `Executor::execute`, completed input and stored-field guard refusals preserve their outcome, error and message without state/history/receipt changes | `refusal-ignores-success-fulfillments`, `subject-refusal-ignores-success-fulfillments` |
| R-149, R-122, R-123 | `Executor::create` selects declared refusals before existence; `Executor::execute_versioned` and `Executor::batch_versioned` bind explicit definition authority before loading, preserve exact historical retries and append atomically | `executor-input-refusal-existing-create`, `executor-input-refusal-missing-execute`, `issue-50-batch-refusal-is-atomic`, `issue-50-malformed-version`, `issue-50-versioned-merge-preserves-proof` |
| R-156–R-158, executor preparation portion | `merge_base` verifies heads, selected first state and highest revision; `branch_tips` determines append parents | `merge-checks-head-and-highest-revision`, `merge-candidate-uses-first-state-and-all-tips`, `merge-appends-over-observation-tips`, `empty-history-is-not-a-merge` |

The complete R-158 durable fork/merge contract additionally requires a lineage-capable provider;
its existing Eventlog provider tests are outside this five-crate provider scope. The selected
executor behavior is exercised through supplied public ports without expanding that scope.
All returned refusals preserve the actual Rust variant; they are not collapsed into a generic
successful result. Arbitrary generated String witnesses report real deserialization errors,
while authored scenarios carry the precise valid records and expectations required here.

Existing `crates/entity-executor/tests/async_recorded_contract.rs` and service retry/fulfillment
suites remain independent checks. They are evidence, not code called by these adapters.
Integration retains exact suites, execution reports, model digest, implementation source identity
and mutation evidence, and gates coverage removals explicitly before authoritative adoption.
