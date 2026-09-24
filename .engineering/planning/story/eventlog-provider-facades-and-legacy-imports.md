---
format: aep.planning-md/1
id: story:eventlog-provider-facades-and-legacy-imports
kind: story
status: implemented
title: Compatible Eventlog provider facades and explicit legacy imports
owner: Entity Runtime maintainers
refs:
- provider: ess
  reference: initiative:ess-evolution
relations:
- decomposes: initiative:entity-runtime
- serves: vision:O2
- depends_on: story:eventlog-recorded-adapter-and-bridge
- informed_by: story:file-store-atomic-groups-from-eventlog
scope:
- confidence: inferred
  path: .github/workflows/
- confidence: inferred
  path: CHANGELOG.md
- confidence: inferred
  path: Cargo.lock
- confidence: inferred
  path: Cargo.toml
- confidence: inferred
  path: README.md
- confidence: inferred
  path: Taskfile.yml
- confidence: cited
  path: crates/entity-cli/
- confidence: cited
  path: crates/entity-eventlog/
- confidence: cited
  path: crates/entity-postgres/
- confidence: cited
  path: crates/entity-shell/
- confidence: cited
  path: crates/entity-sqlite/
- confidence: cited
  path: crates/entity-store/
- confidence: inferred
  path: docs/design/
- confidence: inferred
  path: docs/requirements.md
- confidence: inferred
  path: website/
revision: 19
---
## Outcome

Fulfil approved ESS evolution section3/M2: compatible SQLite/PostgreSQL provider facades over the complete Eventlog recorded adapter, with explicit legacy File/SQLite/PostgreSQL import and preserved caller authority. FileStore atomic-group delivery retains its existing owner story:file-store-atomic-groups-from-eventlog and shares the same author assignment to avoid overlapping provider edits. No second AEP migration engine.

## Acceptance

An adopter can explicitly import a preserved legacy store, reopen through the compatible Eventlog-backed provider surface, and retain complete available history, original identity, query and atomic recorded execution semantics across retry, concurrency and restart, with actual File/SQLite/PostgreSQL gates and exact source evidence.

## Why existing evidence is insufficient

Candidate7fd93ef43d4a91c460c7305f9e3be90d7b0a4c11 provides the complete async adapter and explicit sync bridge, but entity-sqlite and entity-postgres still use direct SQL and entity-cli still selects the legacy entity_store::FileStore. Adapter acceptance does not implement the compatible public facades or explicit old-store acquisition/import. This is original product scope, not an incidental prerequisite.

## Delivery contract

- Write the concrete facade/import design against current public types before source changes. Resolve entity-store→entity-eventlog dependency-cycle risk without adding runtime IO to entity-core or raising independent pure-library minima. Preserve package identities and canonical old-reader bytes.
- Implement compatible state/events/complete decisions/observations/original lookup/revision conflicts/ordered atomic groups and existing query capabilities. Delegate decisions/replay to the sole kernel/executor, preserve pre-admission/conclusive/committed/uncertain distinctions and original receipts.
- Explicitly acquire legacy File/SQLite/PostgreSQL sources and import all available state/history/original IDs/order. Preserve exact evidence and bounded HistoryOrigin assurance; invent no missing genesis, global chronology, old receipt or command. Never reinterpret an old store in place.
- Preserve caller-selected PostgreSQL transport/TLS and from_client capability. Separate read-only open from explicit provision/import mutations; enforce exact owner/scope/generation. A localhost fixture bridge cannot replace caller authority.
- Coordinate the existing FileStore owner: Eventlog groups must prove multi-subject atomicity under competing processes and crash/reopen. Keep the explicit legacy reader/import route.
- Exercise compatibility, real File/SQLite memory+file/disposable PostgreSQL, retries/conflicts/zero-event/observations/repeated-subject groups/cross-subject rollback/restart/imported suffix replay/global ID/tamper/corrupt blob/stale snapshot. Keep compiled fault sensitivity for lost state/events, wrong identity and partial batch visibility.
- Run full task check with actual PostgreSQL, affected runtime1.91/pure-library minima, strict formatting/docs/lints and applicable site validation. Return one immutable complete source/check manifest and requirement-to-evidence report.

## Scope

