# The store — design v0.1

**Status: normative for the provider interfaces.** What lives outside `entity-core` and keeps what
the kernel decided. `docs/requirements.md` is the register this satisfies; `kernel-v0.1.md` § 9
("The shell") and § 10 ("Event sourcing without mandating it") are what this continues.

## 1. Why this is a separate crate and not a module

R-82 places the provider interfaces outside the kernel, and the kernel's purity scan enforces it
whether anybody remembers or not: `entity-core` cannot name a filesystem, a socket, a clock or a
random source, so a provider could not compile there. The separation is not a layering preference
somebody could relax under pressure — it is the property the whole thesis rests on. A kernel that
kept things would give different verdicts at different moments, and a refusal would stop being a
fact.

So `entity-store` depends on `entity-core` and never the reverse. What crosses the boundary is a
`Decision`, which is a value.

## 2. One call, because R-80 says "together"

> The shell owns IO: it loads the instance, calls the kernel, and persists the instance, appends the
> events, updates projections and publishes — **together**.

Two calls a caller is trusted to make in order is not *together*. The failure that permits is a
state that moved with no event explaining it, and from there every projection, every audit answer
and every replay is quietly wrong in a way nothing detects — the artifact looks complete and the
history has a hole in it exactly where somebody will eventually look.

**R-83**: `Store::commit` therefore takes a whole `Decision`. Writing the state and appending the
events are its parts rather than its API, so a provider is *able* to make them one transaction, and
a provider that cannot must say where its window is.

`MemoryStore` closes the window entirely: one `&mut self`, two maps, nothing fallible in between.
`FileStore` cannot, and its module says so in the first screen rather than in a footnote — it
appends events **before** writing state, so a crash between the two leaves a fact recorded whose
state did not land, which replay can recover. The other order loses the event, and nothing can
recover a fact nobody wrote down.

## 3. Optimistic concurrency, and the consequence for a person

Two people acting on the same version of something is not an edge case, it is the ordinary shape of
a team. The choice is between the second write silently replacing the first and the second writer
being told.

**R-84**: every write states what it expected to find, and a store holding anything else refuses.

A lost update is the worst kind of defect to ship: invisible at the moment it happens, and expensive
whenever it is finally noticed — usually by the person whose work disappeared, long after anyone
could reconstruct it. The refusal names **what was expected and what was found**, because a caller
that can see both can re-read and retry, and a caller that can see neither can only give up or
clobber.

The expectation is checked **before anything is written**, so a refusal changes nothing. That is
R-04 — the kernel's own guarantee that a refusal is inert — continued across the one boundary where
it would otherwise be lost.

`Expect` is a named type rather than an `Option<u64>`: *nothing is there* and *revision zero is
there* are different claims, and one of them is not a revision at all.

Revisions are the kernel's (R-44): `1` after creation, `+1` per operation. Nothing in a store
invents one, which is what lets two providers agree about what a stale write is.

## 4. Every provider answers alike, and that is checked

**R-85**: the SPI's whole value is that a caller can swap what is underneath. That is a claim about
*agreement*, and a claim about agreement checked against one implementation is not checked at all.

`tests/both_providers.rs` runs one suite against every provider, naming the one that failed. A
provider that drifted fails the case the other passes, which is the only way this claim can be
maintained rather than asserted.

Two answers are fixed there because they are the ones an implementation is most likely to get
differently: an instance nobody stored is **absent**, not an error — not being there is an answer —
and a refused commit leaves **no trace at all**, neither state nor event.

## 5. What is deliberately not here

Search and blob providers. Each is a story of its own under
`epic:the-shell`; naming them here is what stops this crate growing them by accident.

Locking is also not here, and `FileStore` says so: two processes writing one root can both pass the
revision check before either writes. The check makes concurrent writers visible *within* a process.
Making them safe *across* processes is a database's job, which is a different provider behind the
same traits — and is why these are traits.

## 6. The envelope: what a log needs and the kernel must not invent

A `DomainEvent` is the domain fact. It carries no event id, no time, no correlation, no causation
and no actor, because the kernel has no clock and no id generator (R-01) and one that invented
either would return a different `Decision` for identical inputs.

