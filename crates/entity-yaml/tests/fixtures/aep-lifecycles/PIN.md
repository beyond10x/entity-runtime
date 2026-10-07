# Upstream fixture — AEP lifecycle documents

Copied verbatim, not adapted. These files are what
[`aep_lifecycles.rs`](../../aep_lifecycles.rs) checks `examples/aep/*.yaml` against, and they are
committed here rather than read from a sibling checkout so the equivalence test says the same thing
on a machine that has only this repository.

| | |
|---|---|
| source | `github.com/beyond10x/aep`, `artifacts/lifecycles/*.yaml` |
| pinned commit | `5a2a0e5f3f498e70d7e86f59e2b4693292c32248` — current main source, no tag |
| scope | All lifecycle documents at the recorded source commit, including executable system specifications |
| not copied | `artifacts/lifecycles/.gitkeep`, which is not a lifecycle document |
| copied | 2026-10-07 |
| licence | Apache-2.0, the same as this repository |

```
8982ee715013ddec5b9e8fa81a0283c300d662684c05d65a42c3fd0567329e52  architecture-decision-record.yaml
973ec77a5870ab2c1c74e3108370b4b34d20c84c19b38298f3df804c18563a7e  blocker.yaml
33f43d5af14dc0415114edd6a29027bc1ea0c9932b18fb00fa71367ac471c12d  design.yaml
4e85b28e6b81951f1bfc8f2d7e49b8a7da8789a7245d1eac89379d4e9ae701f2  epic.yaml
b96355a54a706cd16c414407c20049a634522412515a113aa1f39c740a14a136  executable-system-specification.yaml
18007b203c8bcbbf131dd5841807600fdc60a20f3eb62dc0a2b8b511831dca3c  initiative.yaml
7224f7515ead95da321fe1c4dc98ee7c1369303ae7409686aa9211459642efc4  obligation.yaml
1c5864142de72db2f3694eaf0e0aef728651bfe8c28d3b7bc3f1abe53f18f8bb  outbound-claim.yaml
a282c5a1fe9abde13354faaa2c05e8bc2308dc7569a56b6b900d82ff870e9bbd  review-result.yaml
357de517350ef2ee6421bc95dfba81cc2b276db193b878c666a9935d6ee7c142  specification.yaml
d3e735684d24595016e181d53f603c6939d8b2f1dc4e3205bcc4cf02feacc22b  story.yaml
699a162bd39c27af9359a071fdd2af6030b5ae06a8de4c9af7d14f9a047a8068  task.yaml
82af20ed7ad0984ef70ac02a4cb8084826a220a3fcc80faa61db75716bebf77d  vision.yaml
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
outside the gate. Atlas owns that signal: its consumer compatibility workflow compares current
AEP source with this fixture on a schedule, without putting a consumer dependency or the network
inside `task check`.

Refreshing this pin is a coordinated decision: the AEP and Entity Runtime equivalence suites both
record the copied boundary.
