---
format: aep.planning-md/1
id: review-result:er-provider-facades-source-pass-2
kind: review-result
status: active
title: Original final ER provider facade source review, pass 2
relations:
- reviews: story:eventlog-provider-facades-and-legacy-imports
- reviews: story:file-store-atomic-groups-from-eventlog
revision: 1
---
unit: original final M2 ER/Eventlog provider facades and legacy imports, ER working tree at 5df62103b129ef482f9fdafc3cb3f7c194c7b942 plus accepted eight-file correction, companion Eventlog 088c27b5df9745c68d8a2240dbb2038998b03efe
verdict: NEEDS-CHANGE
cases: executed 8→10, red 2
origin: introduced 3 / pre-existing 0 / undecided 0
wrote-outside-worktree: 6 paths
needs-coordinator: correct nonmutating File and SQLite ordinary open, reconcile the stale imported-anchor format sentence, and own all integration/disposition
git --no-pager diff --stat (ER tree; accepted correction was already dirty before review):
```
 CHANGELOG.md                                       |   7 +
 crates/entity-eventlog/src/adapter.rs              |  23 ++-
 crates/entity-eventlog/src/encoding.rs             | 129 ++++++++++++++-
 crates/entity-eventlog/src/facade.rs               |   9 +-
 crates/entity-eventlog/src/sync.rs                 |  53 +++++--
 crates/entity-eventlog/tests/provider_facades.rs   | 175 ++++++++++++++++++++-
 ...eventlog-provider-facades-and-legacy-imports.md |  12 ++
 docs/design/eventlog-recorded-encoding-v0.1.md     |  17 +-
 8 files changed, 401 insertions(+), 24 deletions(-)
```

The eight changed paths above include the accepted source-bound import correction already present at assignment. My delta relative to that composed snapshot is only `crates/entity-eventlog/tests/provider_facades.rs`: one import and two added tests, in `tests-only.patch` (SHA-256 `2f725572e45f866a952e5d92f771cfef1e7c2fb760d8dae5364b1e1d83b1b545`). The initial test file SHA-256 was `d85132b4b9324e8651a3414b7b21f393bf260f11f64f57a48553add2397757bc`; its final SHA-256 is `4042c0feaed9126fc4a483f28937adfa7f14d2fec608f4a956804b750ee851a8`. The other 28 final-union paths still pass their exact initial SHA-256 manifest.

Source subject: original ER base `7fd93ef43d4a91c460c7305f9e3be90d7b0a4c11`, HEAD `5df62103b129ef482f9fdafc3cb3f7c194c7b942` plus accepted correction; final union 29 paths in `../facades-source-review-1/correction-acceptance/final-union-29.sha256` (manifest SHA-256 `b5e8cf410f105fceba8678d8ac001381e699c2ab1ce3a6f518f5fafa4827c905`). The exact eight-file copy is recorded in `source-composition.json` (SHA-256 `37c52f9bfe3ea199e822001393b05d520eb0ae9118e9d150c269976d727d0cef`). Companion subject: clean Eventlog tree `088c27b5df9745c68d8a2240dbb2038998b03efe`, five contracted paths over `43ceaa09ceec610e25891815e33e03e8df92ee28`; no companion files changed.

