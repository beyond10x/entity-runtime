---
sidebar_position: 2
title: Getting started
description: Install entity, validate a refund policy, watch a rule refuse an agent's approval, and store an authorized decision.
---

# Getting started

This walkthrough uses a refund policy to show the whole boundary: an agent proposes approval, a
deterministic rule refuses it, the original state stays as it was, and a human approval is stored
with its provenance. Every output below is from `entity 0.27.0`.

## Install the command

Every release carries `entity` for Linux (x86_64, aarch64), macOS (x86_64, arm64) and Windows
(x86_64) with a `SHA256SUMS` file:
[0.27.0](https://github.com/beyond10x/entity-runtime/releases/tag/0.27.0). Unpack the archive for
your platform and put `entity` on your `PATH`.

Or build it from the tagged source with Rust 1.85 or newer:

```bash
cargo install --git https://github.com/beyond10x/entity-runtime \
  --tag 0.27.0 --locked entity-cli
```

```shell-session
$ entity --version
entity 0.27.0
```

The definition used here is
[`examples/refund.yaml`](https://github.com/beyond10x/entity-runtime/blob/0.27.0/examples/refund.yaml).
Download it into a fresh directory so nothing else collides with its files or store:

```bash
cd "$(mktemp -d)"
curl --fail --silent --show-error --location \
  https://raw.githubusercontent.com/beyond10x/entity-runtime/0.27.0/examples/refund.yaml \
  --output refund.yaml
```

The actor names and timestamps below are sample data, not authenticated identities or a trusted
clock.

## Read the policy

```shell-session
$ entity validate refund.yaml
refund.yaml: valid (refund v1)
1 file(s), 0 invalid
$ entity graph refund.yaml
refund v1: initial draft
draft --submit--> submitted
submitted --approve--> approved
submitted --reject--> rejected
```

Approval has two preconditions. The second one is the policy this walkthrough exercises:

```yaml
preconditions:
  - name: evidence_is_present
    assert:
      gt: [$fields.evidence_count, 0]
    message: a refund cannot be approved without evidence
  - name: large_refunds_need_a_human
    assert:
      any:
        - lte: [$fields.amount_cents, 5000]
        - eq: [$args.actor_role, human]
    message: refunds above 5000 cents require a human actor
```

## Create and submit a refund

The kernel generates no identity, so the caller supplies one. Without `--store`, each command
prints a `Decision` and keeps nothing; the next command reads it back with `@file`:

```bash
entity create --definition refund.yaml --id refund-104 \
  --fields '{"order_id":"order-88","amount_cents":12500,"evidence_count":2}' \
  > draft.json
entity execute --definition refund.yaml --instance @draft.json \
  --operation submit > submitted.json
```

`submitted.json` holds the new instance at revision 2, its complete decision record and the
`RefundSubmitted` event.

## Let the agent propose approval

The trusted shell sets `actor_role` from what it knows about the caller; it is not something the
model gets to choose. Here the caller is an agent:

```shell-session
$ entity execute --definition refund.yaml --instance @submitted.json \
    --operation approve \
    --arguments '{"actor_role":"agent","reason":"customer supplied delivery evidence"}'
{
  "kind": "precondition_failed",
  "message": "precondition 'large_refunds_need_a_human' failed for operation 'approve': refunds above 5000 cents require a human actor",
  "operation": "approve",
  "reason": "refunds above 5000 cents require a human actor",
  "rule": "large_refunds_need_a_human"
}
refused: precondition 'large_refunds_need_a_human' failed for operation 'approve': refunds above 5000 cents require a human actor
```

The JSON goes to standard output and the sentence to standard error; the exit status is `1`. The
request was understood and refused: `submitted.json` is still at revision 2, and no
`RefundApproved` event exists.

## Store an authorized decision

With `--store`, the decision is committed to a File Store directory. A stored command needs the
provenance the kernel cannot invent: a record id, a recorded-at time, and an actor (or
`--no-actor`):

```bash
entity create --definition refund.yaml --id refund-104 \
  --fields '{"order_id":"order-88","amount_cents":12500,"evidence_count":2}' \
  --store ./refund-store --record-id request-104-created \
  --recorded-at 2026-08-31T10:00:00Z --actor support-api > /dev/null
entity execute --definition refund.yaml --store ./refund-store \
  --id refund-104 --operation submit \
  --record-id request-104-submitted --recorded-at 2026-08-31T10:01:00Z \
  --actor support-agent > /dev/null
```

```shell-session
$ entity execute --definition refund.yaml --store ./refund-store \
    --id refund-104 --operation approve \
    --arguments '{"actor_role":"human","reason":"supervisor verified the delivery evidence"}' \
    --record-id request-104-approved --recorded-at 2026-08-31T10:04:00Z \
    --actor supervisor-7 --format text
refund refund-104 is approved (revision 3); record request-104-approved; events: RefundApproved
$ entity list --store ./refund-store --entity refund
refund-104
```

`execute --store` loads the stored instance, decides, and commits at the revision it loaded, so a
concurrent writer is refused instead of overwritten. The stored record keeps the caller-supplied
actor and time, the normalized command, the exact definition, the changes and the events.

## Retry an accepted request

To recover a lost response, repeat the request with the same record id and name the revision it
was decided on. The original record comes back; nothing is written twice:

```shell-session
$ entity execute --definition refund.yaml --store ./refund-store \
    --id refund-104 --operation approve --expected-revision 2 \
    --arguments '{"actor_role":"human","reason":"supervisor verified the delivery evidence"}' \
    --record-id request-104-approved --recorded-at 2026-08-31T10:04:00Z \
    --actor supervisor-7 --format text
refund refund-104 is approved (revision 3); record request-104-approved; events: RefundApproved
```

Without `--expected-revision`, the command decides on the revision the store holds now — 3 — so
the same record id names a different request and is refused:

```shell-session
$ entity execute --definition refund.yaml --store ./refund-store \
    --id refund-104 --operation approve \
    --arguments '{"actor_role":"human","reason":"supervisor verified the delivery evidence"}' \
    --record-id request-104-approved --recorded-at 2026-08-31T10:04:00Z \
    --actor supervisor-7 --format text
{
  "by": "store",
  "detail": "record id \"request-104-approved\" already names different bytes",
  "kind": "record_conflict",
  "refused": true
}
refused: record id "request-104-approved" already names different bytes
```

## Next

- [Model policy as data](./guides/model-policy-as-data.md) for your own domain.
- [Connect an agent](./guides/connect-an-agent.md) without giving it authority.
- [Storage and replay](./concepts/storage.md) for the write contract and every provider.
- [The `entity` CLI reference](./reference/cli.md) for every option.
