# Upstream fixture — AEP lifecycle documents

Copied verbatim, not adapted. These files are what
[`aep_lifecycles.rs`](../../aep_lifecycles.rs) checks `examples/aep/*.yaml` against, and they are
committed here rather than read from a sibling checkout so the equivalence test says the same thing
on a machine that has only this repository.

| | |
|---|---|
| source | `github.com/beyond10x/aep`, `artifacts/lifecycles/*.yaml` |
| pinned commit | `7685336899004a99939dab5447e430c4787167d7` — current main source, no tag; its ladders equal tag `0.80.0` |
| scope | All lifecycle documents at the recorded source commit, including executable system specifications |
| not copied | `artifacts/lifecycles/.gitkeep`, which is not a lifecycle document |
| copied | 2026-11-22 |
| licence | Apache-2.0, the same as this repository |

```
e52a023d3ce76868ed1511b2a273d8b6559697df6a3a14e9403b2152168dcb0c  architecture-decision-record.yaml
973ec77a5870ab2c1c74e3108370b4b34d20c84c19b38298f3df804c18563a7e  blocker.yaml
aa0ac01813eb606bbd1a97c75daccd503d3f06e2ad33557f2588fd0e01063f35  design.yaml
2ccb96bbf7a916c936bcca127aa14ca6a2013f29f2ea7dc50c5b87fa4fc9e20d  epic.yaml
b96355a54a706cd16c414407c20049a634522412515a113aa1f39c740a14a136  executable-system-specification.yaml
b2e277e8dff2b07b619933a0a9c8920f4c0914701c6c41bce0aaba14ecc3380c  initiative.yaml
7224f7515ead95da321fe1c4dc98ee7c1369303ae7409686aa9211459642efc4  obligation.yaml
1c5864142de72db2f3694eaf0e0aef728651bfe8c28d3b7bc3f1abe53f18f8bb  outbound-claim.yaml
a282c5a1fe9abde13354faaa2c05e8bc2308dc7569a56b6b900d82ff870e9bbd  review-result.yaml
360ab1d15a42da429bf51f590c303ef333315fea29eec561e9121de94c7a1630  specification.yaml
ffed93f119dec34fe30353e4625e7cfaa7ff014aef3b14b191317de8a380e1ad  story.yaml
a55bda7112367b5ecf865b76541ef4c32849c2c4b2ed3416312af383e3b145c5  task.yaml
896dae73525253ede45f05324f1c62f6ac55acdecb9bb119e5ed7a24b156d3a2  vision.yaml
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
