---
sidebar_position: 3
title: Refund quickstart
description: Validate a policy, let an agent propose a refund, observe a typed refusal, and record an authorized decision.
---

# Refund quickstart

This walkthrough uses a refund policy to show the whole boundary: an agent proposes approval, a
deterministic rule refuses it, and the original state remains unchanged.

## Install the command

Every release includes `entity` binaries for Linux, macOS, and Windows plus `SHA256SUMS`:
[download a release](https://github.com/beyond10x/entity-runtime/releases).

Or install from the tagged source with Rust 1.85 or newer:

```bash
cargo install --git https://github.com/beyond10x/entity-runtime \
  --tag 0.16.0 entity-cli
entity --version
```

The complete definition used here is
[`examples/refund.yaml`](https://github.com/beyond10x/entity-runtime/blob/main/examples/refund.yaml).
Save it locally as `refund.yaml`.

## Understand the policy

The definition has four states:

```shell-session
$ entity graph refund.yaml
refund v1: initial draft
draft --submit--> submitted
submitted --approve--> approved
submitted --reject--> rejected
```

The same lifecycle can go straight into Markdown, an issue, or an agent report:

```shell-session
$ entity graph refund.yaml --format mermaid
```

```mermaid
stateDiagram-v2
  state "draft" as n0
  state "submitted" as n1
  state "approved" as n2
  state "rejected" as n3
  [*] --> n0
  n0 --> n1: submit
  n1 --> n2: approve
  n1 --> n3: reject
  n2 --> [*]
  n3 --> [*]
```

This block is the command's exact output, not a hand-maintained second diagram.

Approval has two preconditions:

```yaml
preconditions:
  - name: evidence_is_present
    assert: { gt: [$fields.evidence_count, 0] }
    message: a refund cannot be approved without evidence
  - name: large_refunds_need_a_human
    assert:
      any:
        - lte: [$fields.amount_cents, 5000]
        - eq: [$args.actor_role, human]
    message: refunds above 5000 cents require a human actor
```

Validate the complete document before using it:

```shell-session
$ entity validate refund.yaml
refund.yaml: valid (refund v1)
1 file(s), 0 invalid
```

## Create and submit a request

The kernel generates no identity, so the caller supplies one:

```shell-session
$ entity create --definition refund.yaml --id refund-104 \
    --fields '{"order_id":"order-88","amount_cents":12500,"evidence_count":2}' \
  > draft.json

$ entity execute --definition refund.yaml --instance @draft.json \
    --operation submit > submitted.json
```

`submitted.json` is a `Decision`: it contains the new instance, its complete replay record, and the
events produced by `submit`.

## Let the agent propose approval

The trusted shell supplies `actor_role: agent` from its execution context. It is not a label the
model is allowed to choose.

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
$ echo $?
1
```

The operation was understood and refused. `submitted.json` is still submitted at revision 2, and
no `RefundApproved` event exists.

## Authorize and store the decision

For durable use, create and execute against a File Store. Stored commands require provenance the
kernel cannot invent:

```shell-session
$ entity create --definition refund.yaml --id refund-104 \
    --fields '{"order_id":"order-88","amount_cents":12500,"evidence_count":2}' \
    --store ./refund-store --record-id request-104-created \
    --recorded-at 2026-08-31T10:00:00Z --actor support-api

$ entity execute --definition refund.yaml --store ./refund-store \
    --id refund-104 --operation submit \
    --record-id request-104-submitted --recorded-at 2026-08-31T10:01:00Z \
    --actor support-agent

$ entity execute --definition refund.yaml --store ./refund-store \
    --id refund-104 --operation approve \
    --arguments '{"actor_role":"human","reason":"supervisor verified the delivery evidence"}' \
    --record-id request-104-approved --recorded-at 2026-08-31T10:04:00Z \
    --actor supervisor-7 --format text
refund refund-104 is approved (revision 3); record request-104-approved; events: RefundApproved
```

The store compares revision 2 before committing revision 3. A concurrent writer is refused instead
of overwritten. The resulting record includes the caller-supplied actor and time, normalized
command, exact definition snapshot, new instance, changes, and events.

## Next

- [Model policy as data](./modeling)
- [Connect an agent safely](./agent-integration)
- [Understand stored decisions](./storage)
- [Read the complete CLI reference](./cli)
- [Render lifecycle and reference graphs](./graphs)