- Cited: crates/entity-sqlite/, crates/entity-postgres/, crates/entity-store/, crates/entity-eventlog/, crates/entity-cli/, crates/entity-shell/ — existing provider, port, bridge and consumer surfaces; exact files selected by the binding design.
- Inferred: Cargo.toml, Cargo.lock, Taskfile.yml, .github/workflows/, docs/design/, docs/requirements.md, README.md, CHANGELOG.md, website/ — dependency/minimum gates, compatibility design/register and honest adopter documentation.
- New concrete types use the existing recorded-execution ESS home when persisted semantics require them; the accepted adapter formats stay unchanged unless a documented format consequence is necessary.
- Collides with adapter/provider/CLI/workspace source work. One author owns this delivery and FileStore adoption. Root serializes canonical composition and AEP writes.

## Authority and stopping condition

Approved plan ess-evolution-20260915 revision1, SHA2567579145c3de5a1c6f8088fd7fb804d29dac8903ec505048f3ce595c45023b787; accepted AtlasADR0050. Candidate-facing implementation may proceed independently of provider qualification. Final integration requires qualified administration and remaining native provider stages. Respect the recorded administration-review interruption: no equivalent retry or new review budget around it. Closed adapter reviews remain closed.

The author assignment ends once this complete source, the existing FileStore acceptance, required checks and one evidence report are delivered. Root owns the facade/import implementation examination and integration. No publication, release, deployment or real-store cutover. No further work is silently added after completion.

## Complete candidate implementation assignment

## Complete candidate implementation assignment

The separately scoped full facade/import and existing FileStore author assignment is now dispatched to operation_fulfillment_correction after that worker CLOSED the original SDK M6 author contract. Managed tree home-path:sha256:0414f9210bfdb026171659de236715777f3178a78cd97d13278b6390d3749ecc begins at exact7fd93ef43d4a91c460c7305f9e3be90d7b0a4c11; worker lease codex-provider-facades-20260916. Root alone writes canonical AEP and integrates. Full fixed contract: home-path:sha256:9bd55b0b6368342147bfbf77e35b72c8e2e5df6d19d568cad151228657bd6570

Source/design work can proceed independently of provider qualification. Initial design docs/design/eventlog-provider-facades-and-legacy-imports.md resolves dependency direction and retains explicit legacy acquisition; final compatibility must prove usable File/SQLite/PostgreSQL recorded facades and actual CLI/shell routing, complete history/identity/query/receipt/uncertainty behavior, caller-selected PostgreSQL authority, exact imports and File competing-process/crash atomicity. Full original actualPG/minima/strict/docs/site gates and one complete report end the assignment; no narrower facade-only handoff. Original adapter reviews stay closed and denied administration work remains excluded.

Implementation qualification and lifecycle advancement remain subject to the existing adapter/provider acceptance dependency; no completion claim from this assignment. One worker owns both stories to avoid source overlap. Source-only initially while native correction owns heavy2 and M3 bounded1; root allocates compilation after capacity check.

## Complete author acceptance and candidate freeze

Complete author contract closed. The exact 27-path candidate is 5df62103b129ef482f9fdafc3cb3f7c194c7b942 over 7fd93ef43d4a91c460c7305f9e3be90d7b0a4c11, with Eventlog caller-authority candidate 088c27b5df9745c68d8a2240dbb2038998b03efe.

The full task check with actual PostgreSQL and all runtime features passed on unchanged source; independent pure Rust 1.85 checks and feature-enabled Rust 1.91 consumer acceptance passed. The owned disposable database was removed and absence verified. Source manifest SHA256 35ea43b37650bbd6d2d5245504acfdfad9e9401253ba1e01c5331fdc45bd09c6; evidence manifest SHA256 9bc785cd09214cf54892ff9adf71c1a1250397b2b0b12fa94cc19ba10233f81b. Root rehashed every manifest entry, froze exact source, and verified signed common evidence.

The complete report and freeze receipt are retained in the ESS evolution root handoff under waves/0007-er-eventlog-adapter/remaining-provider-facades/. Independent whole source examination and qualified integration remain open. Administration review and four provider-native stages are separate existing blockers; consumer acceptance does not supersede them. No real cutover or publication occurred. The author assignment is closed; root owns original source-review dispatch and integration.

## Original complete implementation source examination

Original complete M2 source examination pass1 CLOSED NEEDS-CHANGE. Exact ER5df62103b129ef482f9fdafc3cb3f7c194c7b942 plus contracted five-path Eventlog caller-authority088c27b5 delta examined. One introduced source-level blocker: facade.rs:376 drops snapshot source_id before per-subject import_anchor, allowing a bare-only physical boundary to appear as exact replay under a different acquisition source. Root independently inspected the reachable facade path; no runtime result is claimed. Report SHA2565f0cc430a4127d124513e248c59cad3dd92d5a5cf99f72fdeca34d4ff6268f3d at local-evidence:ess-evolution/waves/0007-er-eventlog-adapter/facades-source-review-1/report.md.

