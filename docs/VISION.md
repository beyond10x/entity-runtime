# Vision

## The problem

Business objects have rules about how they may change. An order is not fulfilled before it is
approved; a rejected one says why; a closed ticket names its resolution. Today those rules live in
three places at once — a service's code, a database constraint, a paragraph in a runbook — and the
three drift. An agent, or a person under time pressure, finds the one that is not enforced and
writes `status = "fulfilled"` through it.

`engineering-protocols` made this argument for *engineering work*: rules in prose enforce nothing,
so move them into typed, executable definitions and let a program decide what the facts permit.
This repository makes the same argument for *the objects the work is about*. An entity type is
declared once, as data — its fields, its states, the operations that move it between them, the
conditions each operation demands, the invariants every state must satisfy, the events each change
emits — and one kernel executes every such declaration the same way.

## The thesis

> **Command + current state → events. Events + state → new state. Nothing else changes anything.**

Three consequences follow, and each is a design decision rather than a slogan:

1. **The kernel is pure.** No clock, no identifier generator, no filesystem, no network, no random
   source. Whatever the outside world knows enters as an argument; whatever the kernel decides
   leaves as a `Decision`. The same inputs give the same answer, which is what makes a definition
   testable in milliseconds and a production decision replayable a year later.
2. **A refusal changes nothing.** A rule that fails returns a typed reason and leaves the caller's
   instance exactly as it was — no partial transition, no half-emitted events. The reason has an
   address: which operation, which state, which rule.
3. **The lifecycle is not a field.** There is no generic status write. A state changes through a
   named operation with arguments and rules, or it does not change.

The types are dynamic — registered at run time from YAML — because the point is that adding an
entity type, a state or an operation is a change to a document, not a Rust release. The rules are
data — a small predicate AST, not a scripting language — because the point of a rule is that it
can be validated when it is written, evaluated identically everywhere, and rendered by tooling that
never parses code.

## Why this repository, and why now

`engineering-protocols` reasons about stories, designs, ADRs, reviews and evidence as **entities**
with lifecycles, legal moves and events, and it enforces those moves through a `LifecycleRegistry`
over a closed status enum, with hand-written command variants. Its own gap register records the
cost: a status vocabulary that cannot hold a rung it needs, a completion status that "a claim
nothing checks", four lifecycle concepts the model cannot express. Every one of those is a
definition this kernel executes. [`docs/design/engineering-protocols-adoption-v0.1.md`](design/engineering-protocols-adoption-v0.1.md)
lays out the mapping and the phases; the short version is that this repository is meant to become
the thing that repository's artifact model runs on, while that repository keeps what is genuinely
its own — evidence, three-valued predicates, capabilities, the driver.

`eventlog` is the other neighbour: the org's append-only log, folds and projections. This crate is
the *decider* to that *store* — it produces the events; it never keeps them.

## Where this stands

Working, gated by `task check`: the kernel (`entity-core`), the YAML adapter (`entity-yaml`) and
the `entity` command (`entity-cli`), with a requirements register whose every row names the test
that pins it and a purity scan that keeps the kernel IO-free by construction.

0.1.0 was reviewed adversarially — a hands-on pass against the shipped binary and an independent
multi-angle code review — and 0.2.0 is what that produced. The shape held; what did not was the
number of ways a definition could say less than its author meant. A misspelled key, a second
operator in one condition, a constraint on the wrong kind, a template or a nested path that could
never resolve, `$state` read from a precondition where it means the state being moved *to* — each
registered quietly and enforced nothing. All of them are refusals now, at the moment the document
is read. Two claims these documents made were also wrong rather than merely weak, and are corrected
rather than quietly dropped: the lifecycle state was never closed *by the type*, and the purity
scan was walked past by a grouped import. The record, with every reproduction, is
[the review](reviews/2026-08-25-adversarial-review.md).

Not yet, stated plainly: rules are two-valued — a missing reference reads `false`, not `unknown` —
which is enough for a lifecycle ladder and not enough for `engineering-protocols`' evidence gates;
there are no typed references between entities, no projections, no event envelope type, no storage
adapter, and no replay from events. Each is a story in `.engineering/planning/`.

## What this is deliberately not

Not an ORM: an ORM starts from a database and offers persistence ergonomics; this starts from the
object model and derives what a store must do. Not a database, a message bus or a search engine —
it emits what those consume. Not a workflow engine or an orchestrator: it decides one operation on
one instance and stops. Not a scripting runtime: the condition language is data and will grow
operator by operator, never into a language. Not a mandate for event sourcing: the model supports
it and does not require it. And not a place where IO happens: the moment a clock or a socket
appears inside `entity-core`, the purity test fails and the thesis is gone.
