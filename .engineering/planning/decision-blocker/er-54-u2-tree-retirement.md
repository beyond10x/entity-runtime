---
format: aep.planning-md/3
id: decision-blocker:er-54-u2-tree-retirement
kind: decision-blocker
status: cleared
title: The U2 tree of the issue 54 wave cannot be archived without deleting scratch build copies
relations:
- blocks: design:wave-issue-54-entity-core-features
revision: 3
transitions:
- {from: "open", to: "cleared", at: "2026-10-07T08:53:22Z", actor: "human:timo", revision: 3}
---
# The U2 tree of the issue 54 wave cannot be archived

The idle tree `er-54-u2` (its branch merged into `wave/er-54-w2` at `a70a22e5`) fails
`worktree finish --discard-cache --archive` with "patch too large": `build/scratch/base` (a `git archive`
copy of `eb3e275a` with its own Cargo `target/`, 787 MiB) and `build/scratch/adversary/mutant` (a mutated
source copy with its own `target/`) are ignored files the archive must carry, and `discard-cache` does
not recognise nested targets. Deleting a `target/` by hand is against the repository's working rules.

Options: (A) delete the two copies by exact path, then finish, archive and gc the tree; (B) keep the
tree until the worktree tool discards nested tagged targets. Recommended: A, because both copies are
reproducible from pushed commits and no record cites them. Until decided, the wave is not clean.

## Decided

The two nested build directories were removed with `cargo clean` on each copy's own manifest (779.5 MiB and 171.6 MiB), the source copies kept, and the tree finished, archived and removed. Nothing cited either directory.