**R-86** puts those five outside the kernel, in one reference shape so two shells do not each invent
a different one — and keeps **correlation and causation as separate fields**. The most common way
this shape is got wrong is to keep one and call it either name, and they answer different questions:

| field | question | across a five-step flow |
|---|---|---|
| `correlation` | what larger thing was this part of? | the **same** value on all five |
| `causation` | what immediately led to this? | a **different** value on each |

With correlation alone a flow can be gathered but not ordered, and a fork cannot be found. With
causation alone one chain walks backwards but *what else happened because of this request* is
unanswerable. Both, and "why did this happen?" has an answer; either alone and it is inferred from a
timestamp.

**R-87**: every field is written, never defaulted. `actor: None` is a real claim — *nothing human
caused this* — and an absent key must not be able to make it. Serde's derive reads a missing
`Option` as `None`, so the field is forced through `deserialize_with` to make the key required. That
was not caution: the test asserting it **failed on its first run**, against code whose own comment
said the opposite.

**R-88**: the derived identity is `<entity>:<id>@<revision>#<index>~<args>` — coordinates that are
already unique, because a revision is reached once and an index is a position within it, and a
digest of what the event was decided on (R-110), so two events differing only in their arguments
have different identities and a log deduplicating by id cannot keep a forgery in place of the fact.
No clock, no random source, so sealing one decision twice gives the same identities: a test can
assert on one, and a replay can recognise what it has already seen. The digest is FNV-1a over the
arguments' canonical JSON, hand-rolled: an identity component, not a security boundary, and not
worth a dependency. A shell needing opaque ids builds the envelope itself; this is the default, not
the only way.

## 7. Projections: declared here, evaluated there

**R-98**: a definition declares its read models in `projections:` and performs none of them. This
is not the purity rule being applied again out of habit — a projection reads *across* instances, and
the kernel is handed exactly one, so it could not evaluate one whatever the rules said.

The shape is deliberately singular: group by a key, optionally over one lifecycle state. `by_status`
is `key: $state`; `open_per_customer` is `key: $fields.customer` with `in_state: open`. No filters
beyond the state, no joins, no aggregates — the same restraint as the condition language, which
grows operator by operator and never into a language. A read model needing arithmetic is a
consumer's job over what this hands it.

**R-99**: a key naming a field the schema does not declare, or an `in_state` the lifecycle does not
have, is refused at registration. The failure being bought off is the quiet one — a projection that
is silently always empty errors nowhere, and *no results* is indistinguishable from *nothing
matched*.

**R-100**: `BTreeMap` and `BTreeSet` throughout, so two runs give the same bytes and a diff of a read
model is readable. An instance whose key resolves to nothing is **left out**, not filed under an
empty key: a bucket of instances sharing only the property of not having been classified is a bucket
nobody can act on.

## 8. Conformance, and the provider that is wrong on purpose

**R-101**: the suite lives in the crate that owns the traits and *travels to the provider*, as a
public function rather than a test module. `entity-sqlite` cannot be a dependency of `entity-store`
without a cycle, so the alternative would have been each provider writing its own cases — which is
exactly how two implementations come to disagree while both look tested.

**R-102**: `Broken` ignores the revision it is handed and writes anyway. The suite is run against it
and must fail — because a conformance suite nobody has watched fail is a suite nobody knows the
reach of. It must also **localise**: failing every case against one defect would be no more
informative than passing everything.

This is the move `engineering-protocols` made at `0.2.0-wave-3` and has held since: prove the checker
before trusting the check.

**R-103**: `SqliteStore` writes the check and both halves in one transaction, which is the promise
`FileStore` states in its own first screen that it cannot make. Three providers now, and they are
not three for the sake of a number: memory is the reference, file is readable and diffable, and
SQLite is the one that can be atomic. A trait with no transactional implementor is a trait whose
transactional case nobody has tested.

SQLite is **bundled** — compiled from vendored C rather than linked against whatever the machine has.
A store whose behaviour depends on the host's library version is a store two machines can disagree
about, which is the one thing a provider must not be.

