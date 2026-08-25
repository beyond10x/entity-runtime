# Store conventions

What the on-disk store looks like, and which CLI operation changes each part. This file carries only
what the CLI cannot answer at runtime — the vocabulary questions (kinds, statuses, legal moves,
relations) all have commands, and those commands are the authority. See `SKILL.md` §2.

## Layout

```
.engineering/planning/
├── epic/
│   └── passkey-login.md
├── story/
│   ├── credential-store.md
│   └── registration-ceremony.md
└── task/
    └── ceremony-fixtures.md
```

One directory per kind, one file per artifact, no nesting below the kind directory. The store root
defaults to `.engineering/planning/` and moves with `--store <dir>` — a repository may keep more than
one, and nothing in a file records which store it belongs to.

## Names and ids

An artifact's id is `<kind>:<slug>`, and the path is `<kind>/<slug>.md`. The two are the same fact
written twice, so they cannot be allowed to disagree:

| Rule | Why |
|---|---|
| `id` in frontmatter equals `<kind>:<slug>` from the path | the id is how relations point at this file; a mismatch makes an edge resolve to nothing |
| slug is lowercase, hyphen-separated, no dots | it is a filename and an id segment at once |
| the file lives under the directory named by its `kind` | `protocol artifact list --kind` reads the directory, and a misfiled artifact is invisible to it |

Renaming therefore is not a `mv`. Moving a file by hand breaks every relation that names its old id
and leaves `validate` to find the wreckage. Create the new artifact, re-point the relations, and
archive the old one through its lifecycle.

## Mutation ownership

No planning-store file is edited directly. One command surface owns every change:

| Change | Command |
|---|---|
| create an artifact | `protocol artifact new` |
| replace its complete markdown body | `protocol artifact body <id> --from <path|->` |
| add a relation | `protocol artifact relate` |
| move lifecycle status | `protocol artifact move` |

This makes the store a single-writer system: every mutation loads and validates it before writing,
and every changed document receives the same revision semantics.

## Frontmatter

Everything above the `---` is structured. Ownership is what decides whether you may edit it:

| Field | Owner | Notes |
|---|---|---|
| `id` | machine | set at creation, never edited; equals `<kind>:<slug>` |
| `kind` | machine | fixed at creation; changing a kind means a new artifact |
| `status` | machine | **only** `protocol artifact move` writes this — see guardrail 1 |
| `revision` | machine | bumped by the CLI; a review is bound to the revision it saw |
| `relations` | machine | written by `protocol artifact new --relate` and `protocol artifact relate` |
| `title` | descriptive | set at creation; correcting a typo by hand is harmless |
| `summary` | descriptive | one or two sentences; optional |
| `format` | machine | optional; defaults to `aep.planning-md/1` when absent — a file may omit it |

"Machine" means the CLI validates it against a document you do not control from the file. A
hand-written `status` is not a faster move — it is an unvalidated one, and it looks identical to a
legal one afterwards, which is what makes it expensive.

## Body

Everything below the closing `---` is authored by you and the operator, but written through
`protocol artifact body`. There is no required section list at this layer; a kind may declare
expected sections in `artifacts/kinds/<kind>.yaml`, and `protocol artifact kinds` reports what a
kind is for. Write plainly: what this is, why now, what counts as done.

One convention worth keeping: **a story or task carries a single acceptance statement**, one
sentence, in the form of an observable outcome. It is what a reviewer checks against and what the
`plan-reviewer` agent looks for.

## A complete file

`.engineering/planning/story/credential-store.md`:

```markdown
---
format: aep.planning-md/1
id: story:credential-store
kind: story
status: proposed
title: Store and retrieve passkey credentials
summary: Persist WebAuthn credential ids and public keys, and look them up at assertion time.
relations:
- derived_from: epic:passkey-login
revision: 2
---

## Context

Sign-in is being moved to passkeys. The assertion ceremony needs a credential record it can look up
by user handle, and registration needs somewhere to put one. Nothing else in the epic can be
demonstrated until this exists.

## Acceptance

A credential registered through the ceremony is returned by lookup on the next sign-in attempt, and
survives a process restart.

## Notes

Storage backend is settled (the existing Postgres schema); the open question is whether the public
key is stored COSE-encoded or normalised on write. Raised in `epic:passkey-login`.
```

The frontmatter is six machine-owned lines and two descriptive ones. Everything that took thought is
below the fence, which is the intended shape: the CLI keeps the graph honest, and the file stays a
document a person can read.
