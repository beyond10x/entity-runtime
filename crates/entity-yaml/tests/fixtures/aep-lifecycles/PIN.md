# Upstream fixture — `engineering-protocols` lifecycle documents

Copied verbatim, not adapted. These eight files are what
[`aep_lifecycles.rs`](../../aep_lifecycles.rs) checks `examples/aep/*.yaml` against, and they are
committed here rather than read from a sibling checkout so the equivalence test says the same thing
on a machine that has only this repository.

| | |
|---|---|
| source | `github.com/beyond10x/engineering-protocols`, `artifacts/lifecycles/*.yaml` |
| pinned commit | `79b641c9c75411627669dfce7f0bac04d4472463` |
| last upstream change to these files | `7c232ac95ce7d4aefd3f8aa9e031261fafb7e436`, 2026-08-21 — *feat(planning): a markdown store, and status moves the lifecycle checks first* |
| copied | 2026-08-25 |
| licence | Apache-2.0, the same as this repository |

```
8982ee715013ddec5b9e8fa81a0283c300d662684c05d65a42c3fd0567329e52  architecture-decision-record.yaml
ca15b5c1c630b3ca4a794305024edf03b16921c635a6f53da38037562caaa9e5  design.yaml
fe8e08bd3c57f988ed6228ae7060bb7393893ce51d680a171c99f4bb8bfbe858  epic.yaml
602c4fb794f846ed1c280b22d01842e28a1e692dfd752eb7a5fc220819dfeae2  initiative.yaml
a282c5a1fe9abde13354faaa2c05e8bc2308dc7569a56b6b900d82ff870e9bbd  review-result.yaml
357de517350ef2ee6421bc95dfba81cc2b276db193b878c666a9935d6ee7c142  specification.yaml
9c74bc3188153f6e34eb94c68c19fec60f920fd642e64aecb07269ec4fbd8510  story.yaml
638bc428c36f2cd7ffaa0b0ba5cc7306db90805db57cf8b1ec44373cb011c3a0  task.yaml
```

## Refreshing the pin

Copy the files again, update the commit and the sums above, and run `cargo test -p entity-yaml`.
A refresh that makes the test fail is the point of the fixture: it means the upstream ladder moved
and `examples/aep/` has not, which is a fact somebody has to decide about rather than a merge
conflict to resolve.

Until phase 0 has a verdict (`story:aep-mapping-review`), that decision is *this* repository's alone:
nothing in `engineering-protocols` knows these files were copied.
