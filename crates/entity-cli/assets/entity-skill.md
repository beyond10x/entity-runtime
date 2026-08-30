---
name: entity
description: Use when validating entity definitions, creating or executing lifecycle-governed entities, inspecting stored state, or migrating an entity File Store.
---

# Entity CLI

Generated for `entity {{VERSION}}`. Use the same binary for the commands below; run
`entity --version` and the relevant `entity <verb> --help` before relying on remembered flags.

Read the repository's `AGENTS.md` or equivalent before changing definitions or stores.

## Safe workflow

1. Validate the complete definition set together with `entity validate <files...>`. A reference to
   another entity is valid only when that type is in the same set.
2. Use `entity inspect` or `entity graph` to understand states, operations and references.
3. The caller supplies every identifier and every fact from the outside world. Do not invent a
   timestamp, actor, record id, correlation or causation value.
4. A stored `create` or `execute` requires complete recording metadata. Use exactly one of
   `--actor <id>` and `--no-actor`.
5. Treat exit `0` as decided, `1` as a typed kernel/store refusal, and `2` as an invalid invocation.
   Read the structured refusal; do not parse stderr prose.

Never edit File Store files by hand. For a store written before 0.15, stop writers and run:

```console
entity store migrate-file --from OLD --to NEW --dry-run
entity store migrate-file --from OLD --to NEW
```

Migration is out of place: the destination must not exist and the source remains unchanged. Verify
the new store with `entity list --store NEW --entity <type>` before cutover, and retain the old
directory for rollback. Migrated legacy history begins at an unverified snapshot boundary; do not
claim it was replay-verified from genesis.

Use `entity skill --out .agents/skills/entity/SKILL.md` to install a fresh copy of this document.