Case 1: `crates/entity-eventlog/tests/provider_facades.rs:145`, `opening_an_absent_file_authority_does_not_create_provider_bytes`. Public `EventlogFileStore::open` must refuse a missing authority without creating the native root. It returns an error but creates the root. Written first and run alone; red now. Command: `cargo +1.91.0 test -p entity-eventlog --test provider_facades --features sync-bridge,file,sqlite --locked --offline -j 2 opening_an_absent_file_authority_does_not_create_provider_bytes -- --exact --test-threads=1 --nocapture` (four wrappers empty; lld/debug0/inc0; own target and TMPDIR; exit 101). Verbatim output follows:
```
   Compiling proc-macro2 v1.0.107
   Compiling unicode-ident v1.0.24
   Compiling quote v1.0.47
   Compiling serde_core v1.0.229
   Compiling libc v0.2.189
   Compiling syn v3.0.4
   Compiling version_check v0.9.5
   Compiling zmij v1.0.23
   Compiling generic-array v0.14.7
   Compiling serde_json v1.0.151
   Compiling serde v1.0.229
   Compiling serde_derive v1.0.229
   Compiling cfg-if v1.0.4
   Compiling itoa v1.0.18
   Compiling memchr v2.8.3
   Compiling typenum v1.20.1
   Compiling shlex v2.0.1
   Compiling getrandom v0.4.3
   Compiling find-msvc-tools v0.1.11
   Compiling block-buffer v0.10.4
   Compiling cc v1.4.4
   Compiling crypto-common v0.1.7
   Compiling pkg-config v0.3.34
   Compiling vcpkg v0.2.15
   Compiling thiserror v2.0.20
   Compiling digest v0.10.7
   Compiling libsqlite3-sys v0.35.0
   Compiling thiserror-impl v2.0.20
   Compiling deranged v0.5.8
   Compiling foldhash v0.1.5
   Compiling cpufeatures v0.2.17
   Compiling bitflags v2.13.1
   Compiling time-core v0.1.9
   Compiling num-conv v0.2.2
   Compiling powerfmt v0.2.0
   Compiling time v0.3.55
   Compiling sha2 v0.10.9
   Compiling hashbrown v0.15.5
   Compiling uuid v1.26.1
   Compiling entity-core v0.18.1 (home-path:sha256:799a44348fb54324fda068ac350ce48ff659841d42563fa779c4d4a702eb03e0)
   Compiling tokio-macros v2.7.2
   Compiling fs2 v0.4.3
   Compiling pin-project-lite v0.2.17
   Compiling tokio v1.53.1
   Compiling entity-store v0.18.1 (home-path:sha256:b6376ec3e191f23cd9618cdb5779b426c61ff4a23f201a2bfa1e641a7e6c9702)
   Compiling eventlog-core v0.2.1 (https://github.com/beyond10x/eventlog?rev=088c27b5df9745c68d8a2240dbb2038998b03efe#088c27b5)
   Compiling hashlink v0.10.0
   Compiling smallvec v1.15.2
   Compiling getrandom v0.3.4
   Compiling rustix v1.1.4
   Compiling fallible-streaming-iterator v0.1.9
   Compiling fallible-iterator v0.3.0
   Compiling rusqlite v0.37.0
   Compiling linux-raw-sys v0.12.1
   Compiling eventlog-sqlite v0.2.1 (https://github.com/beyond10x/eventlog?rev=088c27b5df9745c68d8a2240dbb2038998b03efe#088c27b5)
   Compiling eventlog-file v0.2.1 (https://github.com/beyond10x/eventlog?rev=088c27b5df9745c68d8a2240dbb2038998b03efe#088c27b5)
   Compiling entity-query v0.18.1 (home-path:sha256:f9233a66388af471b72bcffb1fee715e55e4d2853fdf0788b5e08c861afd81f4)
   Compiling entity-executor v0.18.1 (home-path:sha256:957ecae5aad3cbed21e9c0b68abce5e29f0e2eb6be6c0764eb941694a5ea6447)
   Compiling once_cell v1.21.4
   Compiling fastrand v2.5.0
   Compiling tempfile v3.27.0
   Compiling entity-eventlog v0.18.1 (home-path:sha256:e56e871017e669e45d04b58ac0ddccea91d5aaa9913c23ebbb8732fe93a2552f)
    Finished `test` profile [unoptimized + debuginfo] target(s) in 18.55s
     Running tests/provider_facades.rs (target/m2-final-review/debug/deps/provider_facades-92085856eca913cf)

running 1 test
test opening_an_absent_file_authority_does_not_create_provider_bytes ... 
thread 'opening_an_absent_file_authority_does_not_create_provider_bytes' (2262214) panicked at crates/entity-eventlog/tests/provider_facades.rs:158:5:
ordinary open must not provision native File provider bytes
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace
FAILED

failures:

failures:
    opening_an_absent_file_authority_does_not_create_provider_bytes

test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 8 filtered out; finished in 0.01s

error: test failed, to rerun pass `-p entity-eventlog --test provider_facades`
```

