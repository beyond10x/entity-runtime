# The kernel — design v0.1

**Status: normative for 0.1, revised after the 0.1.0 adversarial review (§ 13).** What `entity-core` does and why; `docs/requirements.md` is the
register this satisfies, and each requirement id appears where the design meets it. Where code and
this document disagree, the document wins until a later revision of it says otherwise.

## 1. The one rule

```text
definition + instance + operation + arguments  ->  Decision { instance, events }
```

Commands never mutate state directly. An operation is evaluated against the current instance and
yields a new instance and the events that describe the change; only the shell decides whether to
keep them (R-03). Because the kernel reaches no clock, no filesystem, no network and no random source
(R-01), the same inputs produce the same `Decision` (R-02) — which is what makes a definition
testable in milliseconds, a run replayable, and a refusal a fact rather than an opinion.

The corollary that matters most to a caller: **a refusal changes nothing** (R-04). The kernel takes
the instance by shared reference and returns a new one; there is no code path that mutates the
caller's copy, so a failed rule cannot leave half a transition behind.

## 2. Vocabulary

| term | meaning |
|---|---|
| **entity type** | a `(entity, version)` pair naming one definition (R-12) |
| **definition** | the data that describes an entity type: schema, lifecycle, rules, creation, operations (R-10) |
| **instance** | one object of an entity type: identity, lifecycle state, revision, fields (R-71) |
| **operation** | a named, argument-taking way to move an instance between states (R-40) |
| **transition** | one edge an operation performs, `from → to` (R-31) |
| **precondition** | a rule an operation checks before mutating (R-50) |
| **invariant** | a rule the entity checks on every materialised state (R-51) |
| **event** | a domain fact an operation emits, with a templated payload (R-43) |
| **decision** | what the kernel returns: the next instance and its events (R-73) |
| **shell** | whatever calls the kernel and does the IO (R-80) |

## 3. The definition model

A definition is a document. The reference syntax is YAML, converted to a definition by
`entity-yaml` without touching a file (R-11); JSON deserialises to the same types, which is how the
kernel's own tests build fixtures. The full example is [`examples/order.yaml`](https://github.com/beyond10x/entity-runtime/blob/main/examples/order.yaml).

```yaml
entity: order          # the type name
version: 1             # default 1; several versions may be registered together
schema: ...            # fields and constraints          § 3.1
lifecycle: ...         # states and the initial one      § 3.2
invariants: ...        # rules on every state            § 3.4
create: ...            # the creation event              § 3.3
operations: ...        # transitions, args, rules, set, events   § 3.3
```

### 3.1 Schema

Fields have a kind — `string`, `integer`, `number`, `boolean`, `enum`, `array`, `object`, `json`
(R-20) — and constraints: `required`, `default`, `min_length`, `max_length`, `min`, `max`, `values`,
`items`, `properties`, `additional_properties`, and `additional_fields` on the schema itself (R-21).
A constraint written on a kind it does not govern — `values` on a `string`, `items` on an `object` —
is **refused at registration** (R-26). Ignoring it would leave an author believing a field is
constrained when nothing checks it, which is the same failure as a rule that silently enforces half
of what it says.
Defaults are applied first, then everything is validated (R-22), and validation **accumulates**: a
document with four broken values reports four errors, each with a path (R-23). A default is applied
at every depth an object or array element already reaches, but never invents the object that would
hold it. Numbers are compared numerically rather than coerced: an integer above `i64::MAX` used to
wrap to `-1`, pass a `max` bound and report a `min` failure naming a value nobody sent (R-20). Undeclared fields are
refused unless the schema opts in (R-24); the fields document itself must be an object (R-25).

The same `ObjectSchema` type describes an operation's arguments (R-40), so arguments get exactly the
defaulting and validation fields get.

### 3.2 Lifecycle

```yaml
lifecycle:
  initial: draft
  states: [draft, submitted, approved, rejected, fulfilled, cancelled]
```

States are an open vocabulary per definition; creation enters `initial` (R-30). Transitions are not
declared here but on operations, because the edge *is* the operation: it has arguments, rules and
events of its own. `from` may be one state or several (R-31); two transitions of one operation
starting from the same state would leave the kernel guessing, so the definition is refused (R-33).

