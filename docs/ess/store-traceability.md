# Storage contract traceability

The storage domain describes the public APIs in `crates/entity-store`. Its authored scenarios
pass complete, independently authored records into the actual providers. They do not call the
legacy `conformance::run` checker or turn existing Rust tests into boolean observations.
`ess/ess-inputs.yaml` is the canonical composition entry point. The retained
`ess/recorded-execution/` entry point owns the typed coordinate inventory; its subject, record,
batch and receipt-member concepts are the coordinates carried by these complete Rust values.
The kernel's dynamic definition and value documents retain their owner in `entity.core`.

## Value and refusal boundary

Every command accepts a `RequestDocument`, a lossless JSON spelling of its listed Rust arguments.
The `returned` outcome represents the public function returning. Its fields are `status` (`ok` or
`error`), `error` (the concrete Rust error variant, empty for success), and `value` (the serialized
actual return value, or a diagnostic document from the actual error). Malformed JSON is reported
as `Deserialize`; it is neither a successful library call nor an adapter/runner failure. This
also makes generated arbitrary String witnesses observable without silently substituting valid
input. Authored scenarios supply complete valid documents and assert exact returns and refusals.

No provider state is constructed by computing an expected result. `Commit` accepts a complete
`Decision`; `CommitRecorded` accepts a complete `RecordedCommit`; `Append` accepts ordered
complete entries and explicit expectations. For append fixtures, omitted comparison bytes are
encoded by the public canonical encoder as request construction, then the writer validates them.
History fixtures likewise retain authored entries, coordinates and lineage while the public
encoders prepare their canonical comparison material. Explicit comparison bytes override this
fixture preparation and reach the implementation unchanged.

Exact numbers remain in `serde_json`'s `arbitrary_precision` representation and cross ESS as
serialized documents. No `f64` conversion occurs. An observation includes complete caller
provenance, with absent optional keys rejected and explicit `null` preserved.

## Provider capability boundary

| Provider | Declared capability | Evidence boundary |
|---|---|---|
| `MemoryStore` | Synchronous state, sorted IDs, events, recorded decisions and observations; ordered `AtomicBatchStore` | In-memory maps; no restart claim |
| `FileStore` | Same synchronous recorded surface; complete subject replacement, reopen, confined paths and out-of-place migration | Only one subject is atomic; **no multi-subject AtomicBatchStore claim** |
| `MemoryRecordedStore` | Async state, identity/batch lookup, mixed history, complete snapshot and atomic recorded append | Original receipt coordinates; native uncertainty and dropped-response scripts |
| Explicit history fixtures | Pure history verification including lineage | Supplied facts only; they establish no provider persistence |

`FileStore::open` defers validation until an operation. `Reopen` constructs another handle on the
same isolated root. `WriteFileFixture`, `ReadFileFixture` and `FileExists` are explicit test
arrangement/observation commands, confined to one scenario's temporary directory. They model
no additional FileStore methods. `SetPosition` and `Tamper` expose the provider's existing named
conformance controls. `Trace` and `ClearTrace` expose its real port-call trace. Directory lifetime
belongs to the scenario, and cleanup occurs after provider use.

## Requirement register