Case 2: `crates/entity-eventlog/tests/provider_facades.rs:164`, `opening_an_absent_sqlite_authority_does_not_create_provider_bytes`. Public `RecordedProviderFacade::start` with a SQLite owner (the path wrapped by `EventlogSqliteStore::open`) must refuse a missing authority without creating a SQLite database. It returns an error but creates the database. Written before any suite run and run alone; red now. Command: `cargo +1.91.0 test -p entity-eventlog --test provider_facades --features sync-bridge,file,sqlite --locked --offline -j 2 opening_an_absent_sqlite_authority_does_not_create_provider_bytes -- --exact --test-threads=1 --nocapture` (same bounded environment; exit 101). Verbatim output follows:
```
   Compiling entity-eventlog v0.18.1 (home-path:sha256:e56e871017e669e45d04b58ac0ddccea91d5aaa9913c23ebbb8732fe93a2552f)
    Finished `test` profile [unoptimized + debuginfo] target(s) in 0.49s
     Running tests/provider_facades.rs (target/m2-final-review/debug/deps/provider_facades-92085856eca913cf)

running 1 test
test opening_an_absent_sqlite_authority_does_not_create_provider_bytes ... 
thread 'opening_an_absent_sqlite_authority_does_not_create_provider_bytes' (2264082) panicked at crates/entity-eventlog/tests/provider_facades.rs:188:5:
ordinary open must not provision native SQLite provider bytes
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace
FAILED

failures:

failures:
    opening_an_absent_sqlite_authority_does_not_create_provider_bytes

test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 9 filtered out; finished in 0.03s

error: test failed, to rerun pass `-p entity-eventlog --test provider_facades`
```

Affected suite, run only after both cases existed: `cargo +1.91.0 test -p entity-eventlog --test provider_facades --features sync-bridge,file,sqlite --locked --offline -j 2 -- --test-threads=1 --nocapture` (same bounded environment; exit 101). Verbatim output follows:
```
    Finished `test` profile [unoptimized + debuginfo] target(s) in 0.07s
     Running tests/provider_facades.rs (target/m2-final-review/debug/deps/provider_facades-92085856eca913cf)

running 10 tests
test file_facade_groups_are_process_atomic_and_survive_a_publication_crash ... ok
test file_facade_preserves_atomic_groups_queries_retries_and_restart ... ok
test legacy_file_import_is_exact_resumable_and_provider_verified ... ok
test legacy_import_refuses_a_different_source_identity_for_an_identical_bare_boundary ... ok
test legacy_import_retains_settled_progress_and_refuses_a_changed_source_boundary ... ok
test legacy_import_revalidates_the_complete_source_before_any_destination_write ... ok
test opening_an_absent_file_authority_does_not_create_provider_bytes ... 
thread 'opening_an_absent_file_authority_does_not_create_provider_bytes' (2268040) panicked at crates/entity-eventlog/tests/provider_facades.rs:158:5:
ordinary open must not provision native File provider bytes
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace
FAILED
test opening_an_absent_sqlite_authority_does_not_create_provider_bytes ... 
thread 'opening_an_absent_sqlite_authority_does_not_create_provider_bytes' (2268043) panicked at crates/entity-eventlog/tests/provider_facades.rs:188:5:
ordinary open must not provision native SQLite provider bytes
FAILED
test sqlite_file_provision_returns_the_exact_reopen_authority ... ok
test sqlite_memory_facade_uses_the_same_complete_surface ... ok

failures:

failures:
    opening_an_absent_file_authority_does_not_create_provider_bytes
    opening_an_absent_sqlite_authority_does_not_create_provider_bytes

test result: FAILED. 8 passed; 2 failed; 0 ignored; 0 measured; 0 filtered out; finished in 11.11s

error: test failed, to rerun pass `-p entity-eventlog --test provider_facades`
```

`cargo +1.91.0 fmt --all -- --check` and `git diff --check` both exited 0 after the tests-only edit. I did not rerun the author’s whole gate; its actual-PostgreSQL, all-feature, pure 1.85, and site evidence is in the contracted correction-acceptance report. This review did not use the PostgreSQL fixture.

