# Kernel design v0.2 — validated inputs and verifiable decisions

**Status:** normative for 0.15.0. This document changes the relevant parts of
[`kernel-v0.1.md`](kernel-v0.1.md); everything it does not change remains normative.

## 1. Validation is a capability boundary

Parsing produces `EntityDefinition`, which is untrusted data. Successful registration produces a
`ValidatedDefinition`; free `create`, `execute`, legacy event folding and registry lookups accept
that handle, never a raw definition. Registration accumulates independent defects and stores
nothing on refusal (R-13, R-113).

YAML mapping keys are unique and merge keys are refused. Defaults distinguish absent from explicit
`null`. Nested defaults are applied before their containing value is validated. Numeric bounds and
condition comparisons use the JSON number's exact decimal value rather than converting through
`f64`. Revisions stop with `RevisionExhausted` before exceeding the signed 64-bit range all shipped
database providers can preserve.

## 2. A decision is replay evidence

`DecisionRecord` contains the canonical validated definition snapshot, normalized create/execute
command, subject, prior and resulting state, complete resulting instance, changed fields and every
event emitted at that revision. `Decision` retains `events` as a compatibility view, but stores use
the events nested in the record.

`replay` treats recorded results as comparison evidence. It validates each definition snapshot,
reruns the normalized command through the ordinary kernel path, and compares the complete record.
An altered command, result, change or event is refused and never becomes state (R-97, R-114).
Legacy event folding remains available for migration, but a `LegacyImport` is explicitly an
unverified snapshot boundary and cannot claim replay from genesis.

## 3. Determinism and evaluation order

Canonicalization is recursive, including nested JSON objects. The eleven-step execution order in
v0.1 remains unchanged. Timestamp parsing is total over UTF-8 and validates Gregorian calendar
dates; values it cannot parse remain `Unknown` rather than panicking or becoming `false`.