Reviewer added legacy_import_refuses_a_different_source_identity_for_an_identical_bare_boundary in provider_facades.rs; UNEXECUTED under the source-only restriction. The caller-authority companion had no source finding; that is not executed provider qualification. Both reviewer leases released, no fixtures/processes or implementation changes. Assignment CLOSED; no follow-on extension. Root owns the one finite original-contract source-identity correction, exact regression execution, required affected/full gates and the original final examination/integration. No third pass, review-budget reset or denied administration examination. Existing provider/native qualification blockers remain.

## Source identity correction

Root owns the single original pass1 source-identity correction in managed tree home-path:sha256:1e92eb2cc0c8aaa1d984da6d98ae2b167258a55ef52406df48b3b930e1937008 at 5df62103b129ef482f9fdafc3cb3f7c194c7b942, lease ess-evolution-source-bound-import-astra-20260917. Eight changed files; exact receipt home-path:sha256:cb3c5ac4b47fd75cde7cc0ba5586b8f375abbb810786c5f278d23b76af73d534, diff SHA fb535a81100e4254f46a9a6ca31361709444a0af06522492c878b4ea7c3c3f89. Source implemented, UNCOMPILED/unverified/unintegrated; selected-file formatting and diff checks0 only. Reviewer assignment remains closed; this is its finite root-owned correction, not a new review unit.

The facade now passes acquisition source_id through the existing sync bridge into the adapter. Source-bound anchors persist the ID even when no envelope exists; exact replay compares that identity before any blob upload or append. Same/different-source behavior therefore does not rely on a fabricated envelope or context label. Persisted meaning changes explicitly: closed er.eventlog.import-anchor/2 wraps the existing anchor payload plus source_id. Version1 remains readable/writable through the existing unbound low-level API and its canonical bytes are unchanged. No source is inferred for v1, so it cannot satisfy a source-bound retry. The reference event payload and recorded-entry/binding formats stay unchanged. Concrete format decision is documented in both existing encoding/facade design pages.

Original reviewer regression source now also reopens the native file facade, requires same-source replay and unchanged provider files, and requires the changed source to return the exact RevisionConflict class with unchanged files. Encoding cases pin v1 bytes, old-reader rejection of v2, closed-field/blank-source refusal and contradictory envelope-source rejection. All are UNEXECUTED under the build stop. These cover the original replay/source-preservation gap; no incidental project or new requirement.

Remaining stopping condition: compile, execute these regressions and existing complete provider/bridge/encoding/facade/minima/full actual-PG/site/common gates on the final source, resolve findings, use the original final source examination once the complete source/checks are ready, then integrate the qualified dependency vector. No third pass, accepted milestone, cutover or publication. M1 administration/native gates remain separate; no denied-work retry. Coherent M7 companion pins must incorporate the corrected ER vector before acceptance.

The original eight-file correction is now composed into the existing ER SQLite compatibility tree ess-evolution-er-sqlite-compatibility-20260916. Source hashes match the prepared composition; CHANGELOG retains both correction and dependency notes. No compilation, runtime verification or integration is claimed. Receipt: home-path:sha256:503775221545c8b7c162ecd125564db1748c82f0c41c895d322bbf716d5d3ec1 Storage custody UNASSIGNED; existing build and cleanup stops remain. Original completion contract and review limits unchanged.

## Original source-identity correction acceptance resumed

2026-09-17T04:01Z: root storage custody restored under Timo's explicit correction; historical build/cleanup stop is superseded. Observed71GiB available disk/45GiB available RAM permits one distinct bounded ER lane alongside S8, max2jobs/1testthread each,20GiBdisk/16GiBRAM stop floors. Existing correction tree ess-evolution-source-bound-import-20260917 is assigned to nested_normal_admission_correction after its prior ESS assignment CLOSED. Exact separate contract local-evidence waves/0007-er-eventlog-adapter/facades-source-review-1/correction-acceptance/brief.md.

This closes the original source-identity finding's missing executable evidence: red original reviewer regression on base, restored source-bound implementation, complete actual File/SQLite/PostgreSQL, facade/bridge/encoding, pure1.85/runtime1.91, full task check and applicable site checks, frozen source/check report. Only the existing eight correction files may change for demonstrated defects; no new audit, task, review budget, skipped tests or denied administration work. Same original Eventlog088c27b5 dependency is provisional; separate SQLite composition is deferred until this source correction is accepted. Stop/close once these required checks and evidence are delivered. Root owns original final review2 and qualified integration; no product milestone or cutover claimed from this assignment.

