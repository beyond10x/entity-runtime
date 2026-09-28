# Production mutation evidence

Each directory retains the exact admitted suite, count report, detailed run, implementation identity and production patch from a deliberate behavioral mutation. Each named scenario failed with a conformance failure and no runner error, unsupported observation or skip. The patches are evidence only; none remains applied to production sources.

| Domain | Applied change | Scenario that became red |
| --- | --- | --- |
| core | Reverse positive exact-number ordering | `entity.core/authored/exact-number-38` |
| store | Bypass the memory recorded provider's stale-revision check | `entity.store/authored/memory-stale-write-preserves-state` |
| executor | Skip recovery of a previously committed request | `entity.executor/authored/saved-definition-retry-precedes-current-authority` |
| shell | Remove the stale-intent check before kernel execution | `entity.shell/authored/memory-stale-intent-precedes-kernel` |
| query | Compare JSON representations instead of exact numeric values | `entity.query/authored/numeric-values-remain-exact` |

These reports belong to their retained intermediate suites, not to a later regenerated suite. Their suite association and implementation identity must remain intact. `core-restored/` additionally records the restored selected case. The final full conformance report establishes the result after all five mutations were restored; its own suite and model digest identify the final contract.

A mutation changes actual production behavior. The adapters still invoke the real APIs and return observed values; they do not inject an expected failure, derive expected values from implementation output, or substitute mock success.
