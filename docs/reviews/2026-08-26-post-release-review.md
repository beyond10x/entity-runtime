# Post-release review — 0.6.0, 0.7.0 (entity-runtime), 0.25.0 (aep)

Two independent read-only reviewers, 2026-08-26, against the pushed commits
`f70bea4`, `311e186` (entity-runtime) and `cefb749`, `694e0a7` (aep).
All three tags are **already published**. Every finding below is against released code.

Reviewer 1 lens: correctness and safety of the code itself.
Reviewer 2 lens: claims vs. code. (pending — appended when it reports)

## Reviewer 1 — 13 findings, 11 CONFIRMED

Severity ordering is mine, not the reviewer's: a finding that makes a *published claim false*
outranks one that only makes the software awkward.

### A. Published claims that are false

| # | claim | what the code does | evidence |
|---|---|---|---|
| 7 | R-97 / 0.6.0 changelog: "replay can reach no state `execute` would have refused" | `replay.rs:119` — `None if index == 0 => {}` skips the `permits` check **and** any comparison with `lifecycle.initial`. A forged creation event lands in any state in `states`, with fields of the wrong type and fields the schema does not declare | one event `{from_state: None, to_state: "closed", revision: 1}` folds to `closed`; `create()` yields `open` |
| 2 | R-106 / `OnDivergence::Refuse` doc (hybrid.rs:66): "Neither side moves" | `hybrid.rs:353-357` commits **local first**, then asks the remote; a remote refusal returns `Err` without undoing the local write, and records no divergence | local at revision 1 after a write the caller was told failed; `divergences recorded: 0` |
| 3 | 0.25.0 commit + tag: "Cycle detection holds over the combined graph" | `artifact.rs:1970` exempts every member-qualified target from the check; `find_cycle` (`artifact.rs:2022`) walks only `self.artifacts`; no combined graph is ever built | same cycle, one prefix apart: unprefixed → refused; `self/`-prefixed → **valid**. Cross-repo cycle → `crossings --strict` exit 0. This is a regression |
| 4 | 0.25.0: an ambiguous reference returns no document; a failed member is empty, never absent | `workspace.rs:138-146` drops any member with `tree: None` from the assembly entirely; `resolve` is called `fetch: false`; `show` discards the unresolved list (`workspace.rs:272`) | same workspace, member `beta` respelled as a pinned git source: ambiguity vanishes and one document is returned, exit 0 |
| 5 | R-108 / 0.7.0: `catch_up` "keeps what it could not replay rather than reporting success" | `hybrid.rs:263` `.ok().flatten()` and `:279` `.unwrap_or_default()` — a local `load` error reads as "the write is gone" | corrupt local state file → `outstanding = 0`, `divergences kept = 0`, remote holds nothing. Corrupt event log → remote state moves with **zero events** behind it |
| 10 | 0.7.0: an unknown wire version "is refused by name" | `lib.rs:279` returns `Err(String)`; `lib.rs:202` maps every transport `Err` to `Unreachable` | a live, answering, version-incompatible remote reports `is_unreachable() = true`. A `ServeStale` hybrid then serves stale data forever against a remote that is up |

### B. Correctness defects that do not contradict a published claim

| # | finding | evidence |
|---|---|---|
| 1 | `SqliteStore` uses `Connection::transaction()` (DEFERRED) with no `busy_timeout` (`entity-sqlite/src/lib.rs:163`), so a shared lock cannot upgrade. Two writers on **unrelated ids** fail ~50% of the time; 70% of same-id conflicts surface as `Backend` rather than `RevisionConflict` — and the crate's own doc tells callers `Backend` means stop retrying | `independent writes: ok=202 backend_failures=198 (of 400)`; same-id: `ok=200 conflict=59 backend=141`. Fix shape: `TransactionBehavior::Immediate` + busy timeout |
| 6 | `catch_up` replays the **entire** local log regardless of the remote's revision (`hybrid.rs:274-281`) | replica at revision 1 → after catch-up its log is `[(E1,1),(E1,1),(E2,2)]`, which no longer folds. Against a SQLite replica it hits the PK and stays outstanding forever |
| 8 | `Hybrid::load` through the `Store`/`StateProvider` trait discards `was_stale` (`hybrid.rs:309-311`), so remote silence becomes `Ok(None)` for every generic caller — including a `Hybrid` nested inside another | `Policy(Remote, RemoteFirst, ServeStale, …)`, remote dark, cache empty: trait `load` → `None`; inherent `load_read` → `was_stale = true` |
| 9 | `FileStore` appends events **before** the state write (`file.rs:121-140`); a failed state write leaves the expectation unchanged, so an ordinary retry appends the events again | two identical `commit(dec(1), Expect::Absent)` → log holds revision 1 twice; `rehydrate` refuses it. ENOSPC or EIO is enough to trigger |
| 11 | `Unreachable` does not survive the wire: `lib.rs:289/295/312` flatten every non-conflict `StoreError` into `Answer::Failed` → `Backend`. The third value collapses one hop out and `WhenUnreachable` stops applying | far side cannot reach its own store → `is_unreachable() = false`. (It is **not** reported as absent, so the headline invariant holds) |
| 12 | SUSPECTED — `FileStore`'s temp path `path.with_extension("json.writing")` (`file.rs:135`) is shared by every writer of one instance, so a concurrent rename can install a half-written file. Module claims "never half-written" | mechanism reproduced with the same fs calls: `reader sees: {"revision":2,"co`. 700 rounds of real two-thread `commit` did not hit it — narrow window |
| 13 | SUSPECTED — no `fsync` anywhere in `FileStore` (`file.flush()` is userspace only). The module's stated recovery story is "replaying the log reaches the state the event describes"; unsynced appends + rename-installed state can persist the state and lose the event, inverting the ordering the doc says was chosen to prevent it | not reproducible without crashing a kernel |