## Correction acceptance closed and original final review active

2026-09-17T04:13Z: original eight-file source-identity correction acceptance CLOSED. Runtime regression failed on base because source-b recovered source-a's bare anchor, then passed on restored correction. All-feature Eventlog and legacy provider suites ran actual PostgreSQL, complete task check0, pure Rust1.85 build0 and site-build0. Missing Expect import in the existing test was the only further source correction; assertions unchanged. Root independently rehashed all29 final source paths and8 check logs. Exact report local-evidence waves/0007-er-eventlog-adapter/facades-source-review-1/correction-acceptance/report.md, union manifest SHA b5e8cf410f105fceba8678d8ac001381e699c2ab1ce3a6f518f5fafa4827c905. Same provisional Eventlog088c27b5 vector; no provider qualification claimed.

The original pass1 report had been retained and cited but was missing its immutable review-result artifact. Root now records that same unchanged report verbatim as review-result:er-provider-facades-source-pass-1 and its fixed disposition. This adds no examination or review budget. Original final pass2 is dispatched to the non-author sdk_final_obligation_correction in managed ess-evolution-facades-source-review-2-20260917 with all29 final ER source paths and the already-contracted five-path Eventlog caller-authority companion, whole original base7fd93ef4. Exact contract local-evidence waves/0007-er-eventlog-adapter/facades-source-review-2/brief.md. Tests only, same2pass limit, no administration examination or equivalent denied-work retry. Stop at one complete final report; no third pass.

Root holds the existing owned actual-PG fixture for immediate final-review checks (container14c32b740eda..., loopback32801), then tears it down at that checkpoint. Worker source lease released; source/caches/evidence retained. S8 continues in its separate lane. Full M2 integration still requires qualified provider and coherent SQLite dependency composition; this source acceptance is not the milestone.

## Final source review closed; finite open correction assigned

2026-09-17T04:27Z: original final pass2 CLOSED NEEDS-CHANGE, immutable review-result:er-provider-facades-source-pass-2. Two executed red regressions demonstrate ordinary File/SQLite facade opens create missing native provider bytes before rejecting absent authority; eight existing cases passed. A third finding corrects a contradictory import-anchor format sentence. No additional review round is authorized or scheduled. Prior source-identity acceptance remains valid at its recorded source; no qualified M2 outcome is claimed.

Root assigns nested_normal_admission_correction a separate finite contract at local-evidence waves/0007-er-eventlog-adapter/final-open-correction/brief.md. It blocks original M2 open/provision acceptance, whose prior full checks lacked these failure cases. Reuse existing ER correction tree; minimal Eventlog existing-open companion at088c27b5 in ess-evolution-existing-open-correction-20260917 may supply missing existing-only File/SQLite APIs. Scope excludes PostgreSQL/admin semantics, denied-review work, new native campaigns or SQLite version changes. Root records final disposition after actual correction verification; no false fixed outcome now.

Stop at both corrected public paths, preserved provision/reopen/legacy behavior, three resolved final findings, exact local-source composition and required affected/full/minimum/site evidence. Root performs verification/integration under existing qualification gates. Two bounded lanes remain: this correction and S8,20GiBdisk/16GiBRAM floors. Existing root PG fixture retained for immediate full correction acceptance; root tears it down at that checkpoint. No publication, cutover or new product requirement.

## Final correction accepted; source assignment closed

All three original final-review findings are corrected and root-verified. Both public ordinary-open regressions retain their original assertion functions verbatim and now pass. ER owners delegate to explicit existing-only File/SQLite APIs; missing native authority is never provisioned, SQLite schema is not created/migrated, valid provision/reopen and File intent recovery remain. The format sentence explicitly preserves /1 and names the source-bound /2 anchor.

Root read the source and rehashed30composedER paths,6Eventlog paths,11checklogs and3originalredlogs. Full Eventlog default gate with actualPG0, full ER actualPG task check including runtime1.91 lane0, Eventlog1.91minimum package0, ERpure1.85build0 and ERsite0. Report SHAaa6e97bf693974ed0435501a4895505983431b4a7fa4bc19f36ab89d5abf881b; exact coordinator receipt local-evidence waves/0007-er-eventlog-adapter/final-open-correction/coordinator-acceptance.json. Original final review disposition fixed, both review budgets stay closed; no third pass. Worker assignment CLOSED, both leases released, no more builds.

