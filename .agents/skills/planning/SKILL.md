---
name: planning
description: Plan engineering work in a governed markdown artifact store — create, relate, move and validate epics, stories, tasks and initiatives through the `protocol` CLI. Use when the user mentions planning, a backlog, an epic, a story, a task, decomposing or breaking down work, an artifact's status ("move this to active", "what is still in draft?", "why can't this be implemented?"), or when the project contains a `.engineering/planning/` directory. Also use before editing any file under `.engineering/planning/`.
---

# Planning in a governed artifact store

## 1. The model

Artifacts are markdown files under `.engineering/planning/<kind>/<slug>.md`: YAML frontmatter the
CLI owns, and a body you and the operator own. Which kinds exist, which statuses each kind may hold
and which moves between them are legal come from validated lifecycle documents, not from convention
and not from this file. The `protocol` CLI is the authority on both, so every question about
vocabulary has a command that answers it.

## 2. Discover, do not memorise

This skill inlines **rules only**. It deliberately carries no list of kinds, statuses, legal moves
or relations. Ask for them at the moment you need them:

| Question | Command |
|---|---|
| What kinds can I create? | `protocol artifact kinds` |
| What edges exist between artifacts? | `protocol artifact relations` |
| What statuses does this kind have, and what moves where? | `protocol artifact lifecycle <kind>` |
| What is already in the store? | `protocol artifact list [--kind k] [--status s] [--format json]` |
| What does it look like as a board? | `protocol artifact board [--kind k]` |
| How is it wired together? | `protocol artifact graph` |

The reason is the reason this project exists. Lifecycle and relation documents are validated and
versioned; a prose copy of them in a skill file is neither, and it goes stale the first time a kind
gains a status. An agent that recites `draft → proposed → active` from memory will confidently
propose an illegal move in a store that renamed one of them. Reading `protocol artifact
lifecycle story` costs one command and cannot be wrong.

When a store is present but you have not looked at it yet in this session, start with `protocol
artifact list` and `protocol artifact kinds`. Two commands buy you the whole vocabulary.

## 3. Five guardrails

These are inlined because they hold whatever the store's vocabulary is.

**1. A status changes only through `protocol artifact move`.** Never edit the `status:` field in
frontmatter, and never write it into a file with a patch or a heredoc. The CLI validates the move
against the kind's lifecycle; a hand-edited status is an unvalidated one, indistinguishable in the
file from a legal one and wrong in exactly the cases that matter.

```console
$ protocol artifact move story:credential-store --to proposed
story:credential-store moved draft -> proposed (revision 2)
```

**2. Every store mutation uses `protocol artifact`; never edit a store file directly.** Creation,
relations, status, and prose use `new`, `relate`, `move`, and `body` respectively. Supply the complete
body from a file or standard input; the CLI preserves frontmatter, validates the store, and bumps the
revision once when bytes change.

```console
$ protocol artifact body story:credential-store --from story-body.md
story:credential-store body replaced (revision 2) at .engineering/planning/story/credential-store.md
```

**3. After a batch of edits, run `protocol artifact validate`, and relay its output verbatim.** It
accumulates every problem rather than stopping at the first, and exits 1 if any remain. Do not
summarise it into "validation failed" — the output names each artifact and each defect, which is the
only part the operator can act on.

**4. A refusal is the answer, not an obstacle.** When the CLI refuses a move it exits 1 and names
every status legal from where the artifact stands. Relay that list. Do not retry with a different
spelling, do not route around it by editing the file, and do not pick an intermediate status to
"get there" without saying so.

```console
$ protocol artifact move story:credential-store --to implemented
story:credential-store is proposed; a story may move to: draft, rejected, active
$ echo $?
1
```

The right response is to tell the operator that the story must be active first, and ask whether to
walk it there — not to perform two moves nobody sanctioned.

**5. A request that is already satisfied gets an artifact, not a question.** Finding that the asked-
for behaviour already exists — or should not be built — is a real result and often a better one than
the change. Record it: write the `specification` that states what you found, cite the code and the
command output that show it, and say plainly that you did not build the thing and why. Then file the
gap you actually found, if there is one.

What this rules out is ending the turn on *which of these two would you like?*. A session that stops
to ask has produced nothing, and there is frequently nobody there to answer — the same work run
non-interactively ends with an empty tree and a question into a log. `adp/default` has a terminal
`declined` state for precisely this outcome, and the distinction it draws is the one that matters
here: **a decline that is written down is a result; a decline that is only said is a run that did
nothing.**

This is not licence to argue with the request. Build what was asked for unless you have evidence it
is already there or actively wrong, put that evidence in the artifact, and leave the decision where
§ 4 leaves every other one — with the operator, who can now read what you found instead of
answering a question.

## 4. Who decides

You propose; the operator decides.

* New artifacts are created in the lifecycle's initial status — `protocol artifact new` does this,
  and it is the correct starting point. Do not immediately move them.
* Status moves and decompositions are **proposals** until confirmed. Say what you would do, to which
  ids, and wait.
* The exception: a move the operator already asked for by name ("move `story:credential-store` to
  active") is confirmed. Perform it and report the result. Asking again is not caution, it is noise.
* Never perform a bulk move autonomously. "Archive everything still in draft" is an instruction;
  inferring it from a tidy-up request is not.

Writing new artifacts and editing bodies needs no confirmation beyond the request that prompted it —
a draft is cheap and reversible. Moving one through its lifecycle is a claim about the state of the
world, and that is the operator's to make.

## 5. A worked decomposition

An epic, two stories derived from it, one move, one validation.

```console
$ protocol artifact new epic passkey-login \
    --title "Passkey login" \
    --summary "Replace password sign-in with WebAuthn passkeys."
created epic:passkey-login (draft) at .engineering/planning/epic/passkey-login.md

$ protocol artifact new story credential-store \
    --title "Store and retrieve passkey credentials" \
    --relate derived_from:epic:passkey-login
created story:credential-store (draft) at .engineering/planning/story/credential-store.md

$ protocol artifact new story registration-ceremony \
    --title "Register a passkey during sign-up" \
    --relate derived_from:epic:passkey-login
created story:registration-ceremony (draft) at .engineering/planning/story/registration-ceremony.md
```

Then write each story's complete body through `protocol artifact body <id> --from <path|->` — one
acceptance statement per story, because guardrail 2 makes the CLI the store's sole writer.

Then the one move the operator asked for, and the check:

```console
$ protocol artifact move story:credential-store --to proposed
story:credential-store moved draft -> proposed (revision 2)

$ protocol artifact validate
3 file(s) in .engineering/planning: 3 artifact(s)
valid
```

Had `validate` found something, its output would have gone to the operator unedited.

## Reference

The on-disk format — directory layout, filename and id rules, which frontmatter fields are
machine-owned, and a complete example file — is one file in this repository, at
`integrations/claude-code/skills/planning/references/store-conventions.md`. It is harness-neutral
and it is **deliberately not duplicated here**: a second copy of a document is a document that goes
stale, which is § 2's argument applied to this repository's own tree rather than to a store's
vocabulary. When this skill is installed outside a checkout, copy that file to
`references/store-conventions.md` beside this one — `integrations/codex/README.md` § *Install* has
the line. Read it before changing a store document. Everything else is a question for the CLI.
