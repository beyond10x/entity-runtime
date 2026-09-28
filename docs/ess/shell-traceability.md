# Shared synchronous shell contract

Scope: `entity-shell`, over caller-supplied `MemoryStore` and `FileStore`. The ESS declaration is
`ess/domains/shell.yaml`; its component is `ess/components/shell.yaml`; authored scenarios live in
`ess/scenarios/shell/`. `checks/ess-conformance/src/shell.rs` calls the actual public Rust APIs.
This register retains the existing requirement IDs and design authority; it does not broaden the
crate into a publisher, projection engine, asynchronous executor, or source of ambient identity.

| Requirement / public contract | Normative and implementation evidence | Named scenarios |
| --- | --- | --- |
| R-80, R-93, R-117: provider-backed create/get/list/events/execute | `docs/design/generated-surfaces-v0.1.md` §§3,5; `StoredRuntime::{create,get,list,events,execute}` in `crates/entity-shell/src/lib.rs`; store provider traits in `crates/entity-store/src/lib.rs` | `memory-round-trip`, `file-round-trip` |
| Missing get/events/execute is typed `NotFound`; an absent entity lists no IDs | `StoredRuntime::get`; events checks existence before reading events; execute loads before further checks | `memory-missing-subjects`, `file-missing-subjects` |
| R-117: observed revision checked before evaluation | generated-surfaces §3; `StoredRuntime::execute`; existing `stale_agent_intent_is_refused_before_the_kernel_or_store_changes_anything` | `memory-stale-intent-precedes-kernel`, `file-stale-intent-precedes-kernel` |
| R-84, R-117: same revision expectation checked at commit | generated-surfaces §§3,5; `commit_recorded(..., Expect::Revision(expected_revision))`; actual provider revision guard | `memory-revision-rechecked-at-commit`, `file-revision-rechecked-at-commit` |
| R-88: original execute retry after state advancement; saved definition normalization | `StoredRuntime::execute` history branch; `docs/design/store-v0.2.md` §1; existing `an_exact_execute_retry_returns_the_original_commit_after_state_has_advanced` | `memory-retry-uses-original-record`, `file-retry-uses-original-record` |
| R-88: exact creation retry after advancement; conflicting fresh creation | `StoredRuntime::create`, real provider global record-identity and revision guards | `memory-create-retry-after-advancement`, `file-create-retry-after-advancement` |
| R-88: changed operation, revision, arguments or provenance conflicts | saved command/provenance comparison in `StoredRuntime::execute`; exact envelope resealing | `memory-changed-retry-conflicts`, `file-changed-retry-conflicts` |
| R-86, R-87: complete recording provenance validated before provider commit | `RecordedCommit::new`, `Recording::seal`, `Envelope::validate` in `entity-store`; shell maps failure to `ShellError::Recording` | `memory-invalid-recording-publishes-nothing`, `file-invalid-recording-publishes-nothing` |
| Stable provider error kinds/boundaries, including unavailable versus absent | `ShellError::{kind,boundary}`; `StateProvider`, `EventProvider`, `HistoryProvider`, `Store` calls | `provider-failures-preserve-classification`, `file-provider-failures-preserve-classification` |
| A failed state/history read cannot become retry success | `StoredRuntime::execute` reads current state before history and propagates either failure | `retry-does-not-hide-read-failure` |
| Kernel evaluation precedes recording validation | ordering in `StoredRuntime::{create,execute}` | `kernel-refusal-precedes-recording-validation` |
| Duplicate events stay ordered and zero-event execution advances revision | actual records returned by the kernel and stored by providers | `memory-round-trip`, `file-round-trip` |

`Create`, `Execute`, `Get`, `List`, and `Events` translate directly to `StoredRuntime`. `Register`
and `Replace` call the real registry; `ClearRegistry` supplies an empty caller registry. `Records`
is a real provider read, independent of the shell's previous return. Returned `value` carries the
exact serialized Rust result; `revision`, `state`, `fields`, `record_id`, `event_types`, and `ids`
are projections of that same actual result. No adapter field claims that an expectation passed.
Errors report `ShellError::kind()` and `boundary()`, the concrete variant, and actual coordinates.
Malformed wire documents are explicit `decode` input refusals.

`Open` and `Reopen` select and reopen real isolated providers. `Fault`, `Race`, `Calls`, and
`ClearCalls` are declared fixture controls, not new production APIs. `Fault` configures one failure
at a named provider edge; successful calls always delegate to the real provider. `Race` uses the
real kernel to prepare a competing recorded decision and commits it through the real provider
immediately before the shell's attempted commit. The scenario then reads actual state, events and
history. `Calls` observes calls made to that delegating provider, never inferred calls.

The retry contract is deliberately synchronous: it first loads current state, then consults the
subject's recorded history. It does not claim the asynchronous executor's recovery-first ordering
or independently verified complete history prefix. The public shared-shell operation signature
accepts ordinary operation arguments, with no explicit service fulfillment argument. Service
fulfillment execution belongs to the core/executor contracts. R-80's wider shell architecture does
not imply this library publishes events or updates projections.

Static boundaries remain enforced by Cargo manifests and the existing kernel purity/dependency
gate. The conformance checker depends on this production crate, which has no ESS dependency. The
shared shell itself performs no direct filesystem/network IO; supplied providers own their edges.

Mutation obligation: disabling `StoredRuntime::execute`'s pre-kernel stale guard must fail
`memory-stale-intent-precedes-kernel` and `file-stale-intent-precedes-kernel`. Checking only the
string `revision_conflict` would miss some guard defects; these scenarios also require the concrete
`StaleRevision` variant, coordinates, store boundary and absence of a commit call. Retained runner
reports and mutation artifacts belong to the integration evidence package; this document alone is
not execution evidence.

Resolved integration finding: the ESS 0.37.0 direct-return extension initially reused
selection's per-string 4096-byte resource ceiling for ordinary command responses. Real `Records`
responses in `memory-round-trip`, `file-round-trip`, `memory-retry-uses-original-record`, and
`file-retry-uses-original-record` exceed that ceiling and are refused as
`ESS-CF-PAYLOAD: response field value: resource`. A minimal ESS reproducer is a command declaring
one String response, a `returns: true` outcome, and an actual response string of 4097 ASCII bytes:
selection validation rejects it even when no selection is declared. The complete history response
remained intact. The governed ESS correction now validates direct responses with a separate
response budget while retaining the bounded selection contract. The complete shell/query lane
passes with full history values; exact reports and mutation evidence are retained by integration.
