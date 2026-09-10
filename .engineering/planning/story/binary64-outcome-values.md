---
format: aep.planning-md/1
id: story:binary64-outcome-values
kind: story
status: draft
title: Preserve finite Binary64 values through recorded execution
relations:
- depends_on: story:decimal-outcome-operands
scope:
- confidence: cited
  path: CHANGELOG.md
- confidence: cited
  path: crates/entity-core
- confidence: cited
  path: crates/entity-surface/src/lib.rs
- confidence: cited
  path: docs/design/kernel-binary64-values-v10.md
- confidence: cited
  path: docs/requirements.md
revision: 4
---
## Source authority

ESS docs/design/binary64-structural-codecs.md defines finite nearest-even conversion from original JSON numeric tokens, signed zero/subnormal/underflow preservation, wrong-kind and overflow refusal, and integral floating serialization markers. Generic Value parsing can lose lexical -0 before field validation. Existing ER Number is exact JSON-decimal data, so it is not this codec. This extends the existing FieldDefinition and outcome model; no new domain entity is introduced.

## Acceptance

Opt-in outcome profile 10 admits an explicit binary64 number encoding, normalizes finite values before predicates and recorded effects, preserves signed zero, and refuses overflow, wrong kinds and unnormalized previous state/identity. A typed raw-JSON decoding path preserves original -0 through nullable, array, map, object and tagged-union paths. Programmatic Value input cannot claim recovery of a sign already lost by a caller. Complete records replay with exact normalized values; profiles 1–9 retain their bytes and old readers refuse profile 10.

## Design

R-135, docs/design/kernel-binary64-values-v10.md. FieldDefinition.number_encoding is optional, omitted on older schemas, valid only on Number and only in profile 10. Its closed Binary64 variant selects finite normalization and admission. ObjectSchema::decode_json consumes raw JSON tokens under the declared schema; it is a pure decoder, not a substitute for registration and execution validation. Retain typed identities, optional templates, decimal operands and all prior profile capabilities.

## Verification and boundaries

Independent IEEE bit vectors cover ties, adjacent large integers, maximum/subnormal magnitudes and signed underflow. Exercise all schema container paths, wrong kinds/private marker objects, profile/constraint refusals, defaults, input selection, state/event/error effects, identity and replay. Mutation must lose signed zero or rounding and fail. Targeted core/projection compatibility, strict Clippy/docs/MSRV and exact previous-reader probe only; no full, ownership, remote or release gate. Public schema projection must identify the codec without pretending JSON Schema enforces conversion. ESS adoption and complete source-token ingestion are subsequent work; generic Number and old readers remain unchanged.

## Verification

Implemented profile 10 with NumberEncoding::Binary64, finite normalization at argument/default/state/event/error boundaries, canonical previous-state/identity admission, typed ObjectSchema::decode_json and explicit projection metadata. No new dependency: serde_json raw_value is enabled only by entity-core. Existing Number and older profiles retain their representation. The raw decoder preserves the original token only when actually given source JSON; Value conversion cannot recover a lost -0 sign.

Five new core behavioral tests pass, covering independently authored IEEE bit patterns through scalar/nullable/array/map/object/union paths, nearest-even ties and adjacent large integers, subnormals/underflow/max finite, wrong JSON kinds/private marker objects/malformed grammar/overflow, default and programmatic argument conversion, state/event/error output, typed identity and full replay sign tampering. A separate surface test verifies codec metadata while demonstrating that JSON Schema does not enforce rounding. cargo test --offline --locked -p entity-core -p entity-graph -p entity-surface passed with doctests. Requirements: 110 requirements, 396 test functions, zero findings; changelog self-test 7/7.

Strict all-target Clippy for core/surface, Rust 1.85.0 all-target core checking, strict core/surface rustdoc, formatting and diff checks pass. Clippy's one test-only unnecessary clone was replaced by slice::from_ref. The exact previous-reader probe against public c05710cab9f1aa4ff9086d620997ce18c0f08a32 passes representative profile 1–9 definition/record byte and replay comparisons, and proves that reader rejects profile-10 definitions/records. This is evidence for that corpus, not exhaustive equivalence. A deliberate abs() mutation in conversion made the raw-decoder test fail with 0 instead of the negative-zero sign bit; the production file was restored byte-for-byte before the passing package run.

Logs: local-evidence:ess-evolution-20260910/er-binary64-{tests,mutation,packages,probe,msrv,clippy-final,rustdoc}.log. Retain the probe source/lock and logs; build targets are owned under /tmp with two jobs, debug information and incremental compilation disabled. No full, database, ownership, remote correctness or release gate ran. ESS must still select the codec and integrate typed raw ingestion, including normalized identity-key construction; SDK/application adoption and ESS synthesis/conformance remain separate required work.

## Publication

Implementation d0d189036673876a0ef714f62682cb52ef7e0f3b is published on origin/feat/binary64-outcome-values through standalone b10x-gates bot. Both author and committer are verified b10x-bot[bot]. The remote acknowledged App-authorized feature-branch creation; no policy/rules were changed and no PR or expensive remote gate was launched. Remote main eaf43090636abce025649e565e5af271c4513eae remains an ancestor.

The compatibility probe now consumes current d0d189036673876a0ef714f62682cb52ef7e0f3b and previous c05710cab9f1aa4ff9086d620997ce18c0f08a32 through exact public Git pins, with no managed-tree dependency. It passes representative profiles 1–9 byte/replay comparisons and old-reader refusal of profile 10; local-evidence:ess-evolution-20260910/er-binary64-published-probe.log. Retain the probe source/lock and all verification logs, reclaim the three owned /tmp build targets, then finish and exact-ID garbage-collect wt-0f030b5b8e1e after publishing this receipt. Keep the initial story status under repository lifecycle policy.

The same goal session owns the next action in ESS story:ess-entity-runtime-lowering: consume this exact revision, select NumberEncoding::Binary64/profile 10, expose typed raw source input decoding and normalize values before deriving typed identity keys. Preserve existing ESS generic literal/canonical contracts and standalone structural codecs; general synthesis/conformance refusals remain until independently implemented. Connectors adoption is not proven by this prerequisite.