The lifecycle state is not a field **of the model**: nothing writes `lifecycle_state` except
`create` and `execute`, the kernel exposes no setter, and there is no generic status write (R-34).

What that does *not* say — and the first draft of this document did, wrongly — is that a caller
cannot hand the kernel an instance carrying whatever state they like. An `EntityInstance` is data a
store round-trips, so its fields are public and it deserialises; the kernel cannot know whether the
instance in front of it is the one it produced. That is the shell's to know, and it is why storing
an instance and appending its events is one job (§ 9, R-80). What the kernel *can* check, and now
does, is that the instance could exist at all: its `(entity, version)` must match, and its state
must be one the definition declares, else `UnknownState` (R-35). A sealed instance type — private
fields, parsed through a validated `Raw` — would make the stronger claim true; it is a breaking
change and a story, and until it lands this paragraph is the honest version. An operation with no transition from the
current state is refused as `InvalidTransition` before any rule is evaluated (R-32). A generic
`PATCH {status: fulfilled}` is the thing this design exists to make impossible: a status change is an operation, with a name, arguments and
rules, or it does not happen.

### 3.3 Operations and creation

```yaml
operations:
  reject:
    arguments:
      fields:
        actor:  { type: string, required: true }
        reason: { type: string, required: true, min_length: 1 }
    transitions:
      - from: submitted
        to: rejected
    preconditions: []
    set:
      rejection_reason: $args.reason
    emits:
      - type: OrderRejected
        payload: { order_id: $id, actor: $args.actor, reason: $fields.rejection_reason }
```

`set` writes fields from templates. Every assignment is resolved against the **pre-operation**
fields, so `set: {a: $fields.b, b: $fields.a}` is a swap and the map has no ordering semantics
(R-41). The result is validated against the schema again (R-42) — an argument that is valid as an
argument may still produce a field that is not. Events are materialised last, from templates that
see the **post-operation** fields (R-43, R-61). `create` may emit one event, whose templates see
`$id`, `$state` and `$fields` and nothing about a previous state.

`revision` is `1` after creation and `+1` per successful operation; a refusal consumes none, and
each event carries the revision it produced (R-44). That is the number a store compares for
optimistic concurrency — the kernel supplies it and does nothing with it.

### 3.4 Rules

Two kinds, distinguished by *what they may see*, which is what keeps them honest (R-52):

| | precondition | invariant |
|---|---|---|
| belongs to | an operation | the entity |
| evaluated | after arguments and transition, before `set` (R-50) | after creation and after every operation, on the **next** state, before events escape (R-51) |
| may read | `$args.*`, `$fields.*`, `$old_fields.*`, `$from_state`, `$to_state`, `$id`, `$entity`, `$version` | `$fields.*`, `$state`, `$id`, `$entity`, `$version` |
| may **not** read | `$state` — in a rule it would mean the state the operation is heading *for*, so `eq: [$state, draft]` on `draft → submitted` refuses every time it should pass | `$args.*`, `$old_fields.*`, `$from_state`, `$to_state` |
| refusal | `PreconditionFailed { operation, rule, message }` | `InvariantViolation { rule, message }` |

An invariant that could read `$args` would be a precondition in disguise, true only for the
operation that happened to supply the argument. Refusing the reference at registration (R-14, R-52)
makes the distinction a property of the definition rather than a convention — and the check follows
the whole path, so `$fields.address.countri` is refused where `$fields.address.country` is accepted,
rather than registering and then reading `false` forever.

Rules carry an optional `name` and `message`; both appear in the refusal, and a rule without a
message gets a default (R-56).

## 4. The condition language

A condition is an AST written directly in YAML (R-53):

```yaml
assert:
  all:
    - exists: $fields.customer_id
    - gt: [$fields.total_cents, 0]
    - any:
        - eq: [$state, draft]
        - in: [$fields.priority, [high, urgent]]
    - not:
        contains: [$fields.tags, blocked]
```

Operators: `all`, `any`, `not`, `exists`, `eq`, `ne`, `gt`, `gte`, `lt`, `lte`, `in`, `contains`,
and the literals `true`/`false`. `all` and `any` must not be empty, and a condition
carries **exactly one** operator: a mapping with two of them, or with a misspelled one, is refused
by name rather than parsed as whichever variant matched first and quietly missing the rest (R-16).
The same rule holds for every key of a definition document — `requried: true` is a defect, not a
comment. Semantics (R-54):

