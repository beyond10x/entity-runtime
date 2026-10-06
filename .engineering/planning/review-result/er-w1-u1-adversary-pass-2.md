---
format: aep.planning-md/3
id: review-result:er-w1-u1-adversary-pass-2
kind: review-result
status: active
title: 'U1 text length: adversary pass 2 (machine paths redacted)'
relations:
- reviews: story:a-condition-reads-the-length-of-a-text
revision: 1
---
**Verdict: red (pass 2).** The correction's two new refusals break replay of decisions that are already stored. `replay` re-checks each record's saved definition (`replay.rs:131`), so a history recorded under a definition the base accepted now fails with `CoreError::Definition`. Both cases pass at base and fail on the current tree.

```
unit: story:a-condition-reads-the-length-of-a-text
verdict: red
cases: executed 336→338, red 2
origin: introduced 2, pre-existing 0, undecided 0
wrote-outside-worktree: ~/.cache/er-w/u1/attack-adversary/ (contents in part 6), ~/.cache/b10x-target/er-w1-u1 (assigned build dir)
needs-coordinator: yes
```

### 1. Diff
I changed one path, a test file: `crates/entity-core/tests/adversary_text_count.rs` (now 317 lines). I only added two cases at the end of it. The coordinator's rewritten case is untouched. The tracked diff (11 files, +4812/−476) is the implementor's. I touched no non-test path.

### 2. Cases (written first, each run alone)
Both cases build a record the base would have written. They decide under the definition minus the part now refused, then put the old definition back as the record's snapshot. A base probe showed this record is byte-identical to the one the base writes when it accepts the old definition directly (`identical bytes: true` ×4, `base replays: true` ×4).

| case | asserts | tree | base b7362882 |
|---|---|---|---|
| `a_decision_recorded_under_a_projection_keyed_on_a_collection_address_still_replays` | a create recorded under a projection keyed on `$fields.tags.count` / `$fields.tags.0` / `$fields.meta.count` replays to the same instance | **red** | green |
| `a_decision_recorded_under_a_quantifier_reading_a_map_count_still_replays` | a create recorded under `for_all g: $g.count lte 5` with `groups: [{count: 1}]` replays | **red** | green |

Red output, verbatim, abbreviated (exit 101 each):
```
a recorded decision no longer replays:
$fields.tags.count: Err(Definition(DefinitionErrors([InvalidTemplate { path: "projections.by_key", message: "`key`: '$fields.tags.count' cannot resolve: 'tags.count' is a collection address, and a projection key reads object members only, …" }])))
$fields.tags.0: Err(Definition(… 'tags.0' is a collection address …))
$fields.meta.count: Err(Definition(… 'meta.count' is a collection address …))
a recorded decision no longer replays: Definition(DefinitionErrors([QuantifierBodyScope { path: "invariants[0].assert.for_all.that.compare.left", … "'$g.count' reads the size of a map inside a quantifier element, …" }]))
```

### 3. Suite, run after the cases existed
`cargo test -p entity-core --locked --no-fail-fast` gave **exit 101**: 336 passed, 2 failed (the two above), 338 executed. The 336 "before" figure comes from `~/.cache/er-w/u1/c1-g-core.log`.

### 4. Findings (working tree of er-w1-u1 over b7362882)
| file:line | what | what reaches it | verdict / origin | case |
|---|---|---|---|---|
| `crates/entity-core/src/validation.rs:1465` | A projection key on a collection address is now refused when a definition is checked, and that includes the saved definition inside a stored record. Projections play no part in any decision, yet those histories stop replaying. | Any `service/1` definition with such a key registered on 0.19.0–0.26.0 (the forms shipped in 0.19.0, `CHANGELOG.md:349-350`), plus its stored decisions. Code-read only: executor retry gives `CorruptHistory` (`entity-executor/src/lib.rs:1153`); shell retry gives `RecordConflict` (`entity-shell/src/lib.rs:198`); store history check gives `CorruptHistory` (`entity-store/src/asynchronous/verify.rs:98`). | NEEDS-CHANGE / introduced | `…projection_keyed_on_a_collection_address_still_replays` |
| `crates/entity-core/src/validation.rs:1507` | The same for map `count` under a quantifier binder: the base accepted it and recorded decisions under it, and now those records fail to replay. | The same paths, for any definition with a quantifier over map elements reading `$g.count`. | NEEDS-CHANGE / introduced | `…quantifier_reading_a_map_count_still_replays` |

Both rows contradict the unit's own claim, "recorded decisions replay unchanged" (CHANGELOG, R-160). One policy decision fixes both. Suggested options, not applied:
- **Separate authoring from replay:** keep the new refusals for new registrations, but check a stored snapshot under the rules it was recorded under. Projection keys would then be skipped on replay, and binder map `count` would keep the base reading so the same bytes come back.
- **Accept the break:** keep the refusals and record this in the CHANGELOG as a breaking change with a migration path, which withdraws the "replay unchanged" claim.

### 5. Attacked and not broken
- **Mutation testing:** 12 of the 13 mutations I applied to the correction (on a scratch copy) are caught by the unit's own suite, not counting my 2 new cases. They cover:
  - dropping `checked`
  - letting a non-text value fall through
  - removing the union reset
  - the ordinal / map / text parts of the projection check
  - both binder refusals
  - passing the reader through the object, array and binder arms
  - the projection guard itself
- **The one survivor (M3, `runtime.rs:2909`):** changing `None` to `_` makes `count.more` past a text answer the length at run time. No address that registration accepts can reach it, so it is equivalent, not a finding.
- **The run-time change "past a checked text, nothing but its length":** the base accepted no path past a typed `string`, so no replayed decision moves.
- **Projections:** a declared object property named `count` stays a valid key. No existing ESS scenario contradicts the new refusals.

### 6. Paths written outside the worktree
- `~/.cache/b10x-target/er-w1-u1`: the assigned build dir.
- `~/.cache/er-w/u1/attack-adversary/`:
  - `base/`: the pass-1 copy had been removed, so I exported b7362882 again. It holds my test file and the probe `crates/entity-core/tests/probe_faithful_record.rs`.
  - `mutant/`: a fresh copy of the tree, restored to the unmutated sources.
  - `orig/`, `mutate.sh`, `mutants.log`, `mut-M*.log`, `p2-*.log`.
  - `base-target/` and `mutant-target/`: my own build dirs, deleted after use because the disk was at 98%.
- Lease `er-w1-u1-adversary`: acquired and released.

### 7.
```findings
- file: crates/entity-core/src/validation.rs
  line: 1465
  category: contract-drift
  severity: blocker
  verdict: NEEDS-CHANGE
  origin: introduced
  message: "replay re-validates each stored snapshot, so a decision recorded under a projection keyed on `<array>.count`, `<array>.<n>` or `<map>.count` (registrable since 0.19.0) no longer replays, against the unit's claim that recorded decisions replay unchanged"
- file: crates/entity-core/src/validation.rs
  line: 1507
  category: contract-drift
  severity: blocker
  verdict: NEEDS-CHANGE
  origin: introduced
  message: "a decision recorded under a quantifier body reading `$g.count` on a map element, which the base registered and decided on, no longer replays because the stored snapshot is now refused"
```