| Source and measured observation | Reachability and consequence | Verdict | Origin |
|---|---|---|---|
| `crates/entity-eventlog/src/sync.rs:239`: the File owner calls native `FileEventStore::open` before checking the already bound authority. New test `provider_facades.rs:158` exited 101 because a missing root existed after the rejected call. Native `Journal::open` creates the root and manifest. | `EventlogFileStore::open` is public, and the opt-in CLI calls it for `--eventlog-config` on create, execute, and list. A missing or removed selected provider path is reachable; an ordinary open silently provisions a new physical store even though it returns an error. | NEEDS-CHANGE | introduced |
| `crates/entity-eventlog/src/sync.rs:261`: the SQLite owner calls native `SqliteEventStore::open` before binding verification. New test `provider_facades.rs:188` exited 101 because a missing database file existed after the rejected call. | `EventlogSqliteStore::open` wraps this public owner path. Opening a missing selected SQLite database creates native bytes before refusing the absent authority, breaking the documented open/provision separation. | NEEDS-CHANGE | introduced |
| `docs/design/eventlog-provider-facades-and-legacy-imports.md:187` still says no imported-anchor encoding changes, while this exact accepted correction encodes a new `er.eventlog.import-anchor/2` blob (`encoding.rs:260` and the same design at :149-155). | The format/compatibility section is the design’s reader-facing migration statement; its contradictory sentence can misstate the format consequence. The code’s v1/v2 behavior itself was not shown broken by this finding. | CONFIRMED | introduced |

Both open-path findings arise from new ER facade dispatch to existing native create-on-open APIs; the native APIs themselves predate this subject. They are one failure class with two independently measured public provider paths. A preflight existence check alone would not prove a fully read-only open under races or on a malformed existing provider; the correction needs to preserve the documented open/provision boundary. The stale format sentence is a documentation warning rather than a separate runtime failure.

Attacked without another concrete failure: the source-bound `/2` anchor’s same/different-source retry path and v1 reader refusal; complete imported-boundary validation, sorted source acquisition and partial-progress reporting; File batch atomicity and process/reopen tests; SQL legacy acquisition and facade dispatch; the unchanged five-path caller-owned PostgreSQL connection authority and driver handoff. The eight pre-existing facade cases passed in the affected suite. PostgreSQL source and caller proof were examined against the contracted reports; this lane did not rerun a live PostgreSQL case or claim a qualified integration result.

Custody: reviewer modified only `crates/entity-eventlog/tests/provider_facades.rs` in the ER tree. The 28 non-test original final-union paths remain byte-for-byte on the accepted manifest, the Eventlog companion tree remains clean, and neither tree was reset, switched, stashed, committed, or published. The reviewer build target is inside the ER tree at `home-path:sha256:4724a1ddf9f068f27a1ab5cf238f24201e0515bd4d0444ff5ef28e1fdaf110e9`. The root-owned live PostgreSQL fixture `14c32b740eda0aba07ed254597a68cbd7d590b3292e2e9d657ce433fa825e1bf` and its env file were not read or mutated. Both reviewer session leases were released with `worktree hook session-end`; root retains tree and fixture custody.

Every path written outside the ER worktree:
- `home-path:sha256:0ee15d3ec925a557f61a24a92cdd19dbab8e776e1ca719519d57c185f2422c13`
- `home-path:sha256:93e3c056f1f6c8273c7aa400b7cbe78aacaa39a563add1b70ec98a231c31d174`
- `home-path:sha256:fcc8288f61500981197fa1098cbdd7604d98a619a9f947127df6ce5835743cdf`
- `home-path:sha256:ee60a8e3a562196fd997204055b85559d5413023b7b1e0f9a5d91bceb50c5774`
- `home-path:sha256:109c4c29c153cb1f44e6398d133e7d0425d3673736270048e36e6b4cde70d39e`
- `/var/tmp/ess-evolution-m2-final-review-20260917` (private runtime TMPDIR, outside Git)

```findings
- file: crates/entity-eventlog/src/sync.rs
  line: 239
  category: acceptance
  severity: blocker
  verdict: NEEDS-CHANGE
  origin: introduced
  message: ordinary File facade open creates a missing native provider root before it rejects the absent bound authority
- file: crates/entity-eventlog/src/sync.rs
  line: 261
  category: acceptance
  severity: blocker
  verdict: NEEDS-CHANGE
  origin: introduced
  message: ordinary SQLite facade open creates a missing native database before it rejects the absent bound authority
- file: docs/design/eventlog-provider-facades-and-legacy-imports.md
  line: 187
  category: contract-drift
  severity: warning
  verdict: CONFIRMED
  origin: introduced
  message: the format section denies imported-anchor encoding changes despite this correction's source-bound version 2 anchor
```