* a condition evaluates to `Truth { True, False, Unknown }` and a rule holds only when the answer
  is `True` (R-57); a *value* question over a reference that resolves to nothing is `Unknown`,
  while `exists` — the one *store* question — stays two-valued (R-58);
* `all` and `any` evaluate **every** operand in declaration order. Kleene's connectives are
  order-independent, so the answer is the same as short-circuiting would give — what is not the
  same is how many unresolved addresses have been gathered when it comes back (R-54, R-57);
* `gt`/`gte`/`lt`/`lte` compare numbers and are `false` when both operands resolve and either is
  not a number;
* `eq`/`ne`/`in`/`contains` compare structurally **except for numbers, which compare numerically**
  (R-54): `100` equals `100.0`. Comparing a number's representation instead would make the operator
  families disagree — `eq: [$fields.total, 100]` false while `all: [gte 100, lte 100]` true — so a
  definition tested with integer fixtures would refuse the same document written with a decimal
  point;
* `contains` is array∋element, string⊇substring, or object∋key.

There is no function call, loop, arithmetic, clock, random source or lookup (R-55). The `Condition`
type has thirteen variants and none of them is "evaluate this string", which is the property that
lets a definition be validated at registration, evaluated identically everywhere, and rendered by
tooling that never parses source code. A richer language (CEL, Rhai, …) could be introduced later
behind the same two rule slots; nothing in this design depends on the AST staying this small,
only on it staying data.

### 4.1 Three values, and which questions can have them

*Missing* and *false* used to collapse into one verdict — the proof of concept's semantics, and
enough to govern a lifecycle ladder. They no longer do (R-57). `engineering-protocols` invariant 5,
"Unknown is not False", is the requirement that forced it, and the consequence is concrete: an
operator told only that an evidence gate failed goes and fixes a review that was never written.

**`Unknown` belongs to the question, not to the operator.** The operators fall into two groups:

| the question | operators | can be `Unknown` |
|---|---|---|
| about the **store** — is there a value at this address? | `exists` | no |
| about a **value** — what does it say? | `eq` `ne` `gt` `gte` `lt` `lte` `in` `contains` | yes |

The kernel holds the instance, so it can always see whether a key carries a value; a presence
question is therefore always answerable, and pretending otherwise would be a lie about what the
kernel knows. What it cannot answer is what an unwritten value says. So `exists` is an ordinary
two-valued predicate (R-58), `not: { exists: x }` negates in the ordinary way, and the third value
appears exactly where there is genuinely nothing to read.

A first draft of this split put the choice in the operator instead — `exists` answering
`True`-or-`Unknown` and a new two-valued `absent` beside it. That produced a pair that were not
each other's negation, which is a rule nobody can hold in their head; and it made the kernel claim
it could not tell whether a field was set, which is false. Confining `Unknown` to value questions
gets the same guarantee with one operator and no asymmetry, and it leaves every lifecycle-shaped
rule evaluating exactly as it did — the two invariant tests that this change first broke went back
to their original assertions unaltered.

The two groups compose through Kleene's `False` dominance. `all: [{exists: x}, {eq: [x, v]}]`
refuses plainly when nothing is recorded, because the presence test decides the rule; a bare
comparison stalls instead and sends somebody to record one. That is the choice a definition author
makes, per rule, in the rule.

Two things count as nothing: a reference that names no key, and a reference to a key that is
**present and null**. The second is the one that matters in practice — `review:` with nothing after
it is how YAML front matter spells *nobody filled this in*, and schema validation cannot catch it
for a `json`-kind field, where `null` is legal. A `null` written as a literal in the definition
stays a value; the author wrote it, so it is an observation.

An `Unknown` rule is its own refusal, carrying **every** unresolved address rather than the first
(R-57). A refusal that says *go and observe* without naming what to observe reproduces, in a type,
exactly the prose-rule failure this whole programme exists to end — and naming one missing fact out
of three costs the operator three round trips.