| Requirements / normative authority | Actual API and implementation | Named scenarios |
|---|---|---|
| R-83, R-85; `store-v0.2.md` §1 | `Store::commit_recorded`, `StateProvider::load`, `EventProvider::events`; `src/memory.rs`, `src/file.rs` | `memory-absence`, `file-absence`, `memory-ordered-duplicate-events`, `file-ordered-duplicate-events` |
| R-84; `store-v0.1.md` §3 | `check`, explicit `Expect`, commit checks before writes | `expectation-absent-is-distinct-from-revision-zero`, `memory-stale-write-preserves-state`, `file-stale-write-preserves-state` |
| R-86, R-87; `store-v0.2.md` §1 | `Envelope::{new,validate}`, `Recording::seal`, `RecordedCommit::validate`, `RecordedObservation::validate` | `envelope-null-provenance-is-explicit`, `memory-invalid-recording-is-inert`, `file-invalid-recording-is-inert`, `recorded-commit-result-must-match-instance`, `observation-revisions-are-positive-signed-range` |
| R-88; `store-v0.2.md` §1 | Global record-ID maps/index, exact recorded retry before current-revision check | `memory-exact-retry-after-state-advances`, `file-exact-retry-after-state-advances`, `memory-global-record-identity`, `file-global-record-identity` |
| R-109; `store-v0.1.md` §11 | `StateProvider::ids` | `memory-listed-identities-are-sorted`, `file-listed-identities-are-sorted` |
| R-112; `store-v0.1.md` §13 | `AtomicBatchStore::commit_batch` on MemoryStore only | `memory-atomic-order`, `memory-atomic-rollback`, `memory-empty-batch` |
| R-103; `store-v0.2.md` §2 | `FileStore::open`, complete subject document reads and replacement | `file-reopen`, `file-corrupt-marker-is-not-absence` |
| R-98–R-100, R-167; `store-v0.1.md` §7 | `project`; definition/version/lifecycle filtering, ordered scalar grouping, service collection address keys read as the kernel reads them | `projection-groups-sorted-identities`, `projection-scalar-and-nested-keys`, `projection-collection-address-keys`; reference validity remains the core registration contract |
| R-103 and library part of R-91; `store-v0.2.md` | `migrate_file_store_v1` | `migration-dry-run-and-out-of-place`, `migration-invalid-source-publishes-nothing` |
| Library acquisition part of R-155; `eventlog-provider-facades-and-legacy-imports.md` | `LegacyStoreSource::acquire_legacy` on FileStore | `legacy-acquisition-of-absent-source-is-read-only`, `legacy-acquisition-preserves-only-known-order` |
| R-123; `recorded-execution-v0.1.md` | `AsyncRecordedWriter::append`, real transaction-local validation and clone/commit | `recorded-memory-atomic-observation-order`, `recorded-memory-atomic-rollback` |
| R-130; `recorded-execution-v0.1.md` | Public writer revalidates `AppendRequest` before script consumption | `recorded-memory-writer-revalidates-public-members` |
| R-125; `recorded-execution-encoding-v0.1.md` | Canonical encoders, checked positions, opaque coordinates | `canonical-byte-encoding-preserves-exact-number-tokens`, `recorded-position-overflow-is-atomic`, `coordinates-retain-opaque-subjects-and-disjoint-batch-namespaces` |
| R-125, R-150, R-153; service framing designs | `record_domain`, `request_domain`, `record_framing`, `read_record_in_domain` | `record-domain-kernel-1`, `record-domain-service-1`, `record-domain-service-2`, `record-domain-service-3`, `older-framing-reader-refuses-before-payload` |
| R-122, R-126, R-128 | `verify_subject_history`, `verify_store_histories`, `verify_complete_store` | `complete-assurance-requires-provider-capture`, `tampered-complete-history-is-rejected`, `imported-boundary-retains-limited-assurance` |
| R-129 | `MemoryRecordedStore::seed_imported`, imported evidence validation | `imported-evidence-cannot-exceed-anchor` |
| R-156–R-158, verifier portions | `branch_heads`, `branch_tips` over actual typed records | `lineage-decisions-produce-two-heads`, `observations-are-tips-but-not-decision-heads` |

R-104's unavailable-vs-absent distinction is additionally exercised through the real executor
with an explicitly supplied unavailable state port (`entity.executor/unreachable-is-never-absent`).
A native in-memory or file provider is never mislabeled as a simulated remote provider.

## Static and retained evidence

Existing `crates/entity-store/tests/{both_providers,concurrency,file_record_index,projections,
legacy_acquisition,async_recorded_adversary,async_recorded_adversary_pass2}.rs` and
`src/asynchronous.rs` remain independent repository checks, including object-safe `Send` futures,
public writer validation, complete snapshot provenance and provider concurrency. Existing framing
fixtures remain unchanged. The ESS checker is a standalone workspace; production crates retain
their dependency graphs and MSRV. Source gates additionally check formatting, lints and rustdoc.

Canonical suites, execution reports, model digest and implementation identity are retained by the
shared checker. A named scenario inventory review is required when regeneration changes coverage.
The adapter's complete response comes from the API; expected values occur only in authored ESS
scenario files. Final authority and completion depend on the integration gate and retained
mutation evidence, not on this document's existence.