## 9. Centralized: the network at the edge, and silence as its own answer

**R-105**: `entity-remote` holds the *protocol* — three requests and their answers — and no network
client. `Transport` is a trait the caller implements, with whatever client, TLS, retries, auth and
timeouts their deployment already has.

Shipping one would have chosen an HTTP stack, a TLS backend and a runtime on an adopter's behalf,
and pulled all three into a repository whose kernel has two dependencies. It would also have made
the gate reach a network to test anything — so the tests would have been mocked regardless, which is
what `LoopbackTransport` is, labelled as such. It runs the *real* JSON round trip in both directions
against the *real* `answer` a server would run; what it stands in for is the network alone.

The wire is versioned, and a request at an unknown version is refused by name. A partial read of a
protocol nobody agreed on is how two deployments come to disagree quietly. The version moves
whenever a tagged enum on the wire grows a variant — `/2` when `Answer` gained `Refused` and
`Unreachable`, `/3` when `Ask` and `Answer` gained `Ids` — because a peer built against the older
shape cannot decode the new variant and must be told so rather than handed a decode failure.

**R-104** is the one that matters most. Every failure to reach the far side becomes
`StoreError::Unreachable`, never a `None`. A remote that did not answer has said **nothing** about
whether the instance exists — and a caller reading silence as *no such thing* creates a duplicate,
or tells somebody their record is gone because a switch was rebooting. This is the condition
language's third value, at the store boundary: `Unknown` is not `False`.

A conflict crosses the wire as a **conflict**, kept apart from a failure. Collapsing them makes a
retry loop out of something no amount of retrying resolves.

## 10. Hybrid: whose copy wins, declared rather than defaulted

**R-106**: `Policy` has no `Default`, and constructing one means answering four questions —
authority, read path, what a silent remote does, what a losing write becomes.

This is the ESS binding rule applied to a store: a binding states its delivery guarantee and what
happens on failure, **both as required words**, because a default here is a system-wide assumption
nobody made on purpose and the failure mode arrives in production, usually as data loss nobody can
date.

**R-107**: serving a stale copy is a legitimate answer, and `Read::was_stale` carries it *at the
point of use* rather than in a log nobody reads. A losing write is recorded as a `Divergence` and
never swallowed — a conflict resolved silently is data loss with good manners.

With the remote as authority, a write it refused **never reaches the local copy**. A cache holding
something the record of truth refused is worse than an empty one: it is confidently wrong, and every
read of it is wrong the same way.

**R-108**: `catch_up` replays what the authority holds **now**, not the decision that diverged — the
local side may have moved on, and replaying a superseded revision pushes the replica to a state the
authority has already left. It keeps what it could not replay, because a reconciliation that cleared
its list on a partial success would report success and lose the rest.

It **merges nothing**. A divergence that returns as a conflict means the other side moved on its
own, and no rule here can know whose version is right. Those stay outstanding for a person, because
the alternative is a machine picking — and a machine picking is how the wrong version wins silently.

A `Divergence` is **data**: it serialises, and `Hybrid::remember` hands one back. A shell that runs
one process per command — a command-line tool over a plan — writes what diverged beside the plan and
gives it to the next process, where `catch_up` finds it. A divergence that lived only as long as the
process that recorded it would be one nobody could act on, in exactly the shell that needs it most.

## 11. Enumeration: a store can say what it holds

**R-109**: `StateProvider::ids(entity)` is every identity held under one entity type, sorted.

Every other question in this crate needs an `(entity, id)` the caller already knows. A shell that
did not write a store — a second process, a rebuild after a crash, an adopter's process hydrating
from a file another run wrote — has no id to ask with, and until this section the honest thing it
could do was refuse. `engineering-protocols`' SQLite backend did exactly that: it refused any row it
had not written itself and told people to point it at an empty database.

Three rules make the answer one a shell can act on:

* **Sorted, byte for byte.** Two calls agree, and two providers agree, so a hydration is the same
  order every run and a test can assert on it. `MemoryStore` walks an ordered map; `FileStore` sorts
  the directory; `SqliteStore` says `ORDER BY id`, whose default collation is byte order.
