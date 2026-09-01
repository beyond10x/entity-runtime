# Upstream fixture — AEP lifecycle documents

Copied verbatim, not adapted. These files are what
[`aep_lifecycles.rs`](../../aep_lifecycles.rs) checks `examples/aep/*.yaml` against, and they are
committed here rather than read from a sibling checkout so the equivalence test says the same thing
on a machine that has only this repository.

| | |
|---|---|
| source | `github.com/beyond10x/aep`, `artifacts/lifecycles/*.yaml` |
| pinned commit | `714d256e6810834d7de0d670c1e8eff0f79a76c8` — tagged `0.40.0` |
| last upstream change to these files | `4d331a0`, 2026-08-26 — *fix: `outbound-claim` starts at `draft`, and the pin says 0.5.2* |
| copied | 2026-09-01 |
| licence | Apache-2.0, the same as this repository |

```
8982ee715013ddec5b9e8fa81a0283c300d662684c05d65a42c3fd0567329e52  architecture-decision-record.yaml
973ec77a5870ab2c1c74e3108370b4b34d20c84c19b38298f3df804c18563a7e  blocker.yaml
ca15b5c1c630b3ca4a794305024edf03b16921c635a6f53da38037562caaa9e5  design.yaml
fe8e08bd3c57f988ed6228ae7060bb7393893ce51d680a171c99f4bb8bfbe858  epic.yaml
602c4fb794f846ed1c280b22d01842e28a1e692dfd752eb7a5fc220819dfeae2  initiative.yaml
7224f7515ead95da321fe1c4dc98ee7c1369303ae7409686aa9211459642efc4  obligation.yaml
006abbc630d78fbc994bf25a93bef97afee252bb14504e2d15499c6d1d72e1a3  outbound-claim.yaml
a282c5a1fe9abde13354faaa2c05e8bc2308dc7569a56b6b900d82ff870e9bbd  review-result.yaml
357de517350ef2ee6421bc95dfba81cc2b276db193b878c666a9935d6ee7c142  specification.yaml
982e7690baf1584c0049969ce731572cb3be8c8919327d42f39d88f3709b03fc  story.yaml
638bc428c36f2cd7ffaa0b0ba5cc7306db90805db57cf8b1ec44373cb011c3a0  task.yaml
db65a35124f2aff6c3c9e14a792cc6316b5a1626df51d9ea3dda54bdda9e994d  vision.yaml
```

## Refreshing the pin

Copy the files again, update the commit and the sums above, and run `cargo test -p entity-yaml`.
A refresh that makes the test fail is the point of the fixture: it means the upstream ladder moved
and `examples/aep/` has not, which is a fact somebody has to decide about rather than a merge
conflict to resolve.

## What the pin does not do

It holds the copy honest — `pin-check` recomputes every sum above on each run of the gate, so a file
that changes here without its sum changing is refused. It says **nothing** about whether the copy is
still what upstream ships.

That gap is not theoretical: `vision.yaml` landed upstream in AEP's predecessor and
this repository stayed green for as long as it took somebody to notice, with an equivalence test
asserting agreement about eight ladders while nine existed. Nothing here reaches
AEP at build or test time, deliberately — a test whose coverage depends on a
sibling checkout says a different thing on a machine that has none — so the signal has to come from
outside the gate. `.github/workflows/upstream-pin.yml` is that signal: it clones upstream on a
schedule and opens the question, without putting the network inside `task check`.

Refreshing this pin is a coordinated decision: the AEP and Entity Runtime equivalence suites both
record the copied boundary.
