# Provider tracking verification

The final integration gate, `task check`, exited 0 with the published Eventlog dependency
`6983cc25eb92e07844b3a6fa3e0decbdb300f43c`. Both real PostgreSQL lanes executed against the
same disposable fixture. All 421 original ESS scenarios and all 17 provider scenarios passed,
with no ESS skips or unsupported cases. The provider suite contains seven authored semantic
cases and ten generated command-response cases. Exact reports and suites are admitted in AEP.

The release performance probe was then run explicitly with Rust 1.91:

```console
CARGO_BUILD_JOBS=2 cargo +1.91.0 test -p entity-eventlog --features sqlite,sync-bridge --release --locked --test shared_clock_cost -- --ignored --nocapture
```

It passed on the first final integrated run: warm batch medians were 11.532148 ms, 13.049370 ms,
and 13.568977 ms for 55, 601 and 1,203 initial provider events. Growth was 1.132x and 1.177x,
below the unchanged 2x bound. The ordinary gate intentionally ignores this timing probe;
this separate command executed it. `performance-final.txt` retains the complete test stdout,
starting at `running 1 test`; compiler output with local paths remains private.

The earlier matched baseline failed (92.517484 ms / 7,038.153041 ms / 12,449.723852 ms).
The first development treatment passed (10.511861 ms / 11.983571 ms / 17.701283 ms).
The final run above follows receipt-validation and projection-alias review corrections and
uses the published Git dependency without path overrides. The governed story and review
records distinguish these runs and retain the negative controls.

Only warm batches have the 2x claim. Cold full verification and full-history output scale
with their inputs and outputs. SQL-visible changes invalidate tracked observations;
raw database-file edits bypassing SQLite are outside this explicitly selected warm policy.
No consumer adoption or complete consumer-invocation benchmark is claimed.

`source.sha256` identifies the implementation and Cargo inputs tested. Verify it from the
repository root with `sha256sum --check`. The final commit additionally records documents
and evidence without changing those inputs.

## Opening from a persisted checkpoint (issue 55)

The provider suite gained four commands and ten authored scenarios for the bounded open
([design](../../../design/recorded-open-checkpoint-v0.1.md)); the seventeen issue 51 contracts kept
their digests. `open-cost-baseline.txt` measures today's complete `ProviderTracked` open before the
change; `open-cost-checkpoint.txt` measures the open from a checkpoint at the previous head and from
one with a two-event suffix after it, with the same seeded stores, sizes and median-of-five method,
and holds the bound the implementation story set: the median open at 1,203 events is at most twice
the median at 55. Neither file is a gate: the probes are `#[ignore]`d release tests, run on a
host shared with other builds, so each file records the load average around each run.
