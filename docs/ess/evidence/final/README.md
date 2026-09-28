# Current integration and release evidence

The [release run](release/README.md) retains the final 0.25.1 manifests, full repository gate,
Rust 1.85 checker build, current producer validation and exact implementation identity.

The suite, report, run, observations and implementation identity in this parent directory are
the earlier successful reader-integration run on 2026-09-28. All 414 scenarios passed and AEP
admitted the exact pair as `ess_conformance_coverage_v1`; `aep-admission.log` records that import.
Its source closure is `567200cbb0faefbaa6efaa7ad849393b7088e60e151c21405e114dbbac07f92c`.
This run precedes final alignment of internal dependency constraints to 0.25.1 and is retained
unchanged because AEP references it and its exact pair is a permanent upstream reader fixture.

The prototype's original structured bytes, gate logs, producer identity and refused admission
remain under [prototype-0.37](../prototype-0.37/README.md). Historical mutation runs under
`../mutations/` retain their own exact intermediate suites and identities. No execution result,
suite version or implementation identity is relabeled when the producer changes. The parent
`task-check.log` and `msrv-checker.log` are retained prototype logs; current final gate logs are
under `release/`.
