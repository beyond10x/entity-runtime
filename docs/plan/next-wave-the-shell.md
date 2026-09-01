# Next waves — one CLI across repositories, then the shell that can hold things

**Status: proposed, not accepted.** Nothing here is a work order.

Three waves. **A** is wanted now and needs no storage at all. **B** builds the shell R-80 already
describes. **C** is centralized and hybrid storage, which is a different problem from durability and
is sequenced after the SPI exists rather than bolted onto it.

---

## Wave A — one CLI, many repositories

**The want, in the operator's words:** *use one CLI cross repo with dependencies.*

**This wave is `aep`' work, not this repository's**, and its stories live in that
repository's store under `epic:one-cli-many-repositories`. It is written up here because it comes
first and because B and C are sequenced behind it — but nothing in it needs the kernel. `protocol
artifact` owns the store, the relation vocabulary and the verbs; assembling several markdown stores
into one graph adds no definition, no rule and no IO this kernel could perform anyway.

Today every repository is an island. `aep artifact` reads **one** store (`--store`), `entity`
reads **one** definition set. A story here that is blocked by a story in `metaharness` cannot say
so, and `aep`' own limitations page lists the gap plainly: *"No federated artifact
graphs across repositories."*

**Why this can go first:** it needs no provider, no database and no network. Every store involved is
already a directory of markdown in a git checkout. What is missing is a way to *name* the other one
and a resolver that reads several at once.

**What already exists and is doing the heavy lifting:** typed references shipped 2026-08-25 (R-27,
R-28). A field can already be `type: ref` naming a target entity type, and `Registry::validate_all`
already asks the cross-definition question **as a set** rather than at registration order. A
cross-repo graph is that same question over a set assembled from more than one source.

| # | story | outcome |
|---|---|---|
| A1 | **`workspace-manifest`** | one file naming the member repositories and where each one's store is — a path or a pinned `git+…#<40-hex>` locator. The pin is not decoration: a governing tree that moves under you is a dependency whose meaning changes without a commit in your repository |
| A2 | **`namespaced-identity`** | an identity is unique **across** members. `story:passkey-login` exists in two repositories today and they are different stories. Decide the spelling once — a member prefix is the obvious candidate — and refuse an ambiguous reference by name rather than resolving it to the nearest match |
| A3 | **`assemble-across-sources`** | one `Registry` and one instance graph built from several stores, with each entity remembering which member it came from. This is `validate_all` over a union, and the failure to design for is a cycle that only exists once two members are read together |
| A4 | **`cross-repo-relations`** | a relation whose target lives in another member. `blocks`, `depends_on`, `derived_from` already exist as a vocabulary; what is new is that the target resolves elsewhere — and that an unresolvable one is a **typed fact**, not an error, because a member you have not checked out is a normal condition, not a broken plan |
| A5 | **`one-cli`** | the verb surface over the assembled graph: `list`, `board`, `graph`, `validate` across members, and a refusal that says *which member* refused |

### The two decisions A cannot avoid

**Where the CLI lives.** `protocol` already carries the planning verbs and may depend on
`entity-core`; `entity` carries the kernel verbs and **may not** depend on `protocol`
(`atlas/architecture/adr/0002`). So the one CLI is `protocol`, using this repository's crates for
assembly and resolution. Naming it here so the alternative — a third binary owning both — is a
decision somebody takes rather than a thing that happens.

**A reference crossing is data, never a manifest edge.** Member X's story pointing at member Y's
story must not become a `Cargo.toml` dependency of X on Y. The arrow rule exists because a kernel
shaped by one adopter answers a narrower question for everyone else, and the same reasoning applies
to a plan: two repositories that cannot be checked out independently are one repository.

---

## Wave B — the shell

`entity-core` decides. Nothing here **holds** anything. That is correct for the kernel and
permanent — R-01 is the thesis — but it was never meant to be the end of the repository. Every
requirement that would let a caller keep an entity is written down and unbuilt:

| requirement | says | state |
|---|---|---|
| R-03 | events are the mutation boundary; the kernel persists nothing and publishes nothing | held |
| R-80 | the **shell** loads the instance, calls the kernel, then persists, appends, projects and publishes — together | no shell but the CLI |
| R-81 | the model is compatible with state persistence **and** event sourcing; a future replay must not open a way to patch lifecycle state directly | unbuilt |
| R-82 | provider interfaces — state, event, search, blob — live **outside** `entity-core` | placed, and `docs/requirements.md:175` says *"the crate does not exist yet"* |

`docs/VISION.md:74` states the same gap from the other side: *"no projections, no event envelope
type, no storage adapter, and no replay from events."*