This closes the finite final correction source-acceptance contract. Full M2 remains unqualified/unintegrated: ER currently consumes exact local Eventlog source through four-crate path patch, which must become the coherent qualified pin before integration. Separate M1 administration/native stages and SQLite-compatible source vector remain. No further provisional M2 acceptance worker is scheduled. Root owns composition and cleanup, preserving30/6source and check manifests. PG fixture is explicitly reassigned to original M3 correction acceptance, not abandoned or torn down; root retains teardown custody after that use. No release, publication, cutover or gate waiver.

## Accepted corrections composed into SQLite compatibility sources

2026-09-17T05:00Z: root composed the final accepted open/provision source, original regressions and corrected format sentence into the existing ER and Eventlog SQLite compatibility companions. All accepted input hashes matched; eight changed source/test/design paths were copied exactly and four earlier ER paths were already identical. The accepted Eventlog changelog addition was combined with the existing SQLite note. Diff checks passed. Exact before/after hashes and patches: local-evidence waves/0012-connectors-adoption/source-composition/final-open-correction/receipt.json.

This is source composition only, not new runtime acceptance or qualified integration. Dependency manifests/locks retain their prior values; temporary acceptance-only path patches were not copied. Coherent qualified pins/locks and required final-vector checks remain. M1 administration/native-stage qualification stays open. No further review pass, denied-work retry, new worker, cutover or publication is authorized by this composition. Root owns the retained dirty companion sources; no build output was allocated.

## Complete local integration acceptance

# M2 runtime adapter, facades and explicit imports — accepted locally

2026-09-18T23:11Z. Implemented / verified / integrated = yes / yes / yes. Root committed the final qualified dependency selection on the existing `integrate/ess-evolution-er-20260915` branch as `8b1757365f628338cf697f53deb9ce76acd25a99`, parent `13b8a1faddcbd368ca636606efa9e4927a9221c8`. Provider is qualified Eventlog `f802eb8b01b44ba04a93394b20f0c07391f7757a`. Integration checkout is clean; all five final manifest/lock hashes in `source.sha256` match after commit. No path override, other dependency update or provisional CPU patch was adopted.

## Acceptance evidence

| Approved existing obligation | Acceptance retained |
| --- | --- |
| Complete asynchronous recorded adapter and explicit synchronous bridge; records, receipts, retry, conflict, fault recovery, binding and service composition | `task-check.log` / `task-check.exit`: full `task check` exited zero, including all-feature entity-eventlog tests and strict Clippy on Rust1.91; File/SQLite and real PostgreSQL provider, bridge, fault and service composition tests executed. Original adapter source reviews and accepted corrections remain closed. |
| Compatible SQLite/PostgreSQL facades and explicit legacy acquisition/import, caller-owned transport and complete receipts | `facades.log` / `facades.exit`: Rust1.91 all-feature entity-postgres/entity-sqlite acceptance exited zero. PostgreSQL migration/app URLs and CA were exported from the owned live TLS fixture, including the separately required migration URL; no unavailable-fixture branch substitutes for acceptance. |
| File facade, multi-subject atomicity, imports and existing-only ordinary open | Full entity-eventlog all-feature gate above includes facade and native-provider acceptance. Original facade source examinations and final-open correction are accepted; their independent regressions are retained unchanged. Provider's newly closed native group crash matrices supply the original private-stage gap evidence. |
| Independent pure library minimum | `pure-minimum.log` / `.exit`: Rust1.85 locked/offline workspace build excluding entity-eventlog exited zero. |
| Common source checks and integrated source identity | `common-check.exit` and `common-verify.exit` zero; signed `common-receipt.json`. Commit changed only four manifests and Cargo.lock; post-commit hashes equal the verified source. |
| Changed adopter documentation | No website source changed in this final pin selection. Earlier accepted facade/open correction includes its successful site gate; unchanged evidence retained in `../final-open-correction/coordinator-acceptance.json`. No redundant site prerequisite added. |

All original M2 source reviews are CLOSED; no third examination or reset. Provider acceptance is linked in `../native-qualification-20260918/integration.md`. Final local integration closes the existing adapter, facade/import and FileStore atomic-group product outcome, not merely an author assignment. No release/publication, live-store migration, downstream SDK/Connectors qualification or final ESS accounting acceptance is claimed.

Next ordered outcome is existing M3 public WriterControl and migration command integration. The confirmed local operator-controlled writer model supplies design facts; actual stop/drain/no-restart custody must still be established before any real store activation.