The shape of `Truth` — the variant names, the Kleene tables, `is_satisfied` meaning `True` alone —
is taken from `engineering-protocols`' own `aep-domain::predicate::Truth` rather than designed
here. Their predicate language has no presence operator at all: six comparison operators, and its
only candidate-shaped `Unknown` is `ValueAbsent`, a comparison it cannot decide. The split above is
therefore the one they already made, written down. Two kernels that disagreed about what `Unknown`
means would disagree about whether a gate passed, which is the seam phase 2 of
`engineering-protocols-adoption-v0.1.md` runs straight through.

## 5. Templates

A template is a JSON/YAML value. A string beginning with `$` is a reference; anything else is a
literal; `$$` escapes a literal leading dollar; arrays and objects are resolved recursively (R-60).
The references are `$id`, `$entity`, `$version`, `$state`/`$to_state`, `$from_state`, `$args`,
`$args.<path>`, `$fields`, `$fields.<path>`, `$old_fields`, `$old_fields.<path>` (R-61).

A template is checked when the definition is registered, against the scope it sits in (R-64): a
creation event has no arguments and no previous state, an operation template may read both, and a
reference to an argument the operation does not declare is refused there and then. A definition that
`entity validate` accepts is therefore one whose templates can resolve — except for a path into a
`json` field, whose shape no schema describes and which stays a run-time `Template` error (R-63).

There is deliberately no `$now`, `uuid()` or lookup (R-62). An operation that needs a timestamp
declares an argument:

```yaml
arguments:
  fields:
    occurred_at: { type: string, required: true }
```

and the shell — which has a clock — supplies it. A reference that does not resolve is a `Template`
error, never a `null` written into an event somebody will later read as a fact (R-63).

## 6. Evaluation order

An operation runs in exactly this order (R-70), and a refusal at any step returns before the next:

```text
 0. instance carries a state the definition declares        UnknownState
 1. instance (entity, version) matches the definition      EntityMismatch
 2. operation exists                                        OperationNotFound
 3. arguments: defaults, then validation                    Validation
 4. transition selected from the current state              InvalidTransition
 5. preconditions, against current state + arguments        PreconditionFailed
 6. set, every assignment against pre-operation fields      Template
 7. resulting fields validated against the schema           Validation
 8. next instance constructed: new state, revision + 1
 9. invariants, against the next state                      InvariantViolation
10. events materialised from templates                      Template
11. Decision { instance, events }
```

Steps 1 and 2 are the identity checks: an instance created under another definition, an
operation the definition does not declare, or a type nobody registered are refused by name
(R-45). The order is part of the contract, not an implementation detail: `InvalidTransition` before
`PreconditionFailed` means "you cannot do that from here" is never masked by "and also your total
is zero", and invariants after `set` means the state a rule judges is the state that would be
stored.

## 7. Outputs and refusals

The identity is the caller's and is opaque to the kernel — which is not the same as absent: an
empty or whitespace id is refused (R-75).

```rust
pub struct EntityInstance { entity, version, id, lifecycle_state, revision, fields }   // R-71
pub struct DomainEvent    { entity, version, id, revision, r#type, payload }           // R-72
pub struct Decision       { instance, events }                                          // R-73
```

`DomainEvent` is the domain fact only. It has no event id, no timestamp, no correlation or causation
and no actor, because the kernel could only invent them (R-72). The shell wraps each event in its
envelope when it records it, and the pair (`id`, `revision`) is enough to place the fact in the
instance's history.

Every refusal is a typed value — `DefinitionError` at registration, `ValidationError` in lists,
`CoreError` at run time — and callers match on variants, never on message text (R-74). The CLI's
`kind` field is `CoreError::kind()`, the variant name in snake case.

## 8. Purity, mechanically

R-01, R-02 and R-05 are not asserted by this document; they are asserted by
`crates/entity-core/tests/purity.rs`, which scans every source file of the kernel for a token that
would reach a clock, the filesystem, the network, the environment, a thread, an async runtime, a
random source or an unordered map, and by the crate's dependency list, which is `serde` and
`serde_json` and which the same test pins. The scan is checked against a planted offence so that a
scan which has stopped seeing anything fails on it rather than passing on everything.

## 9. The shell

The kernel decides; the shell acts (R-80):

```text
request
  │
  ▼  load definition(s), load instance, gather ids/timestamps     ── IO ──
  │
  ▼  Runtime::execute(&instance, operation, arguments)           ── pure ──
  │
  ├─ Err(refusal)  → record the refusal (audit), change nothing
  │
  └─ Ok(decision)  → store decision.instance                      ── IO ──
                     append decision.events (with envelope)
                     update projections / search
                     publish
```

