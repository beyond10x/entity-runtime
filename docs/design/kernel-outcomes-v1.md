# Named command outcomes, opt-in version 1

Owner: `story:versioned-outcome-decisions`. The existing kernel v0.1/v0.2 definition,
execution and replay contracts remain unchanged. This design governs the separate
`entity_core::outcome` API and the new `entity-outcome-definition/1` and
`entity-outcome-record/1` documents (R-126).

The prerequisite is the ESS semantic crosswalk at ESS commit
`568ee56936f26eb1704479886cadf9d9f3ffef19`,
`docs/design/ess-evolution/semantic-crosswalk.md`. This implementation owns named selection,
multi-event creation and observations; it does not claim full ESS lowering. In particular,
the current ObjectSchema/Condition vocabulary still lacks nullable values, typed maps/unions,
quantifiers and several primitive codecs. Those remain required kernel work. No SDK, AEP or
application reader is switched by this addition. Consumer adoption and any alteration of
consumer-verified bytes require the coordinated migration record before pins move.

## Definitions and selection

An outcome definition carries an explicit format discriminator, one entity's schema/lifecycle/
invariants, and commands. The embedded entity definition must have no legacy create event or
operations: there must be only one place choosing effects. It keeps its domain definition version;
that number is not used as a format discriminator. Each command has argument and external
observation schemas and a nonempty map of named outcomes with exactly one default.

Conditions are input predicates, explicit external Boolean observations, wrong-state sets, or
otherwise. Input predicates read arguments and caller-supplied identity/type/version, never an
effect's proposed state. Wrong-state sets are explicit and validated against the lifecycle;
a lowerer must derive them from the source command's moves. External observations are separately
declared Boolean fields with no defaults; omitted optional fields mean unobserved, false means
observed false. A required observation must first pass the declared input schema.
Arguments cannot impersonate this observation channel.

Every nondefault condition is evaluated by the kernel. An Unknown condition refuses selection,
including when a different branch is true; no omitted observation is silently false. More than
one true branch is an ambiguity, independent of declaration/name order. Exactly one true selects
that branch; all false selects the default. This profile assigns no implicit precedence between
external, conditional and wrong-state branches. A source with unresolved precedence cannot be
silently enabled through this profile. Selection diagnostics are not business error outcomes.

Registration validates all branches, even ones no current input reaches. Reuse existing schema,
reference, rule and template validation and accumulate the old validator's errors. Names, branch
shape and new profile restrictions produce located definition errors. A malformed dormant branch
cannot wait until after publication to fail.

## Effects and revisions

Create resolves its field assignments from normalized arguments, runs the existing create schema,
defaults and invariants, then materializes its entire typed event vector at revision 1. It never
creates a temporary persisted state or a second operation to emit another creation event.
Creation event templates can read normalized arguments and final fields, but not previous fields
or a previous state. An existing instance refuses creation before any effect escapes.

Change uses the existing validated operation machinery inside the kernel: each selected branch
has its own transitions, assignments and ordered events, compiled at registration. Selection
still happens from the original command inside ER, not in a service binding. State/argument
validation, transition selection, preconditions, assignments, invariants, event materialization
and revision exhaustion reuse the kernel's existing rules. Only the new whole-command record is
persisted; an internal branch program is not a caller-addressable operation or a second history.

Observe and Refuse leave the optional instance unchanged and emit no domain events. Refuse carries
the declared error name and a schema-checked payload; Observe carries the selected outcome name.
A refused creation can be recorded with no result instance and no invented revision. Each outer
storage envelope must assign its own record identity/order. This API does not claim that current
RecordedObservation provider layouts can already store a no-instance result, nor does it add IO.

Event and error payloads have explicit ObjectSchema contracts. Resolving a JSON template does not
prove that it satisfies the declared payload type. The kernel checks every resulting payload
before returning any decision. All templates preserve literal dollar escaping and exact numbers.

This profile names one prospective entity identity per invocation. Subjectless service commands
and multi-entity assembly still require their explicit execution/storage design; no synthetic
entity type is invented to represent them here.

## Records, trust and compatibility

An invocation carries command, prospective identity, arguments and external observations. It
does not carry a selected branch. Normalize defaults and recursive object ordering before recording.
The record contains the complete definition, normalized invocation, recomputed outcome, optional
result instance, optional declared business error, and ordered events. Its format is explicit;
old definition and decision readers reject the new envelope. Existing serializable types are
unchanged. Every new enum/structure is closed; payload objects are governed by their schemas.

Replay starts with no instance, validates the first pinned definition, and re-executes each
recorded invocation. It compares the complete result with the recorded result before accepting
the step. Definition substitution within the history is refused; moving a definition requires
an explicit migration boundary. A definition pin and durable history authentication still belong
to the caller: internally consistent fabricated history cannot establish external authenticity.

All records in one replay belong to one prospective identity, including observations before
creation. A refused creation followed by a successful creation and a non-mutating observation
is replayable; no observation advances the entity revision. This is a complete new command
history, never a reinterpretation of old event-only history or LegacyImport as earlier decisions.

## Evidence

Independent vectors cover conditional/default/external/wrong-state selection, ambiguity and
Unknown, schema and scope refusal in dormant branches, multi-event creation, state change and
invariants, zero-event observations including no instance, exact payloads, tampered replay and
old-reader discrimination. Mutation must demonstrate that bypassing replay comparison or
silently resolving ambiguity fails the corresponding vector. Existing kernel tests and purity
remain required affected checks. No full gate is authorized for this work.