* **Nothing is an answer; silence is not.** A type nobody stored under is `Ok(vec![])`. A store that
  could not be asked is `Unreachable` — a hybrid whose authority did not answer says so, and only a
  `ServeStale` policy somebody typed turns that into the local copy's listing. An empty list from a
  store that was never asked would rebuild an empty process and call it current.
* **Only what is held.** Every id listed is one `load` answers for. The suite checks this against a
  provider that lists a phantom (`Broken`), because the failure it produces — a shell fetching
  instances that are not there — looks like a bug in the shell.

Not here: filters, pages, queries. An enumeration is the primitive a projection or a search index
folds from, and the fold is the shell's (R-98). The wire carries it as `Ask::Ids`, which is why the
protocol is `entity.store/3`; the CLI as `entity list --store <root> --entity <type>`.

## 12. Postgres: the provider an organisation runs, and a gate that says when it did not run

**R-111**: `entity-postgres` is `Store` over a PostgreSQL connection the caller opens. The same two
tables as `entity-sqlite`, for the same reasons, and the same promise — one transaction, both halves
— where two writers to one instance is the normal case rather than the exception.

Writers of one instance are serialised by a row lock: `commit` reads the held revision `FOR UPDATE`,
so the second writer waits on the first and then sees the revision the first wrote; its stale
`Expect` is refused as `RevisionConflict` naming that revision (R-84 under real concurrency). Two
writers *creating* one identity have no row to lock; both read absent, both insert, and the second
insert fails the primary key — turned into the same conflict, naming the revision the first landed,
by re-reading after the refused insert. `READ COMMITTED` is enough for this and was chosen over
`SERIALIZABLE`, which would refuse with a serialization failure the caller has to retry for a case a
row lock resolves with an answer. `migrate` creates both tables idempotently and is the only DDL:
schema creation is a command, not a README instruction.

The constraint that shapes the provider is this repository's own: the gate reaches no network, and a
provider that cannot be tested without a server cannot be in `task check` unconditionally — while a
provider whose tests silently skip reads exactly like a tested one. So the tests run when
`ENTITY_POSTGRES_URL` names a server, each in a schema of its own so a shared database and parallel
tests do not meet, and the gate's `postgres-check` step prints `postgres-check: skipped,
ENTITY_POSTGRES_URL unset` when they did not. When the variable is set and the server does not
answer, the tests fail: a variable somebody set is a claim that the server is there. CI sets it,
against a service container. The connection is the caller's — no TLS backend, no pool, no
authentication beyond the URL is chosen on an adopter's behalf.

## 13. A command spanning instances is one transaction only where a provider says so

**R-112**: `Store::commit` keeps one instance and its events together. It deliberately cannot claim
that several calls are one transaction: a caller looping over `commit` publishes a successful
prefix before a later conflict, however honest every individual call is. `AtomicBatchStore` is the
additive, stronger contract for a shell whose one command produces several decisions.

An `AtomicCommit` carries one `Decision` and its `Expect`. A batch applies the slice in order against
transaction-local state, so a later entry for the same identity may expect the revision an earlier
entry just produced. An empty slice changes nothing. A revision conflict or provider failure at any
entry rolls every earlier state and event write back; another reader observes the state before the
batch or after it, never a prefix.

`MemoryStore` uses a candidate copy and publishes it once. `SqliteStore` and `PostgresStore` loop
inside one database transaction. Their shared batch suite checks ordered same-identity writes,
rollback after an accepted prefix and the empty case. `Broken` implements the extension by issuing
ordinary commits one at a time, and must fail the rollback case — proof that the suite tests the
stronger claim rather than only replaying the single-commit cases.

`FileStore` does not implement the extension: its documented crash window cannot support it.
Neither does the remote protocol in this version; adding a batch wire request would change bytes a
peer verifies and is therefore a separately versioned coordinated migration. Not implementing the
extension is the honest answer for either provider, and does not weaken the `Store` contract they
already satisfy.