| # | story | outcome | after |
|---|---|---|---|
| B1 | **`provider-spi`** *(exists, `draft`)* | `StateProvider` / `EventProvider` over a `Decision` and an expected revision, in a crate depending on `entity-core` and never the reverse; in-memory reference implementation; optimistic concurrency on `revision` (R-44) refusing a stale write; `entity --store`, so `execute` no longer needs `--instance` | — |
| B2 | **`event-envelope`** *(exists, `draft`)* | `event_id`, `recorded_at`, correlation, causation, actor. An event store without an envelope cannot answer *what caused this*, which is most of why anybody wants one | B1 |
| B3 | **`replay-from-events`** *(exists, `draft`)* | rehydrate an instance from its events. **R-81 is the hard constraint** and the one that would quietly destroy the product: a rehydrate path accepting a lifecycle state as input lets any caller reach any rung, and every ladder in every adopter becomes advisory | B2 |
| B4 | **`projections`** *(exists, `draft`)* | projection definitions folded from the event stream, for search and indexing | B2 |
| B5 | **`a second provider`** *(new)* | one durable implementation — SQLite is the cheapest honest choice — so the SPI is proven by **two** implementors rather than described by one | B1 |
| B6 | **`provider-conformance`** *(new)* | black-box suites any provider runs against itself, **plus a deliberately broken provider the suites are checked against**. `aep` paid for this at `0.2.0-wave-3`: a suite that passes everything tells you nothing about whether it would catch anything | B5 |

---

## Wave C — centralized, and hybrid

**The want, in the operator's words:** *when it becomes to storage, important is also centralized
storage and hybrid storage.*

These are **not** the same requirement as B, and folding them in would get both wrong. B is *can an
instance survive the process*. C is *whose copy is authoritative, and what happens when they
disagree* — a distributed-systems question with answers that must be declared rather than defaulted.

| # | story | outcome | after |
|---|---|---|---|
| C1 | **`remote-provider`** | a provider that talks to a server, over the same traits B1 defines. All network lives here, in the shell, and the purity scan over `entity-core` is untouched | B1, B2 |
| C2 | **`hybrid-provider`** | a **composite** over a local provider and a remote one. This is the story with the real design in it, and it is spelled out below | C1 |
| C3 | **`authority-and-conflict`** | which side wins, stated per entity type rather than globally, and what a losing write becomes — refused, queued, or a recorded divergence. A conflict resolved silently is data loss with good manners | C2 |
| C4 | **`offline-and-catch-up`** | the local side works with the remote unreachable, and reconciles when it returns, using the revision machinery from B1 rather than timestamps | C3 |

### What hybrid has to declare, and why

`aep` learned this in ESS wave 2 and wrote it into its own model: **a binding
states its delivery guarantee and what happens on failure, both as required words** — never
defaulted, because a default here is a system-wide assumption nobody made on purpose and the failure
mode arrives in production.

A hybrid provider is a binding. It must declare, as required words:

* **authority** — which side is the record of truth for this entity type;
* **read path** — local-first, remote-first, or remote-required;
* **on unreachable** — refuse, serve stale and say so, or serve stale silently (and if the third is
  ever allowed, it is declared, never inherited);
* **on divergence** — what a write that lost becomes.

The three-valued habit carries straight over: *the remote could not be reached* is **`Unknown`**, and
must not read as *the entity does not exist*. That distinction is the whole difference between a
sync tool people trust and one they turn off.

### Where the API comes in

An HTTP binding and a NATS binding are the point of *register a set of schemas, entities,
transitions, commands and events and get an API*. They ride on B and C and are **out of both**:
`VISION.md` already holds that *"transports are projections of the model, not part of it"*, and a
server built before a store exists is a server with nowhere to put anything. Worth its own wave once
C1 holds — and worth noting that Wave A's cross-repo assembly is the same shape as a multi-tenant
read, so A is not a detour from it.

---

## The three refusals every wave here must keep

1. **Nothing moves into `entity-core`.** The purity scan stays as it is. The moment a socket or a
   clock appears inside it, ladder verdicts stop being reproducible and the thesis is gone. C1 makes
   this a live risk for the first time, because it is the wave that introduces a network.
2. **Replay is not a setter** (R-81).
3. **`entity-core` stays free of its adopters.** No crate here appears in a manifest of
   `aep`' (`atlas/architecture/adr/0002`).

## Housekeeping these waves should not carry, but somebody should

* **All 19 stories in `.engineering/planning/story/` are `draft`, including four that shipped** —
  `three-valued-conditions`, `aep-lifecycles-as-definitions`, `aep-move-through-kernel` and
  `aep-open-status-vocabulary`. `typed-references` carries a *"Built 2026-08-25"* section while its
  status still says `draft`. The repository that supplies the ladder does not climb its own.
* **`docs/roadmap.md` § 1 is titled "The blocking fact" and states something that is no longer
  true** — that `aep` *"has never been told this repository exists"*, with a grep
  returning zero hits as its evidence. That grep now returns its README, its `AGENTS.md`, its
  `Cargo.toml`, a concepts page and a release post. The section, and the sequencing built on it,
  describe the world before 0.13.0.
