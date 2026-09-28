---
format: aep.planning-md/3
id: executable-system-specification:er-library-contracts
kind: executable-system-specification
status: validated
title: Executable contracts for the five ER libraries
relations:
- serves: vision:O2
- specifies: initiative:entity-runtime
model_digest: a13e6c07be82c913363ea52b8042c6e379343acfda7fcc03a212c0942569da5d
revision: 6
transitions:
- {from: "draft", to: "validated", at: "2026-09-28T03:08:23Z", actor: "human:timo", revision: 3, imported: true}
---
# Executable ESS contracts for ER libraries

## Accepted scope and baseline

The approved plan covers entity-core under kernel/1 and service/1–3, the public entity-store contracts with memory/file/asynchronous recorded memory providers, entity-executor, entity-shell and entity-query. Other ER crates and runtime behavior corrections are excluded. Preserve public APIs, persisted formats, canonical bytes and Rust 1.85. The base is remote main 44c14c05f68025085f4a3a89b6c9f5e544f7921e. The integration worktree is er-ess-contracts-integration on spec/er-executable-contracts-integration.

## Composition and traceability

`ess/ess-inputs.yaml` is the canonical entry point. Five behavioral domains and library components reuse the existing service-semantics and recorded-execution coordinates. Their original entry points remain available; the service-binding inventory remains diagnostic. Shared lossless document types have one owner and explicit references. `docs/ess/README.md` and the five traceability registers connect requirement IDs, design clauses, public APIs, implementation, named scenarios and static Rust obligations. Example entities remain scenario inputs to actual runtime behavior.

## Integration and verification

The standalone Rust/clap workspace `checks/ess-conformance` has its own lockfile and pins ESS libraries to ac6fc6fe2f39b43f016e4d3a9edecb3573f7d6a1, the independently reviewed direct-return extension on released 0.38.0, preserving its existing formats. The matching CLI uses that same Git revision. Upstream work is story:direct-library-return-observations in ESS and pull request beyond10x/ess#185. No ESS package enters the production workspace dependency graph.

The complete manifest validates without unresolved references. CLI and checker canonical model bytes agree. The model digest is a13e6c07be82c913363ea52b8042c6e379343acfda7fcc03a212c0942569da5d. The checker synthesizes a nonempty suite/29 with complete declared coverage and no synthesis refusal, repeats canonical generation byte-for-byte, and executes admitted bytes through ESS's Rust runner. Coverage version 2 fingerprints entire scenario contracts, so removing assertions under unchanged IDs requires explicit review. Build-time source identity rejects stale executables; exact suite/report/run/model/source and executable identity are retained. Report timestamps reflect execution; runtime fixture time remains explicit input.

Adapters call real Rust APIs and return actual values, state, history and receipts. They do not manufacture observations or derive expectations from the implementation. Core returns imply no persistence or publication. All five domains have retained failing production mutations in `docs/ess/evidence/mutations/`, with restored sources and passing runs. Independent review found and closed checker identity/replay/coverage gaps and upstream nested-presence/resource-limit defects. Final execution and repository-gate evidence is retained in `docs/ess/evidence/final/release/`; review evidence is under `docs/ess/evidence/review/`.

## Declared limits and remaining acceptance

Native ESS Binary64 conformance is explicitly refused by the pinned extension. The ER contract instead specifies lossless serialized JSON documents and asserts actual ER wire results, including exact numeric behavior, signed zero and finite/nonfinite doors; it does not weaken a native ESS Binary64 assertion. Minimal old-reader reproducers remain in `ess/reproducers/`.

FileStore retains its single-subject boundary. MemoryRecordedStore does not implement optional refusal recording or lineage creation; executor scenarios exercise those orchestration contracts through explicitly supplied ports without claiming successful durable merge persistence. Query continuation makes no snapshot-isolation guarantee.

The full repository gate, Rust 1.85 checker build and all 414 selected scenarios pass with zero failures, errors, unsupported observations, skips or synthesis refusals. AEP has admitted the exact final suite/29 and report as typed ess_conformance_coverage_v1 evidence. The earlier format-admission blocker is cleared; original prototype evidence remains unchanged. A separate lifecycle compatibility issue still prevents the conforming transition because the shipped ladder requests the legacy ess_conformance kind. Authority adoption waits for a supported typed-evidence route; no hand-asserted replacement record is permitted.