Storing the instance and appending the events are expected to happen together; how is the shell's
business — a transaction, an outbox, an event store that is also the state store. The `entity`
command is the reference shell (R-91, R-93): it reads files and stdin, calls the kernel, prints the
`Decision` or the typed refusal, and exits `0`/`1`/`2` (R-92). It holds no state between
invocations, which is why a `Decision` it prints is accepted back as the next `--instance`.

## 10. Event sourcing without mandating it

Every mutation already crosses the event-producing boundary, so the model supports both storage
styles (R-81):

| style | load | after `execute` |
|---|---|---|
| state persistence | the current instance | store the new instance; publish the events |
| event sourcing | the event history → fold → instance | append the events; the instance is a cache |

The fold does not exist yet. When it is added it must not become a second write path to
`lifecycle_state`: replaying `OrderFulfilled` may set the state to `fulfilled` because the event was
produced by an operation that was permitted to; nothing else may (R-34). `eventlog` — the org's
event-sourcing kit — is the natural home for the append-only side; this crate stays the decider.

## 11. Crates

| crate | is | depends on |
|---|---|---|
| `entity-core` | the kernel, a library (R-90) | `serde`, `serde_json` |
| `entity-yaml` | `&str → EntityDefinition` (R-11) | `entity-core`, `serde_yaml` |
| `entity-cli` | the `entity` command, a clap-derive binary (R-91) | both, `clap`, `serde_yaml` |

Provider interfaces for state, events, search and blobs are not in any of these (R-82); they belong
in a further crate that depends on `entity-core` and never the other way round.

## 12. What may change without changing this document

* Rust type and function names, module layout, error message wording.
* Adding operators to `Condition` or references to templates — each is an addition to R-53/R-61 and
  gets a changelog line.
* Reporting *more* than one `DefinitionError` per registration (R-13 says refused with a typed error;
  accumulating is stronger and welcome).

What may not: the eleven-step order, the two rule scopes, the refusal-changes-nothing property, the
absence of IO, and the absence of `$now`.

## 13. What the 0.1.0 review changed

An adversarial review of the 0.1.0 tag — a hands-on pass against the shipped binary and an
independent multi-angle code review — produced findings in three classes. The record, with each
finding's reproduction, is [`docs/reviews/2026-08-25-adversarial-review.md`](../reviews/2026-08-25-adversarial-review.md).

**Defects the code now refuses**, each a new or widened requirement: a constraint on a kind it does
not govern (R-26); a definition key or condition operator that was silently dropped (R-16); a
second definition registered over an existing one (R-15); an instance claiming an undeclared state
(R-35); a template whose scope could never resolve it (R-64); an empty identity (R-75); an integer
outside `i64` wrapping negative (R-20); a nested reference path nobody checked (R-14); a default
declared inside an object and never applied (R-22); `$state` read from a precondition (R-52);
numbers compared by representation (R-54). In the shell: input parsed as YAML when it was JSON, and
two flags reading one standard input (R-94); DOT written without escaping (R-95); `validate`
stopping at the first bad file (R-96).

**Claims the documents made and the code did not keep.** R-34 said the lifecycle state was closed
*by the type*; it is closed by the kernel's own writes, and § 3.2 now says which is which. R-01's
scan was evadable by a grouped import, an alias, `std::io`, `include_str!` or a line beginning with
`*`; it now strips comments and strings properly, expands every `use` path, and matches whole
words, and it is checked against fourteen plantings and eight lookalikes. The requirements checker
accepted any `fn` as a pin, and could not see a row whose id cell carried a marker; both are fixed,
and the second is now itself a check.

**Judgements recorded rather than changed.** Rules stay two-valued: *missing* reads `false`, not
`unknown`. That is enough for a lifecycle and not enough for an evidence gate, and the difference is
the first thing `engineering-protocols` needs — `story:three-valued-conditions`, and § 4 of
[`engineering-protocols-adoption-v0.1.md`](engineering-protocols-adoption-v0.1.md). A sealed
`EntityInstance` would make R-34's strongest reading true and is a breaking change; it stays on the
roadmap with the reason written down.