### C. Areas checked, nothing found

- Memory store: clean — `memory.rs:76-79` checks before either map is touched.
- SQLite refusal/error paths: clean — revision read is inside the transaction, every early `?` rolls back. Finding 1 is lock behaviour, not half state.
- `RemoteStore` itself never produces `Ok(None)` from a failure — findings 8 and 11 are in `Hybrid` and `answer`.
- Determinism: clean in both repos. No `HashMap`/`HashSet` in any output path; `derived_id` reads no clock and no random source; projections and the assembly index are `BTreeMap`/`BTreeSet`. Nit, not a finding: `derived_id`'s `<entity>:<id>@<rev>#<index>` collides if an entity name may contain `:`.
- Kernel purity R-01: clean — `cargo test -p entity-core --test purity` 4 passed; `entity-core/Cargo.toml` is not in the `f70bea4` diff.
- `Assembly::get()` returns `None` for both `Ambiguous` and `Absent` — finding 4 is upstream, in which members reach the assembly at all.
- `checked_store` / `MemberName` validation: absolute paths, `~`, `:`, `\` and `..` all refused.

Probe sources under `~/.cache/review-probe/` and `~/.cache/ws-probe*`. Nothing was written to either repo.

## Reviewer 2 — 13 findings + 8 minor, all CONFIRMED

Lens: does every published claim survive contact with the code.

### Independently confirmed by BOTH reviewers

| defect | R1 | R2 |
|---|---|---|
| `OnDivergence::Refuse` leaves the local write standing, unrecorded | #2 | #2 |
| cross-member cycle detection does not exist | #3 | #3 |
| `workspace show` discards the unresolved-member list | #4 | #7 |
| `catch_up` reports success while losing/duplicating work | #5, #6 | #1 |

Four defects found twice, by two agents that did not talk to each other. Treat these as settled.

### New from reviewer 2

| # | finding | evidence |
|---|---|---|
| 1 | `catch_up` **does** merge — by machine. `hybrid.rs:282-292` derives the expectation from the remote's *current* revision, so a conflict is structurally unreachable and a remote that moved on independently is silently overwritten. "Merges nothing" is claimed 5× (CHANGELOG, R-108, tag, story summary, module doc) | remote independently at revision 2 → `catch_up outstanding = 0`, remote left with two `TicketClosed` events for one instance. **No test covers the conflict path at all** |
| 4 | `artifact.rs:1970-1972` skips the dangling-reference check for **any** target whose namespace contains `/` — unconditionally, no workspace file needed. A misspelled member (`entity-runtme/story:typo`) now passes silently in a plain single-repo store | probe with no `workspace.yaml`: `story:gamma → story:nonexistent` refused; `story:alpha → entity-runtme/story:typo` accepted. Undisclosed in CHANGELOG and limitations |
| 5 | `entity-sqlite`'s "rolls back both halves" test refuses at the pre-check, before any INSERT — there are no halves to roll back. Reviewer ran its body **verbatim against `FileStore`**, the provider whose own docs say it cannot keep that promise: it passes | `PASS: the sqlite 'rolls back both halves' test body passes verbatim against FileStore`. Byte-identical to `every_provider_leaves_a_refused_commit_with_no_trace` |
| 6 | *"`story:passkey-login` exists in more than one repository today and they are different stories"* — it exists in **zero** members of the shipped workspace. The behaviour is real and tested against a synthetic fixture; the stated fact is not | `protocol workspace show story:passkey-login` → held by no member, exit 1 |
| 8 | Both ER epics ship `status: implemented, revision: 4` with a journal holding exactly one entry each (`created / draft`). Three moves and three revision bumps per epic, unrecorded | `journal.jsonl:42`, `journal.jsonl:76`. The six ER stories and all six EP artifacts show no drift |
| 9 | R-97's revision-gap clause is pinned by a test that asserts a different branch ("must begin with a creation event"). The gap branch (`replay.rs:109-116`) and the `only the first event` branch are asserted nowhere | `grep "gap rebuilds\|had reached"` → the message, no test |
| 10 | **Two of the three 0.6.0 `### Fixed` entries describe defects that never shipped.** `R-90b` appears only in the CHANGELOG text itself (`git log --all -S 'R-90b'`); at 0.5.3 all 64 rows matched `R-\d+`. `envelope.rs` did not exist at `38473ae`, so its `serde` defect could not have been released | The tag message is honest ("the wave's own tests caught on first run"); the `### Fixed` heading is not |
| 11 | R-100's second clause ("an instance whose key resolves to nothing is left out") has no test; its cited test compares a pure function against itself over a `BTreeMap`, so it cannot vary and never touches store iteration order — the risk its own comment names | `projections.rs:84-94`; `projection.rs:56` `key_of → None` untested |
| 12 | Two acceptance lines with nothing behind them: "a hybrid passes the conformance suite under **both** authorities" (only remote-as-authority exists — reviewer's probe shows local *does* pass, so unpinned rather than false); "there is a test that pulls the plug between them" (there is not) | `hybrid.rs:46` |
| 13 | 0.6.0's CHANGELOG — whose preamble is *"Every change a user of the runtime sees"* — has no entry for `create --store`, `execute --store/--id/--entity/--correlation/--recorded-at/--causation/--actor`, `--instance` going required → optional, or the new exit-1 `{"refused": true, "by": "store"}` payload. R-91/R-92/R-93 untouched | `entity-cli/src/main.rs:221-236` |

### Minor (8)

M1 `catch_up` doc promises a returned slice, returns `usize`, and carries `# Errors` on a non-`Result` fn (`hybrid.rs:251-255`) · M2 `parse::workspace` doc is copy-pasted from `parse::project` (`parse.rs:253`) · M3 "run against every provider" covers 2 of 3 · M4 the claim says a member is listed **unresolved**; the shipped word and test say `absent` — distinct states · M5 the limitations page dropped the old line without naming the two new gaps (#3, #4) · M6 `workspace show` on `Absent` names no member, against "every refusal names the member that refused" · M7 `check-requirements.py` says "Four checks", lists five · M8 `store-v0.1.md` §5 sits after §10.

### Reviewer 2 — areas where nothing was found

- **All 45 `pinned by` functions for R-83..R-108 exist, are live `#[test]`, none `#[ignore]`d.** 14 of them sound and non-trivial: R-83, R-84, R-85, R-86, R-87, R-88, R-89, R-98, R-99, R-101, R-102, R-104, R-105, R-107.
- **Stated limits hold.** No `reqwest`/`hyper`/`ureq`/`curl`/`tokio`/`std::net`/`TcpStream` anywhere in `crates/`, dev-deps included; `rusqlite` is `bundled`. `LoopbackTransport` is labelled a stand-in in its own module docs.
- **No write path into another member's store.** No `fs::write`, `File::create`, `create_dir` or `remove_*` in the three new EP files; `Assembly::read` calls only `MarkdownStore::open(root).load()`.
- **EP frontmatter/journal: no drift** across all 6 new artifacts. No `#[allow(missing_docs)]` in either repo. Schema gate green.
- Tag-message counts all check out: 78/83 requirements, 182 test fns, 34 artifacts (ER); 12 gate steps (EP).

## What this means

Three tags are published. The gate was green for all of them and stayed green through every one of these
defects, which is the finding behind the findings: `task check` measures that the cited tests pass, not
that they assert what the register says they assert.

Two of the false claims are mine, written today: the 0.6.0 `### Fixed` section (#10) describes two
defects that never shipped, and the two ER epics (#8) were moved with the CLI in a way the journal did
not record.

---

## Disposition

Every CONFIRMED finding is fixed in `entity-runtime` 0.8.0 and `aep` 0.26.0, whose
changelogs carry the corrections. The two releases this review is against — 0.6.0/0.7.0 and 0.25.0 —
are left exactly as published: a section that shipped is not rewritten.

**The finding behind the findings.** The gate was green for all three releases and stayed green
through every defect here. `task check` verifies that a cited test *passes*, not that it asserts what
the register says it asserts — reviewer 2 ran `entity-sqlite`'s "rolls back both halves" test
verbatim against `FileStore`, the provider whose own documentation says it cannot make that promise,
and it passed. Where a pin was vacuous, 0.8.0 replaces the test rather than the wording, and adds a
guard test that fails if the new one ever stops being evidence.
